use serde::{Deserialize, Serialize};
use tauri::State;
use crate::api::ApiState;

#[derive(Debug, Serialize, Deserialize)]
pub struct LoginRequest {
    pub email: String,
    pub password: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct RegisterRequest {
    pub username: String,
    pub email: String,
    pub password: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct User {
    pub id: String,
    pub username: String,
    pub email: String,
    pub avatar_url: Option<String>,
    pub public_key: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AuthResponse {
    pub token: String,
    pub user: User,
}

#[tauri::command]
pub async fn api_login(
    state: State<'_, ApiState>,
    email: String,
    password: String,
) -> Result<AuthResponse, String> {
    let url = format!("{}/auth/login", state.base_url);
    
    let res = state.client
        .post(&url)
        .json(&LoginRequest { email, password })
        .send()
        .await
        .map_err(|e| format!("Network error: {}", e))?;

    if !res.status().is_success() {
        let status = res.status();
        let text = res.text().await.unwrap_or_default();
        return Err(format!("Login failed ({}): {}", status, text));
    }

    let auth_response: AuthResponse = res.json().await
        .map_err(|e| format!("Failed to parse response: {}", e))?;

    // Store token for subsequent requests
    state.set_token(Some(auth_response.token.clone())).await;

    Ok(auth_response)
}

#[tauri::command]
pub async fn api_register(
    state: State<'_, ApiState>,
    username: String,
    email: String,
    password: String,
) -> Result<AuthResponse, String> {
    let url = format!("{}/auth/register", state.base_url);
    
    let res = state.client
        .post(&url)
        .json(&RegisterRequest { username, email, password })
        .send()
        .await
        .map_err(|e| format!("Network error: {}", e))?;

    if !res.status().is_success() {
        let status = res.status();
        let text = res.text().await.unwrap_or_default();
        return Err(format!("Registration failed ({}): {}", status, text));
    }

    let auth_response: AuthResponse = res.json().await
        .map_err(|e| format!("Failed to parse response: {}", e))?;

    // Store token for subsequent requests
    state.set_token(Some(auth_response.token.clone())).await;

    Ok(auth_response)
}

#[tauri::command]
pub async fn api_logout(state: State<'_, ApiState>) -> Result<(), String> {
    state.set_token(None).await;
    Ok(())
}
