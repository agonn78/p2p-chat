use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    routing::{get, post, put},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use axum::extract::ws::Message as WsMessage;

use crate::auth::AuthUser;
use crate::models::{Channel, ChannelMessage, Server, ServerMemberWithUser};
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(list_servers).post(create_server))
        .route("/:id", get(get_server))
        .route("/:id/members", get(get_members))
        .route("/:id/channels", post(create_channel))
        .route("/:id/channels/:channel_id/typing", post(send_channel_typing))
        .route("/:id/channels/:channel_id/messages", get(get_channel_messages).post(send_channel_message))
        .route("/:id/channels/:channel_id/messages/:message_id", put(edit_channel_message))
        .route("/join/:code", post(join_server))
        .route("/:id/leave", post(leave_server))
}

// Generate random invite code using UUID
fn generate_invite_code() -> String {
    Uuid::new_v4().to_string()[..8].to_uppercase()
}

// === Request/Response types ===

#[derive(Deserialize)]
pub struct CreateServerRequest {
    pub name: String,
    pub icon_url: Option<String>,
}

#[derive(Deserialize)]
pub struct CreateChannelRequest {
    pub name: String,
    pub channel_type: Option<String>,
}

#[derive(Deserialize)]
pub struct SendMessageRequest {
    pub content: String,
    pub nonce: Option<String>,
    pub client_id: Option<Uuid>,
}

#[derive(Deserialize)]
pub struct EditChannelMessageRequest {
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

#[derive(Serialize)]
pub struct ServerWithChannels {
    #[serde(flatten)]
    pub server: Server,
    pub channels: Vec<Channel>,
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
        "#
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
    let invite_code = generate_invite_code();

    // Create server
    let server = sqlx::query_as::<_, Server>(
        r#"
        INSERT INTO servers (name, icon_url, owner_id, invite_code)
        VALUES ($1, $2, $3, $4)
        RETURNING *
        "#
    )
    .bind(&req.name)
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
    sqlx::query(
        "INSERT INTO server_members (server_id, user_id, role) VALUES ($1, $2, 'owner')"
    )
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
        "SELECT COUNT(*) FROM server_members WHERE server_id = $1 AND user_id = $2"
    )
    .bind(id)
    .bind(user.id)
    .fetch_one(&state.db)
    .await
    .unwrap_or(0) > 0;

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
        "SELECT * FROM channels WHERE server_id = $1 ORDER BY position, name"
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
        "SELECT COUNT(*) FROM server_members WHERE server_id = $1 AND user_id = $2"
    )
    .bind(id)
    .bind(user.id)
    .fetch_one(&state.db)
    .await
    .unwrap_or(0) > 0;

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
        "#
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
        "SELECT COUNT(*) FROM server_members WHERE server_id = $1 AND user_id = $2"
    )
    .bind(server.id)
    .bind(user.id)
    .fetch_one(&state.db)
    .await
    .unwrap_or(0) > 0;

    if is_member {
        return Ok(Json(server)); // Already a member, just return
    }

    // Add as member
    sqlx::query(
        "INSERT INTO server_members (server_id, user_id, role) VALUES ($1, $2, 'member')"
    )
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
        "SELECT COUNT(*) FROM servers WHERE id = $1 AND owner_id = $2"
    )
    .bind(id)
    .bind(user.id)
    .fetch_one(&state.db)
    .await
    .unwrap_or(0) > 0;

    if is_owner {
        // Owner can't leave, must delete or transfer
        return Err(StatusCode::FORBIDDEN);
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
    // Check if user is owner or admin
    let role = sqlx::query_scalar::<_, String>(
        "SELECT role FROM server_members WHERE server_id = $1 AND user_id = $2"
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

    // Get max position
    let max_pos = sqlx::query_scalar::<_, Option<i32>>(
        "SELECT MAX(position) FROM channels WHERE server_id = $1"
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
        "#
    )
    .bind(id)
    .bind(&req.name)
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

/// Get channel messages
async fn get_channel_messages(
    State(state): State<AppState>,
    user: AuthUser,
    Path((server_id, channel_id)): Path<(Uuid, Uuid)>,
    Query(params): Query<ChannelPaginationParams>,
) -> Result<Json<Vec<ChannelMessage>>, StatusCode> {
    // Verify membership
    let is_member = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM server_members WHERE server_id = $1 AND user_id = $2"
    )
    .bind(server_id)
    .bind(user.id)
    .fetch_one(&state.db)
    .await
    .unwrap_or(0) > 0;

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
              AND m.created_at < (SELECT created_at FROM messages WHERE id = $3)
            ORDER BY m.created_at DESC
            LIMIT $2
            "#
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
            ORDER BY m.created_at DESC
            LIMIT $2
            "#
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
    // Verify membership
    let is_member = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM server_members WHERE server_id = $1 AND user_id = $2"
    )
    .bind(server_id)
    .bind(user.id)
    .fetch_one(&state.db)
    .await
    .unwrap_or(0) > 0;

    if !is_member {
        return Err(StatusCode::FORBIDDEN);
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
            "#
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
            INSERT INTO messages (channel_id, sender_id, content, nonce, client_id)
            VALUES ($1, $2, $3, $4, $5)
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
    .bind(&req.content)
    .bind(&req.nonce)
    .bind(req.client_id)
    .fetch_one(&state.db)
    .await
    .map_err(|e| {
        tracing::error!("Failed to send channel message: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    // Broadcast via WebSocket to all server members
    let members = sqlx::query_scalar::<_, Uuid>(
        "SELECT user_id FROM server_members WHERE server_id = $1"
    )
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
        "SELECT COUNT(*) FROM server_members WHERE server_id = $1 AND user_id = $2"
    )
    .bind(server_id)
    .bind(user.id)
    .fetch_one(&state.db)
    .await
    .unwrap_or(0) > 0;

    if !is_member {
        return Err(StatusCode::FORBIDDEN);
    }

    let members = sqlx::query_scalar::<_, Uuid>(
        "SELECT user_id FROM server_members WHERE server_id = $1"
    )
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
    // Verify membership
    let is_member = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM server_members WHERE server_id = $1 AND user_id = $2"
    )
    .bind(server_id)
    .bind(user.id)
    .fetch_one(&state.db)
    .await
    .unwrap_or(0) > 0;

    if !is_member {
        return Err(StatusCode::FORBIDDEN);
    }

    // Verify sender ownership
    let existing = sqlx::query_scalar::<_, Option<Uuid>>(
        "SELECT sender_id FROM messages WHERE id = $1"
    )
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
        "#
    )
    .bind(&req.content)
    .bind(&req.nonce)
    .bind(message_id)
    .fetch_one(&state.db)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    // Broadcast to server members
    let members = sqlx::query_scalar::<_, Uuid>(
        "SELECT user_id FROM server_members WHERE server_id = $1"
    )
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
