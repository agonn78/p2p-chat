#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" >/dev/null 2>&1 && pwd)"
PROJECT_ROOT="$(cd -- "$SCRIPT_DIR/.." >/dev/null 2>&1 && pwd)"
DESKTOP_DIR="$PROJECT_ROOT/apps/desktop"
TAURI_CONF="$DESKTOP_DIR/src-tauri/tauri.conf.json"

DRY_RUN=1
DO_REINSTALL=0
DO_DEV=0
DO_BUILD=0
PURGE_APP_DATA=0

APP_IDENTIFIER=""
APP_PRODUCT_NAME=""

declare -a REMOVED

usage() {
    cat <<'USAGE'
Usage:
  clean-macos.sh [--dry-run] [--clean] [--reinstall] [--dev|--build] [--purge-app-data]

Modes:
  --dry-run         Show what would be deleted (default)
  --clean           Delete caches for real
  --reinstall       Run npm ci after cleaning
  --dev             Run npm run tauri dev after cleaning
  --build           Run npm run tauri build after cleaning
  --purge-app-data  Also delete app data/cache directories for this Tauri app

Examples:
  ./scripts/clean-macos.sh --dry-run
  ./scripts/clean-macos.sh --clean
  ./scripts/clean-macos.sh --clean --purge-app-data
  ./scripts/clean-macos.sh --clean --reinstall --dev
USAGE
}

while [[ $# -gt 0 ]]; do
    case "$1" in
        --dry-run)
            DRY_RUN=1
            ;;
        --clean)
            DRY_RUN=0
            ;;
        --reinstall)
            DO_REINSTALL=1
            ;;
        --dev)
            DO_DEV=1
            ;;
        --build)
            DO_BUILD=1
            ;;
        --purge-app-data)
            PURGE_APP_DATA=1
            ;;
        -h|--help)
            usage
            exit 0
            ;;
        *)
            echo "Unknown option: $1" >&2
            usage
            exit 1
            ;;
    esac
    shift
done

if [[ "$DO_DEV" -eq 1 && "$DO_BUILD" -eq 1 ]]; then
    echo "Error: --dev and --build are mutually exclusive." >&2
    exit 1
fi

if [[ "$DRY_RUN" -eq 1 && ("$DO_REINSTALL" -eq 1 || "$DO_DEV" -eq 1 || "$DO_BUILD" -eq 1) ]]; then
    echo "Error: --reinstall/--dev/--build require --clean." >&2
    exit 1
fi

if [[ ! -d "$DESKTOP_DIR" ]]; then
    echo "Error: expected desktop app at $DESKTOP_DIR" >&2
    exit 1
fi

read_tauri_identity() {
    if [[ ! -f "$TAURI_CONF" ]]; then
        echo "Error: missing Tauri config at $TAURI_CONF" >&2
        return 1
    fi

    if ! command -v node >/dev/null 2>&1; then
        echo "Error: node is required to parse tauri.conf.json for --purge-app-data" >&2
        return 1
    fi

    local meta
    meta="$(node -e "const fs=require('fs'); const conf=JSON.parse(fs.readFileSync(process.argv[1],'utf8')); const id=(conf.identifier||'').trim(); const name=(conf.productName||'').trim(); console.log(id); console.log(name);" "$TAURI_CONF")"
    APP_IDENTIFIER="$(printf '%s\n' "$meta" | sed -n '1p')"
    APP_PRODUCT_NAME="$(printf '%s\n' "$meta" | sed -n '2p')"

    if [[ -z "$APP_IDENTIFIER" || -z "$APP_PRODUCT_NAME" ]]; then
        echo "Error: could not read identifier/productName from $TAURI_CONF" >&2
        return 1
    fi
}

is_safe_project_path() {
    local path="$1"
    [[ "$path" == "$PROJECT_ROOT/"* ]]
}

is_safe_appdata_path() {
    local path="$1"
    [[ "$path" == "$HOME/Library/Application Support/"* || "$path" == "$HOME/Library/Caches/"* || "$path" == "$HOME/Library/WebKit/"* ]]
}

record_removed() {
    local path="$1"
    local reason="$2"
    REMOVED+=("$path|$reason")
}

remove_path() {
    local path="$1"
    local reason="$2"
    local scope="$3"

    if [[ ! -e "$path" ]]; then
        return 0
    fi

    if [[ "$scope" == "project" ]]; then
        if ! is_safe_project_path "$path"; then
            echo "[skip] unsafe project path: $path"
            return 0
        fi
    elif [[ "$scope" == "appdata" ]]; then
        if ! is_safe_appdata_path "$path"; then
            echo "[skip] unsafe app data path: $path"
            return 0
        fi
    fi

    if [[ "$DRY_RUN" -eq 1 ]]; then
        echo "[dry-run] would remove: $path ($reason)"
    else
        echo "[clean] removing: $path ($reason)"
        rm -rf -- "$path"
    fi
    record_removed "$path" "$reason"
}

echo "Project root: $PROJECT_ROOT"
if [[ "$DRY_RUN" -eq 1 ]]; then
    echo "Mode: dry-run"
else
    echo "Mode: clean"
fi

declare -a PROJECT_TARGETS=(
    "$DESKTOP_DIR/node_modules|Node dependencies"
    "$DESKTOP_DIR/.vite|Vite cache"
    "$DESKTOP_DIR/node_modules/.vite|Vite pre-bundle cache"
    "$DESKTOP_DIR/dist|Frontend build output"
    "$DESKTOP_DIR/build|Alternate build output"
    "$DESKTOP_DIR/.turbo|Turbo cache"
    "$DESKTOP_DIR/.parcel-cache|Parcel cache"
    "$DESKTOP_DIR/.cache|Generic local cache"
    "$DESKTOP_DIR/.eslintcache|ESLint cache"
    "$DESKTOP_DIR/src-tauri/target|Tauri target artifacts"
    "$PROJECT_ROOT/target|Workspace target artifacts"
)

for item in "${PROJECT_TARGETS[@]}"; do
    path="${item%%|*}"
    reason="${item#*|}"
    remove_path "$path" "$reason" "project"
done

if [[ "$PURGE_APP_DATA" -eq 1 ]]; then
    read_tauri_identity
    echo "Detected app identifier: $APP_IDENTIFIER"
    echo "Detected product name: $APP_PRODUCT_NAME"

    declare -a APP_NAMES=("$APP_IDENTIFIER" "$APP_PRODUCT_NAME")
    for app_name in "${APP_NAMES[@]}"; do
        remove_path "$HOME/Library/Application Support/$app_name" "App data" "appdata"
        remove_path "$HOME/Library/Caches/$app_name" "App cache" "appdata"
        remove_path "$HOME/Library/WebKit/$app_name" "WebView cache" "appdata"
    done
fi

if [[ ${#REMOVED[@]} -eq 0 ]]; then
    echo "No cache paths found to clean."
else
    echo
    echo "Affected paths (${#REMOVED[@]}):"
    for entry in "${REMOVED[@]}"; do
        echo "- ${entry%%|*} [${entry#*|}]"
    done
fi

if [[ "$DRY_RUN" -eq 1 ]]; then
    echo
    echo "Dry-run complete. Re-run with --clean to apply deletion."
    exit 0
fi

if [[ "$DO_REINSTALL" -eq 1 || "$DO_DEV" -eq 1 || "$DO_BUILD" -eq 1 ]]; then
    if ! command -v npm >/dev/null 2>&1; then
        echo "Error: npm is required for --reinstall/--dev/--build" >&2
        exit 1
    fi
fi

if [[ "$DO_REINSTALL" -eq 1 ]]; then
    echo
    echo "[step] npm ci"
    (cd "$DESKTOP_DIR" && npm ci)
fi

if [[ "$DO_DEV" -eq 1 ]]; then
    echo
    echo "[step] npm run tauri dev"
    (cd "$DESKTOP_DIR" && npm run tauri dev)
fi

if [[ "$DO_BUILD" -eq 1 ]]; then
    echo
    echo "[step] npm run tauri build"
    (cd "$DESKTOP_DIR" && npm run tauri build)
fi

echo
echo "Clean workflow completed successfully."
