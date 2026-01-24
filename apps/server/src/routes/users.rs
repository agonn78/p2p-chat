use axum::{
    extract::{Path, Query, State},
    routing::get,
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
        SELECT id, username, avatar_url, last_seen
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
