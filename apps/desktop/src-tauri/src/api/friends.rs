use crate::api::ApiState;
use crate::error::AppResult;
use serde::{Deserialize, Serialize};
use tauri::State;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Friend {
    pub id: String,
    pub username: String,
    pub avatar_url: Option<String>,
    pub status: String,
    pub last_seen: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct FriendRequestPayload {
    username: String,
}

#[tauri::command]
pub async fn api_fetch_friends(state: State<'_, ApiState>) -> AppResult<Vec<Friend>> {
    let token = state.get_token().await.ok_or("Not authenticated")?;

    let url = format!("{}/friends", state.base_url);

    let res = state
        .client
        .get(&url)
        .header("Authorization", format!("Bearer {}", token))
        .send()
        .await
        .map_err(|e| format!("Network error: {}", e))?;

    if !res.status().is_success() {
        let text = res.text().await.unwrap_or_default();
        return Err(format!("Failed to fetch friends: {}", text).into());
    }

    let friends: Vec<Friend> = res
        .json()
        .await
        .map_err(|e| format!("Failed to parse response: {}", e))?;

    Ok(friends)
}

#[tauri::command]
pub async fn api_fetch_pending_requests(state: State<'_, ApiState>) -> AppResult<Vec<Friend>> {
    let token = state.get_token().await.ok_or("Not authenticated")?;

    let url = format!("{}/friends/pending", state.base_url);

    let res = state
        .client
        .get(&url)
        .header("Authorization", format!("Bearer {}", token))
        .send()
        .await
        .map_err(|e| format!("Network error: {}", e))?;

    if !res.status().is_success() {
        let text = res.text().await.unwrap_or_default();
        return Err(format!("Failed to fetch pending requests: {}", text).into());
    }

    let requests: Vec<Friend> = res
        .json()
        .await
        .map_err(|e| format!("Failed to parse response: {}", e))?;

    Ok(requests)
}

#[tauri::command]
pub async fn api_send_friend_request(
    state: State<'_, ApiState>,
    username: String,
) -> AppResult<()> {
    let token = state.get_token().await.ok_or("Not authenticated")?;

    let url = format!("{}/friends/request", state.base_url);

    let res = state
        .client
        .post(&url)
        .header("Authorization", format!("Bearer {}", token))
        .json(&FriendRequestPayload { username })
        .send()
        .await
        .map_err(|e| format!("Network error: {}", e))?;

    if !res.status().is_success() {
        let text = res.text().await.unwrap_or_default();
        return Err(format!("Failed to send friend request: {}", text).into());
    }

    Ok(())
}

#[tauri::command]
pub async fn api_accept_friend(
    state: State<'_, ApiState>,
    friend_id: String,
) -> AppResult<()> {
    let token = state.get_token().await.ok_or("Not authenticated")?;

    let url = format!("{}/friends/accept/{}", state.base_url, friend_id);

    let res = state
        .client
        .post(&url)
        .header("Authorization", format!("Bearer {}", token))
        .send()
        .await
        .map_err(|e| format!("Network error: {}", e))?;

    if !res.status().is_success() {
        let text = res.text().await.unwrap_or_default();
        return Err(format!("Failed to accept friend: {}", text).into());
    }

    Ok(())
}

#[tauri::command]
pub async fn api_fetch_online_friends(state: State<'_, ApiState>) -> AppResult<Vec<String>> {
    let token = state.get_token().await.ok_or("Not authenticated")?;

    let url = format!("{}/friends/online", state.base_url);

    let res = state
        .client
        .get(&url)
        .header("Authorization", format!("Bearer {}", token))
        .send()
        .await
        .map_err(|e| format!("Network error: {}", e))?;

    if !res.status().is_success() {
        let text = res.text().await.unwrap_or_default();
        return Err(format!("Failed to fetch online friends: {}", text).into());
    }

    let online: Vec<String> = res
        .json()
        .await
        .map_err(|e| format!("Failed to parse response: {}", e))?;

    Ok(online)
}
