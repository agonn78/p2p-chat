use axum::{
    extract::Query,
    http::StatusCode,
    response::IntoResponse,
    routing::get,
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;

use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/latest", get(latest_release))
        .route("/tauri/latest", get(tauri_latest_release))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LatestQuery {
    platform: String,
    arch: String,
    channel: Option<String>,
    current_version: Option<String>,
}

#[derive(Debug, Deserialize)]
struct TauriLatestQuery {
    target: String,
    arch: String,
    current_version: Option<String>,
    channel: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct LatestResponse {
    latest_version: String,
    mandatory: bool,
    notes: Option<String>,
    pub_date: Option<String>,
    url: String,
    signature: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    sha256: Option<String>,
}

#[derive(Debug, Serialize)]
struct TauriResponse {
    version: String,
    notes: Option<String>,
    pub_date: Option<String>,
    url: String,
    signature: String,
}

#[derive(Debug, Serialize)]
struct UpdateErrorBody {
    code: &'static str,
    message: String,
}

#[derive(Debug)]
struct ReleaseConfig {
    latest_version: String,
    mandatory: bool,
    notes: Option<String>,
    pub_date: Option<String>,
    url: String,
    signature: String,
    sha256: Option<String>,
}

async fn latest_release(Query(query): Query<LatestQuery>) -> impl IntoResponse {
    let platform = match normalize_platform(&query.platform) {
        Some(value) => value,
        None => {
            return update_error(
                StatusCode::BAD_REQUEST,
                "invalid_platform",
                format!("Unsupported platform '{}'. Use windows|macos|linux", query.platform),
            )
        }
    };

    let arch = match normalize_arch(&query.arch) {
        Some(value) => value,
        None => {
            return update_error(
                StatusCode::BAD_REQUEST,
                "invalid_arch",
                format!(
                    "Unsupported arch '{}'. Use x64|aarch64|x86|armv7",
                    query.arch
                ),
            )
        }
    };
    let channel = normalize_channel(query.channel.as_deref());

    let release = match resolve_release_config(&channel, platform, arch) {
        Ok(value) => value,
        Err(message) => {
            return update_error(StatusCode::SERVICE_UNAVAILABLE, "update_not_configured", message)
        }
    };

    let current = query.current_version.unwrap_or_else(|| "unknown".to_string());
    let update_available = is_update_available(&current, &release.latest_version);

    tracing::info!(
        component = "app.update",
        channel = %channel,
        platform = %platform,
        arch = %arch,
        current_version = %current,
        latest_version = %release.latest_version,
        mandatory = release.mandatory,
        update_available,
        "served app latest metadata"
    );

    (
        StatusCode::OK,
        Json(LatestResponse {
            latest_version: release.latest_version,
            mandatory: release.mandatory,
            notes: release.notes,
            pub_date: release.pub_date,
            url: release.url,
            signature: release.signature,
            sha256: release.sha256,
        }),
    )
        .into_response()
}

async fn tauri_latest_release(Query(query): Query<TauriLatestQuery>) -> impl IntoResponse {
    let platform = match normalize_platform(&query.target) {
        Some(value) => value,
        None => {
            return update_error(
                StatusCode::BAD_REQUEST,
                "invalid_target",
                format!("Unsupported target '{}'. Use windows|darwin|linux", query.target),
            )
        }
    };

    let arch = match normalize_arch(&query.arch) {
        Some(value) => value,
        None => {
            return update_error(
                StatusCode::BAD_REQUEST,
                "invalid_arch",
                format!(
                    "Unsupported arch '{}'. Use x64|aarch64|x86|armv7",
                    query.arch
                ),
            )
        }
    };
    let channel = normalize_channel(query.channel.as_deref());
    let current_version = query.current_version.unwrap_or_else(|| "0.0.0".to_string());

    let release = match resolve_release_config(&channel, platform, arch) {
        Ok(value) => value,
        Err(message) => {
            return update_error(StatusCode::SERVICE_UNAVAILABLE, "update_not_configured", message)
        }
    };

    if !is_update_available(&current_version, &release.latest_version) {
        return StatusCode::NO_CONTENT.into_response();
    }

    tracing::info!(
        component = "app.update",
        channel = %channel,
        platform = %platform,
        arch = %arch,
        current_version = %current_version,
        latest_version = %release.latest_version,
        "served tauri updater metadata"
    );

    (
        StatusCode::OK,
        Json(TauriResponse {
            version: release.latest_version,
            notes: release.notes,
            pub_date: release.pub_date,
            url: release.url,
            signature: release.signature,
        }),
    )
        .into_response()
}

fn update_error(status: StatusCode, code: &'static str, message: String) -> axum::response::Response {
    (status, Json(UpdateErrorBody { code, message })).into_response()
}

fn normalize_platform(value: &str) -> Option<&'static str> {
    match value.trim().to_ascii_lowercase().as_str() {
        "windows" | "win" => Some("windows"),
        "macos" | "mac" | "darwin" | "osx" => Some("macos"),
        "linux" => Some("linux"),
        _ => None,
    }
}

fn normalize_arch(value: &str) -> Option<&'static str> {
    match value.trim().to_ascii_lowercase().as_str() {
        "x86_64" | "amd64" | "x64" => Some("x64"),
        "aarch64" | "arm64" => Some("aarch64"),
        "i686" | "x86" => Some("x86"),
        "armv7" => Some("armv7"),
        _ => None,
    }
}

fn normalize_channel(value: Option<&str>) -> String {
    let normalized = value
        .unwrap_or("stable")
        .trim()
        .to_ascii_lowercase()
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-')
        .collect::<String>();

    if normalized.is_empty() {
        "stable".to_string()
    } else {
        normalized
    }
}

fn resolve_release_config(channel: &str, platform: &str, arch: &str) -> Result<ReleaseConfig, String> {
    let channel_env = channel.to_ascii_uppercase().replace('-', "_");
    let latest_version = read_channel_value(channel, "LATEST_VERSION")
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| env!("CARGO_PKG_VERSION").to_string());

    let mandatory = read_channel_value(channel, "MANDATORY")
        .map(|value| parse_bool(&value))
        .unwrap_or(false);

    let notes = read_channel_value(channel, "NOTES").filter(|value| !value.trim().is_empty());
    let pub_date = read_channel_value(channel, "PUB_DATE").filter(|value| !value.trim().is_empty());

    let suffix = format!("{}_{}", platform.to_ascii_uppercase(), arch.to_ascii_uppercase());

    let url_key = format!("UPDATE_URL_{suffix}");
    let signature_key = format!("UPDATE_SIGNATURE_{suffix}");
    let sha_key = format!("UPDATE_SHA256_{suffix}");

    let url = read_channel_value(channel, &url_key)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| {
            format!(
                "Missing update URL for platform {platform}-{arch}. Set APP_UPDATE_URL_{suffix} or APP_{}_UPDATE_URL_{suffix}",
                channel_env
            )
        })?;

    let signature = read_channel_value(channel, &signature_key)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| {
            format!(
                "Missing update signature for platform {platform}-{arch}. Set APP_UPDATE_SIGNATURE_{suffix} or APP_{}_UPDATE_SIGNATURE_{suffix}",
                channel_env
            )
        })?;

    let sha256 = read_channel_value(channel, &sha_key).filter(|value| !value.trim().is_empty());

    Ok(ReleaseConfig {
        latest_version,
        mandatory,
        notes,
        pub_date,
        url,
        signature,
        sha256,
    })
}

fn read_channel_value(channel: &str, key: &str) -> Option<String> {
    let channel_env = channel.to_ascii_uppercase().replace('-', "_");
    let scoped_key = format!("APP_{}_{}", channel_env, key);
    std::env::var(&scoped_key)
        .ok()
        .or_else(|| std::env::var(format!("APP_{key}")).ok())
}

fn parse_bool(value: &str) -> bool {
    matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "1" | "true" | "yes" | "y" | "on"
    )
}

fn is_update_available(current_version: &str, latest_version: &str) -> bool {
    compare_versions(current_version, latest_version) == Ordering::Less
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
    fn compares_versions_correctly() {
        assert!(is_update_available("1.0.0", "1.0.1"));
        assert!(is_update_available("1.2.9", "1.3.0"));
        assert!(!is_update_available("1.3.0", "1.2.9"));
        assert!(!is_update_available("1.3.0", "1.3.0"));
    }

    #[test]
    fn prerelease_is_older_than_release() {
        assert!(is_update_available("1.2.3-beta.1", "1.2.3"));
        assert!(!is_update_available("1.2.3", "1.2.3-beta.1"));
    }

    #[test]
    fn normalizes_platform_and_arch() {
        assert_eq!(normalize_platform("darwin"), Some("macos"));
        assert_eq!(normalize_platform("windows"), Some("windows"));
        assert_eq!(normalize_arch("x86_64"), Some("x64"));
        assert_eq!(normalize_arch("arm64"), Some("aarch64"));
    }
}
