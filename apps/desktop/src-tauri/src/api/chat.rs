use serde::{Deserialize, Serialize};
use tauri::State;
use crate::api::ApiState;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Room {
    pub id: String,
    pub name: Option<String>,
    pub is_dm: Option<bool>,
    pub created_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub id: String,
    pub room_id: String,
    pub sender_id: Option<String>,
    pub content: String,
    pub nonce: Option<String>,
    pub created_at: Option<String>,
    pub edited_at: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct CreateDmRequest {
    friend_id: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct SendMessageRequest {
    content: String,
    nonce: Option<String>,
}

#[tauri::command]
pub async fn api_create_or_get_dm(
    state: State<'_, ApiState>,
    friend_id: String,
) -> Result<Room, String> {
    let token = state.get_token().await
        .ok_or("Not authenticated")?;
    
    let url = format!("{}/chat/dm", state.base_url);
    
    let res = state.client
        .post(&url)
        .header("Authorization", format!("Bearer {}", token))
        .json(&CreateDmRequest { friend_id })
        .send()
        .await
        .map_err(|e| format!("Network error: {}", e))?;

    if !res.status().is_success() {
        let text = res.text().await.unwrap_or_default();
        return Err(format!("Failed to create DM: {}", text));
    }

    let room: Room = res.json().await
        .map_err(|e| format!("Failed to parse response: {}", e))?;

    Ok(room)
}

#[tauri::command]
pub async fn api_fetch_messages(
    state: State<'_, ApiState>,
    room_id: String,
) -> Result<Vec<Message>, String> {
    let token = state.get_token().await
        .ok_or("Not authenticated")?;
    
    let url = format!("{}/chat/{}/messages", state.base_url, room_id);
    
    let res = state.client
        .get(&url)
        .header("Authorization", format!("Bearer {}", token))
        .send()
        .await
        .map_err(|e| format!("Network error: {}", e))?;

    if !res.status().is_success() {
        let text = res.text().await.unwrap_or_default();
        return Err(format!("Failed to fetch messages: {}", text));
    }

    let messages: Vec<Message> = res.json().await
        .map_err(|e| format!("Failed to parse response: {}", e))?;

    Ok(messages)
}

#[tauri::command]
pub async fn api_send_message(
    state: State<'_, ApiState>,
    room_id: String,
    content: String,
    nonce: Option<String>,
) -> Result<Message, String> {
    let token = state.get_token().await
        .ok_or("Not authenticated")?;
    
    let url = format!("{}/chat/{}/messages", state.base_url, room_id);
    
    let res = state.client
        .post(&url)
        .header("Authorization", format!("Bearer {}", token))
        .json(&SendMessageRequest { content, nonce })
        .send()
        .await
        .map_err(|e| format!("Network error: {}", e))?;

    if !res.status().is_success() {
        let text = res.text().await.unwrap_or_default();
        return Err(format!("Failed to send message: {}", text));
    }

    let message: Message = res.json().await
        .map_err(|e| format!("Failed to parse response: {}", e))?;

    Ok(message)
}

#[tauri::command]
pub async fn api_delete_message(
    state: State<'_, ApiState>,
    room_id: String,
    message_id: String,
) -> Result<(), String> {
    let token = state.get_token().await
        .ok_or("Not authenticated")?;
    
    let url = format!("{}/chat/{}/messages/{}", state.base_url, room_id, message_id);
    
    let res = state.client
        .delete(&url)
        .header("Authorization", format!("Bearer {}", token))
        .send()
        .await
        .map_err(|e| format!("Network error: {}", e))?;

    if !res.status().is_success() {
        let text = res.text().await.unwrap_or_default();
        return Err(format!("Failed to delete message: {}", text));
    }

    Ok(())
}

#[tauri::command]
pub async fn api_delete_all_messages(
    state: State<'_, ApiState>,
    room_id: String,
) -> Result<(), String> {
    let token = state.get_token().await
        .ok_or("Not authenticated")?;
    
    let url = format!("{}/chat/{}/messages", state.base_url, room_id);
    
    let res = state.client
        .delete(&url)
        .header("Authorization", format!("Bearer {}", token))
        .send()
        .await
        .map_err(|e| format!("Network error: {}", e))?;

    if !res.status().is_success() {
        let text = res.text().await.unwrap_or_default();
        return Err(format!("Failed to delete all messages: {}", text));
    }

    Ok(())
}

#[derive(Debug, Serialize, Deserialize)]
struct EditMessageRequest {
    content: String,
    nonce: Option<String>,
}

#[tauri::command]
pub async fn api_edit_message(
    state: State<'_, ApiState>,
    room_id: String,
    message_id: String,
    content: String,
    nonce: Option<String>,
) -> Result<Message, String> {
    let token = state.get_token().await
        .ok_or("Not authenticated")?;
    
    let url = format!("{}/chat/{}/messages/{}", state.base_url, room_id, message_id);
    
    let res = state.client
        .put(&url)
        .header("Authorization", format!("Bearer {}", token))
        .json(&EditMessageRequest { content, nonce })
        .send()
        .await
        .map_err(|e| format!("Network error: {}", e))?;

    if !res.status().is_success() {
        let text = res.text().await.unwrap_or_default();
        return Err(format!("Failed to edit message: {}", text));
    }

    let message: Message = res.json().await
        .map_err(|e| format!("Failed to parse response: {}", e))?;

    Ok(message)
}
