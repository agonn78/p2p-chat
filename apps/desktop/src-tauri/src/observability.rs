use std::sync::OnceLock;

use tracing_subscriber::EnvFilter;
use uuid::Uuid;

static SESSION_TRACE_ID: OnceLock<String> = OnceLock::new();

pub fn init_tracing() {
    let env_filter = std::env::var("APP_LOG_LEVEL")
        .or_else(|_| std::env::var("RUST_LOG"))
        .unwrap_or_else(|_| "info".to_string());

    let subscriber = tracing_subscriber::fmt()
        .with_ansi(false)
        .with_env_filter(EnvFilter::new(env_filter))
        .json()
        .flatten_event(true)
        .with_current_span(true)
        .with_span_list(false)
        .finish();

    let _ = tracing::subscriber::set_global_default(subscriber);

    tracing::info!(
        component = "bootstrap",
        trace_id = %trace_id(),
        protocol_version = crate::protocol::PROTOCOL_VERSION,
        "structured tracing initialized"
    );
}

pub fn trace_id() -> &'static str {
    SESSION_TRACE_ID.get_or_init(|| Uuid::new_v4().to_string())
}

pub fn request_id() -> String {
    Uuid::new_v4().to_string()
}
