use axum::extract::ws::Message;
use dashmap::DashMap;
use sqlx::PgPool;
use std::sync::Arc;
use tokio::sync::mpsc;

pub type Tx = mpsc::UnboundedSender<Message>;
pub type PeerMap = Arc<DashMap<String, Tx>>;
/// Maps user_id -> peer_id they're in call with
pub type ActiveCalls = Arc<DashMap<String, String>>;
/// Maps user_id -> peer_id for ringing calls (caller and callee entries)
pub type PendingCalls = Arc<DashMap<String, String>>;

#[derive(Clone)]
pub struct AppState {
    pub peers: PeerMap,
    pub db: PgPool,
    /// Tracks which users are currently in a call (user_id -> peer_id)
    pub active_calls: ActiveCalls,
    /// Tracks pending/ringing calls before acceptance (user_id -> peer_id)
    pub pending_calls: PendingCalls,
}

impl AppState {
    pub fn new(pool: PgPool) -> Self {
        Self {
            peers: Arc::new(DashMap::new()),
            db: pool,
            active_calls: Arc::new(DashMap::new()),
            pending_calls: Arc::new(DashMap::new()),
        }
    }

    /// Check if a user is currently busy (active call or pending call)
    pub fn is_busy(&self, user_id: &str) -> bool {
        self.active_calls.contains_key(user_id) || self.pending_calls.contains_key(user_id)
    }

    /// Start tracking a pending ringing call between two users.
    pub fn start_pending_call(&self, caller_id: &str, callee_id: &str) {
        self.pending_calls
            .insert(caller_id.to_string(), callee_id.to_string());
        self.pending_calls
            .insert(callee_id.to_string(), caller_id.to_string());
    }

    /// Accept a pending call and promote it to active call.
    pub fn accept_pending_call(&self, caller_id: &str, callee_id: &str) -> bool {
        let caller_peer = self.pending_calls.get(caller_id).map(|v| v.value().clone());
        let callee_peer = self.pending_calls.get(callee_id).map(|v| v.value().clone());

        if caller_peer.as_deref() != Some(callee_id) || callee_peer.as_deref() != Some(caller_id) {
            return false;
        }

        self.pending_calls.remove(caller_id);
        self.pending_calls.remove(callee_id);
        self.start_call(caller_id, callee_id);
        true
    }

    /// Cancel a specific pending pair.
    pub fn cancel_pending_pair(&self, user1: &str, user2: &str) -> bool {
        let p1 = self.pending_calls.get(user1).map(|v| v.value().clone());
        let p2 = self.pending_calls.get(user2).map(|v| v.value().clone());

        if p1.as_deref() != Some(user2) || p2.as_deref() != Some(user1) {
            return false;
        }

        self.pending_calls.remove(user1);
        self.pending_calls.remove(user2);
        true
    }

    /// Cancel any pending call for a user and return the other peer id.
    pub fn cancel_pending_call(&self, user_id: &str) -> Option<String> {
        if let Some((_, peer_id)) = self.pending_calls.remove(user_id) {
            self.pending_calls.remove(peer_id.as_str());
            Some(peer_id)
        } else {
            None
        }
    }

    /// Start tracking a call between two users
    pub fn start_call(&self, user1: &str, user2: &str) {
        self.active_calls
            .insert(user1.to_string(), user2.to_string());
        self.active_calls
            .insert(user2.to_string(), user1.to_string());
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

#[cfg(test)]
mod tests {
    use super::*;

    fn test_state() -> AppState {
        let pool = PgPool::connect_lazy("postgres://postgres:postgres@localhost/test")
            .expect("lazy postgres pool");
        AppState::new(pool)
    }

    #[tokio::test]
    async fn pending_call_can_be_promoted_to_active() {
        let state = test_state();
        state.start_pending_call("alice", "bob");

        assert!(state.is_busy("alice"));
        assert!(state.is_busy("bob"));
        assert!(state.accept_pending_call("alice", "bob"));

        assert_eq!(
            state.active_calls.get("alice").map(|v| v.value().clone()),
            Some("bob".to_string())
        );
        assert_eq!(
            state.active_calls.get("bob").map(|v| v.value().clone()),
            Some("alice".to_string())
        );
        assert!(state.pending_calls.get("alice").is_none());
        assert!(state.pending_calls.get("bob").is_none());
    }

    #[tokio::test]
    async fn cancel_pending_pair_clears_both_sides() {
        let state = test_state();
        state.start_pending_call("alice", "bob");

        assert!(state.cancel_pending_pair("alice", "bob"));
        assert!(!state.is_busy("alice"));
        assert!(!state.is_busy("bob"));
    }
}
