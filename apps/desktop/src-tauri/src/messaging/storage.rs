use std::path::PathBuf;

use sqlx::{
    sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions},
    SqlitePool,
};

use super::domain::{ConversationKind, MessageStatus, OutboxMessage, PersistedMessage};
use super::error::MessagingError;

#[derive(Debug, sqlx::FromRow)]
struct MessageRow {
    local_id: String,
    server_id: Option<String>,
    client_id: Option<String>,
    sender_id: Option<String>,
    sender_username: Option<String>,
    target_kind: String,
    target_id: String,
    content: String,
    nonce: Option<String>,
    created_at: String,
    edited_at: Option<String>,
    status: String,
}

#[derive(Debug, sqlx::FromRow)]
struct OutboxRow {
    client_id: String,
    target_kind: String,
    target_id: String,
    server_scope_id: Option<String>,
    sender_id: Option<String>,
    content: String,
    nonce: Option<String>,
    created_at: String,
    attempts: i64,
    last_error: Option<String>,
}

#[derive(Clone)]
pub struct MessagingStorage {
    pool: SqlitePool,
}

impl MessagingStorage {
    pub async fn new(db_path: PathBuf) -> Result<Self, MessagingError> {
        if let Some(parent) = db_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let options = SqliteConnectOptions::new()
            .filename(db_path)
            .create_if_missing(true)
            .journal_mode(SqliteJournalMode::Wal)
            .foreign_keys(true);

        let pool = SqlitePoolOptions::new()
            .max_connections(5)
            .connect_with(options)
            .await?;

        let storage = Self { pool };
        storage.init_schema().await?;
        Ok(storage)
    }

    async fn init_schema(&self) -> Result<(), MessagingError> {
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS local_messages (
                local_id TEXT PRIMARY KEY,
                server_id TEXT UNIQUE,
                client_id TEXT UNIQUE,
                sender_id TEXT,
                sender_username TEXT,
                target_kind TEXT NOT NULL,
                target_id TEXT NOT NULL,
                content TEXT NOT NULL,
                nonce TEXT,
                created_at TEXT NOT NULL,
                edited_at TEXT,
                status TEXT NOT NULL,
                updated_at TEXT NOT NULL DEFAULT (datetime('now'))
            )
            "#,
        )
        .execute(&self.pool)
        .await?;

        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_local_messages_target_time ON local_messages(target_kind, target_id, created_at DESC)",
        )
        .execute(&self.pool)
        .await?;

        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS outbox (
                client_id TEXT PRIMARY KEY,
                target_kind TEXT NOT NULL,
                target_id TEXT NOT NULL,
                server_scope_id TEXT,
                sender_id TEXT,
                content TEXT NOT NULL,
                nonce TEXT,
                created_at TEXT NOT NULL,
                attempts INTEGER NOT NULL DEFAULT 0,
                last_error TEXT
            )
            "#,
        )
        .execute(&self.pool)
        .await?;

        sqlx::query("CREATE INDEX IF NOT EXISTS idx_outbox_created_at ON outbox(created_at ASC)")
            .execute(&self.pool)
            .await?;

        Ok(())
    }

    pub async fn upsert_message(&self, msg: &PersistedMessage) -> Result<(), MessagingError> {
        if msg.client_id.is_some() {
            sqlx::query(
                r#"
                INSERT INTO local_messages (
                    local_id, server_id, client_id, sender_id, sender_username,
                    target_kind, target_id, content, nonce, created_at, edited_at, status
                )
                VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
                ON CONFLICT(client_id) DO UPDATE SET
                    local_id = excluded.local_id,
                    server_id = excluded.server_id,
                    sender_id = excluded.sender_id,
                    sender_username = excluded.sender_username,
                    target_kind = excluded.target_kind,
                    target_id = excluded.target_id,
                    content = excluded.content,
                    nonce = excluded.nonce,
                    created_at = excluded.created_at,
                    edited_at = excluded.edited_at,
                    status = excluded.status,
                    updated_at = datetime('now')
                "#,
            )
            .bind(&msg.local_id)
            .bind(&msg.server_id)
            .bind(&msg.client_id)
            .bind(&msg.sender_id)
            .bind(&msg.sender_username)
            .bind(msg.target_kind.as_str())
            .bind(&msg.target_id)
            .bind(&msg.content)
            .bind(&msg.nonce)
            .bind(&msg.created_at)
            .bind(&msg.edited_at)
            .bind(msg.status.as_str())
            .execute(&self.pool)
            .await?;
            return Ok(());
        }

        if msg.server_id.is_some() {
            sqlx::query(
                r#"
                INSERT INTO local_messages (
                    local_id, server_id, client_id, sender_id, sender_username,
                    target_kind, target_id, content, nonce, created_at, edited_at, status
                )
                VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
                ON CONFLICT(server_id) DO UPDATE SET
                    local_id = excluded.local_id,
                    sender_id = excluded.sender_id,
                    sender_username = excluded.sender_username,
                    target_kind = excluded.target_kind,
                    target_id = excluded.target_id,
                    content = excluded.content,
                    nonce = excluded.nonce,
                    created_at = excluded.created_at,
                    edited_at = excluded.edited_at,
                    status = excluded.status,
                    updated_at = datetime('now')
                "#,
            )
            .bind(&msg.local_id)
            .bind(&msg.server_id)
            .bind(&msg.client_id)
            .bind(&msg.sender_id)
            .bind(&msg.sender_username)
            .bind(msg.target_kind.as_str())
            .bind(&msg.target_id)
            .bind(&msg.content)
            .bind(&msg.nonce)
            .bind(&msg.created_at)
            .bind(&msg.edited_at)
            .bind(msg.status.as_str())
            .execute(&self.pool)
            .await?;
            return Ok(());
        }

        sqlx::query(
            r#"
            INSERT INTO local_messages (
                local_id, server_id, client_id, sender_id, sender_username,
                target_kind, target_id, content, nonce, created_at, edited_at, status
            )
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            ON CONFLICT(local_id) DO UPDATE SET
                sender_id = excluded.sender_id,
                sender_username = excluded.sender_username,
                target_kind = excluded.target_kind,
                target_id = excluded.target_id,
                content = excluded.content,
                nonce = excluded.nonce,
                created_at = excluded.created_at,
                edited_at = excluded.edited_at,
                status = excluded.status,
                updated_at = datetime('now')
            "#,
        )
        .bind(&msg.local_id)
        .bind(&msg.server_id)
        .bind(&msg.client_id)
        .bind(&msg.sender_id)
        .bind(&msg.sender_username)
        .bind(msg.target_kind.as_str())
        .bind(&msg.target_id)
        .bind(&msg.content)
        .bind(&msg.nonce)
        .bind(&msg.created_at)
        .bind(&msg.edited_at)
        .bind(msg.status.as_str())
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    pub async fn load_messages(
        &self,
        target_kind: ConversationKind,
        target_id: &str,
        before_local_id: Option<&str>,
        limit: i64,
    ) -> Result<Vec<PersistedMessage>, MessagingError> {
        let rows = if let Some(before) = before_local_id {
            sqlx::query_as::<_, MessageRow>(
                r#"
                SELECT local_id, server_id, client_id, sender_id, sender_username, target_kind, target_id,
                       content, nonce, created_at, edited_at, status
                FROM local_messages
                WHERE target_kind = ?
                  AND target_id = ?
                  AND created_at < (SELECT created_at FROM local_messages WHERE local_id = ? LIMIT 1)
                ORDER BY created_at DESC
                LIMIT ?
                "#,
            )
            .bind(target_kind.as_str())
            .bind(target_id)
            .bind(before)
            .bind(limit)
            .fetch_all(&self.pool)
            .await?
        } else {
            sqlx::query_as::<_, MessageRow>(
                r#"
                SELECT local_id, server_id, client_id, sender_id, sender_username, target_kind, target_id,
                       content, nonce, created_at, edited_at, status
                FROM local_messages
                WHERE target_kind = ?
                  AND target_id = ?
                ORDER BY created_at DESC
                LIMIT ?
                "#,
            )
            .bind(target_kind.as_str())
            .bind(target_id)
            .bind(limit)
            .fetch_all(&self.pool)
            .await?
        };

        let mut messages = rows
            .into_iter()
            .map(Self::row_to_message)
            .collect::<Vec<_>>();
        messages.reverse();
        Ok(messages)
    }

    pub async fn enqueue_outbox(&self, item: &OutboxMessage) -> Result<(), MessagingError> {
        sqlx::query(
            r#"
            INSERT INTO outbox (
                client_id, target_kind, target_id, server_scope_id, sender_id,
                content, nonce, created_at, attempts, last_error
            )
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            ON CONFLICT(client_id) DO UPDATE SET
                target_kind = excluded.target_kind,
                target_id = excluded.target_id,
                server_scope_id = excluded.server_scope_id,
                sender_id = excluded.sender_id,
                content = excluded.content,
                nonce = excluded.nonce,
                created_at = excluded.created_at,
                attempts = excluded.attempts,
                last_error = excluded.last_error
            "#,
        )
        .bind(&item.client_id)
        .bind(item.target_kind.as_str())
        .bind(&item.target_id)
        .bind(&item.server_scope_id)
        .bind(&item.sender_id)
        .bind(&item.content)
        .bind(&item.nonce)
        .bind(&item.created_at)
        .bind(item.attempts)
        .bind(&item.last_error)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    pub async fn remove_outbox(&self, client_id: &str) -> Result<(), MessagingError> {
        sqlx::query("DELETE FROM outbox WHERE client_id = ?")
            .bind(client_id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn update_outbox_error(
        &self,
        client_id: &str,
        err: &str,
    ) -> Result<(), MessagingError> {
        sqlx::query(
            "UPDATE outbox SET attempts = attempts + 1, last_error = ? WHERE client_id = ?",
        )
        .bind(err)
        .bind(client_id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn list_outbox(&self, limit: i64) -> Result<Vec<OutboxMessage>, MessagingError> {
        let rows = sqlx::query_as::<_, OutboxRow>(
            r#"
            SELECT client_id, target_kind, target_id, server_scope_id, sender_id,
                   content, nonce, created_at, attempts, last_error
            FROM outbox
            ORDER BY created_at ASC
            LIMIT ?
            "#,
        )
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows.into_iter().map(Self::row_to_outbox).collect())
    }

    pub async fn update_status_by_server_id(
        &self,
        message_id: &str,
        status: MessageStatus,
    ) -> Result<(), MessagingError> {
        sqlx::query(
            "UPDATE local_messages SET status = ?, updated_at = datetime('now') WHERE server_id = ? OR local_id = ?"
        )
        .bind(status.as_str())
        .bind(message_id)
        .bind(message_id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    fn row_to_message(row: MessageRow) -> PersistedMessage {
        PersistedMessage {
            local_id: row.local_id,
            server_id: row.server_id,
            client_id: row.client_id,
            sender_id: row.sender_id,
            sender_username: row.sender_username,
            target_kind: match row.target_kind.as_str() {
                "channel" => ConversationKind::Channel,
                _ => ConversationKind::Dm,
            },
            target_id: row.target_id,
            content: row.content,
            nonce: row.nonce,
            created_at: row.created_at,
            edited_at: row.edited_at,
            status: match row.status.as_str() {
                "sending" => MessageStatus::Sending,
                "delivered" => MessageStatus::Delivered,
                "read" => MessageStatus::Read,
                "failed" => MessageStatus::Failed,
                _ => MessageStatus::Sent,
            },
        }
    }

    fn row_to_outbox(row: OutboxRow) -> OutboxMessage {
        OutboxMessage {
            client_id: row.client_id,
            target_kind: match row.target_kind.as_str() {
                "channel" => ConversationKind::Channel,
                _ => ConversationKind::Dm,
            },
            target_id: row.target_id,
            server_scope_id: row.server_scope_id,
            sender_id: row.sender_id,
            content: row.content,
            nonce: row.nonce,
            created_at: row.created_at,
            attempts: row.attempts,
            last_error: row.last_error,
        }
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
    async fn upsert_message_deduplicates_on_client_id() {
        let db_path = temp_db_path("messaging-storage-upsert-client");
        let storage = MessagingStorage::new(db_path.clone())
            .await
            .expect("storage init");

        let first = PersistedMessage {
            local_id: "local-c1".to_string(),
            server_id: None,
            client_id: Some("c1".to_string()),
            sender_id: Some("u1".to_string()),
            sender_username: Some("alice".to_string()),
            target_kind: ConversationKind::Dm,
            target_id: "room-1".to_string(),
            content: "hello".to_string(),
            nonce: None,
            created_at: "2026-02-12T00:00:00Z".to_string(),
            edited_at: None,
            status: MessageStatus::Sending,
        };

        let second = PersistedMessage {
            local_id: "msg-1".to_string(),
            server_id: Some("msg-1".to_string()),
            client_id: Some("c1".to_string()),
            sender_id: Some("u1".to_string()),
            sender_username: Some("alice".to_string()),
            target_kind: ConversationKind::Dm,
            target_id: "room-1".to_string(),
            content: "hello (ack)".to_string(),
            nonce: None,
            created_at: "2026-02-12T00:00:01Z".to_string(),
            edited_at: None,
            status: MessageStatus::Sent,
        };

        storage
            .upsert_message(&first)
            .await
            .expect("insert pending");
        storage.upsert_message(&second).await.expect("upsert ack");

        let loaded = storage
            .load_messages(ConversationKind::Dm, "room-1", None, 50)
            .await
            .expect("load messages");

        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].client_id.as_deref(), Some("c1"));
        assert_eq!(loaded[0].server_id.as_deref(), Some("msg-1"));
        assert_eq!(loaded[0].status, MessageStatus::Sent);
        assert_eq!(loaded[0].content, "hello (ack)");

        let _ = std::fs::remove_file(db_path);
    }

    #[tokio::test]
    async fn outbox_error_and_remove_work() {
        let db_path = temp_db_path("messaging-storage-outbox");
        let storage = MessagingStorage::new(db_path.clone())
            .await
            .expect("storage init");

        let outbox = OutboxMessage {
            client_id: "c2".to_string(),
            target_kind: ConversationKind::Channel,
            target_id: "ch-1".to_string(),
            server_scope_id: Some("srv-1".to_string()),
            sender_id: Some("u1".to_string()),
            content: "hello channel".to_string(),
            nonce: None,
            created_at: "2026-02-12T00:01:00Z".to_string(),
            attempts: 0,
            last_error: None,
        };

        storage
            .enqueue_outbox(&outbox)
            .await
            .expect("enqueue outbox");
        storage
            .update_outbox_error("c2", "timeout")
            .await
            .expect("update outbox error");

        let listed = storage.list_outbox(10).await.expect("list outbox");
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].attempts, 1);
        assert_eq!(listed[0].last_error.as_deref(), Some("timeout"));

        storage.remove_outbox("c2").await.expect("remove outbox");
        let listed = storage
            .list_outbox(10)
            .await
            .expect("list outbox after remove");
        assert!(listed.is_empty());

        let _ = std::fs::remove_file(db_path);
    }

    #[tokio::test]
    async fn load_messages_honors_before_cursor() {
        let db_path = temp_db_path("messaging-storage-before-cursor");
        let storage = MessagingStorage::new(db_path.clone())
            .await
            .expect("storage init");

        let m1 = PersistedMessage {
            local_id: "m1".to_string(),
            server_id: Some("m1".to_string()),
            client_id: None,
            sender_id: Some("u1".to_string()),
            sender_username: Some("alice".to_string()),
            target_kind: ConversationKind::Dm,
            target_id: "room-1".to_string(),
            content: "first".to_string(),
            nonce: None,
            created_at: "2026-02-12T00:00:00Z".to_string(),
            edited_at: None,
            status: MessageStatus::Sent,
        };

        let m2 = PersistedMessage {
            local_id: "m2".to_string(),
            server_id: Some("m2".to_string()),
            client_id: None,
            sender_id: Some("u1".to_string()),
            sender_username: Some("alice".to_string()),
            target_kind: ConversationKind::Dm,
            target_id: "room-1".to_string(),
            content: "second".to_string(),
            nonce: None,
            created_at: "2026-02-12T00:00:01Z".to_string(),
            edited_at: None,
            status: MessageStatus::Sent,
        };

        let m3 = PersistedMessage {
            local_id: "m3".to_string(),
            server_id: Some("m3".to_string()),
            client_id: None,
            sender_id: Some("u1".to_string()),
            sender_username: Some("alice".to_string()),
            target_kind: ConversationKind::Dm,
            target_id: "room-1".to_string(),
            content: "third".to_string(),
            nonce: None,
            created_at: "2026-02-12T00:00:02Z".to_string(),
            edited_at: None,
            status: MessageStatus::Sent,
        };

        storage.upsert_message(&m1).await.expect("insert m1");
        storage.upsert_message(&m2).await.expect("insert m2");
        storage.upsert_message(&m3).await.expect("insert m3");

        let page = storage
            .load_messages(ConversationKind::Dm, "room-1", None, 2)
            .await
            .expect("load page");
        assert_eq!(page.len(), 2);
        assert_eq!(page[0].local_id, "m2");
        assert_eq!(page[1].local_id, "m3");

        let older = storage
            .load_messages(ConversationKind::Dm, "room-1", Some("m2"), 2)
            .await
            .expect("load older page");
        assert_eq!(older.len(), 1);
        assert_eq!(older[0].local_id, "m1");

        let _ = std::fs::remove_file(db_path);
    }
}
