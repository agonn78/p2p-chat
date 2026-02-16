use crate::api::ApiState;
use crate::error::AppResult;
use crate::messaging::domain::{
    ConversationKind, MessageStatus as LocalMessageStatus, PersistedMessage,
};
use crate::MessagingState;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use tauri::State;
use url::form_urlencoded::byte_serialize;
use uuid::Uuid;

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
pub struct VoiceChannelParticipant {
    pub user_id: String,
    pub username: String,
    pub joined_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChannelMessage {
    pub id: String,
    pub client_id: Option<String>,
    pub channel_id: String,
    pub sender_id: Option<String>,
    pub sender_username: Option<String>,
    pub content: String,
    pub nonce: Option<String>,
    pub created_at: Option<String>,
    pub edited_at: Option<String>,
    pub status: Option<String>,
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
    client_id: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct TypingRequest {
    is_typing: bool,
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

fn persisted_to_channel_message(message: PersistedMessage) -> ChannelMessage {
    ChannelMessage {
        id: message.server_id.clone().unwrap_or(message.local_id),
        client_id: message.client_id,
        channel_id: message.target_id,
        sender_id: message.sender_id,
        sender_username: message.sender_username,
        content: message.content,
        nonce: message.nonce,
        created_at: Some(message.created_at),
        edited_at: message.edited_at,
        status: Some(message.status.as_str().to_string()),
    }
}

fn api_to_persisted_channel_message(
    channel_id: &str,
    message: &ChannelMessage,
) -> PersistedMessage {
    let server_id = message.id.clone();
    PersistedMessage {
        local_id: server_id.clone(),
        server_id: Some(server_id),
        client_id: message.client_id.clone(),
        sender_id: message.sender_id.clone(),
        sender_username: message.sender_username.clone(),
        target_kind: ConversationKind::Channel,
        target_id: channel_id.to_string(),
        content: message.content.clone(),
        nonce: message.nonce.clone(),
        created_at: message
            .created_at
            .clone()
            .unwrap_or_else(|| Utc::now().to_rfc3339()),
        edited_at: message.edited_at.clone(),
        status: parse_local_status(message.status.as_deref()),
    }
}

#[tauri::command]
pub async fn api_fetch_servers(state: State<'_, ApiState>) -> AppResult<Vec<Server>> {
    let token = state.get_token().await.ok_or("Not authenticated")?;

    let url = format!("{}/servers", state.base_url);

    let res = state
        .client
        .get(&url)
        .header("Authorization", format!("Bearer {}", token))
        .send()
        .await
        .map_err(|e| format!("Network error: {}", e))?;

    if !res.status().is_success() {
        let text = res.text().await.unwrap_or_default();
        return Err((format!("Failed to fetch servers: {}", text)).into());
    }

    let servers: Vec<Server> = res
        .json()
        .await
        .map_err(|e| format!("Failed to parse response: {}", e))?;

    Ok(servers)
}

#[tauri::command]
pub async fn api_create_server(
    state: State<'_, ApiState>,
    name: String,
    icon_url: Option<String>,
) -> AppResult<Server> {
    let token = state.get_token().await.ok_or("Not authenticated")?;

    let url = format!("{}/servers", state.base_url);

    let res = state
        .client
        .post(&url)
        .header("Authorization", format!("Bearer {}", token))
        .json(&CreateServerRequest { name, icon_url })
        .send()
        .await
        .map_err(|e| format!("Network error: {}", e))?;

    if !res.status().is_success() {
        let text = res.text().await.unwrap_or_default();
        return Err((format!("Failed to create server: {}", text)).into());
    }

    let server: Server = res
        .json()
        .await
        .map_err(|e| format!("Failed to parse response: {}", e))?;

    Ok(server)
}

#[tauri::command]
pub async fn api_join_server(
    state: State<'_, ApiState>,
    invite_code: String,
) -> AppResult<Server> {
    let token = state.get_token().await.ok_or("Not authenticated")?;

    let url = format!("{}/servers/join/{}", state.base_url, invite_code);

    let res = state
        .client
        .post(&url)
        .header("Authorization", format!("Bearer {}", token))
        .send()
        .await
        .map_err(|e| format!("Network error: {}", e))?;

    if !res.status().is_success() {
        let text = res.text().await.unwrap_or_default();
        return Err((format!("Failed to join server: {}", text)).into());
    }

    let server: Server = res
        .json()
        .await
        .map_err(|e| format!("Failed to parse response: {}", e))?;

    Ok(server)
}

#[tauri::command]
pub async fn api_leave_server(state: State<'_, ApiState>, server_id: String) -> AppResult<()> {
    let token = state.get_token().await.ok_or("Not authenticated")?;

    let url = format!("{}/servers/{}/leave", state.base_url, server_id);

    let res = state
        .client
        .post(&url)
        .header("Authorization", format!("Bearer {}", token))
        .send()
        .await
        .map_err(|e| format!("Network error: {}", e))?;

    if !res.status().is_success() {
        let text = res.text().await.unwrap_or_default();
        return Err((format!("Failed to leave server: {}", text)).into());
    }

    Ok(())
}

#[tauri::command]
pub async fn api_fetch_server_details(
    state: State<'_, ApiState>,
    server_id: String,
) -> AppResult<ServerWithChannels> {
    let token = state.get_token().await.ok_or("Not authenticated")?;

    let url = format!("{}/servers/{}", state.base_url, server_id);

    let res = state
        .client
        .get(&url)
        .header("Authorization", format!("Bearer {}", token))
        .send()
        .await
        .map_err(|e| format!("Network error: {}", e))?;

    if !res.status().is_success() {
        let text = res.text().await.unwrap_or_default();
        return Err((format!("Failed to fetch server details: {}", text)).into());
    }

    let data: ServerWithChannels = res
        .json()
        .await
        .map_err(|e| format!("Failed to parse response: {}", e))?;

    Ok(data)
}

#[tauri::command]
pub async fn api_create_channel(
    state: State<'_, ApiState>,
    server_id: String,
    name: String,
    channel_type: Option<String>,
) -> AppResult<Channel> {
    let token = state.get_token().await.ok_or("Not authenticated")?;

    let url = format!("{}/servers/{}/channels", state.base_url, server_id);

    let res = state
        .client
        .post(&url)
        .header("Authorization", format!("Bearer {}", token))
        .json(&CreateChannelRequest { name, channel_type })
        .send()
        .await
        .map_err(|e| format!("Network error: {}", e))?;

    if !res.status().is_success() {
        let text = res.text().await.unwrap_or_default();
        return Err((format!("Failed to create channel: {}", text)).into());
    }

    let channel: Channel = res
        .json()
        .await
        .map_err(|e| format!("Failed to parse response: {}", e))?;

    Ok(channel)
}

#[tauri::command]
pub async fn api_fetch_server_members(
    state: State<'_, ApiState>,
    server_id: String,
) -> AppResult<Vec<ServerMember>> {
    let token = state.get_token().await.ok_or("Not authenticated")?;

    let url = format!("{}/servers/{}/members", state.base_url, server_id);

    let res = state
        .client
        .get(&url)
        .header("Authorization", format!("Bearer {}", token))
        .send()
        .await
        .map_err(|e| format!("Network error: {}", e))?;

    if !res.status().is_success() {
        let text = res.text().await.unwrap_or_default();
        return Err((format!("Failed to fetch members: {}", text)).into());
    }

    let members: Vec<ServerMember> = res
        .json()
        .await
        .map_err(|e| format!("Failed to parse response: {}", e))?;

    Ok(members)
}

#[tauri::command]
pub async fn api_fetch_channel_messages(
    state: State<'_, ApiState>,
    messaging: State<'_, MessagingState>,
    server_id: String,
    channel_id: String,
    before: Option<String>,
    limit: Option<i64>,
) -> AppResult<Vec<ChannelMessage>> {
    let token = state.get_token().await.ok_or("Not authenticated")?;

    let limit = limit.unwrap_or(100).clamp(1, 200);

    let url = format!(
        "{}/servers/{}/channels/{}/messages",
        state.base_url, server_id, channel_id
    );

    let before_cursor = before.clone();
    let mut query_params: Vec<(String, String)> = Vec::new();
    if let Some(before_id) = before {
        query_params.push(("before".to_string(), before_id));
    }
    query_params.push(("limit".to_string(), limit.to_string()));

    let remote_res = state
        .client
        .get(&url)
        .header("Authorization", format!("Bearer {}", token))
        .query(&query_params)
        .send()
        .await
        .map_err(|e| format!("Network error: {}", e));

    match remote_res {
        Ok(res) if res.status().is_success() => {
            let remote_messages: Vec<ChannelMessage> = res
                .json()
                .await
                .map_err(|e| format!("Failed to parse response: {}", e))?;

            let persisted = remote_messages
                .iter()
                .map(|m| api_to_persisted_channel_message(&channel_id, m))
                .collect::<Vec<_>>();
            if let Err(err) = messaging.service.cache_remote_messages(&persisted).await {
                eprintln!("[Messaging] Failed to cache channel messages: {}", err);
                return Ok(remote_messages);
            }

            match messaging
                .service
                .load_messages(
                    ConversationKind::Channel,
                    &channel_id,
                    before_cursor.as_deref(),
                    limit,
                )
                .await
            {
                Ok(local_messages) if !local_messages.is_empty() => Ok(local_messages
                    .into_iter()
                    .map(persisted_to_channel_message)
                    .collect()),
                Ok(_) => Ok(remote_messages),
                Err(err) => {
                    eprintln!(
                        "[Messaging] Failed to load cached channel messages: {}",
                        err
                    );
                    Ok(remote_messages)
                }
            }
        }
        Ok(res) => {
            let text = res.text().await.unwrap_or_default();
            let remote_error = format!("Failed to fetch channel messages: {}", text);

            let cached = messaging
                .service
                .load_messages(
                    ConversationKind::Channel,
                    &channel_id,
                    before_cursor.as_deref(),
                    limit,
                )
                .await
                .map_err(|e| format!("{} (cache unavailable: {})", remote_error, e))?;

            if !cached.is_empty() {
                return Ok(cached
                    .into_iter()
                    .map(persisted_to_channel_message)
                    .collect());
            }

            Err(remote_error.into())
        }
        Err(remote_error) => {
            let cached = messaging
                .service
                .load_messages(
                    ConversationKind::Channel,
                    &channel_id,
                    before_cursor.as_deref(),
                    limit,
                )
                .await
                .map_err(|e| format!("{} (cache unavailable: {})", remote_error, e))?;

            if !cached.is_empty() {
                return Ok(cached
                    .into_iter()
                    .map(persisted_to_channel_message)
                    .collect());
            }

            Err(remote_error.into())
        }
    }
}

#[tauri::command]
pub async fn api_send_channel_message(
    state: State<'_, ApiState>,
    messaging: State<'_, MessagingState>,
    server_id: String,
    channel_id: String,
    content: String,
    nonce: Option<String>,
    client_id: Option<String>,
) -> AppResult<ChannelMessage> {
    let token = state.get_token().await.ok_or("Not authenticated")?;

    let url = format!(
        "{}/servers/{}/channels/{}/messages",
        state.base_url, server_id, channel_id
    );
    let resolved_client_id = client_id.unwrap_or_else(|| Uuid::new_v4().to_string());

    if let Err(err) = messaging
        .service
        .create_pending_message(
            ConversationKind::Channel,
            &channel_id,
            Some(server_id.clone()),
            None,
            content.clone(),
            nonce.clone(),
            resolved_client_id.clone(),
        )
        .await
    {
        eprintln!(
            "[Messaging] Failed to persist pending channel message: {}",
            err
        );
    }

    let res = state
        .client
        .post(&url)
        .header("Authorization", format!("Bearer {}", token))
        .json(&SendChannelMessageRequest {
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
            eprintln!("[Messaging] Failed to mark channel send failure: {}", err);
        }
        return Err((format!("Failed to send channel message: {}", text)).into());
    }

    let mut message: ChannelMessage = res
        .json()
        .await
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
            ConversationKind::Channel,
            &channel_id,
            message.id.clone(),
            message.client_id.clone(),
            message.sender_id.clone(),
            message.sender_username.clone(),
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
        eprintln!(
            "[Messaging] Failed to persist sent channel message: {}",
            err
        );
    }

    Ok(message)
}

#[tauri::command]
pub async fn api_send_channel_typing(
    state: State<'_, ApiState>,
    server_id: String,
    channel_id: String,
    is_typing: bool,
) -> AppResult<()> {
    let token = state.get_token().await.ok_or("Not authenticated")?;

    let url = format!(
        "{}/servers/{}/channels/{}/typing",
        state.base_url, server_id, channel_id
    );

    let res = state
        .client
        .post(&url)
        .header("Authorization", format!("Bearer {}", token))
        .json(&TypingRequest { is_typing })
        .send()
        .await
        .map_err(|e| format!("Network error: {}", e))?;

    if !res.status().is_success() {
        let text = res.text().await.unwrap_or_default();
        return Err((format!("Failed to send channel typing: {}", text)).into());
    }

    Ok(())
}

#[tauri::command]
pub async fn api_fetch_voice_channel_presence(
    state: State<'_, ApiState>,
    server_id: String,
    channel_id: String,
) -> AppResult<Vec<VoiceChannelParticipant>> {
    let token = state.get_token().await.ok_or("Not authenticated")?;

    let url = format!(
        "{}/servers/{}/channels/{}/voice",
        state.base_url, server_id, channel_id
    );

    let res = state
        .client
        .get(&url)
        .header("Authorization", format!("Bearer {}", token))
        .send()
        .await
        .map_err(|e| format!("Network error: {}", e))?;

    if !res.status().is_success() {
        let text = res.text().await.unwrap_or_default();
        return Err((format!("Failed to fetch voice presence: {}", text)).into());
    }

    let participants: Vec<VoiceChannelParticipant> = res
        .json()
        .await
        .map_err(|e| format!("Failed to parse response: {}", e))?;

    Ok(participants)
}

#[tauri::command]
pub async fn api_join_voice_channel(
    state: State<'_, ApiState>,
    server_id: String,
    channel_id: String,
) -> AppResult<()> {
    let token = state.get_token().await.ok_or("Not authenticated")?;

    let url = format!(
        "{}/servers/{}/channels/{}/voice/join",
        state.base_url, server_id, channel_id
    );

    let res = state
        .client
        .post(&url)
        .header("Authorization", format!("Bearer {}", token))
        .send()
        .await
        .map_err(|e| format!("Network error: {}", e))?;

    if !res.status().is_success() {
        let text = res.text().await.unwrap_or_default();
        return Err((format!("Failed to join voice channel: {}", text)).into());
    }

    Ok(())
}

#[tauri::command]
pub async fn api_leave_voice_channel(
    state: State<'_, ApiState>,
    server_id: String,
    channel_id: String,
) -> AppResult<()> {
    let token = state.get_token().await.ok_or("Not authenticated")?;

    let url = format!(
        "{}/servers/{}/channels/{}/voice/leave",
        state.base_url, server_id, channel_id
    );

    let res = state
        .client
        .post(&url)
        .header("Authorization", format!("Bearer {}", token))
        .send()
        .await
        .map_err(|e| format!("Network error: {}", e))?;

    if !res.status().is_success() {
        let text = res.text().await.unwrap_or_default();
        return Err((format!("Failed to leave voice channel: {}", text)).into());
    }

    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageReactionSummary {
    pub emoji: String,
    pub user_ids: Vec<String>,
    pub count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerBanEntry {
    pub user_id: String,
    pub username: String,
    pub banned_by: String,
    pub banned_by_username: String,
    pub reason: Option<String>,
    pub created_at: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct UpdateMemberRoleRequest {
    role: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct BanMemberRequest {
    reason: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct ReactionRequest {
    emoji: String,
}

#[tauri::command]
pub async fn api_delete_server(
    state: State<'_, ApiState>,
    server_id: String,
) -> AppResult<()> {
    let token = state.get_token().await.ok_or("Not authenticated")?;
    let url = format!("{}/servers/{}", state.base_url, server_id);

    let res = state
        .client
        .delete(&url)
        .header("Authorization", format!("Bearer {}", token))
        .send()
        .await
        .map_err(|e| format!("Network error: {}", e))?;

    if !res.status().is_success() {
        let text = res.text().await.unwrap_or_default();
        return Err((format!("Failed to delete server: {}", text)).into());
    }

    Ok(())
}

#[tauri::command]
pub async fn api_regenerate_server_invite(
    state: State<'_, ApiState>,
    server_id: String,
) -> AppResult<Server> {
    let token = state.get_token().await.ok_or("Not authenticated")?;
    let url = format!("{}/servers/{}/invite/regenerate", state.base_url, server_id);

    let res = state
        .client
        .post(&url)
        .header("Authorization", format!("Bearer {}", token))
        .send()
        .await
        .map_err(|e| format!("Network error: {}", e))?;

    if !res.status().is_success() {
        let text = res.text().await.unwrap_or_default();
        return Err((format!("Failed to regenerate invite: {}", text)).into());
    }

    Ok(
        res.json()
            .await
            .map_err(|e| format!("Failed to parse response: {}", e))?,
    )
}

#[tauri::command]
pub async fn api_update_member_role(
    state: State<'_, ApiState>,
    server_id: String,
    member_id: String,
    role: String,
) -> AppResult<()> {
    let token = state.get_token().await.ok_or("Not authenticated")?;
    let url = format!(
        "{}/servers/{}/members/{}/role",
        state.base_url, server_id, member_id
    );

    let res = state
        .client
        .put(&url)
        .header("Authorization", format!("Bearer {}", token))
        .json(&UpdateMemberRoleRequest { role })
        .send()
        .await
        .map_err(|e| format!("Network error: {}", e))?;

    if !res.status().is_success() {
        let text = res.text().await.unwrap_or_default();
        return Err((format!("Failed to update member role: {}", text)).into());
    }

    Ok(())
}

#[tauri::command]
pub async fn api_kick_member(
    state: State<'_, ApiState>,
    server_id: String,
    member_id: String,
) -> AppResult<()> {
    let token = state.get_token().await.ok_or("Not authenticated")?;
    let url = format!(
        "{}/servers/{}/members/{}/kick",
        state.base_url, server_id, member_id
    );

    let res = state
        .client
        .post(&url)
        .header("Authorization", format!("Bearer {}", token))
        .send()
        .await
        .map_err(|e| format!("Network error: {}", e))?;

    if !res.status().is_success() {
        let text = res.text().await.unwrap_or_default();
        return Err((format!("Failed to kick member: {}", text)).into());
    }

    Ok(())
}

#[tauri::command]
pub async fn api_ban_member(
    state: State<'_, ApiState>,
    server_id: String,
    member_id: String,
    reason: Option<String>,
) -> AppResult<()> {
    let token = state.get_token().await.ok_or("Not authenticated")?;
    let url = format!(
        "{}/servers/{}/members/{}/ban",
        state.base_url, server_id, member_id
    );

    let res = state
        .client
        .post(&url)
        .header("Authorization", format!("Bearer {}", token))
        .json(&BanMemberRequest { reason })
        .send()
        .await
        .map_err(|e| format!("Network error: {}", e))?;

    if !res.status().is_success() {
        let text = res.text().await.unwrap_or_default();
        return Err((format!("Failed to ban member: {}", text)).into());
    }

    Ok(())
}

#[tauri::command]
pub async fn api_list_server_bans(
    state: State<'_, ApiState>,
    server_id: String,
) -> AppResult<Vec<ServerBanEntry>> {
    let token = state.get_token().await.ok_or("Not authenticated")?;
    let url = format!("{}/servers/{}/bans", state.base_url, server_id);

    let res = state
        .client
        .get(&url)
        .header("Authorization", format!("Bearer {}", token))
        .send()
        .await
        .map_err(|e| format!("Network error: {}", e))?;

    if !res.status().is_success() {
        let text = res.text().await.unwrap_or_default();
        return Err((format!("Failed to list bans: {}", text)).into());
    }

    Ok(
        res.json()
            .await
            .map_err(|e| format!("Failed to parse response: {}", e))?,
    )
}

#[tauri::command]
pub async fn api_unban_member(
    state: State<'_, ApiState>,
    server_id: String,
    member_id: String,
) -> AppResult<()> {
    let token = state.get_token().await.ok_or("Not authenticated")?;
    let url = format!(
        "{}/servers/{}/bans/{}",
        state.base_url, server_id, member_id
    );

    let res = state
        .client
        .delete(&url)
        .header("Authorization", format!("Bearer {}", token))
        .send()
        .await
        .map_err(|e| format!("Network error: {}", e))?;

    if !res.status().is_success() {
        let text = res.text().await.unwrap_or_default();
        return Err((format!("Failed to unban member: {}", text)).into());
    }

    Ok(())
}

#[tauri::command]
pub async fn api_search_channel_messages(
    state: State<'_, ApiState>,
    server_id: String,
    channel_id: String,
    query: String,
    limit: Option<i64>,
) -> AppResult<Vec<ChannelMessage>> {
    let token = state.get_token().await.ok_or("Not authenticated")?;
    let url = format!(
        "{}/servers/{}/channels/{}/messages/search",
        state.base_url, server_id, channel_id
    );

    let mut params = vec![("q".to_string(), query)];
    if let Some(limit) = limit {
        params.push(("limit".to_string(), limit.to_string()));
    }

    let res = state
        .client
        .get(&url)
        .header("Authorization", format!("Bearer {}", token))
        .query(&params)
        .send()
        .await
        .map_err(|e| format!("Network error: {}", e))?;

    if !res.status().is_success() {
        let text = res.text().await.unwrap_or_default();
        return Err((format!("Failed to search channel messages: {}", text)).into());
    }

    Ok(
        res.json()
            .await
            .map_err(|e| format!("Failed to parse response: {}", e))?,
    )
}

#[tauri::command]
pub async fn api_fetch_channel_message_reactions(
    state: State<'_, ApiState>,
    server_id: String,
    channel_id: String,
    message_id: String,
) -> AppResult<Vec<MessageReactionSummary>> {
    let token = state.get_token().await.ok_or("Not authenticated")?;
    let url = format!(
        "{}/servers/{}/channels/{}/messages/{}/reactions",
        state.base_url, server_id, channel_id, message_id
    );

    let res = state
        .client
        .get(&url)
        .header("Authorization", format!("Bearer {}", token))
        .send()
        .await
        .map_err(|e| format!("Network error: {}", e))?;

    if !res.status().is_success() {
        let text = res.text().await.unwrap_or_default();
        return Err((format!("Failed to fetch reactions: {}", text)).into());
    }

    Ok(
        res.json()
            .await
            .map_err(|e| format!("Failed to parse response: {}", e))?,
    )
}

#[tauri::command]
pub async fn api_add_channel_message_reaction(
    state: State<'_, ApiState>,
    server_id: String,
    channel_id: String,
    message_id: String,
    emoji: String,
) -> AppResult<Vec<MessageReactionSummary>> {
    let token = state.get_token().await.ok_or("Not authenticated")?;
    let url = format!(
        "{}/servers/{}/channels/{}/messages/{}/reactions",
        state.base_url, server_id, channel_id, message_id
    );

    let res = state
        .client
        .post(&url)
        .header("Authorization", format!("Bearer {}", token))
        .json(&ReactionRequest { emoji })
        .send()
        .await
        .map_err(|e| format!("Network error: {}", e))?;

    if !res.status().is_success() {
        let text = res.text().await.unwrap_or_default();
        return Err((format!("Failed to add reaction: {}", text)).into());
    }

    Ok(
        res.json()
            .await
            .map_err(|e| format!("Failed to parse response: {}", e))?,
    )
}

#[tauri::command]
pub async fn api_remove_channel_message_reaction(
    state: State<'_, ApiState>,
    server_id: String,
    channel_id: String,
    message_id: String,
    emoji: String,
) -> AppResult<Vec<MessageReactionSummary>> {
    let token = state.get_token().await.ok_or("Not authenticated")?;
    let encoded_emoji: String = byte_serialize(emoji.as_bytes()).collect();
    let url = format!(
        "{}/servers/{}/channels/{}/messages/{}/reactions/{}",
        state.base_url, server_id, channel_id, message_id, encoded_emoji
    );

    let res = state
        .client
        .delete(&url)
        .header("Authorization", format!("Bearer {}", token))
        .send()
        .await
        .map_err(|e| format!("Network error: {}", e))?;

    if !res.status().is_success() {
        let text = res.text().await.unwrap_or_default();
        return Err((format!("Failed to remove reaction: {}", text)).into());
    }

    Ok(
        res.json()
            .await
            .map_err(|e| format!("Failed to parse response: {}", e))?,
    )
}

#[tauri::command]
pub async fn api_fetch_channel_thread_messages(
    state: State<'_, ApiState>,
    server_id: String,
    channel_id: String,
    message_id: String,
) -> AppResult<Vec<ChannelMessage>> {
    let token = state.get_token().await.ok_or("Not authenticated")?;
    let url = format!(
        "{}/servers/{}/channels/{}/messages/{}/thread",
        state.base_url, server_id, channel_id, message_id
    );

    let res = state
        .client
        .get(&url)
        .header("Authorization", format!("Bearer {}", token))
        .send()
        .await
        .map_err(|e| format!("Network error: {}", e))?;

    if !res.status().is_success() {
        let text = res.text().await.unwrap_or_default();
        return Err((format!("Failed to fetch thread messages: {}", text)).into());
    }

    Ok(
        res.json()
            .await
            .map_err(|e| format!("Failed to parse response: {}", e))?,
    )
}

#[tauri::command]
pub async fn api_send_channel_thread_message(
    state: State<'_, ApiState>,
    server_id: String,
    channel_id: String,
    message_id: String,
    content: String,
    nonce: Option<String>,
    client_id: Option<String>,
) -> AppResult<ChannelMessage> {
    let token = state.get_token().await.ok_or("Not authenticated")?;
    let url = format!(
        "{}/servers/{}/channels/{}/messages/{}/thread",
        state.base_url, server_id, channel_id, message_id
    );

    let res = state
        .client
        .post(&url)
        .header("Authorization", format!("Bearer {}", token))
        .json(&SendChannelMessageRequest {
            content,
            nonce,
            client_id,
        })
        .send()
        .await
        .map_err(|e| format!("Network error: {}", e))?;

    if !res.status().is_success() {
        let text = res.text().await.unwrap_or_default();
        return Err((format!("Failed to send thread message: {}", text)).into());
    }

    Ok(
        res.json()
            .await
            .map_err(|e| format!("Failed to parse response: {}", e))?,
    )
}
