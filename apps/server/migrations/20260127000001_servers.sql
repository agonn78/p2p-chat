-- Servers table
CREATE TABLE servers (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name VARCHAR(100) NOT NULL,
    icon_url TEXT,
    owner_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    invite_code VARCHAR(8) UNIQUE NOT NULL,
    created_at TIMESTAMPTZ DEFAULT NOW()
);

-- Channels (text or voice)
CREATE TABLE channels (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    server_id UUID NOT NULL REFERENCES servers(id) ON DELETE CASCADE,
    name VARCHAR(100) NOT NULL,
    channel_type VARCHAR(10) NOT NULL DEFAULT 'text',
    position INT DEFAULT 0,
    created_at TIMESTAMPTZ DEFAULT NOW()
);

-- Server Members
CREATE TABLE server_members (
    server_id UUID NOT NULL REFERENCES servers(id) ON DELETE CASCADE,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    role VARCHAR(20) NOT NULL DEFAULT 'member',
    joined_at TIMESTAMPTZ DEFAULT NOW(),
    PRIMARY KEY (server_id, user_id)
);

-- Add channel_id to messages for server channel messages
ALTER TABLE messages ADD COLUMN channel_id UUID REFERENCES channels(id) ON DELETE CASCADE;
ALTER TABLE messages ALTER COLUMN room_id DROP NOT NULL;

-- Indexes
CREATE INDEX idx_servers_owner ON servers(owner_id);
CREATE INDEX idx_channels_server ON channels(server_id);
CREATE INDEX idx_server_members_server ON server_members(server_id);
CREATE INDEX idx_server_members_user ON server_members(user_id);
CREATE INDEX idx_messages_channel ON messages(channel_id, created_at DESC);
