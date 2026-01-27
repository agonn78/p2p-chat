-- Make room_id nullable to support channel messages that don't belong to a DM room
ALTER TABLE messages ALTER COLUMN room_id DROP NOT NULL;
