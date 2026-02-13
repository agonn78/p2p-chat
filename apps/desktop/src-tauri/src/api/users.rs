use crate::api::ApiState;
use serde::{Deserialize, Serialize};
use tauri::State;

#[derive(Debug, Serialize, Deserialize)]
pub struct PublicKeyRequest {
    pub public_key: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PublicKeyResponse {
    pub user_id: String,
    pub public_key: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct UserProfile {
    pub id: String,
    pub username: String,
    pub avatar_url: Option<String>,
    pub last_seen: Option<String>,
    pub public_key: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct UserSettings {
    pub user_id: String,
    pub allow_dm_from_strangers: bool,
    pub enable_mention_notifications: bool,
    pub enable_sound_notifications: bool,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct UpdateProfileRequest {
    username: Option<String>,
    avatar_url: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct UpdateSettingsRequest {
    allow_dm_from_strangers: Option<bool>,
    enable_mention_notifications: Option<bool>,
    enable_sound_notifications: Option<bool>,
}

#[tauri::command]
pub async fn api_upload_public_key(
    state: State<'_, ApiState>,
    public_key: String,
) -> Result<(), String> {
    let token = state.get_token().await.ok_or("Not authenticated")?;

    let url = format!("{}/users/me/public-key", state.base_url);

    let res = state
        .client
        .put(&url)
        .header("Authorization", format!("Bearer {}", token))
        .json(&PublicKeyRequest { public_key })
        .send()
        .await
        .map_err(|e| format!("Network error: {}", e))?;

    if !res.status().is_success() {
        let text = res.text().await.unwrap_or_default();
        return Err(format!("Failed to upload public key: {}", text));
    }

    Ok(())
}

#[tauri::command]
pub async fn api_fetch_user_public_key(
    state: State<'_, ApiState>,
    user_id: String,
) -> Result<Option<String>, String> {
    let token = state.get_token().await.ok_or("Not authenticated")?;

    let url = format!("{}/users/{}/public-key", state.base_url, user_id);

    let res = state
        .client
        .get(&url)
        .header("Authorization", format!("Bearer {}", token))
        .send()
        .await
        .map_err(|e| format!("Network error: {}", e))?;

    if !res.status().is_success() {
        return Ok(None);
    }

    let data: PublicKeyResponse = res
        .json()
        .await
        .map_err(|e| format!("Failed to parse response: {}", e))?;

    Ok(data.public_key)
}

#[tauri::command]
pub async fn api_fetch_my_profile(state: State<'_, ApiState>) -> Result<UserProfile, String> {
    let token = state.get_token().await.ok_or("Not authenticated")?;

    let url = format!("{}/users/me", state.base_url);

    let res = state
        .client
        .get(&url)
        .header("Authorization", format!("Bearer {}", token))
        .send()
        .await
        .map_err(|e| format!("Network error: {}", e))?;

    if !res.status().is_success() {
        let text = res.text().await.unwrap_or_default();
        return Err(format!("Failed to fetch profile: {}", text));
    }

    res.json()
        .await
        .map_err(|e| format!("Failed to parse response: {}", e))
}

#[tauri::command]
pub async fn api_update_my_profile(
    state: State<'_, ApiState>,
    username: Option<String>,
    avatar_url: Option<String>,
) -> Result<UserProfile, String> {
    let token = state.get_token().await.ok_or("Not authenticated")?;

    let url = format!("{}/users/me", state.base_url);

    let res = state
        .client
        .put(&url)
        .header("Authorization", format!("Bearer {}", token))
        .json(&UpdateProfileRequest {
            username,
            avatar_url,
        })
        .send()
        .await
        .map_err(|e| format!("Network error: {}", e))?;

    if !res.status().is_success() {
        let text = res.text().await.unwrap_or_default();
        return Err(format!("Failed to update profile: {}", text));
    }

    res.json()
        .await
        .map_err(|e| format!("Failed to parse response: {}", e))
}

#[tauri::command]
pub async fn api_fetch_my_settings(state: State<'_, ApiState>) -> Result<UserSettings, String> {
    let token = state.get_token().await.ok_or("Not authenticated")?;

    let url = format!("{}/users/me/settings", state.base_url);

    let res = state
        .client
        .get(&url)
        .header("Authorization", format!("Bearer {}", token))
        .send()
        .await
        .map_err(|e| format!("Network error: {}", e))?;

    if !res.status().is_success() {
        let text = res.text().await.unwrap_or_default();
        return Err(format!("Failed to fetch settings: {}", text));
    }

    res.json()
        .await
        .map_err(|e| format!("Failed to parse response: {}", e))
}

#[tauri::command]
pub async fn api_update_my_settings(
    state: State<'_, ApiState>,
    allow_dm_from_strangers: Option<bool>,
    enable_mention_notifications: Option<bool>,
    enable_sound_notifications: Option<bool>,
) -> Result<UserSettings, String> {
    let token = state.get_token().await.ok_or("Not authenticated")?;

    let url = format!("{}/users/me/settings", state.base_url);

    let res = state
        .client
        .put(&url)
        .header("Authorization", format!("Bearer {}", token))
        .json(&UpdateSettingsRequest {
            allow_dm_from_strangers,
            enable_mention_notifications,
            enable_sound_notifications,
        })
        .send()
        .await
        .map_err(|e| format!("Network error: {}", e))?;

    if !res.status().is_success() {
        let text = res.text().await.unwrap_or_default();
        return Err(format!("Failed to update settings: {}", text));
    }

    res.json()
        .await
        .map_err(|e| format!("Failed to parse response: {}", e))
}
