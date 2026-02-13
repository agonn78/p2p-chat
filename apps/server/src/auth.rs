use argon2::{
    password_hash::{rand_core::OsRng, PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
    Argon2,
};
use axum::{
    async_trait,
    extract::FromRequestParts,
    http::{request::Parts, StatusCode},
    response::{IntoResponse, Response},
    Json, RequestPartsExt,
};
use axum_extra::{
    headers::{authorization::Bearer, Authorization},
    TypedHeader,
};
use chrono::{Duration, Utc};
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use std::sync::LazyLock;
use uuid::Uuid;
use validator::Validate;

use crate::models::User;
use crate::validation::{normalize_email, normalize_username, validate_username};

// JWT secret loaded from environment variable
static JWT_SECRET: LazyLock<Vec<u8>> = LazyLock::new(|| match std::env::var("JWT_SECRET") {
    Ok(secret) => {
        tracing::info!("JWT_SECRET loaded from environment");
        secret.into_bytes()
    }
    Err(_) => {
        tracing::warn!(
            "⚠️  JWT_SECRET not set! Using insecure default. Set JWT_SECRET env var in production!"
        );
        b"dev-only-insecure-default-key-change-me".to_vec()
    }
});

#[derive(Debug, Serialize, Deserialize)]
pub struct Claims {
    pub sub: String, // user id
    pub username: String,
    pub exp: i64,
    pub iat: i64,
}

#[derive(Debug, Deserialize, Validate)]
pub struct RegisterRequest {
    #[validate(length(min = 3, max = 32), custom(function = "validate_username"))]
    pub username: String,
    #[validate(email, length(max = 255))]
    pub email: String,
    #[validate(length(min = 8, max = 128))]
    pub password: String,
}

#[derive(Debug, Deserialize, Validate)]
pub struct LoginRequest {
    #[validate(email, length(max = 255))]
    pub email: String,
    #[validate(length(min = 8, max = 128))]
    pub password: String,
}

#[derive(Debug, Serialize)]
pub struct AuthResponse {
    pub token: String,
    pub user: crate::models::UserPublic,
}

#[derive(Debug, thiserror::Error)]
pub enum AuthError {
    #[error("Invalid credentials")]
    InvalidCredentials,
    #[error("User already exists")]
    UserExists,
    #[error("Invalid token")]
    InvalidToken,
    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),
    #[error("Password hash error")]
    PasswordHash,
    #[error("Validation error: {0}")]
    Validation(String),
}

impl IntoResponse for AuthError {
    fn into_response(self) -> Response {
        let (status, message) = match self {
            AuthError::InvalidCredentials => (StatusCode::UNAUTHORIZED, self.to_string()),
            AuthError::UserExists => (StatusCode::CONFLICT, self.to_string()),
            AuthError::InvalidToken => (StatusCode::UNAUTHORIZED, self.to_string()),
            AuthError::Database(_) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Database error".to_string(),
            ),
            AuthError::PasswordHash => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Server error".to_string(),
            ),
            AuthError::Validation(_) => (StatusCode::BAD_REQUEST, self.to_string()),
        };
        (status, Json(serde_json::json!({ "error": message }))).into_response()
    }
}

/// Hash password using Argon2
pub fn hash_password(password: &str) -> Result<String, AuthError> {
    let salt = SaltString::generate(&mut OsRng);
    let argon2 = Argon2::default();
    argon2
        .hash_password(password.as_bytes(), &salt)
        .map(|hash| hash.to_string())
        .map_err(|_| AuthError::PasswordHash)
}

/// Verify password against hash
pub fn verify_password(password: &str, hash: &str) -> Result<bool, AuthError> {
    let parsed_hash = PasswordHash::new(hash).map_err(|_| AuthError::PasswordHash)?;
    Ok(Argon2::default()
        .verify_password(password.as_bytes(), &parsed_hash)
        .is_ok())
}

/// Generate JWT token
pub fn generate_token(user: &User) -> Result<String, AuthError> {
    let now = Utc::now();
    let exp = now + Duration::hours(24);

    let claims = Claims {
        sub: user.id.to_string(),
        username: user.username.clone(),
        exp: exp.timestamp(),
        iat: now.timestamp(),
    };

    encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(&JWT_SECRET),
    )
    .map_err(|_| AuthError::InvalidToken)
}

/// Validate JWT token and return claims
pub fn validate_token(token: &str) -> Result<Claims, AuthError> {
    decode::<Claims>(
        token,
        &DecodingKey::from_secret(&JWT_SECRET),
        &Validation::default(),
    )
    .map(|data| data.claims)
    .map_err(|_| AuthError::InvalidToken)
}

/// Register a new user
pub async fn register(pool: &PgPool, req: RegisterRequest) -> Result<AuthResponse, AuthError> {
    req.validate()
        .map_err(|e| AuthError::Validation(e.to_string()))?;

    let username = normalize_username(&req.username);
    let email = normalize_email(&req.email);
    let password_hash = hash_password(&req.password)?;

    let user = sqlx::query_as::<_, User>(
        r#"
        INSERT INTO users (username, email, password_hash)
        VALUES ($1, $2, $3)
        RETURNING *
        "#,
    )
    .bind(&username)
    .bind(&email)
    .bind(&password_hash)
    .fetch_one(pool)
    .await
    .map_err(|e| {
        if let sqlx::Error::Database(ref db_err) = e {
            if db_err.constraint() == Some("users_username_key")
                || db_err.constraint() == Some("users_email_key")
            {
                return AuthError::UserExists;
            }
        }
        AuthError::Database(e)
    })?;

    let token = generate_token(&user)?;
    Ok(AuthResponse {
        token,
        user: user.into(),
    })
}

/// Login user
pub async fn login(pool: &PgPool, req: LoginRequest) -> Result<AuthResponse, AuthError> {
    req.validate()
        .map_err(|e| AuthError::Validation(e.to_string()))?;

    let email = normalize_email(&req.email);
    let user = sqlx::query_as::<_, User>(r#"SELECT * FROM users WHERE email = $1"#)
        .bind(&email)
        .fetch_optional(pool)
        .await?
        .ok_or(AuthError::InvalidCredentials)?;

    if !verify_password(&req.password, &user.password_hash)? {
        return Err(AuthError::InvalidCredentials);
    }

    // Update last_seen
    sqlx::query("UPDATE users SET last_seen = NOW() WHERE id = $1")
        .bind(user.id)
        .execute(pool)
        .await?;

    let token = generate_token(&user)?;
    Ok(AuthResponse {
        token,
        user: user.into(),
    })
}

/// Get current user from token
pub async fn get_me(pool: &PgPool, user_id: Uuid) -> Result<User, AuthError> {
    sqlx::query_as::<_, User>("SELECT * FROM users WHERE id = $1")
        .bind(user_id)
        .fetch_optional(pool)
        .await?
        .ok_or(AuthError::InvalidCredentials)
}

/// Authenticated user extractor for Axum
#[derive(Debug, Clone)]
pub struct AuthUser {
    pub id: Uuid,
    pub username: String,
}

#[async_trait]
impl<S> FromRequestParts<S> for AuthUser
where
    S: Send + Sync,
{
    type Rejection = AuthError;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        let TypedHeader(Authorization(bearer)) = parts
            .extract::<TypedHeader<Authorization<Bearer>>>()
            .await
            .map_err(|_| AuthError::InvalidToken)?;

        let claims = validate_token(bearer.token())?;
        let user_id = Uuid::parse_str(&claims.sub).map_err(|_| AuthError::InvalidToken)?;

        Ok(AuthUser {
            id: user_id,
            username: claims.username,
        })
    }
}
