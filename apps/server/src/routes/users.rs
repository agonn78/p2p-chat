use axum::{
    extract::{Path, Query, State},
    routing::{get, put},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;
use validator::Validate;

use crate::auth::{AuthError, AuthUser};
use crate::state::AppState;
use crate::validation::{normalize_username, validate_avatar_url, validate_username};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/me", get(get_my_profile).put(update_my_profile))
        .route("/me/settings", get(get_my_settings).put(update_my_settings))
        .route("/search", get(search_users))
        .route("/:id", get(get_user))
        .route("/:id/public-key", get(get_user_public_key))
        .route("/me/public-key", put(set_my_public_key))
}

#[derive(Debug, Deserialize, Validate)]
pub struct SearchQuery {
    #[validate(length(min = 1, max = 64))]
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

#[derive(Debug, Deserialize)]
pub struct UpdateProfileRequest {
    pub username: Option<String>,
    pub avatar_url: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateSettingsRequest {
    pub allow_dm_from_strangers: Option<bool>,
    pub enable_mention_notifications: Option<bool>,
    pub enable_sound_notifications: Option<bool>,
}

#[derive(Debug, Serialize, FromRow)]
pub struct UserSettingsResponse {
    pub user_id: Uuid,
    pub allow_dm_from_strangers: bool,
    pub enable_mention_notifications: bool,
    pub enable_sound_notifications: bool,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

/// Search users by username
async fn search_users(
    State(state): State<AppState>,
    _user: AuthUser,
    Query(query): Query<SearchQuery>,
) -> Result<Json<Vec<UserSearchResult>>, AuthError> {
    query
        .validate()
        .map_err(|e| AuthError::Validation(e.to_string()))?;

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

async fn ensure_settings_row(state: &AppState, user_id: Uuid) -> Result<(), AuthError> {
    sqlx::query("INSERT INTO user_settings (user_id) VALUES ($1) ON CONFLICT (user_id) DO NOTHING")
        .bind(user_id)
        .execute(&state.db)
        .await?;
    Ok(())
}

/// Get current user's profile
async fn get_my_profile(
    State(state): State<AppState>,
    user: AuthUser,
) -> Result<Json<UserPublicResult>, AuthError> {
    let profile = sqlx::query_as::<_, UserPublicResult>(
        r#"
        SELECT id, username, avatar_url, last_seen, public_key
        FROM users
        WHERE id = $1
        "#,
    )
    .bind(user.id)
    .fetch_optional(&state.db)
    .await?
    .ok_or(AuthError::InvalidCredentials)?;

    Ok(Json(profile))
}

/// Update current user's profile (username/avatar)
async fn update_my_profile(
    State(state): State<AppState>,
    user: AuthUser,
    Json(payload): Json<UpdateProfileRequest>,
) -> Result<Json<UserPublicResult>, AuthError> {
    if payload.username.is_none() && payload.avatar_url.is_none() {
        return Err(AuthError::Validation(
            "No profile fields provided".to_string(),
        ));
    }

    let username_value = if let Some(username) = payload.username.as_deref() {
        validate_username(username).map_err(|e| AuthError::Validation(e.to_string()))?;
        Some(normalize_username(username))
    } else {
        None
    };

    let (avatar_set, avatar_value) = if let Some(raw_avatar) = payload.avatar_url.as_deref() {
        let trimmed = raw_avatar.trim();
        if trimmed.is_empty() {
            (true, None)
        } else {
            validate_avatar_url(trimmed).map_err(|e| AuthError::Validation(e.to_string()))?;
            (true, Some(trimmed.to_string()))
        }
    } else {
        (false, None)
    };

    let username_set = username_value.is_some();

    let updated = sqlx::query_as::<_, UserPublicResult>(
        r#"
        UPDATE users
        SET
            username = CASE WHEN $1 THEN $2 ELSE username END,
            avatar_url = CASE WHEN $3 THEN $4 ELSE avatar_url END
        WHERE id = $5
        RETURNING id, username, avatar_url, last_seen, public_key
        "#,
    )
    .bind(username_set)
    .bind(username_value)
    .bind(avatar_set)
    .bind(avatar_value)
    .bind(user.id)
    .fetch_one(&state.db)
    .await
    .map_err(|e| {
        if let sqlx::Error::Database(ref db_err) = e {
            if db_err.constraint() == Some("users_username_key") {
                return AuthError::UserExists;
            }
        }
        AuthError::Database(e)
    })?;

    Ok(Json(updated))
}

/// Get current user's notification/privacy settings
async fn get_my_settings(
    State(state): State<AppState>,
    user: AuthUser,
) -> Result<Json<UserSettingsResponse>, AuthError> {
    ensure_settings_row(&state, user.id).await?;

    let settings = sqlx::query_as::<_, UserSettingsResponse>(
        r#"
        SELECT
            user_id,
            allow_dm_from_strangers,
            enable_mention_notifications,
            enable_sound_notifications,
            created_at,
            updated_at
        FROM user_settings
        WHERE user_id = $1
        "#,
    )
    .bind(user.id)
    .fetch_one(&state.db)
    .await?;

    Ok(Json(settings))
}

/// Update current user's notification/privacy settings
async fn update_my_settings(
    State(state): State<AppState>,
    user: AuthUser,
    Json(payload): Json<UpdateSettingsRequest>,
) -> Result<Json<UserSettingsResponse>, AuthError> {
    ensure_settings_row(&state, user.id).await?;

    let settings = sqlx::query_as::<_, UserSettingsResponse>(
        r#"
        UPDATE user_settings
        SET
            allow_dm_from_strangers = COALESCE($1, allow_dm_from_strangers),
            enable_mention_notifications = COALESCE($2, enable_mention_notifications),
            enable_sound_notifications = COALESCE($3, enable_sound_notifications),
            updated_at = NOW()
        WHERE user_id = $4
        RETURNING
            user_id,
            allow_dm_from_strangers,
            enable_mention_notifications,
            enable_sound_notifications,
            created_at,
            updated_at
        "#,
    )
    .bind(payload.allow_dm_from_strangers)
    .bind(payload.enable_mention_notifications)
    .bind(payload.enable_sound_notifications)
    .bind(user.id)
    .fetch_one(&state.db)
    .await?;

    Ok(Json(settings))
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
    let row: Option<(Option<String>,)> =
        sqlx::query_as("SELECT public_key FROM users WHERE id = $1")
            .bind(user_id)
            .fetch_optional(&state.db)
            .await?;

    match row {
        Some((public_key,)) => Ok(Json(PublicKeyResponse {
            user_id,
            public_key,
        })),
        None => Err(AuthError::InvalidCredentials),
    }
}

/// Set current user's public key
async fn set_my_public_key(
    State(state): State<AppState>,
    user: AuthUser,
    Json(payload): Json<SetPublicKeyRequest>,
) -> Result<Json<PublicKeyResponse>, AuthError> {
    if payload.public_key.trim().is_empty() || payload.public_key.len() > 4096 {
        return Err(AuthError::Validation(
            "Invalid public key payload".to_string(),
        ));
    }

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
