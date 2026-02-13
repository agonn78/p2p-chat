use std::path::PathBuf;

use chrono::Utc;

use super::domain::{ConversationKind, MessageStatus, OutboxMessage, PersistedMessage};
use super::error::MessagingError;
use super::storage::MessagingStorage;

#[derive(Clone)]
pub struct MessagingService {
    storage: MessagingStorage,
}

impl MessagingService {
    pub async fn new(db_path: PathBuf) -> Result<Self, MessagingError> {
        Ok(Self {
            storage: MessagingStorage::new(db_path).await?,
        })
    }

    pub async fn create_pending_message(
        &self,
        target_kind: ConversationKind,
        target_id: &str,
        server_scope_id: Option<String>,
        sender_id: Option<String>,
        content: String,
        nonce: Option<String>,
        client_id: String,
    ) -> Result<PersistedMessage, MessagingError> {
        let now = Utc::now().to_rfc3339();
        let local_id = format!("local-{}", client_id);

        let message = PersistedMessage {
            local_id,
            server_id: None,
            client_id: Some(client_id.clone()),
            sender_id: sender_id.clone(),
            sender_username: None,
            target_kind,
            target_id: target_id.to_string(),
            content: content.clone(),
            nonce: nonce.clone(),
            created_at: now.clone(),
            edited_at: None,
            status: MessageStatus::Sending,
        };

        let outbox = OutboxMessage {
            client_id,
            target_kind,
            target_id: target_id.to_string(),
            server_scope_id,
            sender_id,
            content,
            nonce,
            created_at: now,
            attempts: 0,
            last_error: None,
        };

        self.storage.upsert_message(&message).await?;
        self.storage.enqueue_outbox(&outbox).await?;

        Ok(message)
    }

    pub async fn mark_send_failed(
        &self,
        client_id: &str,
        error: &str,
    ) -> Result<(), MessagingError> {
        self.storage.update_outbox_error(client_id, error).await?;

        // client_id is unique and corresponds to local pending row
        let local_id = format!("local-{}", client_id);
        self.storage
            .update_status_by_server_id(&local_id, MessageStatus::Failed)
            .await?;
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn mark_send_success(
        &self,
        target_kind: ConversationKind,
        target_id: &str,
        server_id: String,
        client_id: Option<String>,
        sender_id: Option<String>,
        sender_username: Option<String>,
        content: String,
        nonce: Option<String>,
        created_at: String,
        edited_at: Option<String>,
        status: MessageStatus,
    ) -> Result<PersistedMessage, MessagingError> {
        let message = PersistedMessage {
            local_id: server_id.clone(),
            server_id: Some(server_id),
            client_id: client_id.clone(),
            sender_id,
            sender_username,
            target_kind,
            target_id: target_id.to_string(),
            content,
            nonce,
            created_at,
            edited_at,
            status,
        };

        self.storage.upsert_message(&message).await?;
        if let Some(client_id) = client_id {
            self.storage.remove_outbox(&client_id).await?;
        }

        Ok(message)
    }

    pub async fn cache_remote_messages(
        &self,
        messages: &[PersistedMessage],
    ) -> Result<(), MessagingError> {
        for message in messages {
            self.storage.upsert_message(message).await?;
        }
        Ok(())
    }

    pub async fn load_messages(
        &self,
        target_kind: ConversationKind,
        target_id: &str,
        before: Option<&str>,
        limit: i64,
    ) -> Result<Vec<PersistedMessage>, MessagingError> {
        self.storage
            .load_messages(target_kind, target_id, before, limit)
            .await
    }

    pub async fn list_outbox(&self, limit: i64) -> Result<Vec<OutboxMessage>, MessagingError> {
        self.storage.list_outbox(limit).await
    }

    pub async fn clear_outbox(&self, client_id: &str) -> Result<(), MessagingError> {
        self.storage.remove_outbox(client_id).await
    }

    pub async fn set_status_by_server_id(
        &self,
        message_id: &str,
        status: MessageStatus,
    ) -> Result<(), MessagingError> {
        self.storage.update_status_by_server_id(message_id, status).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    fn temp_db_path(prefix: &str) -> PathBuf {
        std::env::temp_dir().join(format!("{}-{}.sqlite", prefix, Uuid::new_v4()))
    }

    #[tokio::test]
    async fn pending_then_success_replaces_local_and_clears_outbox() {
        let db_path = temp_db_path("messaging-service-success");
        let service = MessagingService::new(db_path.clone()).await.expect("service init");

        let pending = service
            .create_pending_message(
                ConversationKind::Dm,
                "room-1",
                None,
                Some("u1".to_string()),
                "hello".to_string(),
                None,
                "c3".to_string(),
            )
            .await
            .expect("create pending");

        assert_eq!(pending.status, MessageStatus::Sending);

        service
            .mark_send_success(
                ConversationKind::Dm,
                "room-1",
                "srv-msg-1".to_string(),
                Some("c3".to_string()),
                Some("u1".to_string()),
                Some("alice".to_string()),
                "hello".to_string(),
                None,
                "2026-02-12T00:10:00Z".to_string(),
                None,
                MessageStatus::Sent,
            )
            .await
            .expect("mark send success");

        let outbox = service.list_outbox(10).await.expect("list outbox");
        assert!(outbox.is_empty());

        let loaded = service
            .load_messages(ConversationKind::Dm, "room-1", None, 50)
            .await
            .expect("load messages");
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].server_id.as_deref(), Some("srv-msg-1"));
        assert_eq!(loaded[0].status, MessageStatus::Sent);

        let _ = std::fs::remove_file(db_path);
    }

    #[tokio::test]
    async fn failed_send_marks_message_failed_and_tracks_attempts() {
        let db_path = temp_db_path("messaging-service-failed");
        let service = MessagingService::new(db_path.clone()).await.expect("service init");

        service
            .create_pending_message(
                ConversationKind::Channel,
                "ch-1",
                Some("srv-1".to_string()),
                Some("u1".to_string()),
                "retry me".to_string(),
                None,
                "c4".to_string(),
            )
            .await
            .expect("create pending");

        service
            .mark_send_failed("c4", "network down")
            .await
            .expect("mark failed");

        let outbox = service.list_outbox(10).await.expect("list outbox");
        assert_eq!(outbox.len(), 1);
        assert_eq!(outbox[0].attempts, 1);
        assert_eq!(outbox[0].last_error.as_deref(), Some("network down"));

        let loaded = service
            .load_messages(ConversationKind::Channel, "ch-1", None, 50)
            .await
            .expect("load messages");
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].status, MessageStatus::Failed);

        let _ = std::fs::remove_file(db_path);
    }

    #[tokio::test]
    async fn set_status_updates_cached_message_by_server_id() {
        let db_path = temp_db_path("messaging-service-status");
        let service = MessagingService::new(db_path.clone()).await.expect("service init");

        service
            .mark_send_success(
                ConversationKind::Dm,
                "room-1",
                "srv-msg-2".to_string(),
                Some("c5".to_string()),
                Some("u1".to_string()),
                Some("alice".to_string()),
                "hello".to_string(),
                None,
                "2026-02-12T00:20:00Z".to_string(),
                None,
                MessageStatus::Sent,
            )
            .await
            .expect("insert sent message");

        service
            .set_status_by_server_id("srv-msg-2", MessageStatus::Read)
            .await
            .expect("set read status");

        let loaded = service
            .load_messages(ConversationKind::Dm, "room-1", None, 50)
            .await
            .expect("load messages");
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].status, MessageStatus::Read);

        let _ = std::fs::remove_file(db_path);
    }
}
