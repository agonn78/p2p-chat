use axum::{
    extract::{Path, Query, State},
    routing::{get, put},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

use crate::auth::{AuthError, AuthUser};
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/search", get(search_users))
        .route("/:id", get(get_user))
        .route("/:id/public-key", get(get_user_public_key))
        .route("/me/public-key", put(set_my_public_key))
}

#[derive(Debug, Deserialize)]
pub struct SearchQuery {
    pub q: String,
}

#[derive(Debug, Serialize, FromRow)]
pub struct UserSearchResult {
    pub id: Uuid,
    pub username: String,
    pub avatar_url: Option<String>,
}

#[derive(Debug, Serialize, FromRow)]
pub struct UserPublicResult {
    pub id: Uuid,
    pub username: String,
    pub avatar_url: Option<String>,
    pub last_seen: Option<chrono::DateTime<chrono::Utc>>,
    pub public_key: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct PublicKeyResponse {
    pub user_id: Uuid,
    pub public_key: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct SetPublicKeyRequest {
    pub public_key: String,
}

/// Search users by username
async fn search_users(
    State(state): State<AppState>,
    _user: AuthUser,
    Query(query): Query<SearchQuery>,
) -> Result<Json<Vec<UserSearchResult>>, AuthError> {
    let users = sqlx::query_as::<_, UserSearchResult>(
        r#"
        SELECT id, username, avatar_url
        FROM users
        WHERE username ILIKE $1
        LIMIT 20
        "#,
    )
    .bind(format!("%{}%", query.q))
    .fetch_all(&state.db)
    .await?;

    Ok(Json(users))
}

/// Get a specific user's public profile
async fn get_user(
    State(state): State<AppState>,
    _user: AuthUser,
    Path(user_id): Path<Uuid>,
) -> Result<Json<UserPublicResult>, AuthError> {
    let user = sqlx::query_as::<_, UserPublicResult>(
        r#"
        SELECT id, username, avatar_url, last_seen, public_key
        FROM users
        WHERE id = $1
        "#,
    )
    .bind(user_id)
    .fetch_optional(&state.db)
    .await?
    .ok_or(AuthError::InvalidCredentials)?;

    Ok(Json(user))
}

/// Get a user's public key for E2EE
async fn get_user_public_key(
    State(state): State<AppState>,
    _user: AuthUser,
    Path(user_id): Path<Uuid>,
) -> Result<Json<PublicKeyResponse>, AuthError> {
    let row: Option<(Option<String>,)> = sqlx::query_as(
        "SELECT public_key FROM users WHERE id = $1"
    )
    .bind(user_id)
    .fetch_optional(&state.db)
    .await?;

    match row {
        Some((public_key,)) => Ok(Json(PublicKeyResponse { user_id, public_key })),
        None => Err(AuthError::InvalidCredentials),
    }
}

/// Set current user's public key
async fn set_my_public_key(
    State(state): State<AppState>,
    user: AuthUser,
    Json(payload): Json<SetPublicKeyRequest>,
) -> Result<Json<PublicKeyResponse>, AuthError> {
    sqlx::query("UPDATE users SET public_key = $1 WHERE id = $2")
        .bind(&payload.public_key)
        .bind(user.id)
        .execute(&state.db)
        .await?;

    println!("🔐 User {} set their public key", user.id);

    Ok(Json(PublicKeyResponse {
        user_id: user.id,
        public_key: Some(payload.public_key),
    }))
}
