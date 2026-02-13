use axum::extract::ws::Message as WsMessage;
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    routing::{delete, get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use uuid::Uuid;
use validator::Validate;

use crate::auth::{AuthError, AuthUser};
use crate::models::{Message, Room};
use crate::state::AppState;
use crate::validation::{extract_mentions, validate_emoji, validate_message_content};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/dm", post(create_or_get_dm))
        .route("/:room_id/typing", post(send_typing))
        .route("/:room_id/read", post(mark_room_read))
        .route("/:room_id/messages/search", get(search_messages))
        .route(
            "/:room_id/messages",
            get(get_messages)
                .post(send_message)
                .delete(delete_all_messages),
        )
        .route(
            "/:room_id/messages/:message_id/delivered",
            post(mark_message_delivered),
        )
        .route(
            "/:room_id/messages/:message_id/reactions",
            get(get_message_reactions).post(add_message_reaction),
        )
        .route(
            "/:room_id/messages/:message_id/reactions/:emoji",
            delete(remove_message_reaction),
        )
        .route(
            "/:room_id/messages/:message_id/thread",
            get(get_thread_messages).post(send_thread_message),
        )
        .route(
            "/:room_id/messages/:message_id",
            delete(delete_message).put(edit_message),
        )
}

#[derive(Deserialize)]
struct CreateDmRequest {
    friend_id: Uuid,
}

#[derive(Deserialize, Validate)]
struct SendMessageRequest {
    #[validate(
        length(min = 1, max = 4000),
        custom(function = "validate_message_content")
    )]
    content: String,
    nonce: Option<String>, // E2EE nonce
    client_id: Option<Uuid>,
    parent_message_id: Option<Uuid>,
}

#[derive(Deserialize)]
struct PaginationParams {
    before: Option<String>, // cursor: message ID to fetch before
    limit: Option<i64>,     // max messages to return (default 50, max 100)
}

#[derive(Deserialize, Validate)]
struct EditMessageRequest {
    #[validate(
        length(min = 1, max = 4000),
        custom(function = "validate_message_content")
    )]
    content: String,
    nonce: Option<String>,
}

#[derive(Deserialize)]
struct TypingRequest {
    is_typing: bool,
}

#[derive(Deserialize)]
struct ReadRequest {
    upto_message_id: Option<Uuid>,
}

#[derive(Deserialize, Validate)]
struct SearchMessagesQuery {
    #[validate(length(min = 1, max = 128))]
    q: String,
    limit: Option<i64>,
}

#[derive(Deserialize, Validate)]
struct ReactionRequest {
    #[validate(length(min = 1, max = 32), custom(function = "validate_emoji"))]
    emoji: String,
}

#[derive(Debug, Clone, Serialize)]
struct MessageReactionSummary {
    emoji: String,
    user_ids: Vec<Uuid>,
    count: usize,
}

async fn fetch_message_reactions(
    state: &AppState,
    message_id: Uuid,
) -> Result<Vec<MessageReactionSummary>, AuthError> {
    let rows = sqlx::query_as::<_, (String, Uuid)>(
        "SELECT emoji, user_id FROM message_reactions WHERE message_id = $1 ORDER BY created_at ASC"
    )
    .bind(message_id)
    .fetch_all(&state.db)
    .await?;

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

/// Create or get existing DM room with a friend
async fn create_or_get_dm(
    State(state): State<AppState>,
    user: AuthUser,
    Json(req): Json<CreateDmRequest>,
) -> Result<Json<Room>, AuthError> {
    let target_allows_strangers = sqlx::query_scalar::<_, bool>(
        "SELECT allow_dm_from_strangers FROM user_settings WHERE user_id = $1",
    )
    .bind(req.friend_id)
    .fetch_optional(&state.db)
    .await?
    .unwrap_or(true);

    if !target_allows_strangers {
        let are_friends = sqlx::query_scalar::<_, i64>(
            r#"
            SELECT COUNT(*)
            FROM friendships
            WHERE status = 'accepted'
              AND ((user_id = $1 AND friend_id = $2) OR (user_id = $2 AND friend_id = $1))
            "#,
        )
        .bind(user.id)
        .bind(req.friend_id)
        .fetch_one(&state.db)
        .await?
            > 0;

        if !are_friends {
            return Err(AuthError::Validation(
                "User does not accept DMs from non-friends".to_string(),
            ));
        }
    }

    // 1. Check if DM already exists
    let existing_room = sqlx::query_as::<_, Room>(
        r#"
        SELECT r.*
        FROM rooms r
        JOIN room_members rm1 ON r.id = rm1.room_id
        JOIN room_members rm2 ON r.id = rm2.room_id
        WHERE r.is_dm = true 
        AND rm1.user_id = $1 
        AND rm2.user_id = $2
        LIMIT 1
        "#,
    )
    .bind(user.id)
    .bind(req.friend_id)
    .fetch_optional(&state.db)
    .await?;

    if let Some(room) = existing_room {
        return Ok(Json(room));
    }

    // 2. Create new DM room transactionally
    let mut tx = state.db.begin().await?;

    let room_id = Uuid::new_v4();
    let room =
        sqlx::query_as::<_, Room>("INSERT INTO rooms (id, is_dm) VALUES ($1, true) RETURNING *")
            .bind(room_id)
            .fetch_one(&mut *tx)
            .await?;

    // Add members
    sqlx::query("INSERT INTO room_members (room_id, user_id) VALUES ($1, $2), ($1, $3)")
        .bind(room_id)
        .bind(user.id)
        .bind(req.friend_id)
        .execute(&mut *tx)
        .await?;

    tx.commit().await?;

    Ok(Json(room))
}

/// Get messages for a room (with pagination)
async fn get_messages(
    State(state): State<AppState>,
    user: AuthUser,
    Path(room_id): Path<Uuid>,
    Query(params): Query<PaginationParams>,
) -> Result<Json<Vec<Message>>, AuthError> {
    // Verify membership
    let is_member = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM room_members WHERE room_id = $1 AND user_id = $2",
    )
    .bind(room_id)
    .bind(user.id)
    .fetch_one(&state.db)
    .await?
        > 0;

    if !is_member {
        return Err(AuthError::InvalidToken);
    }

    let limit = params.limit.unwrap_or(100).clamp(1, 200);

    let messages = if let Some(before_id) = params.before {
        let before_uuid = Uuid::parse_str(&before_id).map_err(|_| AuthError::InvalidToken)?;
        sqlx::query_as::<_, Message>(
            "SELECT * FROM messages WHERE room_id = $1 AND parent_message_id IS NULL AND created_at < (SELECT created_at FROM messages WHERE id = $3) ORDER BY created_at DESC LIMIT $2"
        )
        .bind(room_id)
        .bind(limit)
        .bind(before_uuid)
        .fetch_all(&state.db)
        .await?
    } else {
        sqlx::query_as::<_, Message>(
            "SELECT * FROM messages WHERE room_id = $1 AND parent_message_id IS NULL ORDER BY created_at DESC LIMIT $2"
        )
        .bind(room_id)
        .bind(limit)
        .fetch_all(&state.db)
        .await?
    };

    // Reverse to return in chronological order
    let mut messages = messages;
    messages.reverse();

    Ok(Json(messages))
}

/// Send a message to a room
async fn send_message(
    State(state): State<AppState>,
    user: AuthUser,
    Path(room_id): Path<Uuid>,
    Json(req): Json<SendMessageRequest>,
) -> Result<Json<Message>, AuthError> {
    req.validate()
        .map_err(|e| AuthError::Validation(e.to_string()))?;

    let content = req.content.trim().to_string();

    // Verify membership
    let members =
        sqlx::query_scalar::<_, Uuid>("SELECT user_id FROM room_members WHERE room_id = $1")
            .bind(room_id)
            .fetch_all(&state.db)
            .await?;

    if !members.contains(&user.id) {
        return Err(AuthError::InvalidToken);
    }

    if let Some(parent_message_id) = req.parent_message_id {
        let parent_in_room = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM messages WHERE id = $1 AND room_id = $2",
        )
        .bind(parent_message_id)
        .bind(room_id)
        .fetch_one(&state.db)
        .await?
            > 0;

        if !parent_in_room {
            return Err(AuthError::Validation("Invalid parent message".to_string()));
        }
    }

    // Deduplicate retries (same sender + client_id)
    if let Some(client_id) = req.client_id {
        if let Some(existing) = sqlx::query_as::<_, Message>(
            "SELECT * FROM messages WHERE room_id = $1 AND sender_id = $2 AND client_id = $3 LIMIT 1"
        )
        .bind(room_id)
        .bind(user.id)
        .bind(client_id)
        .fetch_optional(&state.db)
        .await?
        {
            return Ok(Json(existing));
        }
    }

    // Insert message with nonce/client_id for E2EE + retry-safe dedup
    let message = sqlx::query_as::<_, Message>(
        r#"
        INSERT INTO messages (room_id, sender_id, content, nonce, client_id, parent_message_id)
        VALUES ($1, $2, $3, $4, $5, $6)
        RETURNING *
        "#,
    )
    .bind(room_id)
    .bind(user.id)
    .bind(&content)
    .bind(&req.nonce)
    .bind(req.client_id)
    .bind(req.parent_message_id)
    .fetch_one(&state.db)
    .await?;

    // Log if message is encrypted
    if req.nonce.is_some() {
        println!("🔐 Encrypted message sent (nonce present)");
    }

    // Broadcast via WebSocket
    let ws_payload = serde_json::json!({
        "type": "NEW_MESSAGE",
        "message": message
    });
    let ws_text = serde_json::to_string(&ws_payload).unwrap();

    println!("📢 Broadcasting message to {} room members", members.len());
    for member_id in members {
        if let Some(peer_tx) = state.peers.get(&member_id.to_string()) {
            match peer_tx.send(WsMessage::Text(ws_text.clone())) {
                Ok(_) => println!("    ✅ Sent to {}", member_id),
                Err(e) => eprintln!("    ❌ Failed to send to {}: {}", member_id, e),
            }
        }
    }

    let mention_usernames = extract_mentions(&content);
    for mention_username in mention_usernames {
        let target_user_id = sqlx::query_scalar::<_, Uuid>(
            r#"
            SELECT u.id
            FROM users u
            INNER JOIN room_members rm ON rm.user_id = u.id
            WHERE rm.room_id = $1
              AND LOWER(u.username) = LOWER($2)
            LIMIT 1
            "#,
        )
        .bind(room_id)
        .bind(&mention_username)
        .fetch_optional(&state.db)
        .await?;

        if let Some(mentioned_user_id) = target_user_id {
            if mentioned_user_id == user.id {
                continue;
            }
            if let Some(peer_tx) = state.peers.get(&mentioned_user_id.to_string()) {
                let mention_payload = serde_json::json!({
                    "type": "MENTION_ALERT",
                    "context": "dm",
                    "room_id": room_id,
                    "message_id": message.id,
                    "mentioned_user_id": mentioned_user_id,
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

/// Broadcast typing indicator for DM room
async fn send_typing(
    State(state): State<AppState>,
    user: AuthUser,
    Path(room_id): Path<Uuid>,
    Json(req): Json<TypingRequest>,
) -> Result<StatusCode, AuthError> {
    let member_ids =
        sqlx::query_scalar::<_, Uuid>("SELECT user_id FROM room_members WHERE room_id = $1")
            .bind(room_id)
            .fetch_all(&state.db)
            .await?;

    if !member_ids.contains(&user.id) {
        return Err(AuthError::InvalidToken);
    }

    let ws_payload = serde_json::json!({
        "type": "TYPING",
        "room_id": room_id,
        "user_id": user.id,
        "is_typing": req.is_typing,
    });
    let ws_text = serde_json::to_string(&ws_payload).unwrap();

    for member_id in member_ids {
        if member_id == user.id {
            continue;
        }
        if let Some(peer_tx) = state.peers.get(&member_id.to_string()) {
            let _ = peer_tx.send(WsMessage::Text(ws_text.clone()));
        }
    }

    Ok(StatusCode::NO_CONTENT)
}

/// Mark one message as delivered by current user (receiver)
async fn mark_message_delivered(
    State(state): State<AppState>,
    user: AuthUser,
    Path((room_id, message_id)): Path<(Uuid, Uuid)>,
) -> Result<StatusCode, AuthError> {
    let is_member = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM room_members WHERE room_id = $1 AND user_id = $2",
    )
    .bind(room_id)
    .bind(user.id)
    .fetch_one(&state.db)
    .await?
        > 0;

    if !is_member {
        return Err(AuthError::InvalidToken);
    }

    // upsert delivered_at (first delivery wins)
    sqlx::query(
        r#"
        INSERT INTO message_receipts (message_id, user_id, delivered_at)
        VALUES ($1, $2, NOW())
        ON CONFLICT (message_id, user_id)
        DO UPDATE SET delivered_at = COALESCE(message_receipts.delivered_at, EXCLUDED.delivered_at)
        "#,
    )
    .bind(message_id)
    .bind(user.id)
    .execute(&state.db)
    .await?;

    // Notify sender about delivery status
    let sender_id =
        sqlx::query_scalar::<_, Option<Uuid>>("SELECT sender_id FROM messages WHERE id = $1")
            .bind(message_id)
            .fetch_optional(&state.db)
            .await?
            .flatten();

    if let Some(sender_id) = sender_id {
        if sender_id != user.id {
            if let Some(peer_tx) = state.peers.get(&sender_id.to_string()) {
                let ws_payload = serde_json::json!({
                    "type": "MESSAGE_STATUS",
                    "room_id": room_id,
                    "message_id": message_id,
                    "status": "delivered",
                    "user_id": user.id,
                });
                let ws_text = serde_json::to_string(&ws_payload).unwrap();
                let _ = peer_tx.send(WsMessage::Text(ws_text));
            }
        }
    }

    Ok(StatusCode::NO_CONTENT)
}

/// Mark all unread messages in room as read by current user
async fn mark_room_read(
    State(state): State<AppState>,
    user: AuthUser,
    Path(room_id): Path<Uuid>,
    Json(req): Json<ReadRequest>,
) -> Result<StatusCode, AuthError> {
    let is_member = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM room_members WHERE room_id = $1 AND user_id = $2",
    )
    .bind(room_id)
    .bind(user.id)
    .fetch_one(&state.db)
    .await?
        > 0;

    if !is_member {
        return Err(AuthError::InvalidToken);
    }

    let rows: Vec<(Uuid, Option<Uuid>)> = if let Some(upto_message_id) = req.upto_message_id {
        sqlx::query_as::<_, (Uuid, Option<Uuid>)>(
            r#"
            SELECT m.id, m.sender_id
            FROM messages m
            LEFT JOIN message_receipts mr ON mr.message_id = m.id AND mr.user_id = $2
            WHERE m.room_id = $1
              AND m.sender_id IS NOT NULL
              AND m.sender_id <> $2
              AND (mr.read_at IS NULL)
              AND m.created_at <= (SELECT created_at FROM messages WHERE id = $3)
            "#,
        )
        .bind(room_id)
        .bind(user.id)
        .bind(upto_message_id)
        .fetch_all(&state.db)
        .await?
    } else {
        sqlx::query_as::<_, (Uuid, Option<Uuid>)>(
            r#"
            SELECT m.id, m.sender_id
            FROM messages m
            LEFT JOIN message_receipts mr ON mr.message_id = m.id AND mr.user_id = $2
            WHERE m.room_id = $1
              AND m.sender_id IS NOT NULL
              AND m.sender_id <> $2
              AND (mr.read_at IS NULL)
            "#,
        )
        .bind(room_id)
        .bind(user.id)
        .fetch_all(&state.db)
        .await?
    };

    for (message_id, sender_id) in rows {
        sqlx::query(
            r#"
            INSERT INTO message_receipts (message_id, user_id, delivered_at, read_at)
            VALUES ($1, $2, NOW(), NOW())
            ON CONFLICT (message_id, user_id)
            DO UPDATE SET
                delivered_at = COALESCE(message_receipts.delivered_at, NOW()),
                read_at = COALESCE(message_receipts.read_at, NOW())
            "#,
        )
        .bind(message_id)
        .bind(user.id)
        .execute(&state.db)
        .await?;

        if let Some(sender_id) = sender_id {
            if let Some(peer_tx) = state.peers.get(&sender_id.to_string()) {
                let ws_payload = serde_json::json!({
                    "type": "MESSAGE_STATUS",
                    "room_id": room_id,
                    "message_id": message_id,
                    "status": "read",
                    "user_id": user.id,
                });
                let ws_text = serde_json::to_string(&ws_payload).unwrap();
                let _ = peer_tx.send(WsMessage::Text(ws_text));
            }
        }
    }

    Ok(StatusCode::NO_CONTENT)
}

/// Search messages in a DM room.
async fn search_messages(
    State(state): State<AppState>,
    user: AuthUser,
    Path(room_id): Path<Uuid>,
    Query(params): Query<SearchMessagesQuery>,
) -> Result<Json<Vec<Message>>, AuthError> {
    params
        .validate()
        .map_err(|e| AuthError::Validation(e.to_string()))?;

    let is_member = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM room_members WHERE room_id = $1 AND user_id = $2",
    )
    .bind(room_id)
    .bind(user.id)
    .fetch_one(&state.db)
    .await?
        > 0;

    if !is_member {
        return Err(AuthError::InvalidToken);
    }

    let limit = params.limit.unwrap_or(50).clamp(1, 100);
    let query = format!("%{}%", params.q.trim());

    let messages = sqlx::query_as::<_, Message>(
        "SELECT * FROM messages WHERE room_id = $1 AND content ILIKE $2 ORDER BY created_at DESC LIMIT $3"
    )
    .bind(room_id)
    .bind(query)
    .bind(limit)
    .fetch_all(&state.db)
    .await?;

    Ok(Json(messages))
}

/// List reactions for a DM message.
async fn get_message_reactions(
    State(state): State<AppState>,
    user: AuthUser,
    Path((room_id, message_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<Vec<MessageReactionSummary>>, AuthError> {
    let is_member = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM room_members WHERE room_id = $1 AND user_id = $2",
    )
    .bind(room_id)
    .bind(user.id)
    .fetch_one(&state.db)
    .await?
        > 0;
    if !is_member {
        return Err(AuthError::InvalidToken);
    }

    let message_in_room = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM messages WHERE id = $1 AND room_id = $2",
    )
    .bind(message_id)
    .bind(room_id)
    .fetch_one(&state.db)
    .await?
        > 0;
    if !message_in_room {
        return Err(AuthError::InvalidCredentials);
    }

    Ok(Json(fetch_message_reactions(&state, message_id).await?))
}

/// Add reaction to a DM message.
async fn add_message_reaction(
    State(state): State<AppState>,
    user: AuthUser,
    Path((room_id, message_id)): Path<(Uuid, Uuid)>,
    Json(req): Json<ReactionRequest>,
) -> Result<Json<Vec<MessageReactionSummary>>, AuthError> {
    req.validate()
        .map_err(|e| AuthError::Validation(e.to_string()))?;

    let member_ids =
        sqlx::query_scalar::<_, Uuid>("SELECT user_id FROM room_members WHERE room_id = $1")
            .bind(room_id)
            .fetch_all(&state.db)
            .await?;
    if !member_ids.contains(&user.id) {
        return Err(AuthError::InvalidToken);
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
    .await?;

    let reactions = fetch_message_reactions(&state, message_id).await?;
    let payload = serde_json::json!({
        "type": "MESSAGE_REACTIONS",
        "room_id": room_id,
        "message_id": message_id,
        "reactions": reactions,
    });
    let text = serde_json::to_string(&payload).unwrap();
    for member_id in member_ids {
        if let Some(peer_tx) = state.peers.get(&member_id.to_string()) {
            let _ = peer_tx.send(WsMessage::Text(text.clone()));
        }
    }

    Ok(Json(fetch_message_reactions(&state, message_id).await?))
}

/// Remove reaction from a DM message.
async fn remove_message_reaction(
    State(state): State<AppState>,
    user: AuthUser,
    Path((room_id, message_id, emoji)): Path<(Uuid, Uuid, String)>,
) -> Result<Json<Vec<MessageReactionSummary>>, AuthError> {
    validate_emoji(&emoji).map_err(|e| AuthError::Validation(e.to_string()))?;

    let member_ids =
        sqlx::query_scalar::<_, Uuid>("SELECT user_id FROM room_members WHERE room_id = $1")
            .bind(room_id)
            .fetch_all(&state.db)
            .await?;
    if !member_ids.contains(&user.id) {
        return Err(AuthError::InvalidToken);
    }

    sqlx::query(
        "DELETE FROM message_reactions WHERE message_id = $1 AND user_id = $2 AND emoji = $3",
    )
    .bind(message_id)
    .bind(user.id)
    .bind(emoji.trim())
    .execute(&state.db)
    .await?;

    let reactions = fetch_message_reactions(&state, message_id).await?;
    let payload = serde_json::json!({
        "type": "MESSAGE_REACTIONS",
        "room_id": room_id,
        "message_id": message_id,
        "reactions": reactions,
    });
    let text = serde_json::to_string(&payload).unwrap();
    for member_id in member_ids {
        if let Some(peer_tx) = state.peers.get(&member_id.to_string()) {
            let _ = peer_tx.send(WsMessage::Text(text.clone()));
        }
    }

    Ok(Json(fetch_message_reactions(&state, message_id).await?))
}

/// Send a thread reply inside a DM room.
async fn send_thread_message(
    State(state): State<AppState>,
    user: AuthUser,
    Path((room_id, message_id)): Path<(Uuid, Uuid)>,
    Json(mut req): Json<SendMessageRequest>,
) -> Result<Json<Message>, AuthError> {
    req.parent_message_id = Some(message_id);
    send_message(State(state), user, Path(room_id), Json(req)).await
}

/// List thread replies in a DM room.
async fn get_thread_messages(
    State(state): State<AppState>,
    user: AuthUser,
    Path((room_id, message_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<Vec<Message>>, AuthError> {
    let is_member = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM room_members WHERE room_id = $1 AND user_id = $2",
    )
    .bind(room_id)
    .bind(user.id)
    .fetch_one(&state.db)
    .await?
        > 0;
    if !is_member {
        return Err(AuthError::InvalidToken);
    }

    let messages = sqlx::query_as::<_, Message>(
        "SELECT * FROM messages WHERE room_id = $1 AND parent_message_id = $2 ORDER BY created_at ASC"
    )
    .bind(room_id)
    .bind(message_id)
    .fetch_all(&state.db)
    .await?;

    Ok(Json(messages))
}

/// Delete a single message (own messages only)
async fn delete_message(
    State(state): State<AppState>,
    user: AuthUser,
    Path((room_id, message_id)): Path<(Uuid, Uuid)>,
) -> Result<StatusCode, AuthError> {
    // Verify membership
    let is_member = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM room_members WHERE room_id = $1 AND user_id = $2",
    )
    .bind(room_id)
    .bind(user.id)
    .fetch_one(&state.db)
    .await?
        > 0;

    if !is_member {
        return Err(AuthError::InvalidToken);
    }

    // Delete only if user is the sender
    let result =
        sqlx::query("DELETE FROM messages WHERE id = $1 AND room_id = $2 AND sender_id = $3")
            .bind(message_id)
            .bind(room_id)
            .bind(user.id)
            .execute(&state.db)
            .await?;

    if result.rows_affected() == 0 {
        println!("⚠️ Delete failed: message not found or not owned by user");
        return Err(AuthError::InvalidToken);
    }

    println!("🗑️ Message {} deleted by user {}", message_id, user.id);

    // Broadcast deletion via WebSocket
    let members =
        sqlx::query_scalar::<_, Uuid>("SELECT user_id FROM room_members WHERE room_id = $1")
            .bind(room_id)
            .fetch_all(&state.db)
            .await?;

    let ws_payload = serde_json::json!({
        "type": "MESSAGE_DELETED",
        "message_id": message_id,
        "room_id": room_id
    });
    let ws_text = serde_json::to_string(&ws_payload).unwrap();

    for member_id in members {
        if let Some(peer_tx) = state.peers.get(&member_id.to_string()) {
            let _ = peer_tx.send(WsMessage::Text(ws_text.clone()));
        }
    }

    Ok(StatusCode::NO_CONTENT)
}

/// Delete all messages in a room (admin/owner only, or for /deleteall command in DMs)
async fn delete_all_messages(
    State(state): State<AppState>,
    user: AuthUser,
    Path(room_id): Path<Uuid>,
) -> Result<StatusCode, AuthError> {
    // Verify membership
    let is_member = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM room_members WHERE room_id = $1 AND user_id = $2",
    )
    .bind(room_id)
    .bind(user.id)
    .fetch_one(&state.db)
    .await?
        > 0;

    if !is_member {
        return Err(AuthError::InvalidToken);
    }

    // Check if this is a DM room (any member can delete in DMs)
    let is_dm = sqlx::query_scalar::<_, bool>("SELECT is_dm FROM rooms WHERE id = $1")
        .bind(room_id)
        .fetch_one(&state.db)
        .await
        .unwrap_or(false);

    if !is_dm {
        // For non-DM rooms, only allow if we have no server role check
        // (this endpoint is mainly for DMs via /deleteall command)
        return Err(AuthError::InvalidToken);
    }

    // Delete all messages in the room
    let result = sqlx::query("DELETE FROM messages WHERE room_id = $1")
        .bind(room_id)
        .execute(&state.db)
        .await?;

    println!(
        "🗑️ Deleted {} messages from room {} (by user {})",
        result.rows_affected(),
        room_id,
        user.id
    );

    // Broadcast deletion via WebSocket
    let members =
        sqlx::query_scalar::<_, Uuid>("SELECT user_id FROM room_members WHERE room_id = $1")
            .bind(room_id)
            .fetch_all(&state.db)
            .await?;

    let ws_payload = serde_json::json!({
        "type": "ALL_MESSAGES_DELETED",
        "room_id": room_id
    });
    let ws_text = serde_json::to_string(&ws_payload).unwrap();

    for member_id in members {
        if let Some(peer_tx) = state.peers.get(&member_id.to_string()) {
            let _ = peer_tx.send(WsMessage::Text(ws_text.clone()));
        }
    }

    Ok(StatusCode::NO_CONTENT)
}

/// Edit a message (only the original sender can edit)
async fn edit_message(
    State(state): State<AppState>,
    user: AuthUser,
    Path((room_id, message_id)): Path<(Uuid, Uuid)>,
    Json(req): Json<EditMessageRequest>,
) -> Result<Json<Message>, (StatusCode, String)> {
    req.validate()
        .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?;

    // Verify the message exists and belongs to this user
    let existing =
        sqlx::query_as::<_, Message>("SELECT * FROM messages WHERE id = $1 AND room_id = $2")
            .bind(message_id)
            .bind(room_id)
            .fetch_optional(&state.db)
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
            .ok_or_else(|| (StatusCode::NOT_FOUND, "Message not found".to_string()))?;

    if existing.sender_id != Some(user.id) {
        return Err((
            StatusCode::FORBIDDEN,
            "You can only edit your own messages".to_string(),
        ));
    }

    // Update the message
    let updated = sqlx::query_as::<_, Message>(
        "UPDATE messages SET content = $1, nonce = $2, edited_at = NOW() WHERE id = $3 RETURNING *",
    )
    .bind(req.content.trim())
    .bind(&req.nonce)
    .bind(message_id)
    .fetch_one(&state.db)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // Broadcast edit to room members via WebSocket
    let member_ids: Vec<Uuid> =
        sqlx::query_scalar("SELECT user_id FROM room_members WHERE room_id = $1")
            .bind(room_id)
            .fetch_all(&state.db)
            .await
            .unwrap_or_default();

    let ws_payload = serde_json::json!({
        "type": "MESSAGE_EDITED",
        "message": updated
    });
    let ws_text = serde_json::to_string(&ws_payload).unwrap();

    for member_id in member_ids {
        if let Some(peer_tx) = state.peers.get(&member_id.to_string()) {
            let _ = peer_tx.send(WsMessage::Text(ws_text.clone()));
        }
    }

    Ok(Json(updated))
}
