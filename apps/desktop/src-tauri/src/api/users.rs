use serde::{Deserialize, Serialize};
use tauri::State;
use crate::api::ApiState;

#[derive(Debug, Serialize, Deserialize)]
pub struct PublicKeyRequest {
    pub public_key: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct UserPublicKey {
    pub id: String,
    pub username: String,
    pub public_key: Option<String>,
}

#[tauri::command]
pub async fn api_upload_public_key(
    state: State<'_, ApiState>,
    public_key: String,
) -> Result<(), String> {
    let token = state.get_token().await
        .ok_or("Not authenticated")?;
    
    let url = format!("{}/users/me/public-key", state.base_url);
    
    let res = state.client
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
    let token = state.get_token().await
        .ok_or("Not authenticated")?;
    
    let url = format!("{}/users/{}/public-key", state.base_url, user_id);
    
    let res = state.client
        .get(&url)
        .header("Authorization", format!("Bearer {}", token))
        .send()
        .await
        .map_err(|e| format!("Network error: {}", e))?;

    if !res.status().is_success() {
        return Ok(None);
    }

    let data: UserPublicKey = res.json().await
        .map_err(|e| format!("Failed to parse response: {}", e))?;

    Ok(data.public_key)
}
