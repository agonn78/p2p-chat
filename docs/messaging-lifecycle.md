# Messaging Lifecycle

This document explains how message send/fetch/retry flows work in the desktop app.

## Send Flow (DM + Channel)

1. Frontend creates an optimistic message with `status: sending` and a `client_id`.
2. Tauri API stores a local pending row and enqueues an outbox item in SQLite.
3. Tauri sends the HTTP request to the server using that same `client_id`.
4. On success, Tauri updates local cache to server-backed message (`server_id`) and removes outbox item.
5. On failure, Tauri increments outbox attempts and marks local message as `failed`.

## Status Flow

- Initial local status: `sending`
- Server ack status: `sent`
- Receiver delivery event: `delivered`
- Receiver read event: `read`
- Local send failure: `failed`

Frontend applies monotonic status updates (no regression, for example `read` will not drop back to `delivered`).

## Fetch Flow

1. Tauri tries to fetch from server (cursor + limit).
2. If successful, remote messages are cached into local SQLite.
3. Tauri returns cached messages when available (includes local unsent/pending state).
4. If network fetch fails, Tauri falls back to local SQLite cache.

## Outbox Retry

- Outbox is drained by calling `api_drain_outbox`.
- The frontend triggers this:
  - after authentication/startup
  - after websocket reconnect
- Retries are deduplicated server-side via `client_id`.

## Cursor/Pagination Notes

- Initial page targets latest messages (`limit=100`).
- Older pages are fetched using `before=<message_id>`.
- UI preserves scroll anchor when prepending older messages.
