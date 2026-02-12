-- Add edited_at column for message editing
ALTER TABLE messages ADD COLUMN edited_at TIMESTAMPTZ DEFAULT NULL;
