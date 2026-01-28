use serde::{Deserialize, Serialize};
use tauri::State;
use crate::api::ApiState;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Server {
    pub id: String,
    pub name: String,
    pub icon_url: Option<String>,
    pub owner_id: String,
    pub invite_code: String,
    pub created_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Channel {
    pub id: String,
    pub server_id: String,
    pub name: String,
    pub channel_type: String,
    pub position: Option<i32>,
    pub created_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerMember {
    pub user_id: String,
    pub username: String,
    pub avatar_url: Option<String>,
    pub role: String,
    pub last_seen: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChannelMessage {
    pub id: String,
    pub channel_id: String,
    pub sender_id: Option<String>,
    pub content: String,
    pub nonce: Option<String>,
    pub created_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerWithChannels {
    #[serde(flatten)]
    pub server: Server,
    pub channels: Vec<Channel>,
}

#[derive(Debug, Serialize, Deserialize)]
struct CreateServerRequest {
    name: String,
    icon_url: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct CreateChannelRequest {
    name: String,
    channel_type: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct SendChannelMessageRequest {
    content: String,
    nonce: Option<String>,
}

#[tauri::command]
pub async fn api_fetch_servers(
    state: State<'_, ApiState>,
) -> Result<Vec<Server>, String> {
    let token = state.get_token().await
        .ok_or("Not authenticated")?;
    
    let url = format!("{}/servers", state.base_url);
    
    let res = state.client
        .get(&url)
        .header("Authorization", format!("Bearer {}", token))
        .send()
        .await
        .map_err(|e| format!("Network error: {}", e))?;

    if !res.status().is_success() {
        let text = res.text().await.unwrap_or_default();
        return Err(format!("Failed to fetch servers: {}", text));
    }

    let servers: Vec<Server> = res.json().await
        .map_err(|e| format!("Failed to parse response: {}", e))?;

    Ok(servers)
}

#[tauri::command]
pub async fn api_create_server(
    state: State<'_, ApiState>,
    name: String,
    icon_url: Option<String>,
) -> Result<Server, String> {
    let token = state.get_token().await
        .ok_or("Not authenticated")?;
    
    let url = format!("{}/servers", state.base_url);
    
    let res = state.client
        .post(&url)
        .header("Authorization", format!("Bearer {}", token))
        .json(&CreateServerRequest { name, icon_url })
        .send()
        .await
        .map_err(|e| format!("Network error: {}", e))?;

    if !res.status().is_success() {
        let text = res.text().await.unwrap_or_default();
        return Err(format!("Failed to create server: {}", text));
    }

    let server: Server = res.json().await
        .map_err(|e| format!("Failed to parse response: {}", e))?;

    Ok(server)
}

#[tauri::command]
pub async fn api_join_server(
    state: State<'_, ApiState>,
    invite_code: String,
) -> Result<Server, String> {
    let token = state.get_token().await
        .ok_or("Not authenticated")?;
    
    let url = format!("{}/servers/join/{}", state.base_url, invite_code);
    
    let res = state.client
        .post(&url)
        .header("Authorization", format!("Bearer {}", token))
        .send()
        .await
        .map_err(|e| format!("Network error: {}", e))?;

    if !res.status().is_success() {
        let text = res.text().await.unwrap_or_default();
        return Err(format!("Failed to join server: {}", text));
    }

    let server: Server = res.json().await
        .map_err(|e| format!("Failed to parse response: {}", e))?;

    Ok(server)
}

#[tauri::command]
pub async fn api_leave_server(
    state: State<'_, ApiState>,
    server_id: String,
) -> Result<(), String> {
    let token = state.get_token().await
        .ok_or("Not authenticated")?;
    
    let url = format!("{}/servers/{}/leave", state.base_url, server_id);
    
    let res = state.client
        .post(&url)
        .header("Authorization", format!("Bearer {}", token))
        .send()
        .await
        .map_err(|e| format!("Network error: {}", e))?;

    if !res.status().is_success() {
        let text = res.text().await.unwrap_or_default();
        return Err(format!("Failed to leave server: {}", text));
    }

    Ok(())
}

#[tauri::command]
pub async fn api_fetch_server_details(
    state: State<'_, ApiState>,
    server_id: String,
) -> Result<ServerWithChannels, String> {
    let token = state.get_token().await
        .ok_or("Not authenticated")?;
    
    let url = format!("{}/servers/{}", state.base_url, server_id);
    
    let res = state.client
        .get(&url)
        .header("Authorization", format!("Bearer {}", token))
        .send()
        .await
        .map_err(|e| format!("Network error: {}", e))?;

    if !res.status().is_success() {
        let text = res.text().await.unwrap_or_default();
        return Err(format!("Failed to fetch server details: {}", text));
    }

    let data: ServerWithChannels = res.json().await
        .map_err(|e| format!("Failed to parse response: {}", e))?;

    Ok(data)
}

#[tauri::command]
pub async fn api_create_channel(
    state: State<'_, ApiState>,
    server_id: String,
    name: String,
    channel_type: Option<String>,
) -> Result<Channel, String> {
    let token = state.get_token().await
        .ok_or("Not authenticated")?;
    
    let url = format!("{}/servers/{}/channels", state.base_url, server_id);
    
    let res = state.client
        .post(&url)
        .header("Authorization", format!("Bearer {}", token))
        .json(&CreateChannelRequest { name, channel_type })
        .send()
        .await
        .map_err(|e| format!("Network error: {}", e))?;

    if !res.status().is_success() {
        let text = res.text().await.unwrap_or_default();
        return Err(format!("Failed to create channel: {}", text));
    }

    let channel: Channel = res.json().await
        .map_err(|e| format!("Failed to parse response: {}", e))?;

    Ok(channel)
}

#[tauri::command]
pub async fn api_fetch_server_members(
    state: State<'_, ApiState>,
    server_id: String,
) -> Result<Vec<ServerMember>, String> {
    let token = state.get_token().await
        .ok_or("Not authenticated")?;
    
    let url = format!("{}/servers/{}/members", state.base_url, server_id);
    
    let res = state.client
        .get(&url)
        .header("Authorization", format!("Bearer {}", token))
        .send()
        .await
        .map_err(|e| format!("Network error: {}", e))?;

    if !res.status().is_success() {
        let text = res.text().await.unwrap_or_default();
        return Err(format!("Failed to fetch members: {}", text));
    }

    let members: Vec<ServerMember> = res.json().await
        .map_err(|e| format!("Failed to parse response: {}", e))?;

    Ok(members)
}

#[tauri::command]
pub async fn api_fetch_channel_messages(
    state: State<'_, ApiState>,
    server_id: String,
    channel_id: String,
) -> Result<Vec<ChannelMessage>, String> {
    let token = state.get_token().await
        .ok_or("Not authenticated")?;
    
    let url = format!("{}/servers/{}/channels/{}/messages", state.base_url, server_id, channel_id);
    
    let res = state.client
        .get(&url)
        .header("Authorization", format!("Bearer {}", token))
        .send()
        .await
        .map_err(|e| format!("Network error: {}", e))?;

    if !res.status().is_success() {
        let text = res.text().await.unwrap_or_default();
        return Err(format!("Failed to fetch channel messages: {}", text));
    }

    let messages: Vec<ChannelMessage> = res.json().await
        .map_err(|e| format!("Failed to parse response: {}", e))?;

    Ok(messages)
}

#[tauri::command]
pub async fn api_send_channel_message(
    state: State<'_, ApiState>,
    server_id: String,
    channel_id: String,
    content: String,
    nonce: Option<String>,
) -> Result<ChannelMessage, String> {
    let token = state.get_token().await
        .ok_or("Not authenticated")?;
    
    let url = format!("{}/servers/{}/channels/{}/messages", state.base_url, server_id, channel_id);
    
    let res = state.client
        .post(&url)
        .header("Authorization", format!("Bearer {}", token))
        .json(&SendChannelMessageRequest { content, nonce })
        .send()
        .await
        .map_err(|e| format!("Network error: {}", e))?;

    if !res.status().is_success() {
        let text = res.text().await.unwrap_or_default();
        return Err(format!("Failed to send channel message: {}", text));
    }

    let message: ChannelMessage = res.json().await
        .map_err(|e| format!("Failed to parse response: {}", e))?;

    Ok(message)
}
