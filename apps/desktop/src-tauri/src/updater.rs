use crate::config;
use crate::error::{AppError, AppResult};
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::sync::Mutex;
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_updater::{Update, UpdaterExt};

const UPDATE_EVENT_NAME: &str = "app-update-download";

#[derive(Default)]
pub struct PendingUpdate(pub Mutex<Option<Update>>);

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AvailableUpdate {
    pub current_version: String,
    pub latest_version: String,
    pub mandatory: bool,
    pub notes: Option<String>,
    pub pub_date: Option<String>,
    pub channel: String,
    pub platform: String,
    pub arch: String,
    pub url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sha256: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "event", content = "data")]
pub enum DownloadEvent {
    #[serde(rename_all = "camelCase")]
    Started { content_length: Option<u64> },
    #[serde(rename_all = "camelCase")]
    Progress {
        chunk_length: usize,
        downloaded: u64,
        content_length: Option<u64>,
    },
    Finished,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LatestResponse {
    latest_version: String,
    mandatory: bool,
    notes: Option<String>,
    pub_date: Option<String>,
    url: String,
    signature: String,
    sha256: Option<String>,
}

#[tauri::command]
pub async fn app_check_for_updates(
    app: AppHandle,
    pending_update: State<'_, PendingUpdate>,
) -> AppResult<Option<AvailableUpdate>> {
    let channel = update_channel();
    let platform = current_platform().to_string();
    let arch = current_arch().to_string();
    let current_version = app.package_info().version.to_string();

    let latest = fetch_latest_metadata(&channel, &platform, &arch, &current_version).await?;
    if compare_versions(&current_version, &latest.latest_version) != Ordering::Less {
        clear_pending_update(&pending_update)?;
        tracing::info!(
            component = "updater",
            channel = %channel,
            platform = %platform,
            arch = %arch,
            current_version = %current_version,
            latest_version = %latest.latest_version,
            "no update required"
        );
        return Ok(None);
    }

    if latest.url.trim().is_empty() || latest.signature.trim().is_empty() {
        return Err(
            AppError::protocol("Update metadata is missing required URL or signature fields")
                .with_details("Server must provide url + signature for updater"),
        );
    }

    let pubkey = updater_pubkey().ok_or_else(|| {
        AppError::validation(
            "TAURI_UPDATER_PUBLIC_KEY is not configured; update verification cannot proceed",
        )
    })?;

    let update_endpoint = url::Url::parse(&format!(
        "{}/app/tauri/latest?target={{{{target}}}}&arch={{{{arch}}}}&current_version={{{{current_version}}}}&channel={}",
        update_base_url(),
        channel
    ))
    .map_err(|err| {
        AppError::validation("Invalid updater endpoint URL").with_details(err.to_string())
    })?;

    let update = app
        .updater_builder()
        .pubkey(pubkey)
        .endpoints(vec![update_endpoint])
        .map_err(|err| {
            AppError::validation("Invalid updater endpoint configuration")
                .with_details(err.to_string())
        })?
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|err| {
            AppError::internal("Failed to initialize updater builder")
                .with_details(err.to_string())
        })?
        .check()
        .await
        .map_err(|err| {
            AppError::network("Update check failed")
                .with_details(format_updater_error("check", &err))
        })?;

    let Some(update) = update else {
        clear_pending_update(&pending_update)?;
        tracing::info!(
            component = "updater",
            channel = %channel,
            platform = %platform,
            arch = %arch,
            current_version = %current_version,
            latest_version = %latest.latest_version,
            "updater endpoint reported no update"
        );
        return Ok(None);
    };

    if normalize_version(&update.version) != normalize_version(&latest.latest_version) {
        return Err(AppError::protocol("Server version mismatch between latest metadata and updater")
            .with_details(format!(
                "latest endpoint returned {}, updater endpoint returned {}",
                latest.latest_version, update.version
            )));
    }

    let result = AvailableUpdate {
        current_version,
        latest_version: latest.latest_version,
        mandatory: latest.mandatory,
        notes: latest.notes,
        pub_date: latest.pub_date,
        channel,
        platform,
        arch,
        url: latest.url,
        sha256: latest.sha256,
    };

    {
        let mut slot = pending_update
            .0
            .lock()
            .map_err(|_| AppError::internal("Updater state lock poisoned"))?;
        *slot = Some(update);
    }

    tracing::info!(
        component = "updater",
        current_version = %result.current_version,
        latest_version = %result.latest_version,
        channel = %result.channel,
        platform = %result.platform,
        arch = %result.arch,
        mandatory = result.mandatory,
        "update available"
    );

    Ok(Some(result))
}

#[tauri::command]
pub async fn app_download_and_install_update(
    app: AppHandle,
    pending_update: State<'_, PendingUpdate>,
) -> AppResult<()> {
    let update = {
        let mut slot = pending_update
            .0
            .lock()
            .map_err(|_| AppError::internal("Updater state lock poisoned"))?;
        slot.take().ok_or_else(|| {
            AppError::validation("No pending update. Call checkForUpdates() first")
        })?
    };

    tracing::info!(component = "updater", "starting update download and install");

    let mut started = false;
    let mut downloaded = 0_u64;
    let app_handle = app.clone();

    update
        .download_and_install(
            |chunk_length, content_length| {
                downloaded = downloaded.saturating_add(chunk_length as u64);

                if !started {
                    started = true;
                    let _ = app_handle.emit(
                        UPDATE_EVENT_NAME,
                        DownloadEvent::Started { content_length },
                    );
                }

                let _ = app_handle.emit(
                    UPDATE_EVENT_NAME,
                    DownloadEvent::Progress {
                        chunk_length,
                        downloaded,
                        content_length,
                    },
                );
            },
            || {
                let _ = app_handle.emit(UPDATE_EVENT_NAME, DownloadEvent::Finished);
            },
        )
        .await
        .map_err(|err| {
            AppError::network("Failed to download or install update")
                .with_details(format_updater_error("download_and_install", &err))
        })?;

    tracing::info!(component = "updater", "update downloaded and installed");
    Ok(())
}

#[tauri::command]
pub fn app_restart_after_update(app: AppHandle) -> AppResult<()> {
    tracing::info!(component = "updater", "restarting app after update install");
    tauri::process::restart(&app.env());
}

async fn fetch_latest_metadata(
    channel: &str,
    platform: &str,
    arch: &str,
    current_version: &str,
) -> AppResult<LatestResponse> {
    let mut latest_url = url::Url::parse(&format!("{}/app/latest", update_base_url()))
        .map_err(|err| AppError::validation("Invalid update server URL").with_details(err.to_string()))?;

    latest_url
        .query_pairs_mut()
        .append_pair("platform", platform)
        .append_pair("arch", arch)
        .append_pair("channel", channel)
        .append_pair("currentVersion", current_version);

    let response = reqwest::Client::new()
        .get(latest_url.clone())
        .timeout(Duration::from_secs(15))
        .send()
        .await
        .map_err(|err| {
            AppError::network("Unable to reach update metadata endpoint").with_details(err.to_string())
        })?;

    if response.status() == reqwest::StatusCode::NO_CONTENT {
        return Err(AppError::protocol("Update metadata endpoint returned no content")
            .with_details("Expected latest metadata JSON payload"));
    }

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(
            AppError::network(format!("Update metadata check failed with HTTP {status}"))
                .with_details(body),
        );
    }

    response
        .json::<LatestResponse>()
        .await
        .map_err(|err| AppError::protocol("Invalid update metadata payload").with_details(err.to_string()))
}

fn clear_pending_update(pending_update: &State<'_, PendingUpdate>) -> AppResult<()> {
    let mut slot = pending_update
        .0
        .lock()
        .map_err(|_| AppError::internal("Updater state lock poisoned"))?;
    *slot = None;
    Ok(())
}

fn update_base_url() -> String {
    std::env::var("APP_UPDATE_BASE_URL")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| config::API_URL.to_string())
        .trim_end_matches('/')
        .to_string()
}

fn update_channel() -> String {
    let value = std::env::var("APP_UPDATE_CHANNEL")
        .ok()
        .unwrap_or_else(|| "stable".to_string());

    let normalized: String = value
        .to_ascii_lowercase()
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric() || *ch == '-')
        .collect();

    if normalized.is_empty() {
        "stable".to_string()
    } else {
        normalized
    }
}

fn updater_pubkey() -> Option<String> {
    std::env::var("TAURI_UPDATER_PUBLIC_KEY")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn current_platform() -> &'static str {
    match std::env::consts::OS {
        "windows" => "windows",
        "macos" => "macos",
        "linux" => "linux",
        _ => "linux",
    }
}

fn current_arch() -> &'static str {
    match std::env::consts::ARCH {
        "x86_64" => "x64",
        "aarch64" => "aarch64",
        "x86" => "x86",
        "armv7" => "armv7",
        _ => "x64",
    }
}

fn format_updater_error(context: &str, err: &tauri_plugin_updater::Error) -> String {
    format!("{context}: {err}")
}

fn normalize_version(value: &str) -> String {
    value.trim().trim_start_matches('v').to_string()
}

fn compare_versions(current: &str, latest: &str) -> Ordering {
    let Some(current_parts) = parse_version(current) else {
        return Ordering::Equal;
    };
    let Some(latest_parts) = parse_version(latest) else {
        return Ordering::Equal;
    };

    let core_cmp = current_parts.core.cmp(&latest_parts.core);
    if core_cmp != Ordering::Equal {
        return core_cmp;
    }

    match (&current_parts.pre, &latest_parts.pre) {
        (None, None) => Ordering::Equal,
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        (Some(current_pre), Some(latest_pre)) => current_pre.cmp(latest_pre),
    }
}

#[derive(Debug)]
struct ParsedVersion {
    core: [u64; 3],
    pre: Option<String>,
}

fn parse_version(raw: &str) -> Option<ParsedVersion> {
    let normalized = raw.trim().trim_start_matches('v');
    if normalized.is_empty() {
        return None;
    }

    let mut split = normalized.splitn(2, '-');
    let core = split.next()?;
    let pre = split.next().map(|value| value.to_string());

    let mut parts = [0_u64; 3];
    for (index, part) in core.split('.').take(3).enumerate() {
        if part.is_empty() {
            return None;
        }
        parts[index] = part.parse::<u64>().ok()?;
    }

    Some(ParsedVersion { core: parts, pre })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compare_versions_handles_patch_updates() {
        assert_eq!(compare_versions("1.0.0", "1.0.1"), Ordering::Less);
        assert_eq!(compare_versions("1.0.1", "1.0.0"), Ordering::Greater);
        assert_eq!(compare_versions("1.0.1", "1.0.1"), Ordering::Equal);
    }

    #[test]
    fn compare_versions_handles_prerelease() {
        assert_eq!(compare_versions("1.2.3-beta.1", "1.2.3"), Ordering::Less);
        assert_eq!(compare_versions("1.2.3", "1.2.3-beta.1"), Ordering::Greater);
    }
}
