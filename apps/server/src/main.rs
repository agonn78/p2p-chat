mod auth;
mod models;
mod routes;
mod state;

use axum::{
    extract::{ws::{Message, WebSocket, WebSocketUpgrade}, State},
    response::IntoResponse,
    routing::get,
    Router,
};
use futures::{sink::SinkExt, stream::StreamExt};
use std::env;
use tokio::sync::mpsc;
use shared_proto::signaling::SignalingMessage;
use crate::state::AppState;
use sqlx::postgres::PgPoolOptions;
use axum::http::HeaderValue;
use tower_http::{
    cors::{Any, CorsLayer},
    trace::TraceLayer,
};

#[tokio::main]
async fn main() {
    // initialize tracing
    tracing_subscriber::fmt::init();

    let database_url = env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://user:password@p2p-chat-db:5432/p2p_chat".to_string());
    
    tracing::info!("Connecting to database...");
    
    // Connect to DB with retry
    let pool = loop {
        match PgPoolOptions::new()
            .max_connections(10)
            .connect(&database_url)
            .await
        {
            Ok(pool) => break pool,
            Err(e) => {
                tracing::warn!("Failed to connect to database: {}. Retrying in 2s...", e);
                tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
            }
        }
    };

    tracing::info!("Connected to database!");

    // Run migrations
    tracing::info!("Running migrations...");
    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .expect("Failed to run migrations");
    tracing::info!("Migrations complete!");

    // Seed test users only if SEED_TEST_USERS=true (dev/testing only)
    if std::env::var("SEED_TEST_USERS").map(|v| v == "true").unwrap_or(false) {
        tracing::info!("SEED_TEST_USERS=true, seeding test users...");
        seed_test_users(&pool).await;
    }

    // Initialize state
    let app_state = AppState::new(pool.clone());

    // CORS configuration - load allowed origins from env, or default to Any (dev)
    let cors = match std::env::var("ALLOWED_ORIGINS") {
        Ok(origins_str) => {
            let origins: Vec<HeaderValue> = origins_str
                .split(',')
                .filter_map(|o| o.trim().parse::<HeaderValue>().ok())
                .collect();
            tracing::info!("CORS: allowing origins: {:?}", origins);
            CorsLayer::new()
                .allow_origin(origins)
                .allow_methods(Any)
                .allow_headers(Any)
                .expose_headers(Any)
                .max_age(std::time::Duration::from_secs(3600))
        }
        Err(_) => {
            tracing::warn!("⚠️  ALLOWED_ORIGINS not set, allowing all origins. Set ALLOWED_ORIGINS in production!");
            CorsLayer::new()
                .allow_origin(Any)
                .allow_methods(Any)
                .allow_headers(Any)
                .expose_headers(Any)
                .max_age(std::time::Duration::from_secs(3600))
        }
    };

    // Build router
    let app = Router::new()
        .route("/health", get(health_check))
        .route("/ws", get(ws_handler))
        .nest("/auth", routes::auth::router())
        .nest("/friends", routes::friends::router())
        .nest("/users", routes::users::router())
        .nest("/chat", routes::chat::router())
        .nest("/servers", routes::servers::router())
        .layer(cors)
        .layer(TraceLayer::new_for_http())
        .with_state(app_state);

    // Run server
    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
    tracing::info!("🚀 Server listening on {}", listener.local_addr().unwrap());
    axum::serve(listener, app).await.unwrap();
}

/// Seed test users for Mac and Windows clients
async fn seed_test_users(pool: &sqlx::PgPool) {
    let users = [
        ("User_Mac", "mac@test.com", "password123"),
        ("User_Windows", "windows@test.com", "password123"),
    ];

    for (username, email, password) in users {
        let existing = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM users WHERE username = $1")
            .bind(username)
            .fetch_one(pool)
            .await
            .unwrap_or(0);

        if existing == 0 {
            let password_hash = auth::hash_password(password).expect("Failed to hash password");
            let result = sqlx::query(
                "INSERT INTO users (username, email, password_hash) VALUES ($1, $2, $3)"
            )
            .bind(username)
            .bind(email)
            .bind(&password_hash)
            .execute(pool)
            .await;

            match result {
                Ok(_) => tracing::info!("✅ Created test user: {}", username),
                Err(e) => tracing::warn!("Failed to create test user {}: {}", username, e),
            }
        } else {
            tracing::info!("Test user {} already exists", username);
        }
    }
}

async fn health_check() -> &'static str {
    "OK"
}

async fn ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
) -> impl IntoResponse {
    ws.on_upgrade(|socket| handle_socket(socket, state))
}

fn rewrite_offer_for_peer(target_id: String, sdp: String, from_id: &str) -> (String, SignalingMessage) {
    (
        target_id,
        SignalingMessage::Offer {
            target_id: from_id.to_string(),
            sdp,
        },
    )
}

fn rewrite_answer_for_peer(target_id: String, sdp: String, from_id: &str) -> (String, SignalingMessage) {
    (
        target_id,
        SignalingMessage::Answer {
            target_id: from_id.to_string(),
            sdp,
        },
    )
}

fn rewrite_candidate_for_peer(
    target_id: String,
    candidate: String,
    sdp_mid: Option<String>,
    sdp_m_line_index: Option<u16>,
    from_id: &str,
) -> (String, SignalingMessage) {
    (
        target_id,
        SignalingMessage::Candidate {
            target_id: from_id.to_string(),
            candidate,
            sdp_mid,
            sdp_m_line_index,
        },
    )
}

async fn handle_socket(socket: WebSocket, state: AppState) {
    let (mut sender, mut receiver) = socket.split();
    let (tx, mut rx) = mpsc::unbounded_channel();

    // Spawn a task to forward messages from the channel to the websocket
    tokio::spawn(async move {
        while let Some(msg) = rx.recv().await {
            if let Err(e) = sender.send(msg).await {
                tracing::error!("Failed to send message: {}", e);
                break;
            }
        }
    });

    let mut my_id: Option<String> = None;
    let mut my_username: Option<String> = None;

    while let Some(Ok(msg)) = receiver.next().await {
        if let Message::Text(text) = msg {
            // Attempt to parse as SignalingMessage
            if let Ok(signal) = serde_json::from_str::<SignalingMessage>(&text) {
                match signal {
                    SignalingMessage::Identify { user_id } => {
                        println!("🆔 User {} identified on WebSocket", user_id);
                        state.peers.insert(user_id.clone(), tx.clone());
                        let peer_count = state.peers.len();
                        println!("📊 Current connected peers: {} total", peer_count);
                        
                        // Fetch username from DB for call notifications
                        if let Ok(row) = sqlx::query_scalar::<_, String>(
                            "SELECT username FROM users WHERE id = $1::uuid"
                        )
                        .bind(&user_id)
                        .fetch_optional(&state.db)
                        .await {
                            my_username = row;
                        }
                        
                        my_id = Some(user_id);
                    }
                    
                    // === WebRTC Signaling (forward to target) ===
                    // Important: when forwarding, rewrite `target_id` to the sender id
                    // so the receiver knows who to reply to.
                    SignalingMessage::Offer { target_id, sdp } => {
                        let from_id = match &my_id {
                            Some(id) => id.clone(),
                            None => {
                                tracing::warn!("Received offer before identify");
                                continue;
                            }
                        };

                        let (target_id, forwarded) = rewrite_offer_for_peer(target_id, sdp, &from_id);

                        if let Some(peer_tx) = state.peers.get(&target_id) {
                            if let Ok(msg) = serde_json::to_string(&forwarded) {
                                let _ = peer_tx.send(Message::Text(msg));
                            }
                        } else {
                            tracing::warn!("Target peer {} not found", target_id);
                        }
                    }
                    SignalingMessage::Answer { target_id, sdp } => {
                        let from_id = match &my_id {
                            Some(id) => id.clone(),
                            None => {
                                tracing::warn!("Received answer before identify");
                                continue;
                            }
                        };

                        let (target_id, forwarded) = rewrite_answer_for_peer(target_id, sdp, &from_id);

                        if let Some(peer_tx) = state.peers.get(&target_id) {
                            if let Ok(msg) = serde_json::to_string(&forwarded) {
                                let _ = peer_tx.send(Message::Text(msg));
                            }
                        } else {
                            tracing::warn!("Target peer {} not found", target_id);
                        }
                    }
                    SignalingMessage::Candidate { target_id, candidate, sdp_mid, sdp_m_line_index } => {
                        let from_id = match &my_id {
                            Some(id) => id.clone(),
                            None => {
                                tracing::warn!("Received candidate before identify");
                                continue;
                            }
                        };

                        let (target_id, forwarded) = rewrite_candidate_for_peer(
                            target_id,
                            candidate,
                            sdp_mid,
                            sdp_m_line_index,
                            &from_id,
                        );

                        if let Some(peer_tx) = state.peers.get(&target_id) {
                            if let Ok(msg) = serde_json::to_string(&forwarded) {
                                let _ = peer_tx.send(Message::Text(msg));
                            }
                        } else {
                            tracing::warn!("Target peer {} not found", target_id);
                        }
                    }
                    
                    // === Call Signaling ===
                    
                    SignalingMessage::CallInitiate { target_id, public_key } => {
                        let caller_id = match &my_id {
                            Some(id) => id.clone(),
                            None => {
                                tracing::warn!("Received call initiate before identify");
                                continue;
                            }
                        };
                        let caller_name = my_username.clone().unwrap_or_else(|| "Unknown".to_string());

                        if caller_id == target_id {
                            continue;
                        }

                        // Caller already busy (in-call or already ringing)
                        if state.is_busy(&caller_id) {
                            if let Some(caller_tx) = state.peers.get(&caller_id) {
                                let busy = SignalingMessage::CallBusy { caller_id };
                                let msg = serde_json::to_string(&busy).unwrap();
                                let _ = caller_tx.send(Message::Text(msg));
                            }
                            continue;
                        }
                        
                        // Check if target is online
                        if let Some(peer_tx) = state.peers.get(&target_id) {
                            // Check if target is busy
                            if state.is_busy(&target_id) {
                                // Send busy signal back to caller
                                if let Some(caller_tx) = state.peers.get(&caller_id) {
                                    let busy = SignalingMessage::CallBusy { caller_id: target_id.clone() };
                                    let msg = serde_json::to_string(&busy).unwrap();
                                    let _ = caller_tx.send(Message::Text(msg));
                                }
                            } else {
                                // Track ringing state before the call is accepted
                                state.start_pending_call(&caller_id, &target_id);

                                // Forward as IncomingCall to target
                                let incoming = SignalingMessage::IncomingCall {
                                    caller_id: caller_id.clone(),
                                    caller_name,
                                    public_key,
                                };
                                let msg = serde_json::to_string(&incoming).unwrap();
                                let _ = peer_tx.send(Message::Text(msg));
                                tracing::info!("📞 Call initiated to {}", target_id);

                                // Ring timeout: if still pending after 30s, clear it and notify caller.
                                let timeout_state = state.clone();
                                let timeout_caller = caller_id.clone();
                                let timeout_target = target_id.clone();
                                tokio::spawn(async move {
                                    tokio::time::sleep(tokio::time::Duration::from_secs(30)).await;
                                    if timeout_state.cancel_pending_pair(&timeout_caller, &timeout_target) {
                                        if let Some(caller_tx) = timeout_state.peers.get(&timeout_caller) {
                                            let unavailable = SignalingMessage::CallUnavailable {
                                                target_id: timeout_target,
                                                reason: "timeout".to_string(),
                                            };
                                            let msg = serde_json::to_string(&unavailable).unwrap();
                                            let _ = caller_tx.send(Message::Text(msg));
                                        }
                                    }
                                });
                            }
                        } else {
                            tracing::warn!("Target {} not online for call", target_id);

                            if let Some(caller_tx) = state.peers.get(&caller_id) {
                                let unavailable = SignalingMessage::CallUnavailable {
                                    target_id,
                                    reason: "offline".to_string(),
                                };
                                let msg = serde_json::to_string(&unavailable).unwrap();
                                let _ = caller_tx.send(Message::Text(msg));
                            }
                        }
                    }
                    
                    SignalingMessage::CallAccept { caller_id, public_key } => {
                        let callee_id = match &my_id {
                            Some(id) => id.clone(),
                            None => {
                                tracing::warn!("Received call accept before identify");
                                continue;
                            }
                        };

                        // Promote pending ringing state to active call.
                        if !state.accept_pending_call(&caller_id, &callee_id) {
                            tracing::warn!(
                                "Ignoring call accept: no pending call between caller={} and callee={}",
                                caller_id,
                                callee_id
                            );
                            if let Some(callee_tx) = state.peers.get(&callee_id) {
                                let unavailable = SignalingMessage::CallUnavailable {
                                    target_id: caller_id,
                                    reason: "expired".to_string(),
                                };
                                let msg = serde_json::to_string(&unavailable).unwrap();
                                let _ = callee_tx.send(Message::Text(msg));
                            }
                            continue;
                        }
                        
                        // Forward CallAccepted to caller
                        if let Some(caller_tx) = state.peers.get(&caller_id) {
                            let accepted = SignalingMessage::CallAccepted {
                                target_id: callee_id,
                                public_key,
                            };
                            let msg = serde_json::to_string(&accepted).unwrap();
                            let _ = caller_tx.send(Message::Text(msg));
                            tracing::info!("✅ Call accepted, notifying caller {}", caller_id);
                        }
                    }
                    
                    SignalingMessage::CallDecline { caller_id } => {
                        let callee_id = match &my_id {
                            Some(id) => id.clone(),
                            None => {
                                tracing::warn!("Received call decline before identify");
                                continue;
                            }
                        };
                        let _ = state.cancel_pending_pair(&caller_id, &callee_id);

                        // Forward CallDeclined to caller
                        if let Some(caller_tx) = state.peers.get(&caller_id) {
                            let declined = SignalingMessage::CallDeclined {
                                target_id: my_id.clone().unwrap_or_default(),
                            };
                            let msg = serde_json::to_string(&declined).unwrap();
                            let _ = caller_tx.send(Message::Text(msg));
                            tracing::info!("❌ Call declined to {}", caller_id);
                        }
                    }
                    
                    SignalingMessage::CallEnd { peer_id } => {
                        let user_id = my_id.clone().unwrap_or_default();
                        
                        // End the call tracking
                        state.end_call(&user_id);
                        
                        // Notify peer
                        if let Some(peer_tx) = state.peers.get(&peer_id) {
                            let ended = SignalingMessage::CallEnded {
                                peer_id: user_id,
                            };
                            let msg = serde_json::to_string(&ended).unwrap();
                            let _ = peer_tx.send(Message::Text(msg));
                            tracing::info!("📴 Call ended with {}", peer_id);
                        }
                    }
                    
                    SignalingMessage::CallCancel { target_id } => {
                        let caller_id = match &my_id {
                            Some(id) => id.clone(),
                            None => {
                                tracing::warn!("Received call cancel before identify");
                                continue;
                            }
                        };
                        let _ = state.cancel_pending_pair(&caller_id, &target_id);

                        // Forward CallCancelled to target (callee)
                        if let Some(peer_tx) = state.peers.get(&target_id) {
                            let cancelled = SignalingMessage::CallCancelled {
                                caller_id,
                            };
                            let msg = serde_json::to_string(&cancelled).unwrap();
                            let _ = peer_tx.send(Message::Text(msg));
                            tracing::info!("🚫 Call cancelled to {}", target_id);
                        }
                    }
                    
                    // These are server->client only, ignore if received
                    SignalingMessage::IncomingCall { .. } |
                    SignalingMessage::CallAccepted { .. } |
                    SignalingMessage::CallDeclined { .. } |
                    SignalingMessage::CallEnded { .. } |
                    SignalingMessage::CallBusy { .. } |
                    SignalingMessage::CallCancelled { .. } |
                    SignalingMessage::CallUnavailable { .. } => {}
                }
            }
        }
    }

    // Cleanup on disconnect
    if let Some(id) = my_id {
        // If user was in an active call, notify peer
        if let Some(peer_id) = state.end_call(&id) {
            if let Some(peer_tx) = state.peers.get(&peer_id) {
                let ended = SignalingMessage::CallEnded { peer_id: id.clone() };
                let msg = serde_json::to_string(&ended).unwrap();
                match peer_tx.send(Message::Text(msg)) {
                    Ok(_) => tracing::info!("📴 User {} disconnected, notified peer {}", id, peer_id),
                    Err(e) => tracing::warn!("📴 User {} disconnected, failed to notify peer {}: {}", id, peer_id, e),
                }
            } else {
                tracing::warn!("📴 User {} disconnected, peer {} already gone", id, peer_id);
            }
        } else if let Some(peer_id) = state.cancel_pending_call(&id) {
            // If user disconnects while ringing, notify peer call is unavailable.
            if let Some(peer_tx) = state.peers.get(&peer_id) {
                let unavailable = SignalingMessage::CallUnavailable {
                    target_id: id.clone(),
                    reason: "peer_disconnected".to_string(),
                };
                let msg = serde_json::to_string(&unavailable).unwrap();
                let _ = peer_tx.send(Message::Text(msg));
            }
        }
        
        state.peers.remove(&id);
        tracing::info!("User disconnected: {}", id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rewrite_offer_uses_sender_as_peer_id() {
        let (target, forwarded) = rewrite_offer_for_peer(
            "receiver-id".to_string(),
            "offer-sdp".to_string(),
            "sender-id",
        );

        assert_eq!(target, "receiver-id");
        match forwarded {
            SignalingMessage::Offer { target_id, sdp } => {
                assert_eq!(target_id, "sender-id");
                assert_eq!(sdp, "offer-sdp");
            }
            _ => panic!("Expected SignalingMessage::Offer"),
        }
    }

    #[test]
    fn rewrite_answer_uses_sender_as_peer_id() {
        let (target, forwarded) = rewrite_answer_for_peer(
            "receiver-id".to_string(),
            "answer-sdp".to_string(),
            "sender-id",
        );

        assert_eq!(target, "receiver-id");
        match forwarded {
            SignalingMessage::Answer { target_id, sdp } => {
                assert_eq!(target_id, "sender-id");
                assert_eq!(sdp, "answer-sdp");
            }
            _ => panic!("Expected SignalingMessage::Answer"),
        }
    }

    #[test]
    fn rewrite_candidate_uses_sender_as_peer_id() {
        let (target, forwarded) = rewrite_candidate_for_peer(
            "receiver-id".to_string(),
            "candidate-a".to_string(),
            Some("0".to_string()),
            Some(1),
            "sender-id",
        );

        assert_eq!(target, "receiver-id");
        match forwarded {
            SignalingMessage::Candidate {
                target_id,
                candidate,
                sdp_mid,
                sdp_m_line_index,
            } => {
                assert_eq!(target_id, "sender-id");
                assert_eq!(candidate, "candidate-a");
                assert_eq!(sdp_mid.as_deref(), Some("0"));
                assert_eq!(sdp_m_line_index, Some(1));
            }
            _ => panic!("Expected SignalingMessage::Candidate"),
        }
    }
}

