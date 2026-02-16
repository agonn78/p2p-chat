# Cache Reset Guide (Windows + macOS)

This repository now includes safe cleanup scripts for desktop/Tauri cache divergence issues.

## Quick commands

Run from `apps/desktop` (recommended):

- Dry run (show only): `npm run clean`
- Hard clean: `npm run clean:hard`
- Hard clean + app data purge: `npm run clean:purge`
- Clean + reinstall + run dev: `npm run fresh:dev`
- Clean + reinstall + build: `npm run fresh:build`

Run scripts directly from repo root:

- macOS: `./scripts/clean-macos.sh --clean --reinstall --dev`
- Windows: `powershell -NoProfile -ExecutionPolicy Bypass -File .\scripts\clean-windows.ps1 --clean --reinstall --dev`

## Supported flags

- `--dry-run`: list files/folders that would be removed (default mode)
- `--clean`: apply deletion
- `--reinstall`: run `npm ci` after cleanup
- `--dev`: run `npm run tauri dev` after cleanup
- `--build`: run `npm run tauri build` after cleanup
- `--purge-app-data`: additionally delete app data/cache dirs detected from `apps/desktop/src-tauri/tauri.conf.json` (`identifier` + `productName`)

`--dev` and `--build` are mutually exclusive.

## What gets deleted

| Path / group | Why it is removed | What it resets |
| --- | --- | --- |
| `apps/desktop/node_modules` | JS dependency drift | Fresh dependency tree |
| `apps/desktop/.vite`, `apps/desktop/node_modules/.vite` | Vite transform cache | Re-bundling consistency |
| `apps/desktop/dist`, `apps/desktop/build` | old build outputs | Clean frontend artifacts |
| `apps/desktop/.turbo`, `apps/desktop/.parcel-cache`, `apps/desktop/.cache`, `apps/desktop/.eslintcache` | tool caches | deterministic lint/build behavior |
| `apps/desktop/src-tauri/target`, `target` | Rust/Tauri artifacts | full Rust/Tauri rebuild |
| App data paths (`--purge-app-data` only) | local persisted app state | local DB/store/session + webview caches |

## App data purge scope

Purge is **opt-in** and only removes directories for the current app (from Tauri config):

- macOS roots:
  - `~/Library/Application Support/<identifier|productName>`
  - `~/Library/Caches/<identifier|productName>`
  - `~/Library/WebKit/<identifier|productName>`
- Windows roots:
  - `%APPDATA%\<identifier|productName>`
  - `%LOCALAPPDATA%\<identifier|productName>`
  - `%LOCALAPPDATA%\<identifier|productName>.WebView2`

The scripts include safety guards to avoid deleting outside project/app-data roots.
