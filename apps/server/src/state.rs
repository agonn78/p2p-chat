use std::sync::Arc;
use dashmap::DashMap;
use axum::extract::ws::Message;
use tokio::sync::mpsc;
use sqlx::PgPool;

pub type Tx = mpsc::UnboundedSender<Message>;
pub type PeerMap = Arc<DashMap<String, Tx>>;

#[derive(Clone)]
pub struct AppState {
    pub peers: PeerMap,
    pub db: PgPool,
}

impl AppState {
    pub fn new(pool: PgPool) -> Self {
        Self {
            peers: Arc::new(DashMap::new()),
            db: pool,
        }
    }
}
