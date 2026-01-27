use std::sync::Arc;
use dashmap::DashMap;
use axum::extract::ws::Message;
use tokio::sync::mpsc;
use sqlx::PgPool;

pub type Tx = mpsc::UnboundedSender<Message>;
pub type PeerMap = Arc<DashMap<String, Tx>>;
/// Maps user_id -> peer_id they're in call with
pub type ActiveCalls = Arc<DashMap<String, String>>;

#[derive(Clone)]
pub struct AppState {
    pub peers: PeerMap,
    pub db: PgPool,
    /// Tracks which users are currently in a call (user_id -> peer_id)
    pub active_calls: ActiveCalls,
}

impl AppState {
    pub fn new(pool: PgPool) -> Self {
        Self {
            peers: Arc::new(DashMap::new()),
            db: pool,
            active_calls: Arc::new(DashMap::new()),
        }
    }
    
    /// Check if a user is in a call
    pub fn is_in_call(&self, user_id: &str) -> bool {
        self.active_calls.contains_key(user_id)
    }
    
    /// Start tracking a call between two users
    pub fn start_call(&self, user1: &str, user2: &str) {
        self.active_calls.insert(user1.to_string(), user2.to_string());
        self.active_calls.insert(user2.to_string(), user1.to_string());
    }
    
    /// End a call for a user (also removes peer)
    pub fn end_call(&self, user_id: &str) -> Option<String> {
        if let Some((_, peer_id)) = self.active_calls.remove(user_id) {
            self.active_calls.remove(&peer_id);
            Some(peer_id)
        } else {
            None
        }
    }
}
