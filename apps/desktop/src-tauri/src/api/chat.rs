use serde::{Deserialize, Serialize};
use tauri::State;
use crate::api::ApiState;
use crate::messaging::domain::{ConversationKind, MessageStatus as LocalMessageStatus, PersistedMessage};
use crate::MessagingState;
use chrono::Utc;
use uuid::Uuid;

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

fn parse_local_status(status: Option<&str>) -> LocalMessageStatus {
    match status {
        Some("sending") => LocalMessageStatus::Sending,
        Some("delivered") => LocalMessageStatus::Delivered,
        Some("read") => LocalMessageStatus::Read,
        Some("failed") => LocalMessageStatus::Failed,
        _ => LocalMessageStatus::Sent,
    }
}

fn persisted_to_api_message(message: PersistedMessage) -> Message {
    Message {
        id: message.server_id.clone().unwrap_or(message.local_id),
        client_id: message.client_id,
        room_id: message.target_id,
        sender_id: message.sender_id,
        content: message.content,
        nonce: message.nonce,
        created_at: Some(message.created_at),
        edited_at: message.edited_at,
        status: Some(message.status.as_str().to_string()),
    }
}

fn api_to_persisted_message(room_id: &str, message: &Message) -> PersistedMessage {
    let server_id = message.id.clone();
    PersistedMessage {
        local_id: server_id.clone(),
        server_id: Some(server_id),
        client_id: message.client_id.clone(),
        sender_id: message.sender_id.clone(),
        sender_username: None,
        target_kind: ConversationKind::Dm,
        target_id: room_id.to_string(),
        content: message.content.clone(),
        nonce: message.nonce.clone(),
        created_at: message.created_at.clone().unwrap_or_else(|| Utc::now().to_rfc3339()),
        edited_at: message.edited_at.clone(),
        status: parse_local_status(message.status.as_deref()),
    }
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
    messaging: State<'_, MessagingState>,
    room_id: String,
    before: Option<String>,
    limit: Option<i64>,
) -> Result<Vec<Message>, String> {
    let token = state.get_token().await
        .ok_or("Not authenticated")?;

    let limit = limit.unwrap_or(100).clamp(1, 200);
    
    let url = format!("{}/chat/{}/messages", state.base_url, room_id);

    let before_cursor = before.clone();
    let mut query_params: Vec<(String, String)> = Vec::new();
    if let Some(before_id) = before {
        query_params.push(("before".to_string(), before_id));
    }
    query_params.push(("limit".to_string(), limit.to_string()));
    
    let remote_res = state.client
        .get(&url)
        .header("Authorization", format!("Bearer {}", token))
        .query(&query_params)
        .send()
        .await
        .map_err(|e| format!("Network error: {}", e));

    match remote_res {
        Ok(res) if res.status().is_success() => {
            let remote_messages: Vec<Message> = res
                .json()
                .await
                .map_err(|e| format!("Failed to parse response: {}", e))?;

            let persisted = remote_messages
                .iter()
                .map(|m| api_to_persisted_message(&room_id, m))
                .collect::<Vec<_>>();
            if let Err(err) = messaging.service.cache_remote_messages(&persisted).await {
                eprintln!("[Messaging] Failed to cache DM messages: {}", err);
                return Ok(remote_messages);
            }

            match messaging
                .service
                .load_messages(ConversationKind::Dm, &room_id, before_cursor.as_deref(), limit)
                .await
            {
                Ok(local_messages) if !local_messages.is_empty() => {
                    Ok(local_messages.into_iter().map(persisted_to_api_message).collect())
                }
                Ok(_) => Ok(remote_messages),
                Err(err) => {
                    eprintln!("[Messaging] Failed to load cached DM messages: {}", err);
                    Ok(remote_messages)
                }
            }
        }
        Ok(res) => {
            let text = res.text().await.unwrap_or_default();
            let remote_error = format!("Failed to fetch messages: {}", text);

            let cached = messaging
                .service
                .load_messages(ConversationKind::Dm, &room_id, before_cursor.as_deref(), limit)
                .await
                .map_err(|e| format!("{} (cache unavailable: {})", remote_error, e))?;

            if !cached.is_empty() {
                return Ok(cached.into_iter().map(persisted_to_api_message).collect());
            }

            Err(remote_error)
        }
        Err(remote_error) => {
            let cached = messaging
                .service
                .load_messages(ConversationKind::Dm, &room_id, before_cursor.as_deref(), limit)
                .await
                .map_err(|e| format!("{} (cache unavailable: {})", remote_error, e))?;

            if !cached.is_empty() {
                return Ok(cached.into_iter().map(persisted_to_api_message).collect());
            }

            Err(remote_error)
        }
    }
}

#[tauri::command]
pub async fn api_send_message(
    state: State<'_, ApiState>,
    messaging: State<'_, MessagingState>,
    room_id: String,
    content: String,
    nonce: Option<String>,
    client_id: Option<String>,
) -> Result<Message, String> {
    let token = state.get_token().await
        .ok_or("Not authenticated")?;
    
    let url = format!("{}/chat/{}/messages", state.base_url, room_id);
    let resolved_client_id = client_id.unwrap_or_else(|| Uuid::new_v4().to_string());

    if let Err(err) = messaging
        .service
        .create_pending_message(
            ConversationKind::Dm,
            &room_id,
            None,
            None,
            content.clone(),
            nonce.clone(),
            resolved_client_id.clone(),
        )
        .await
    {
        eprintln!("[Messaging] Failed to persist pending DM message: {}", err);
    }
    
    let res = state.client
        .post(&url)
        .header("Authorization", format!("Bearer {}", token))
        .json(&SendMessageRequest {
            content: content.clone(),
            nonce: nonce.clone(),
            client_id: Some(resolved_client_id.clone()),
        })
        .send()
        .await
        .map_err(|e| format!("Network error: {}", e))?;

    if !res.status().is_success() {
        let text = res.text().await.unwrap_or_default();
        if let Err(err) = messaging
            .service
            .mark_send_failed(&resolved_client_id, &text)
            .await
        {
            eprintln!("[Messaging] Failed to mark DM send failure: {}", err);
        }
        return Err(format!("Failed to send message: {}", text));
    }

    let mut message: Message = res.json().await
        .map_err(|e| format!("Failed to parse response: {}", e))?;

    if message.client_id.is_none() {
        message.client_id = Some(resolved_client_id.clone());
    }
    if message.status.is_none() {
        message.status = Some("sent".to_string());
    }

    if let Err(err) = messaging
        .service
        .mark_send_success(
            ConversationKind::Dm,
            &room_id,
            message.id.clone(),
            message.client_id.clone(),
            message.sender_id.clone(),
            None,
            message.content.clone(),
            message.nonce.clone(),
            message
                .created_at
                .clone()
                .unwrap_or_else(|| Utc::now().to_rfc3339()),
            message.edited_at.clone(),
            parse_local_status(message.status.as_deref()),
        )
        .await
    {
        eprintln!("[Messaging] Failed to persist sent DM message: {}", err);
    }

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
