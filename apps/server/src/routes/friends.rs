use axum::{
    extract::{Path, State},
    routing::{delete, get, post},
    Json, Router,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;
use validator::Validate;

use crate::auth::{AuthError, AuthUser};
use crate::state::AppState;
use crate::validation::validate_username;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(list_friends))
        .route("/request", post(send_request))
        .route("/accept/:id", post(accept_request))
        .route("/:id/accept", post(accept_request))
        .route("/reject/:id", post(reject_request))
        .route("/:id/reject", post(reject_request))
        .route("/:id", delete(remove_friend))
        .route("/pending", get(list_pending))
        .route("/online", get(list_online_friends))
}

#[derive(Debug, Serialize, FromRow)]
pub struct FriendInfo {
    pub id: Uuid,
    pub username: String,
    pub avatar_url: Option<String>,
    pub status: String,
    pub last_seen: Option<DateTime<Utc>>,
}

#[derive(Debug, Deserialize, Validate)]
pub struct FriendRequest {
    #[validate(length(min = 3, max = 32), custom(function = "validate_username"))]
    pub username: String,
}

/// Get all accepted friends
async fn list_friends(
    State(state): State<AppState>,
    user: AuthUser,
) -> Result<Json<Vec<FriendInfo>>, AuthError> {
    let friends = sqlx::query_as::<_, FriendInfo>(
        r#"
        SELECT u.id, u.username, u.avatar_url, f.status, u.last_seen
        FROM friendships f
        JOIN users u ON (
            (f.friend_id = u.id AND f.user_id = $1)
            OR (f.user_id = u.id AND f.friend_id = $1)
        )
        WHERE f.status = 'accepted'
        AND u.id != $1
        "#,
    )
    .bind(user.id)
    .fetch_all(&state.db)
    .await?;

    Ok(Json(friends))
}

/// Send a friend request
async fn send_request(
    State(state): State<AppState>,
    user: AuthUser,
    Json(req): Json<FriendRequest>,
) -> Result<Json<serde_json::Value>, AuthError> {
    req.validate()
        .map_err(|e| AuthError::Validation(e.to_string()))?;

    // Find target user
    let target = sqlx::query_scalar::<_, Uuid>("SELECT id FROM users WHERE username = $1")
        .bind(req.username.trim())
        .fetch_optional(&state.db)
        .await?
        .ok_or(AuthError::InvalidCredentials)?;

    if target == user.id {
        return Ok(Json(serde_json::json!({ "error": "Cannot add yourself" })));
    }

    // Check if friendship exists
    let existing = sqlx::query_scalar::<_, i64>(
        r#"
        SELECT COUNT(*) FROM friendships 
        WHERE (user_id = $1 AND friend_id = $2) OR (user_id = $2 AND friend_id = $1)
        "#,
    )
    .bind(user.id)
    .bind(target)
    .fetch_one(&state.db)
    .await?;

    if existing > 0 {
        return Ok(Json(
            serde_json::json!({ "error": "Friend request already exists" }),
        ));
    }

    // Create friendship request
    sqlx::query("INSERT INTO friendships (user_id, friend_id, status) VALUES ($1, $2, 'pending')")
        .bind(user.id)
        .bind(target)
        .execute(&state.db)
        .await?;

    Ok(Json(
        serde_json::json!({ "success": true, "message": "Friend request sent" }),
    ))
}

/// Accept a friend request
async fn accept_request(
    State(state): State<AppState>,
    user: AuthUser,
    Path(sender_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, AuthError> {
    tracing::info!(
        "Received accept request for sender_id: {} from user: {}",
        sender_id,
        user.id
    );

    let result = sqlx::query(
        r#"
        UPDATE friendships 
        SET status = 'accepted' 
        WHERE user_id = $1 AND friend_id = $2 AND status = 'pending'
        "#,
    )
    .bind(sender_id)
    .bind(user.id)
    .execute(&state.db)
    .await?;

    if result.rows_affected() == 0 {
        return Ok(Json(serde_json::json!({ "error": "Request not found" })));
    }

    Ok(Json(
        serde_json::json!({ "success": true, "message": "Friend request accepted" }),
    ))
}

/// Reject a friend request
async fn reject_request(
    State(state): State<AppState>,
    user: AuthUser,
    Path(sender_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, AuthError> {
    sqlx::query(
        "DELETE FROM friendships WHERE user_id = $1 AND friend_id = $2 AND status = 'pending'",
    )
    .bind(sender_id)
    .bind(user.id)
    .execute(&state.db)
    .await?;

    Ok(Json(serde_json::json!({ "success": true })))
}

/// Remove a friend
async fn remove_friend(
    State(state): State<AppState>,
    user: AuthUser,
    Path(friend_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, AuthError> {
    sqlx::query(
        r#"
        DELETE FROM friendships 
        WHERE (user_id = $1 AND friend_id = $2) OR (user_id = $2 AND friend_id = $1)
        "#,
    )
    .bind(user.id)
    .bind(friend_id)
    .execute(&state.db)
    .await?;

    Ok(Json(serde_json::json!({ "success": true })))
}

/// List pending friend requests (received)
async fn list_pending(
    State(state): State<AppState>,
    user: AuthUser,
) -> Result<Json<Vec<FriendInfo>>, AuthError> {
    let pending = sqlx::query_as::<_, FriendInfo>(
        r#"
        SELECT u.id, u.username, u.avatar_url, f.status, u.last_seen
        FROM friendships f
        JOIN users u ON f.user_id = u.id
        WHERE f.friend_id = $1 AND f.status = 'pending'
        "#,
    )
    .bind(user.id)
    .fetch_all(&state.db)
    .await?;

    Ok(Json(pending))
}

/// List friends who are currently online (connected via WebSocket)
async fn list_online_friends(
    State(state): State<AppState>,
    user: AuthUser,
) -> Result<Json<Vec<String>>, AuthError> {
    // Get all friend IDs
    let friends = sqlx::query_scalar::<_, Uuid>(
        r#"
        SELECT CASE 
            WHEN f.user_id = $1 THEN f.friend_id 
            ELSE f.user_id 
        END as friend_id
        FROM friendships f
        WHERE (f.user_id = $1 OR f.friend_id = $1) AND f.status = 'accepted'
        "#,
    )
    .bind(user.id)
    .fetch_all(&state.db)
    .await?;

    // Check which are online (in peers map)
    let online: Vec<String> = friends
        .into_iter()
        .filter(|id| state.peers.contains_key(&id.to_string()))
        .map(|id| id.to_string())
        .collect();

    Ok(Json(online))
}
