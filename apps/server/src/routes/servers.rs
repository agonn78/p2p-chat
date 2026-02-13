use axum::extract::ws::Message as WsMessage;
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    routing::{delete, get, post, put},
    Json, Router,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use uuid::Uuid;
use validator::Validate;

use crate::auth::AuthUser;
use crate::models::{Channel, ChannelMessage, Server, ServerMemberWithUser};
use crate::state::AppState;
use crate::validation::{
    extract_mentions, validate_avatar_url, validate_channel_name, validate_emoji,
    validate_message_content, validate_server_name,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(list_servers).post(create_server))
        .route("/:id", get(get_server).delete(delete_server))
        .route("/:id/invite/regenerate", post(regenerate_invite_code))
        .route("/:id/members", get(get_members))
        .route("/:id/members/:member_id/role", put(update_member_role))
        .route("/:id/members/:member_id/kick", post(kick_member))
        .route("/:id/members/:member_id/ban", post(ban_member))
        .route("/:id/bans", get(list_server_bans))
        .route("/:id/bans/:member_id", delete(unban_member))
        .route("/:id/channels", post(create_channel))
        .route(
            "/:id/channels/:channel_id",
            put(update_channel).delete(delete_channel),
        )
        .route(
            "/:id/channels/:channel_id/typing",
            post(send_channel_typing),
        )
        .route(
            "/:id/channels/:channel_id/voice",
            get(get_voice_channel_presence),
        )
        .route(
            "/:id/channels/:channel_id/voice/join",
            post(join_voice_channel),
        )
        .route(
            "/:id/channels/:channel_id/voice/leave",
            post(leave_voice_channel),
        )
        .route(
            "/:id/channels/:channel_id/messages/search",
            get(search_channel_messages),
        )
        .route(
            "/:id/channels/:channel_id/messages",
            get(get_channel_messages).post(send_channel_message),
        )
        .route(
            "/:id/channels/:channel_id/messages/:message_id/reactions",
            get(get_channel_message_reactions).post(add_channel_message_reaction),
        )
        .route(
            "/:id/channels/:channel_id/messages/:message_id/reactions/:emoji",
            delete(remove_channel_message_reaction),
        )
        .route(
            "/:id/channels/:channel_id/messages/:message_id/thread",
            get(get_channel_thread_messages).post(send_channel_thread_message),
        )
        .route(
            "/:id/channels/:channel_id/messages/:message_id",
            put(edit_channel_message),
        )
        .route("/join/:code", post(join_server))
        .route("/:id/leave", post(leave_server))
}

// Generate random invite code using UUID
fn generate_invite_code() -> String {
    Uuid::new_v4().to_string()[..8].to_uppercase()
}

// === Request/Response types ===

#[derive(Deserialize, Validate)]
pub struct CreateServerRequest {
    #[validate(length(min = 2, max = 100), custom(function = "validate_server_name"))]
    pub name: String,
    pub icon_url: Option<String>,
}

#[derive(Deserialize, Validate)]
pub struct CreateChannelRequest {
    #[validate(length(min = 1, max = 64), custom(function = "validate_channel_name"))]
    pub name: String,
    pub channel_type: Option<String>,
}

#[derive(Deserialize, Validate)]
pub struct SendMessageRequest {
    #[validate(
        length(min = 1, max = 4000),
        custom(function = "validate_message_content")
    )]
    pub content: String,
    pub nonce: Option<String>,
    pub client_id: Option<Uuid>,
    pub parent_message_id: Option<Uuid>,
}

#[derive(Deserialize, Validate)]
pub struct EditChannelMessageRequest {
    #[validate(
        length(min = 1, max = 4000),
        custom(function = "validate_message_content")
    )]
    pub content: String,
    pub nonce: Option<String>,
}

#[derive(Deserialize)]
pub struct ChannelPaginationParams {
    pub before: Option<Uuid>,
    pub limit: Option<i64>,
}

#[derive(Deserialize)]
pub struct TypingRequest {
    pub is_typing: bool,
}

#[derive(Deserialize, Validate)]
pub struct SearchChannelMessagesQuery {
    #[validate(length(min = 1, max = 128))]
    pub q: String,
    pub limit: Option<i64>,
}

#[derive(Deserialize)]
pub struct UpdateChannelRequest {
    pub name: Option<String>,
    pub position: Option<i32>,
}

#[derive(Deserialize)]
pub struct UpdateMemberRoleRequest {
    pub role: String,
}

#[derive(Deserialize)]
pub struct BanMemberRequest {
    pub reason: Option<String>,
}

#[derive(Deserialize, Validate)]
pub struct ReactionRequest {
    #[validate(length(min = 1, max = 32), custom(function = "validate_emoji"))]
    pub emoji: String,
}

#[derive(Serialize)]
pub struct ServerWithChannels {
    #[serde(flatten)]
    pub server: Server,
    pub channels: Vec<Channel>,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct VoiceChannelParticipant {
    pub user_id: Uuid,
    pub username: String,
    pub joined_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct ServerBanEntry {
    pub user_id: Uuid,
    pub username: String,
    pub banned_by: Uuid,
    pub banned_by_username: String,
    pub reason: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize)]
pub struct MessageReactionSummary {
    pub emoji: String,
    pub user_ids: Vec<Uuid>,
    pub count: usize,
}

fn can_manage_members(role: &str) -> bool {
    role == "owner" || role == "admin"
}

fn can_manage_target(actor_role: &str, target_role: &str) -> bool {
    match actor_role {
        "owner" => target_role != "owner",
        "admin" => target_role == "member",
        _ => false,
    }
}

async fn fetch_server_role(
    state: &AppState,
    server_id: Uuid,
    user_id: Uuid,
) -> Result<Option<String>, StatusCode> {
    sqlx::query_scalar::<_, String>(
        "SELECT role FROM server_members WHERE server_id = $1 AND user_id = $2",
    )
    .bind(server_id)
    .bind(user_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

async fn fetch_message_reactions(
    state: &AppState,
    message_id: Uuid,
) -> Result<Vec<MessageReactionSummary>, StatusCode> {
    let rows = sqlx::query_as::<_, (String, Uuid)>(
        "SELECT emoji, user_id FROM message_reactions WHERE message_id = $1 ORDER BY created_at ASC"
    )
    .bind(message_id)
    .fetch_all(&state.db)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let mut grouped: BTreeMap<String, Vec<Uuid>> = BTreeMap::new();
    for (emoji, user_id) in rows {
        grouped.entry(emoji).or_default().push(user_id);
    }

    Ok(grouped
        .into_iter()
        .map(|(emoji, user_ids)| {
            let count = user_ids.len();
            MessageReactionSummary {
                emoji,
                user_ids,
                count,
            }
        })
        .collect())
}

async fn broadcast_voice_presence(
    state: &AppState,
    server_id: Uuid,
    channel_id: Uuid,
    user_id: Uuid,
    joined: bool,
) -> Result<(), StatusCode> {
    let members =
        sqlx::query_scalar::<_, Uuid>("SELECT user_id FROM server_members WHERE server_id = $1")
            .bind(server_id)
            .fetch_all(&state.db)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let ws_payload = serde_json::json!({
        "type": "VOICE_PRESENCE",
        "server_id": server_id,
        "channel_id": channel_id,
        "user_id": user_id,
        "joined": joined,
    });
    let ws_text = serde_json::to_string(&ws_payload).unwrap();

    for member_id in members {
        if let Some(peer_tx) = state.peers.get(&member_id.to_string()) {
            let _ = peer_tx.send(WsMessage::Text(ws_text.clone()));
        }
    }

    Ok(())
}

// === Handlers ===

/// List all servers the user is a member of
async fn list_servers(
    State(state): State<AppState>,
    user: AuthUser,
) -> Result<Json<Vec<Server>>, StatusCode> {
    let servers = sqlx::query_as::<_, Server>(
        r#"
        SELECT s.* FROM servers s
        INNER JOIN server_members sm ON s.id = sm.server_id
        WHERE sm.user_id = $1
        ORDER BY s.name
        "#,
    )
    .bind(user.id)
    .fetch_all(&state.db)
    .await
    .map_err(|e| {
        tracing::error!("Failed to list servers: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    Ok(Json(servers))
}

/// Create a new server
async fn create_server(
    State(state): State<AppState>,
    user: AuthUser,
    Json(req): Json<CreateServerRequest>,
) -> Result<Json<Server>, StatusCode> {
    req.validate().map_err(|_| StatusCode::BAD_REQUEST)?;

    if let Some(icon_url) = req.icon_url.as_deref() {
        validate_avatar_url(icon_url).map_err(|_| StatusCode::BAD_REQUEST)?;
    }

    let normalized_name = req.name.trim();
    let invite_code = generate_invite_code();

    // Create server
    let server = sqlx::query_as::<_, Server>(
        r#"
        INSERT INTO servers (name, icon_url, owner_id, invite_code)
        VALUES ($1, $2, $3, $4)
        RETURNING *
        "#,
    )
    .bind(normalized_name)
    .bind(&req.icon_url)
    .bind(user.id)
    .bind(&invite_code)
    .fetch_one(&state.db)
    .await
    .map_err(|e| {
        tracing::error!("Failed to create server: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    // Add owner as member
    sqlx::query("INSERT INTO server_members (server_id, user_id, role) VALUES ($1, $2, 'owner')")
        .bind(server.id)
        .bind(user.id)
        .execute(&state.db)
        .await
        .map_err(|e| {
            tracing::error!("Failed to add owner as member: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    // Create default #general channel
    sqlx::query(
        "INSERT INTO channels (server_id, name, channel_type, position) VALUES ($1, 'general', 'text', 0)"
    )
    .bind(server.id)
    .execute(&state.db)
    .await
    .map_err(|e| {
        tracing::error!("Failed to create default channel: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    tracing::info!("Server '{}' created by user {}", server.name, user.id);
    Ok(Json(server))
}

/// Get server details with channels
async fn get_server(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<ServerWithChannels>, StatusCode> {
    // Verify membership
    let is_member = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM server_members WHERE server_id = $1 AND user_id = $2",
    )
    .bind(id)
    .bind(user.id)
    .fetch_one(&state.db)
    .await
    .unwrap_or(0)
        > 0;

    if !is_member {
        return Err(StatusCode::FORBIDDEN);
    }

    let server = sqlx::query_as::<_, Server>("SELECT * FROM servers WHERE id = $1")
        .bind(id)
        .fetch_optional(&state.db)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    let channels = sqlx::query_as::<_, Channel>(
        "SELECT * FROM channels WHERE server_id = $1 ORDER BY position, name",
    )
    .bind(id)
    .fetch_all(&state.db)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(ServerWithChannels { server, channels }))
}

/// Get server members
async fn get_members(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<Vec<ServerMemberWithUser>>, StatusCode> {
    // Verify membership
    let is_member = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM server_members WHERE server_id = $1 AND user_id = $2",
    )
    .bind(id)
    .bind(user.id)
    .fetch_one(&state.db)
    .await
    .unwrap_or(0)
        > 0;

    if !is_member {
        return Err(StatusCode::FORBIDDEN);
    }

    let members = sqlx::query_as::<_, ServerMemberWithUser>(
        r#"
        SELECT u.id as user_id, u.username, u.avatar_url, sm.role, u.last_seen
        FROM server_members sm
        INNER JOIN users u ON sm.user_id = u.id
        WHERE sm.server_id = $1
        ORDER BY 
            CASE sm.role 
                WHEN 'owner' THEN 0 
                WHEN 'admin' THEN 1 
                ELSE 2 
            END,
            u.username
        "#,
    )
    .bind(id)
    .fetch_all(&state.db)
    .await
    .map_err(|e| {
        tracing::error!("Failed to get members: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    Ok(Json(members))
}

/// Join server by invite code
async fn join_server(
    State(state): State<AppState>,
    user: AuthUser,
    Path(code): Path<String>,
) -> Result<Json<Server>, StatusCode> {
    let server = sqlx::query_as::<_, Server>("SELECT * FROM servers WHERE invite_code = $1")
        .bind(&code)
        .fetch_optional(&state.db)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    // Check if already a member
    let is_member = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM server_members WHERE server_id = $1 AND user_id = $2",
    )
    .bind(server.id)
    .bind(user.id)
    .fetch_one(&state.db)
    .await
    .unwrap_or(0)
        > 0;

    if is_member {
        return Ok(Json(server)); // Already a member, just return
    }

    let is_banned = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM server_bans WHERE server_id = $1 AND user_id = $2",
    )
    .bind(server.id)
    .bind(user.id)
    .fetch_one(&state.db)
    .await
    .unwrap_or(0)
        > 0;

    if is_banned {
        return Err(StatusCode::FORBIDDEN);
    }

    // Add as member
    sqlx::query("INSERT INTO server_members (server_id, user_id, role) VALUES ($1, $2, 'member')")
        .bind(server.id)
        .bind(user.id)
        .execute(&state.db)
        .await
        .map_err(|e| {
            tracing::error!("Failed to join server: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    tracing::info!("User {} joined server '{}'", user.id, server.name);
    Ok(Json(server))
}

/// Leave server
async fn leave_server(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, StatusCode> {
    // Check if user is owner
    let is_owner = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM servers WHERE id = $1 AND owner_id = $2",
    )
    .bind(id)
    .bind(user.id)
    .fetch_one(&state.db)
    .await
    .unwrap_or(0)
        > 0;

    if is_owner {
        // Owner can't leave, must delete or transfer
        return Err(StatusCode::FORBIDDEN);
    }

    // If user is currently in a voice channel for this server, remove and broadcast leave.
    let active_voice_channels = sqlx::query_scalar::<_, Uuid>(
        "SELECT channel_id FROM voice_channel_sessions WHERE server_id = $1 AND user_id = $2",
    )
    .bind(id)
    .bind(user.id)
    .fetch_all(&state.db)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    if !active_voice_channels.is_empty() {
        sqlx::query("DELETE FROM voice_channel_sessions WHERE server_id = $1 AND user_id = $2")
            .bind(id)
            .bind(user.id)
            .execute(&state.db)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

        for channel_id in active_voice_channels {
            let _ = broadcast_voice_presence(&state, id, channel_id, user.id, false).await;
        }
    }

    sqlx::query("DELETE FROM server_members WHERE server_id = $1 AND user_id = $2")
        .bind(id)
        .bind(user.id)
        .execute(&state.db)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(StatusCode::NO_CONTENT)
}

/// Create channel in server
async fn create_channel(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<Uuid>,
    Json(req): Json<CreateChannelRequest>,
) -> Result<Json<Channel>, StatusCode> {
    req.validate().map_err(|_| StatusCode::BAD_REQUEST)?;

    // Check if user is owner or admin
    let role = sqlx::query_scalar::<_, String>(
        "SELECT role FROM server_members WHERE server_id = $1 AND user_id = $2",
    )
    .bind(id)
    .bind(user.id)
    .fetch_optional(&state.db)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
    .ok_or(StatusCode::FORBIDDEN)?;

    if role != "owner" && role != "admin" {
        return Err(StatusCode::FORBIDDEN);
    }

    let channel_type = req.channel_type.unwrap_or_else(|| "text".to_string());
    if channel_type != "text" && channel_type != "voice" {
        return Err(StatusCode::BAD_REQUEST);
    }

    // Get max position
    let max_pos = sqlx::query_scalar::<_, Option<i32>>(
        "SELECT MAX(position) FROM channels WHERE server_id = $1",
    )
    .bind(id)
    .fetch_one(&state.db)
    .await
    .unwrap_or(Some(0))
    .unwrap_or(0);

    let channel = sqlx::query_as::<_, Channel>(
        r#"
        INSERT INTO channels (server_id, name, channel_type, position)
        VALUES ($1, $2, $3, $4)
        RETURNING *
        "#,
    )
    .bind(id)
    .bind(req.name.trim())
    .bind(&channel_type)
    .bind(max_pos + 1)
    .fetch_one(&state.db)
    .await
    .map_err(|e| {
        tracing::error!("Failed to create channel: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    Ok(Json(channel))
}

/// List users currently connected to a voice channel.
async fn get_voice_channel_presence(
    State(state): State<AppState>,
    user: AuthUser,
    Path((server_id, channel_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<Vec<VoiceChannelParticipant>>, StatusCode> {
    let is_member = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM server_members WHERE server_id = $1 AND user_id = $2",
    )
    .bind(server_id)
    .bind(user.id)
    .fetch_one(&state.db)
    .await
    .unwrap_or(0)
        > 0;

    if !is_member {
        return Err(StatusCode::FORBIDDEN);
    }

    let is_voice = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM channels WHERE id = $1 AND server_id = $2 AND channel_type = 'voice'",
    )
    .bind(channel_id)
    .bind(server_id)
    .fetch_one(&state.db)
    .await
    .unwrap_or(0)
        > 0;

    if !is_voice {
        return Err(StatusCode::NOT_FOUND);
    }

    let participants = sqlx::query_as::<_, VoiceChannelParticipant>(
        r#"
        SELECT vcs.user_id, u.username, vcs.joined_at
        FROM voice_channel_sessions vcs
        INNER JOIN users u ON u.id = vcs.user_id
        WHERE vcs.server_id = $1 AND vcs.channel_id = $2
        ORDER BY vcs.joined_at ASC
        "#,
    )
    .bind(server_id)
    .bind(channel_id)
    .fetch_all(&state.db)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(participants))
}

/// Join (or move to) a voice channel.
async fn join_voice_channel(
    State(state): State<AppState>,
    user: AuthUser,
    Path((server_id, channel_id)): Path<(Uuid, Uuid)>,
) -> Result<StatusCode, StatusCode> {
    let is_member = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM server_members WHERE server_id = $1 AND user_id = $2",
    )
    .bind(server_id)
    .bind(user.id)
    .fetch_one(&state.db)
    .await
    .unwrap_or(0)
        > 0;

    if !is_member {
        return Err(StatusCode::FORBIDDEN);
    }

    let is_voice = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM channels WHERE id = $1 AND server_id = $2 AND channel_type = 'voice'",
    )
    .bind(channel_id)
    .bind(server_id)
    .fetch_one(&state.db)
    .await
    .unwrap_or(0)
        > 0;

    if !is_voice {
        return Err(StatusCode::BAD_REQUEST);
    }

    let previous = sqlx::query_as::<_, (Uuid, Uuid)>(
        "SELECT server_id, channel_id FROM voice_channel_sessions WHERE user_id = $1",
    )
    .bind(user.id)
    .fetch_optional(&state.db)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    sqlx::query(
        r#"
        INSERT INTO voice_channel_sessions (channel_id, server_id, user_id)
        VALUES ($1, $2, $3)
        ON CONFLICT (user_id)
        DO UPDATE SET
            channel_id = EXCLUDED.channel_id,
            server_id = EXCLUDED.server_id,
            joined_at = NOW()
        "#,
    )
    .bind(channel_id)
    .bind(server_id)
    .bind(user.id)
    .execute(&state.db)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    if let Some((prev_server_id, prev_channel_id)) = previous {
        if prev_server_id != server_id || prev_channel_id != channel_id {
            let _ =
                broadcast_voice_presence(&state, prev_server_id, prev_channel_id, user.id, false)
                    .await;
        }
    }

    let _ = broadcast_voice_presence(&state, server_id, channel_id, user.id, true).await;
    Ok(StatusCode::NO_CONTENT)
}

/// Leave a voice channel.
async fn leave_voice_channel(
    State(state): State<AppState>,
    user: AuthUser,
    Path((server_id, channel_id)): Path<(Uuid, Uuid)>,
) -> Result<StatusCode, StatusCode> {
    let result = sqlx::query(
        "DELETE FROM voice_channel_sessions WHERE server_id = $1 AND channel_id = $2 AND user_id = $3"
    )
    .bind(server_id)
    .bind(channel_id)
    .bind(user.id)
    .execute(&state.db)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    if result.rows_affected() > 0 {
        let _ = broadcast_voice_presence(&state, server_id, channel_id, user.id, false).await;
    }

    Ok(StatusCode::NO_CONTENT)
}

/// Get channel messages
async fn get_channel_messages(
    State(state): State<AppState>,
    user: AuthUser,
    Path((server_id, channel_id)): Path<(Uuid, Uuid)>,
    Query(params): Query<ChannelPaginationParams>,
) -> Result<Json<Vec<ChannelMessage>>, StatusCode> {
    // Verify membership
    let is_member = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM server_members WHERE server_id = $1 AND user_id = $2",
    )
    .bind(server_id)
    .bind(user.id)
    .fetch_one(&state.db)
    .await
    .unwrap_or(0)
        > 0;

    if !is_member {
        return Err(StatusCode::FORBIDDEN);
    }

    let limit = params.limit.unwrap_or(100).clamp(1, 200);

    let mut messages = if let Some(before_id) = params.before {
        sqlx::query_as::<_, ChannelMessage>(
            r#"
            SELECT
                m.id,
                m.client_id,
                m.channel_id,
                m.sender_id,
                u.username as sender_username,
                m.content,
                m.nonce,
                m.created_at,
                m.edited_at
            FROM messages m
            LEFT JOIN users u ON u.id = m.sender_id
            WHERE m.channel_id = $1
              AND m.parent_message_id IS NULL
              AND m.created_at < (SELECT created_at FROM messages WHERE id = $3)
            ORDER BY m.created_at DESC
            LIMIT $2
            "#,
        )
        .bind(channel_id)
        .bind(limit)
        .bind(before_id)
        .fetch_all(&state.db)
        .await
    } else {
        sqlx::query_as::<_, ChannelMessage>(
            r#"
            SELECT
                m.id,
                m.client_id,
                m.channel_id,
                m.sender_id,
                u.username as sender_username,
                m.content,
                m.nonce,
                m.created_at,
                m.edited_at
            FROM messages m
            LEFT JOIN users u ON u.id = m.sender_id
            WHERE m.channel_id = $1
              AND m.parent_message_id IS NULL
            ORDER BY m.created_at DESC
            LIMIT $2
            "#,
        )
        .bind(channel_id)
        .bind(limit)
        .fetch_all(&state.db)
        .await
    }
    .map_err(|e| {
        tracing::error!("Failed to get channel messages: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    messages.reverse();

    Ok(Json(messages))
}

/// Send message to channel
async fn send_channel_message(
    State(state): State<AppState>,
    user: AuthUser,
    Path((server_id, channel_id)): Path<(Uuid, Uuid)>,
    Json(req): Json<SendMessageRequest>,
) -> Result<Json<ChannelMessage>, StatusCode> {
    req.validate().map_err(|_| StatusCode::BAD_REQUEST)?;
    let content = req.content.trim().to_string();

    // Verify membership
    let is_member = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM server_members WHERE server_id = $1 AND user_id = $2",
    )
    .bind(server_id)
    .bind(user.id)
    .fetch_one(&state.db)
    .await
    .unwrap_or(0)
        > 0;

    if !is_member {
        return Err(StatusCode::FORBIDDEN);
    }

    if let Some(parent_message_id) = req.parent_message_id {
        let parent_in_channel = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM messages WHERE id = $1 AND channel_id = $2",
        )
        .bind(parent_message_id)
        .bind(channel_id)
        .fetch_one(&state.db)
        .await
        .unwrap_or(0)
            > 0;

        if !parent_in_channel {
            return Err(StatusCode::BAD_REQUEST);
        }
    }

    if let Some(client_id) = req.client_id {
        if let Some(existing) = sqlx::query_as::<_, ChannelMessage>(
            r#"
            SELECT
                m.id,
                m.client_id,
                m.channel_id,
                m.sender_id,
                u.username as sender_username,
                m.content,
                m.nonce,
                m.created_at,
                m.edited_at
            FROM messages m
            LEFT JOIN users u ON u.id = m.sender_id
            WHERE m.channel_id = $1 AND m.sender_id = $2 AND m.client_id = $3
            LIMIT 1
            "#,
        )
        .bind(channel_id)
        .bind(user.id)
        .bind(client_id)
        .fetch_optional(&state.db)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        {
            return Ok(Json(existing));
        }
    }

    let message = sqlx::query_as::<_, ChannelMessage>(
        r#"
        WITH inserted AS (
            INSERT INTO messages (channel_id, sender_id, content, nonce, client_id, parent_message_id)
            VALUES ($1, $2, $3, $4, $5, $6)
            RETURNING id, client_id, channel_id, sender_id, content, nonce, created_at, edited_at
        )
        SELECT
            i.id,
            i.client_id,
            i.channel_id,
            i.sender_id,
            u.username as sender_username,
            i.content,
            i.nonce,
            i.created_at,
            i.edited_at
        FROM inserted i
        LEFT JOIN users u ON u.id = i.sender_id
        "#
    )
    .bind(channel_id)
    .bind(user.id)
    .bind(&content)
    .bind(&req.nonce)
    .bind(req.client_id)
    .bind(req.parent_message_id)
    .fetch_one(&state.db)
    .await
    .map_err(|e| {
        tracing::error!("Failed to send channel message: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    // Broadcast via WebSocket to all server members
    let members =
        sqlx::query_scalar::<_, Uuid>("SELECT user_id FROM server_members WHERE server_id = $1")
            .bind(server_id)
            .fetch_all(&state.db)
            .await
            .unwrap_or_default();

    let ws_payload = serde_json::json!({
        "type": "NEW_CHANNEL_MESSAGE",
        "server_id": server_id,
        "channel_id": channel_id,
        "message": message
    });
    let ws_text = serde_json::to_string(&ws_payload).unwrap();

    for member_id in members {
        if let Some(peer_tx) = state.peers.get(&member_id.to_string()) {
            let _ = peer_tx.send(WsMessage::Text(ws_text.clone()));
        }
    }

    let mention_usernames = extract_mentions(&content);
    for mention_username in mention_usernames {
        let target_user = sqlx::query_as::<_, (Uuid, String)>(
            r#"
            SELECT u.id, u.username
            FROM users u
            INNER JOIN server_members sm ON sm.user_id = u.id
            WHERE sm.server_id = $1
              AND LOWER(u.username) = LOWER($2)
            LIMIT 1
            "#,
        )
        .bind(server_id)
        .bind(&mention_username)
        .fetch_optional(&state.db)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

        if let Some((mentioned_user_id, mentioned_username)) = target_user {
            if mentioned_user_id == user.id {
                continue;
            }
            if let Some(peer_tx) = state.peers.get(&mentioned_user_id.to_string()) {
                let mention_payload = serde_json::json!({
                    "type": "MENTION_ALERT",
                    "context": "channel",
                    "server_id": server_id,
                    "channel_id": channel_id,
                    "message_id": message.id,
                    "mentioned_user_id": mentioned_user_id,
                    "mentioned_username": mentioned_username,
                    "sender_id": user.id,
                    "sender_username": user.username,
                });
                let mention_text = serde_json::to_string(&mention_payload).unwrap();
                let _ = peer_tx.send(WsMessage::Text(mention_text));
            }
        }
    }

    Ok(Json(message))
}

/// Broadcast typing indicator in a server channel
async fn send_channel_typing(
    State(state): State<AppState>,
    user: AuthUser,
    Path((server_id, channel_id)): Path<(Uuid, Uuid)>,
    Json(req): Json<TypingRequest>,
) -> Result<StatusCode, StatusCode> {
    let is_member = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM server_members WHERE server_id = $1 AND user_id = $2",
    )
    .bind(server_id)
    .bind(user.id)
    .fetch_one(&state.db)
    .await
    .unwrap_or(0)
        > 0;

    if !is_member {
        return Err(StatusCode::FORBIDDEN);
    }

    let members =
        sqlx::query_scalar::<_, Uuid>("SELECT user_id FROM server_members WHERE server_id = $1")
            .bind(server_id)
            .fetch_all(&state.db)
            .await
            .unwrap_or_default();

    let ws_payload = serde_json::json!({
        "type": "CHANNEL_TYPING",
        "server_id": server_id,
        "channel_id": channel_id,
        "user_id": user.id,
        "is_typing": req.is_typing,
    });
    let ws_text = serde_json::to_string(&ws_payload).unwrap();

    for member_id in members {
        if member_id == user.id {
            continue;
        }
        if let Some(peer_tx) = state.peers.get(&member_id.to_string()) {
            let _ = peer_tx.send(WsMessage::Text(ws_text.clone()));
        }
    }

    Ok(StatusCode::NO_CONTENT)
}

/// Edit a channel message (only original sender can edit)
async fn edit_channel_message(
    State(state): State<AppState>,
    user: AuthUser,
    Path((server_id, _channel_id, message_id)): Path<(Uuid, Uuid, Uuid)>,
    Json(req): Json<EditChannelMessageRequest>,
) -> Result<Json<ChannelMessage>, StatusCode> {
    req.validate().map_err(|_| StatusCode::BAD_REQUEST)?;

    // Verify membership
    let is_member = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM server_members WHERE server_id = $1 AND user_id = $2",
    )
    .bind(server_id)
    .bind(user.id)
    .fetch_one(&state.db)
    .await
    .unwrap_or(0)
        > 0;

    if !is_member {
        return Err(StatusCode::FORBIDDEN);
    }

    // Verify sender ownership
    let existing =
        sqlx::query_scalar::<_, Option<Uuid>>("SELECT sender_id FROM messages WHERE id = $1")
            .bind(message_id)
            .fetch_optional(&state.db)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
            .ok_or(StatusCode::NOT_FOUND)?;

    if existing != Some(user.id) {
        return Err(StatusCode::FORBIDDEN);
    }

    let updated = sqlx::query_as::<_, ChannelMessage>(
        r#"
        WITH updated AS (
            UPDATE messages
            SET content = $1, nonce = $2, edited_at = NOW()
            WHERE id = $3
            RETURNING id, client_id, channel_id, sender_id, content, nonce, created_at, edited_at
        )
        SELECT
            u2.id,
            u2.client_id,
            u2.channel_id,
            u2.sender_id,
            u.username as sender_username,
            u2.content,
            u2.nonce,
            u2.created_at,
            u2.edited_at
        FROM updated u2
        LEFT JOIN users u ON u.id = u2.sender_id
        "#,
    )
    .bind(req.content.trim())
    .bind(&req.nonce)
    .bind(message_id)
    .fetch_one(&state.db)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    // Broadcast to server members
    let members =
        sqlx::query_scalar::<_, Uuid>("SELECT user_id FROM server_members WHERE server_id = $1")
            .bind(server_id)
            .fetch_all(&state.db)
            .await
            .unwrap_or_default();

    let ws_payload = serde_json::json!({
        "type": "CHANNEL_MESSAGE_EDITED",
        "server_id": server_id,
        "message": updated
    });
    let ws_text = serde_json::to_string(&ws_payload).unwrap();

    for member_id in members {
        if let Some(peer_tx) = state.peers.get(&member_id.to_string()) {
            let _ = peer_tx.send(WsMessage::Text(ws_text.clone()));
        }
    }

    Ok(Json(updated))
}

/// Delete a server (owner only).
async fn delete_server(
    State(state): State<AppState>,
    user: AuthUser,
    Path(server_id): Path<Uuid>,
) -> Result<StatusCode, StatusCode> {
    let member_ids =
        sqlx::query_scalar::<_, Uuid>("SELECT user_id FROM server_members WHERE server_id = $1")
            .bind(server_id)
            .fetch_all(&state.db)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let result = sqlx::query("DELETE FROM servers WHERE id = $1 AND owner_id = $2")
        .bind(server_id)
        .bind(user.id)
        .execute(&state.db)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    if result.rows_affected() == 0 {
        return Err(StatusCode::FORBIDDEN);
    }

    let ws_payload = serde_json::json!({
        "type": "SERVER_DELETED",
        "server_id": server_id,
    });
    let ws_text = serde_json::to_string(&ws_payload).unwrap();
    for member_id in member_ids {
        if let Some(peer_tx) = state.peers.get(&member_id.to_string()) {
            let _ = peer_tx.send(WsMessage::Text(ws_text.clone()));
        }
    }

    Ok(StatusCode::NO_CONTENT)
}

/// Regenerate invite code (owner/admin).
async fn regenerate_invite_code(
    State(state): State<AppState>,
    user: AuthUser,
    Path(server_id): Path<Uuid>,
) -> Result<Json<Server>, StatusCode> {
    let role = fetch_server_role(&state, server_id, user.id)
        .await?
        .ok_or(StatusCode::FORBIDDEN)?;

    if !can_manage_members(&role) {
        return Err(StatusCode::FORBIDDEN);
    }

    for _ in 0..5 {
        let invite_code = generate_invite_code();
        let updated = sqlx::query_as::<_, Server>(
            "UPDATE servers SET invite_code = $1 WHERE id = $2 RETURNING *",
        )
        .bind(&invite_code)
        .bind(server_id)
        .fetch_one(&state.db)
        .await;

        match updated {
            Ok(server) => return Ok(Json(server)),
            Err(sqlx::Error::Database(db_err))
                if db_err.constraint() == Some("servers_invite_code_key") =>
            {
                continue;
            }
            Err(_) => return Err(StatusCode::INTERNAL_SERVER_ERROR),
        }
    }

    Err(StatusCode::INTERNAL_SERVER_ERROR)
}

/// Update a member role (owner only).
async fn update_member_role(
    State(state): State<AppState>,
    user: AuthUser,
    Path((server_id, member_id)): Path<(Uuid, Uuid)>,
    Json(req): Json<UpdateMemberRoleRequest>,
) -> Result<StatusCode, StatusCode> {
    let actor_role = fetch_server_role(&state, server_id, user.id)
        .await?
        .ok_or(StatusCode::FORBIDDEN)?;
    if actor_role != "owner" {
        return Err(StatusCode::FORBIDDEN);
    }

    let target_role = fetch_server_role(&state, server_id, member_id)
        .await?
        .ok_or(StatusCode::NOT_FOUND)?;
    if target_role == "owner" {
        return Err(StatusCode::FORBIDDEN);
    }

    let role = req.role.trim().to_ascii_lowercase();
    if role != "admin" && role != "member" {
        return Err(StatusCode::BAD_REQUEST);
    }

    sqlx::query("UPDATE server_members SET role = $1 WHERE server_id = $2 AND user_id = $3")
        .bind(role)
        .bind(server_id)
        .bind(member_id)
        .execute(&state.db)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(StatusCode::NO_CONTENT)
}

/// Kick member from server (owner/admin).
async fn kick_member(
    State(state): State<AppState>,
    user: AuthUser,
    Path((server_id, member_id)): Path<(Uuid, Uuid)>,
) -> Result<StatusCode, StatusCode> {
    if member_id == user.id {
        return Err(StatusCode::BAD_REQUEST);
    }

    let actor_role = fetch_server_role(&state, server_id, user.id)
        .await?
        .ok_or(StatusCode::FORBIDDEN)?;
    if !can_manage_members(&actor_role) {
        return Err(StatusCode::FORBIDDEN);
    }

    let target_role = fetch_server_role(&state, server_id, member_id)
        .await?
        .ok_or(StatusCode::NOT_FOUND)?;
    if !can_manage_target(&actor_role, &target_role) {
        return Err(StatusCode::FORBIDDEN);
    }

    sqlx::query("DELETE FROM voice_channel_sessions WHERE server_id = $1 AND user_id = $2")
        .bind(server_id)
        .bind(member_id)
        .execute(&state.db)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let result = sqlx::query("DELETE FROM server_members WHERE server_id = $1 AND user_id = $2")
        .bind(server_id)
        .bind(member_id)
        .execute(&state.db)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    if result.rows_affected() == 0 {
        return Err(StatusCode::NOT_FOUND);
    }

    if let Some(peer_tx) = state.peers.get(&member_id.to_string()) {
        let kicked_payload = serde_json::json!({
            "type": "SERVER_KICKED",
            "server_id": server_id,
        });
        let kicked_text = serde_json::to_string(&kicked_payload).unwrap();
        let _ = peer_tx.send(WsMessage::Text(kicked_text));
    }

    Ok(StatusCode::NO_CONTENT)
}

/// Ban member from server (owner/admin).
async fn ban_member(
    State(state): State<AppState>,
    user: AuthUser,
    Path((server_id, member_id)): Path<(Uuid, Uuid)>,
    Json(req): Json<BanMemberRequest>,
) -> Result<StatusCode, StatusCode> {
    if member_id == user.id {
        return Err(StatusCode::BAD_REQUEST);
    }

    let actor_role = fetch_server_role(&state, server_id, user.id)
        .await?
        .ok_or(StatusCode::FORBIDDEN)?;
    if !can_manage_members(&actor_role) {
        return Err(StatusCode::FORBIDDEN);
    }

    if let Some(target_role) = fetch_server_role(&state, server_id, member_id).await? {
        if !can_manage_target(&actor_role, &target_role) {
            return Err(StatusCode::FORBIDDEN);
        }
    }

    let reason = req
        .reason
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty());

    sqlx::query(
        r#"
        INSERT INTO server_bans (server_id, user_id, banned_by, reason)
        VALUES ($1, $2, $3, $4)
        ON CONFLICT (server_id, user_id)
        DO UPDATE SET banned_by = EXCLUDED.banned_by, reason = EXCLUDED.reason, created_at = NOW()
        "#,
    )
    .bind(server_id)
    .bind(member_id)
    .bind(user.id)
    .bind(reason)
    .execute(&state.db)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    sqlx::query("DELETE FROM voice_channel_sessions WHERE server_id = $1 AND user_id = $2")
        .bind(server_id)
        .bind(member_id)
        .execute(&state.db)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    sqlx::query("DELETE FROM server_members WHERE server_id = $1 AND user_id = $2")
        .bind(server_id)
        .bind(member_id)
        .execute(&state.db)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    if let Some(peer_tx) = state.peers.get(&member_id.to_string()) {
        let banned_payload = serde_json::json!({
            "type": "SERVER_BANNED",
            "server_id": server_id,
            "reason": reason,
        });
        let banned_text = serde_json::to_string(&banned_payload).unwrap();
        let _ = peer_tx.send(WsMessage::Text(banned_text));
    }

    Ok(StatusCode::NO_CONTENT)
}

/// List banned members (owner/admin).
async fn list_server_bans(
    State(state): State<AppState>,
    user: AuthUser,
    Path(server_id): Path<Uuid>,
) -> Result<Json<Vec<ServerBanEntry>>, StatusCode> {
    let actor_role = fetch_server_role(&state, server_id, user.id)
        .await?
        .ok_or(StatusCode::FORBIDDEN)?;
    if !can_manage_members(&actor_role) {
        return Err(StatusCode::FORBIDDEN);
    }

    let bans = sqlx::query_as::<_, ServerBanEntry>(
        r#"
        SELECT
            sb.user_id,
            u.username,
            sb.banned_by,
            bu.username AS banned_by_username,
            sb.reason,
            sb.created_at
        FROM server_bans sb
        INNER JOIN users u ON u.id = sb.user_id
        INNER JOIN users bu ON bu.id = sb.banned_by
        WHERE sb.server_id = $1
        ORDER BY sb.created_at DESC
        "#,
    )
    .bind(server_id)
    .fetch_all(&state.db)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(bans))
}

/// Remove a server ban (owner/admin).
async fn unban_member(
    State(state): State<AppState>,
    user: AuthUser,
    Path((server_id, member_id)): Path<(Uuid, Uuid)>,
) -> Result<StatusCode, StatusCode> {
    let actor_role = fetch_server_role(&state, server_id, user.id)
        .await?
        .ok_or(StatusCode::FORBIDDEN)?;
    if !can_manage_members(&actor_role) {
        return Err(StatusCode::FORBIDDEN);
    }

    let result = sqlx::query("DELETE FROM server_bans WHERE server_id = $1 AND user_id = $2")
        .bind(server_id)
        .bind(member_id)
        .execute(&state.db)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    if result.rows_affected() == 0 {
        return Err(StatusCode::NOT_FOUND);
    }

    Ok(StatusCode::NO_CONTENT)
}

/// Update channel metadata (owner/admin).
async fn update_channel(
    State(state): State<AppState>,
    user: AuthUser,
    Path((server_id, channel_id)): Path<(Uuid, Uuid)>,
    Json(req): Json<UpdateChannelRequest>,
) -> Result<Json<Channel>, StatusCode> {
    let actor_role = fetch_server_role(&state, server_id, user.id)
        .await?
        .ok_or(StatusCode::FORBIDDEN)?;
    if !can_manage_members(&actor_role) {
        return Err(StatusCode::FORBIDDEN);
    }

    if req.name.is_none() && req.position.is_none() {
        return Err(StatusCode::BAD_REQUEST);
    }

    let (name_set, name_value) = if let Some(name) = req.name.as_deref() {
        validate_channel_name(name).map_err(|_| StatusCode::BAD_REQUEST)?;
        (true, Some(name.trim().to_string()))
    } else {
        (false, None)
    };

    let updated = sqlx::query_as::<_, Channel>(
        r#"
        UPDATE channels
        SET
            name = CASE WHEN $1 THEN $2 ELSE name END,
            position = COALESCE($3, position)
        WHERE id = $4 AND server_id = $5
        RETURNING *
        "#,
    )
    .bind(name_set)
    .bind(name_value)
    .bind(req.position)
    .bind(channel_id)
    .bind(server_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
    .ok_or(StatusCode::NOT_FOUND)?;

    Ok(Json(updated))
}

/// Delete channel (owner/admin). Keeps at least one text channel.
async fn delete_channel(
    State(state): State<AppState>,
    user: AuthUser,
    Path((server_id, channel_id)): Path<(Uuid, Uuid)>,
) -> Result<StatusCode, StatusCode> {
    let actor_role = fetch_server_role(&state, server_id, user.id)
        .await?
        .ok_or(StatusCode::FORBIDDEN)?;
    if !can_manage_members(&actor_role) {
        return Err(StatusCode::FORBIDDEN);
    }

    let channel_type = sqlx::query_scalar::<_, String>(
        "SELECT channel_type FROM channels WHERE id = $1 AND server_id = $2",
    )
    .bind(channel_id)
    .bind(server_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
    .ok_or(StatusCode::NOT_FOUND)?;

    if channel_type == "text" {
        let text_count = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM channels WHERE server_id = $1 AND channel_type = 'text'",
        )
        .bind(server_id)
        .fetch_one(&state.db)
        .await
        .unwrap_or(1);
        if text_count <= 1 {
            return Err(StatusCode::BAD_REQUEST);
        }
    }

    sqlx::query("DELETE FROM channels WHERE id = $1 AND server_id = $2")
        .bind(channel_id)
        .bind(server_id)
        .execute(&state.db)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(StatusCode::NO_CONTENT)
}

/// Search messages in a channel.
async fn search_channel_messages(
    State(state): State<AppState>,
    user: AuthUser,
    Path((server_id, channel_id)): Path<(Uuid, Uuid)>,
    Query(params): Query<SearchChannelMessagesQuery>,
) -> Result<Json<Vec<ChannelMessage>>, StatusCode> {
    params.validate().map_err(|_| StatusCode::BAD_REQUEST)?;

    let is_member = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM server_members WHERE server_id = $1 AND user_id = $2",
    )
    .bind(server_id)
    .bind(user.id)
    .fetch_one(&state.db)
    .await
    .unwrap_or(0)
        > 0;

    if !is_member {
        return Err(StatusCode::FORBIDDEN);
    }

    let limit = params.limit.unwrap_or(50).clamp(1, 100);
    let query = format!("%{}%", params.q.trim());

    let messages = sqlx::query_as::<_, ChannelMessage>(
        r#"
        SELECT
            m.id,
            m.client_id,
            m.channel_id,
            m.sender_id,
            u.username as sender_username,
            m.content,
            m.nonce,
            m.created_at,
            m.edited_at
        FROM messages m
        LEFT JOIN users u ON u.id = m.sender_id
        WHERE m.channel_id = $1
          AND m.content ILIKE $2
        ORDER BY m.created_at DESC
        LIMIT $3
        "#,
    )
    .bind(channel_id)
    .bind(query)
    .bind(limit)
    .fetch_all(&state.db)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(messages))
}

/// List reactions for a channel message.
async fn get_channel_message_reactions(
    State(state): State<AppState>,
    user: AuthUser,
    Path((server_id, channel_id, message_id)): Path<(Uuid, Uuid, Uuid)>,
) -> Result<Json<Vec<MessageReactionSummary>>, StatusCode> {
    let is_member = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM server_members WHERE server_id = $1 AND user_id = $2",
    )
    .bind(server_id)
    .bind(user.id)
    .fetch_one(&state.db)
    .await
    .unwrap_or(0)
        > 0;
    if !is_member {
        return Err(StatusCode::FORBIDDEN);
    }

    let exists = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM messages WHERE id = $1 AND channel_id = $2",
    )
    .bind(message_id)
    .bind(channel_id)
    .fetch_one(&state.db)
    .await
    .unwrap_or(0)
        > 0;
    if !exists {
        return Err(StatusCode::NOT_FOUND);
    }

    let reactions = fetch_message_reactions(&state, message_id).await?;
    Ok(Json(reactions))
}

/// Add reaction to a channel message.
async fn add_channel_message_reaction(
    State(state): State<AppState>,
    user: AuthUser,
    Path((server_id, channel_id, message_id)): Path<(Uuid, Uuid, Uuid)>,
    Json(req): Json<ReactionRequest>,
) -> Result<Json<Vec<MessageReactionSummary>>, StatusCode> {
    req.validate().map_err(|_| StatusCode::BAD_REQUEST)?;

    let is_member = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM server_members WHERE server_id = $1 AND user_id = $2",
    )
    .bind(server_id)
    .bind(user.id)
    .fetch_one(&state.db)
    .await
    .unwrap_or(0)
        > 0;
    if !is_member {
        return Err(StatusCode::FORBIDDEN);
    }

    sqlx::query(
        r#"
        INSERT INTO message_reactions (message_id, user_id, emoji)
        VALUES ($1, $2, $3)
        ON CONFLICT (message_id, user_id, emoji) DO NOTHING
        "#,
    )
    .bind(message_id)
    .bind(user.id)
    .bind(req.emoji.trim())
    .execute(&state.db)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let reactions = fetch_message_reactions(&state, message_id).await?;

    let members =
        sqlx::query_scalar::<_, Uuid>("SELECT user_id FROM server_members WHERE server_id = $1")
            .bind(server_id)
            .fetch_all(&state.db)
            .await
            .unwrap_or_default();
    let payload = serde_json::json!({
        "type": "CHANNEL_MESSAGE_REACTIONS",
        "server_id": server_id,
        "channel_id": channel_id,
        "message_id": message_id,
        "reactions": reactions,
    });
    let text = serde_json::to_string(&payload).unwrap();
    for member_id in members {
        if let Some(peer_tx) = state.peers.get(&member_id.to_string()) {
            let _ = peer_tx.send(WsMessage::Text(text.clone()));
        }
    }

    Ok(Json(fetch_message_reactions(&state, message_id).await?))
}

/// Remove reaction from a channel message.
async fn remove_channel_message_reaction(
    State(state): State<AppState>,
    user: AuthUser,
    Path((server_id, channel_id, message_id, emoji)): Path<(Uuid, Uuid, Uuid, String)>,
) -> Result<Json<Vec<MessageReactionSummary>>, StatusCode> {
    validate_emoji(&emoji).map_err(|_| StatusCode::BAD_REQUEST)?;

    let is_member = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM server_members WHERE server_id = $1 AND user_id = $2",
    )
    .bind(server_id)
    .bind(user.id)
    .fetch_one(&state.db)
    .await
    .unwrap_or(0)
        > 0;
    if !is_member {
        return Err(StatusCode::FORBIDDEN);
    }

    sqlx::query(
        "DELETE FROM message_reactions WHERE message_id = $1 AND user_id = $2 AND emoji = $3",
    )
    .bind(message_id)
    .bind(user.id)
    .bind(emoji.trim())
    .execute(&state.db)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let reactions = fetch_message_reactions(&state, message_id).await?;
    let members =
        sqlx::query_scalar::<_, Uuid>("SELECT user_id FROM server_members WHERE server_id = $1")
            .bind(server_id)
            .fetch_all(&state.db)
            .await
            .unwrap_or_default();
    let payload = serde_json::json!({
        "type": "CHANNEL_MESSAGE_REACTIONS",
        "server_id": server_id,
        "channel_id": channel_id,
        "message_id": message_id,
        "reactions": reactions,
    });
    let text = serde_json::to_string(&payload).unwrap();
    for member_id in members {
        if let Some(peer_tx) = state.peers.get(&member_id.to_string()) {
            let _ = peer_tx.send(WsMessage::Text(text.clone()));
        }
    }

    Ok(Json(fetch_message_reactions(&state, message_id).await?))
}

/// Send a reply in a channel thread.
async fn send_channel_thread_message(
    State(state): State<AppState>,
    user: AuthUser,
    Path((server_id, channel_id, message_id)): Path<(Uuid, Uuid, Uuid)>,
    Json(mut req): Json<SendMessageRequest>,
) -> Result<Json<ChannelMessage>, StatusCode> {
    req.parent_message_id = Some(message_id);
    send_channel_message(State(state), user, Path((server_id, channel_id)), Json(req)).await
}

/// Return thread replies for a channel message.
async fn get_channel_thread_messages(
    State(state): State<AppState>,
    user: AuthUser,
    Path((server_id, channel_id, message_id)): Path<(Uuid, Uuid, Uuid)>,
) -> Result<Json<Vec<ChannelMessage>>, StatusCode> {
    let is_member = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM server_members WHERE server_id = $1 AND user_id = $2",
    )
    .bind(server_id)
    .bind(user.id)
    .fetch_one(&state.db)
    .await
    .unwrap_or(0)
        > 0;
    if !is_member {
        return Err(StatusCode::FORBIDDEN);
    }

    let messages = sqlx::query_as::<_, ChannelMessage>(
        r#"
        SELECT
            m.id,
            m.client_id,
            m.channel_id,
            m.sender_id,
            u.username as sender_username,
            m.content,
            m.nonce,
            m.created_at,
            m.edited_at
        FROM messages m
        LEFT JOIN users u ON u.id = m.sender_id
        WHERE m.channel_id = $1 AND m.parent_message_id = $2
        ORDER BY m.created_at ASC
        "#,
    )
    .bind(channel_id)
    .bind(message_id)
    .fetch_all(&state.db)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(messages))
}
