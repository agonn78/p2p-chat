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
    pub client_id: Option<String>,
    pub room_id: String,
    pub sender_id: Option<String>,
    pub content: String,
    pub nonce: Option<String>,
    pub created_at: Option<String>,
    pub edited_at: Option<String>,
    pub status: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct CreateDmRequest {
    friend_id: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct SendMessageRequest {
    content: String,
    nonce: Option<String>,
    client_id: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct TypingRequest {
    is_typing: bool,
}

#[derive(Debug, Serialize, Deserialize)]
struct ReadRequest {
    upto_message_id: Option<String>,
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
    before: Option<String>,
    limit: Option<i64>,
) -> Result<Vec<Message>, String> {
    let token = state.get_token().await
        .ok_or("Not authenticated")?;
    
    let url = format!("{}/chat/{}/messages", state.base_url, room_id);

    let mut query_params: Vec<(String, String)> = Vec::new();
    if let Some(before_id) = before {
        query_params.push(("before".to_string(), before_id));
    }
    if let Some(limit) = limit {
        query_params.push(("limit".to_string(), limit.to_string()));
    }
    
    let res = state.client
        .get(&url)
        .header("Authorization", format!("Bearer {}", token))
        .query(&query_params)
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
    client_id: Option<String>,
) -> Result<Message, String> {
    let token = state.get_token().await
        .ok_or("Not authenticated")?;
    
    let url = format!("{}/chat/{}/messages", state.base_url, room_id);
    
    let res = state.client
        .post(&url)
        .header("Authorization", format!("Bearer {}", token))
        .json(&SendMessageRequest { content, nonce, client_id })
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
pub async fn api_send_typing(
    state: State<'_, ApiState>,
    room_id: String,
    is_typing: bool,
) -> Result<(), String> {
    let token = state.get_token().await
        .ok_or("Not authenticated")?;

    let url = format!("{}/chat/{}/typing", state.base_url, room_id);

    let res = state.client
        .post(&url)
        .header("Authorization", format!("Bearer {}", token))
        .json(&TypingRequest { is_typing })
        .send()
        .await
        .map_err(|e| format!("Network error: {}", e))?;

    if !res.status().is_success() {
        let text = res.text().await.unwrap_or_default();
        return Err(format!("Failed to send typing: {}", text));
    }

    Ok(())
}

#[tauri::command]
pub async fn api_mark_message_delivered(
    state: State<'_, ApiState>,
    room_id: String,
    message_id: String,
) -> Result<(), String> {
    let token = state.get_token().await
        .ok_or("Not authenticated")?;

    let url = format!("{}/chat/{}/messages/{}/delivered", state.base_url, room_id, message_id);

    let res = state.client
        .post(&url)
        .header("Authorization", format!("Bearer {}", token))
        .send()
        .await
        .map_err(|e| format!("Network error: {}", e))?;

    if !res.status().is_success() {
        let text = res.text().await.unwrap_or_default();
        return Err(format!("Failed to mark delivered: {}", text));
    }

    Ok(())
}

#[tauri::command]
pub async fn api_mark_room_read(
    state: State<'_, ApiState>,
    room_id: String,
    upto_message_id: Option<String>,
) -> Result<(), String> {
    let token = state.get_token().await
        .ok_or("Not authenticated")?;

    let url = format!("{}/chat/{}/read", state.base_url, room_id);

    let res = state.client
        .post(&url)
        .header("Authorization", format!("Bearer {}", token))
        .json(&ReadRequest { upto_message_id })
        .send()
        .await
        .map_err(|e| format!("Network error: {}", e))?;

    if !res.status().is_success() {
        let text = res.text().await.unwrap_or_default();
        return Err(format!("Failed to mark room read: {}", text));
    }

    Ok(())
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
