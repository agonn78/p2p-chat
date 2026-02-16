# App Updates (Server = Source of Truth)

This project now uses a server-driven update flow for the desktop app.

## Implemented architecture

### Desktop app (Tauri)

- Rust updater commands:
  - `app_check_for_updates`
  - `app_download_and_install_update`
  - `app_restart_after_update`
- Progress event emitted to frontend:
  - `app-update-download`
- Frontend service:
  - `apps/desktop/src/services/updateService.ts`
- Frontend UX:
  - `apps/desktop/src/components/AppUpdateNotice.tsx`
  - Optional update: dismissible notice + "Mettre a jour"
  - Mandatory update: blocking modal until updated
  - After install: "Redemarrer"

### Server endpoints

- `GET /app/latest?platform=windows|macos|linux&arch=x64|aarch64|x86|armv7&channel=stable|beta&currentVersion=...`
  - Returns source-of-truth JSON for app UX + policy (`mandatory`, notes, URL, signature, optional sha256).
- `GET /app/tauri/latest?target=windows|darwin|linux&arch=x86_64|aarch64|...&current_version=...&channel=...`
  - Returns Tauri updater payload (`200`) or `204` when no update is needed.

Both endpoints are backed by the same environment configuration.

## Response formats

### `/app/latest`

```json
{
  "latestVersion": "1.2.3",
  "mandatory": false,
  "notes": "Fixes and improvements",
  "pubDate": "2026-02-16T10:00:00Z",
  "url": "https://cdn.example.com/p2p-chat-setup.exe",
  "signature": "<minisign signature>",
  "sha256": "optional-sha256"
}
```

### `/app/tauri/latest`

```json
{
  "version": "1.2.3",
  "notes": "Fixes and improvements",
  "pub_date": "2026-02-16T10:00:00Z",
  "url": "https://cdn.example.com/p2p-chat-setup.exe",
  "signature": "<minisign signature>"
}
```

## Required environment variables

### Desktop app

- `TAURI_UPDATER_PUBLIC_KEY` (required for secure install)
- `APP_UPDATE_BASE_URL` (optional, defaults to `API_URL`)
- `APP_UPDATE_CHANNEL` (optional, default: `stable`)

### Server

Shared/default keys:

- `APP_LATEST_VERSION` (fallback defaults to current server package version)
- `APP_MANDATORY` (`true|false`, optional)
- `APP_NOTES` (optional)
- `APP_PUB_DATE` (optional RFC3339)

Per-platform artifact keys (required for real updates):

- `APP_UPDATE_URL_WINDOWS_X64`
- `APP_UPDATE_SIGNATURE_WINDOWS_X64`
- `APP_UPDATE_SHA256_WINDOWS_X64` (optional)
- `APP_UPDATE_URL_MACOS_X64`
- `APP_UPDATE_SIGNATURE_MACOS_X64`
- `APP_UPDATE_URL_MACOS_AARCH64`
- `APP_UPDATE_SIGNATURE_MACOS_AARCH64`

Channel-specific override support (optional):

- Prefix keys with `APP_<CHANNEL>_...`
- Example: `APP_BETA_LATEST_VERSION`, `APP_BETA_UPDATE_URL_WINDOWS_X64`

`<CHANNEL>` is normalized uppercase with `-` converted to `_`.

## Release publishing checklist

1. Generate and securely store Tauri updater signing keys.
2. Build signed updater artifacts (`.sig` required).
3. Upload artifacts + signatures to your release storage/CDN.
4. Update server env vars for latest version and per-platform URLs/signatures.
5. Set/update `TAURI_UPDATER_PUBLIC_KEY` in desktop runtime environment.
6. Restart server and verify endpoints (`/app/latest`, `/app/tauri/latest`).
7. Launch one Windows and one macOS client and confirm both detect the same target version.

## Manual validation matrix

### No update available

- Set `currentVersion == latestVersion`.
- Expected: no update prompt in UI.

### Update available (optional)

- Set higher latest version, `mandatory=false`.
- Expected: notice appears, can dismiss, install works, restart button appears.

### Update available (mandatory)

- Set `mandatory=true`.
- Expected: blocking modal, no dismiss until install/restart.

### Network failure

- Simulate update API down / timeout.
- Expected: human-readable error in notice, app remains usable (unless mandatory update already active).

### Invalid signature / wrong key

- Mismatch signature or public key.
- Expected: install fails safely, app binary not replaced.

### Asset missing (404)

- URL points to missing file.
- Expected: download/install fails with clear error.

### Cross-platform convergence

- Windows + macOS clients check updates against same channel.
- Expected: both resolve to same semantic target version for their own artifact URL.
