use serde::Serialize;

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AppErrorCode {
    Network,
    Protocol,
    Auth,
    Storage,
    Validation,
    Internal,
}

#[derive(Debug, Clone, Serialize)]
pub struct AppError {
    pub code: AppErrorCode,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trace_id: Option<String>,
}

pub type AppResult<T> = Result<T, AppError>;

impl AppError {
    pub fn new(code: AppErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            details: None,
            trace_id: Some(crate::observability::trace_id().to_string()),
        }
    }

    pub fn with_details(mut self, details: impl Into<String>) -> Self {
        self.details = Some(details.into());
        self
    }

    pub fn network(message: impl Into<String>) -> Self {
        Self::new(AppErrorCode::Network, message)
    }

    pub fn protocol(message: impl Into<String>) -> Self {
        Self::new(AppErrorCode::Protocol, message)
    }

    pub fn auth(message: impl Into<String>) -> Self {
        Self::new(AppErrorCode::Auth, message)
    }

    pub fn storage(message: impl Into<String>) -> Self {
        Self::new(AppErrorCode::Storage, message)
    }

    pub fn validation(message: impl Into<String>) -> Self {
        Self::new(AppErrorCode::Validation, message)
    }

    pub fn internal(message: impl Into<String>) -> Self {
        Self::new(AppErrorCode::Internal, message)
    }
}

impl std::fmt::Display for AppError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for AppError {}

impl From<&str> for AppError {
    fn from(value: &str) -> Self {
        from_string(value.to_string())
    }
}

impl From<String> for AppError {
    fn from(value: String) -> Self {
        from_string(value)
    }
}

impl From<reqwest::Error> for AppError {
    fn from(value: reqwest::Error) -> Self {
        AppError::network("Network request failed").with_details(value.to_string())
    }
}

impl From<serde_json::Error> for AppError {
    fn from(value: serde_json::Error) -> Self {
        AppError::protocol("Invalid JSON payload").with_details(value.to_string())
    }
}

impl From<sqlx::Error> for AppError {
    fn from(value: sqlx::Error) -> Self {
        AppError::storage("Database operation failed").with_details(value.to_string())
    }
}

impl From<url::ParseError> for AppError {
    fn from(value: url::ParseError) -> Self {
        AppError::validation("Invalid URL").with_details(value.to_string())
    }
}

impl From<crate::messaging::error::MessagingError> for AppError {
    fn from(value: crate::messaging::error::MessagingError) -> Self {
        AppError::storage("Messaging storage failure").with_details(value.to_string())
    }
}

fn from_string(value: String) -> AppError {
    let lowered = value.to_lowercase();

    if lowered.contains("not authenticated")
        || lowered.contains("unauthorized")
        || lowered.contains("invalidtoken")
    {
        return AppError::auth(value);
    }

    if lowered.contains("network error")
        || lowered.contains("timed out")
        || lowered.contains("dns")
        || lowered.contains("connection")
    {
        return AppError::network(value);
    }

    if lowered.contains("protocol") || lowered.contains("unsupported status") {
        return AppError::protocol(value);
    }

    if lowered.contains("database") || lowered.contains("sqlite") || lowered.contains("storage") {
        return AppError::storage(value);
    }

    if lowered.contains("invalid") || lowered.contains("missing") {
        return AppError::validation(value);
    }

    AppError::internal(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serializes_error_payload_shape() {
        let err = AppError::network("Network error: timeout").with_details("socket timeout");
        let json = serde_json::to_value(err).expect("serialize app error");

        assert_eq!(json["code"], "network");
        assert_eq!(json["message"], "Network error: timeout");
        assert_eq!(json["details"], "socket timeout");
        assert!(json.get("trace_id").is_some());
    }
}
