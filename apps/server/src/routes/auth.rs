use axum::{
    extract::State,
    routing::{get, post},
    Json, Router,
};

use crate::auth::{self, AuthError, AuthResponse, AuthUser, LoginRequest, RegisterRequest};
use crate::models::UserPublic;
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/register", post(register))
        .route("/login", post(login))
        .route("/me", get(me))
}

async fn register(
    State(state): State<AppState>,
    Json(req): Json<RegisterRequest>,
) -> Result<Json<AuthResponse>, AuthError> {
    let response = auth::register(&state.db, req).await?;
    Ok(Json(response))
}

async fn login(
    State(state): State<AppState>,
    Json(req): Json<LoginRequest>,
) -> Result<Json<AuthResponse>, AuthError> {
    let response = auth::login(&state.db, req).await?;
    Ok(Json(response))
}

async fn me(State(state): State<AppState>, user: AuthUser) -> Result<Json<UserPublic>, AuthError> {
    let user = auth::get_me(&state.db, user.id).await?;
    Ok(Json(user.into()))
}
