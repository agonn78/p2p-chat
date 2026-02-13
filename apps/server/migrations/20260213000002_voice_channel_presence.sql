CREATE TABLE IF NOT EXISTS voice_channel_sessions (
    channel_id UUID NOT NULL REFERENCES channels(id) ON DELETE CASCADE,
    server_id UUID NOT NULL REFERENCES servers(id) ON DELETE CASCADE,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    joined_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (channel_id, user_id)
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_voice_channel_sessions_user
ON voice_channel_sessions(user_id);

CREATE INDEX IF NOT EXISTS idx_voice_channel_sessions_channel
ON voice_channel_sessions(channel_id, joined_at);

CREATE INDEX IF NOT EXISTS idx_voice_channel_sessions_server
ON voice_channel_sessions(server_id, channel_id);
