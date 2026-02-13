# Voice Calls and Voice Channel Presence

## WebSocket authentication for signaling

- Desktop now sends `identify` with both `user_id` and JWT token.
- Server validates the token and rejects identify payloads where token subject does not match `user_id`.
- This prevents spoofing another user id over the signaling socket.

## Voice call reliability

- Server tracks two call states:
  - `pending` (ringing)
  - `active` (accepted)
- Ringing calls expire after 30 seconds if not accepted.
- New `call_unavailable` signaling event is emitted for:
  - offline target
  - expired ringing call
  - peer disconnected while ringing

## ICE/TURN configuration

Desktop media engine reads ICE settings from environment at runtime.

Supported variables:

- `ICE_SERVERS_JSON`: JSON array of ICE servers.
  - Example:
    ```json
    [
      {"urls":["stun:stun.l.google.com:19302"]},
      {"urls":["turn:turn.example.com:3478?transport=udp"],"username":"user","credential":"pass"}
    ]
    ```
- `STUN_URLS`: comma-separated fallback STUN URLs.
- `TURN_URLS`: comma-separated TURN URLs.
- `TURN_USERNAME`: TURN username.
- `TURN_PASSWORD` or `TURN_CREDENTIAL`: TURN credential.

If no environment is set, fallback is:

- `stun:stun.l.google.com:19302`

## Voice channels (server channels)

Server exposes presence endpoints:

- `GET /servers/:id/channels/:channel_id/voice`
- `POST /servers/:id/channels/:channel_id/voice/join`
- `POST /servers/:id/channels/:channel_id/voice/leave`

Presence updates are pushed by websocket with event type:

- `VOICE_PRESENCE` with payload fields `server_id`, `channel_id`, `user_id`, `joined`.
