# Feature Pack (2026-02-13)

This batch adds server-side support for moderation, reactions, threading, profile settings, search, and baseline API hardening.

## Security hardening

- Request validation for auth, DM/channel message writes, friend requests, and search.
- Global in-memory rate limiting middleware with stricter limits on auth and websocket endpoints.
- Public key payload validation on `/users/me/public-key`.

## Server administration

- `DELETE /servers/:id` (owner only)
- `POST /servers/:id/invite/regenerate` (owner/admin)
- `PUT /servers/:id/members/:member_id/role` (owner only)
- `POST /servers/:id/members/:member_id/kick` (owner/admin)
- `POST /servers/:id/members/:member_id/ban` (owner/admin)
- `GET /servers/:id/bans` (owner/admin)
- `DELETE /servers/:id/bans/:member_id` (owner/admin)
- `PUT /servers/:id/channels/:channel_id` (owner/admin)
- `DELETE /servers/:id/channels/:channel_id` (owner/admin, keeps at least one text channel)

## Messaging improvements

- Reactions for DM and channel messages.
- Threading support via `parent_message_id` with thread listing/send endpoints.
- Search endpoints for DM and channel messages.
- Mention detection with websocket `MENTION_ALERT` events.

## User profile and settings

- `GET /users/me`
- `PUT /users/me` (username/avatar)
- `GET /users/me/settings`
- `PUT /users/me/settings`

## Data model changes

Migration: `apps/server/migrations/20260213000003_feature_pack.sql`

- `messages.parent_message_id`
- `message_reactions`
- `message_attachments` (metadata only)
- `user_settings`
- `server_bans`
