use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ConversationKind {
    Dm,
    Channel,
}

impl ConversationKind {
    pub fn as_str(self) -> &'static str {
        match self {
            ConversationKind::Dm => "dm",
            ConversationKind::Channel => "channel",
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MessageStatus {
    Sending,
    Sent,
    Delivered,
    Read,
    Failed,
}

impl MessageStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            MessageStatus::Sending => "sending",
            MessageStatus::Sent => "sent",
            MessageStatus::Delivered => "delivered",
            MessageStatus::Read => "read",
            MessageStatus::Failed => "failed",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersistedMessage {
    pub local_id: String,
    pub server_id: Option<String>,
    pub client_id: Option<String>,
    pub sender_id: Option<String>,
    pub sender_username: Option<String>,
    pub target_kind: ConversationKind,
    pub target_id: String,
    pub content: String,
    pub nonce: Option<String>,
    pub created_at: String,
    pub edited_at: Option<String>,
    pub status: MessageStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutboxMessage {
    pub client_id: String,
    pub target_kind: ConversationKind,
    pub target_id: String,
    pub server_scope_id: Option<String>,
    pub sender_id: Option<String>,
    pub content: String,
    pub nonce: Option<String>,
    pub created_at: String,
    pub attempts: i64,
    pub last_error: Option<String>,
}
