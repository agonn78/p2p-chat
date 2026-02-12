-- Client-generated id for dedup/retry support
ALTER TABLE messages
ADD COLUMN IF NOT EXISTS client_id UUID;

-- Unique per sender/client id to prevent duplicate inserts on retries
CREATE UNIQUE INDEX IF NOT EXISTS idx_messages_sender_client_id
ON messages(sender_id, client_id)
WHERE client_id IS NOT NULL;

-- Delivery/read receipts (works for both DM and channel messages)
CREATE TABLE IF NOT EXISTS message_receipts (
    message_id UUID NOT NULL REFERENCES messages(id) ON DELETE CASCADE,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    delivered_at TIMESTAMPTZ,
    read_at TIMESTAMPTZ,
    PRIMARY KEY (message_id, user_id)
);

CREATE INDEX IF NOT EXISTS idx_message_receipts_user
ON message_receipts(user_id, read_at);

CREATE INDEX IF NOT EXISTS idx_message_receipts_message
ON message_receipts(message_id);
