pub mod auth;
pub mod chat;
pub mod friends;
pub mod servers;
pub mod users;

use reqwest::Client;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Shared API state for all HTTP requests
pub struct ApiState {
    pub client: Client,
    pub base_url: String,
    pub token: Arc<RwLock<Option<String>>>,
}

impl ApiState {
    pub fn new(base_url: String) -> Self {
        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .expect("Failed to create HTTP client");

        Self {
            client,
            base_url,
            token: Arc::new(RwLock::new(None)),
        }
    }

    pub async fn set_token(&self, token: Option<String>) {
        let mut write_guard = self.token.write().await;
        *write_guard = token;
    }

    pub async fn get_token(&self) -> Option<String> {
        self.token.read().await.clone()
    }

    /// Build a request with auth header if token exists
    pub async fn auth_header(&self) -> Option<String> {
        self.get_token().await.map(|t| format!("Bearer {}", t))
    }
}
