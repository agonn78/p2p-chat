pub mod auth;
pub mod chat;
pub mod friends;
pub mod servers;
pub mod users;

use crate::error::{AppError, AppResult};
use crate::observability;
use crate::protocol;
use reqwest::Client;
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use std::sync::Arc;
use tokio::sync::RwLock;

/// Shared API state for all HTTP requests
pub struct ApiState {
    pub client: Client,
    pub base_url: String,
    pub token: Arc<RwLock<Option<String>>>,
    pub trace_id: String,
    pub protocol_version: u8,
}

impl ApiState {
    pub fn new(base_url: String) -> Self {
        let mut default_headers = HeaderMap::new();
        let protocol_header = HeaderValue::from_str(&protocol::PROTOCOL_VERSION.to_string())
            .unwrap_or_else(|_| HeaderValue::from_static("1"));
        let trace_header = HeaderValue::from_str(observability::trace_id())
            .unwrap_or_else(|_| HeaderValue::from_static("unknown-trace"));
        let request_header = HeaderValue::from_str(observability::trace_id())
            .unwrap_or_else(|_| HeaderValue::from_static("unknown-request"));
        default_headers.insert(
            HeaderName::from_static("x-protocol-version"),
            protocol_header,
        );
        default_headers.insert(HeaderName::from_static("x-trace-id"), trace_header);
        default_headers.insert(HeaderName::from_static("x-request-id"), request_header);

        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .default_headers(default_headers)
            .build()
            .expect("Failed to create HTTP client");

        Self {
            client,
            base_url,
            token: Arc::new(RwLock::new(None)),
            trace_id: observability::trace_id().to_string(),
            protocol_version: protocol::PROTOCOL_VERSION,
        }
    }

    pub async fn set_token(&self, token: Option<String>) {
        let mut write_guard = self.token.write().await;
        *write_guard = token;
    }

    pub async fn get_token(&self) -> Option<String> {
        self.token.read().await.clone()
    }

    pub async fn bearer_token(&self) -> AppResult<String> {
        self.get_token()
            .await
            .ok_or_else(|| AppError::auth("Not authenticated"))
    }

    pub fn request_id(&self) -> String {
        observability::request_id()
    }

    pub fn with_request_metadata(
        &self,
        builder: reqwest::RequestBuilder,
        request_id: &str,
    ) -> reqwest::RequestBuilder {
        builder.header(protocol::HEADER_REQUEST_ID, request_id)
    }

    /// Build a request with auth header if token exists
    pub async fn auth_header(&self) -> Option<String> {
        self.get_token().await.map(|t| format!("Bearer {}", t))
    }
}
