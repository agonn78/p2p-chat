use axum::{
    extract::{Path, State},
    routing::{get, post, delete},
    Json, Router,
    http::StatusCode,
};
use serde::Deserialize;
use uuid::Uuid;
use axum::extract::ws::Message as WsMessage;

use crate::auth::{AuthError, AuthUser};
use crate::models::{Message, Room};
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/dm", post(create_or_get_dm))
        .route("/:room_id/messages", get(get_messages).post(send_message).delete(delete_all_messages))
        .route("/:room_id/messages/:message_id", delete(delete_message))
}

#[derive(Deserialize)]
struct CreateDmRequest {
    friend_id: Uuid,
}

#[derive(Deserialize)]
struct SendMessageRequest {
    content: String,
    nonce: Option<String>,  // E2EE nonce
}

/// Create or get existing DM room with a friend
async fn create_or_get_dm(
    State(state): State<AppState>,
    user: AuthUser,
    Json(req): Json<CreateDmRequest>,
) -> Result<Json<Room>, AuthError> {
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
        "#
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
    let room = sqlx::query_as::<_, Room>(
        "INSERT INTO rooms (id, is_dm) VALUES ($1, true) RETURNING *"
    )
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

/// Get messages for a room
async fn get_messages(
    State(state): State<AppState>,
    user: AuthUser,
    Path(room_id): Path<Uuid>,
) -> Result<Json<Vec<Message>>, AuthError> {
    // Verify membership
    let is_member = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM room_members WHERE room_id = $1 AND user_id = $2"
    )
    .bind(room_id)
    .bind(user.id)
    .fetch_one(&state.db)
    .await? > 0;

    if !is_member {
        return Err(AuthError::InvalidToken);
    }

    let messages = sqlx::query_as::<_, Message>(
        "SELECT * FROM messages WHERE room_id = $1 ORDER BY created_at ASC LIMIT 100"
    )
    .bind(room_id)
    .fetch_all(&state.db)
    .await?;

    Ok(Json(messages))
}

/// Send a message to a room
async fn send_message(
    State(state): State<AppState>,
    user: AuthUser,
    Path(room_id): Path<Uuid>,
    Json(req): Json<SendMessageRequest>,
) -> Result<Json<Message>, AuthError> {
    // Verify membership
    let members = sqlx::query_scalar::<_, Uuid>(
        "SELECT user_id FROM room_members WHERE room_id = $1"
    )
    .bind(room_id)
    .fetch_all(&state.db)
    .await?;

    if !members.contains(&user.id) {
        return Err(AuthError::InvalidToken);
    }

    // Insert message with nonce for E2EE
    let message = sqlx::query_as::<_, Message>(
        r#"
        INSERT INTO messages (room_id, sender_id, content, nonce)
        VALUES ($1, $2, $3, $4)
        RETURNING *
        "#
    )
    .bind(room_id)
    .bind(user.id)
    .bind(&req.content)
    .bind(&req.nonce)
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

    Ok(Json(message))
}

/// Delete a single message (own messages only)
async fn delete_message(
    State(state): State<AppState>,
    user: AuthUser,
    Path((room_id, message_id)): Path<(Uuid, Uuid)>,
) -> Result<StatusCode, AuthError> {
    // Verify membership
    let is_member = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM room_members WHERE room_id = $1 AND user_id = $2"
    )
    .bind(room_id)
    .bind(user.id)
    .fetch_one(&state.db)
    .await? > 0;

    if !is_member {
        return Err(AuthError::InvalidToken);
    }

    // Delete only if user is the sender
    let result = sqlx::query(
        "DELETE FROM messages WHERE id = $1 AND room_id = $2 AND sender_id = $3"
    )
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
    let members = sqlx::query_scalar::<_, Uuid>(
        "SELECT user_id FROM room_members WHERE room_id = $1"
    )
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

/// Delete all messages in a room (for /deleteall command)
async fn delete_all_messages(
    State(state): State<AppState>,
    user: AuthUser,
    Path(room_id): Path<Uuid>,
) -> Result<StatusCode, AuthError> {
    // Verify membership
    let is_member = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM room_members WHERE room_id = $1 AND user_id = $2"
    )
    .bind(room_id)
    .bind(user.id)
    .fetch_one(&state.db)
    .await? > 0;

    if !is_member {
        return Err(AuthError::InvalidToken);
    }

    // Delete all messages in the room
    let result = sqlx::query("DELETE FROM messages WHERE room_id = $1")
        .bind(room_id)
        .execute(&state.db)
        .await?;

    println!("🗑️ Deleted {} messages from room {} (by user {})", 
             result.rows_affected(), room_id, user.id);

    // Broadcast deletion via WebSocket
    let members = sqlx::query_scalar::<_, Uuid>(
        "SELECT user_id FROM room_members WHERE room_id = $1"
    )
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
