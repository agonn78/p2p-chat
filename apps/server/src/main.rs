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

#[tokio::main]
async fn main() {
    // initialize tracing
    tracing_subscriber::fmt::init();

    let database_url = env::var("DATABASE_URL").unwrap_or_else(|_| "postgres://user:password@db:5432/p2p_chat".to_string());
    
    // Connect to DB (Retry loop could be added here for robustness in docker-compose)
    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(&database_url)
        .await
        .expect("Failed to connect to Postgres");

    // Initialize state
    let state = AppState::new(pool);

    // build our application with a route
    let app = Router::new()
        .route("/health", get(health_check))
        .route("/ws", get(ws_handler))
        .with_state(state);

    // run our app with hyper, listening globally on port 3000
    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
    tracing::info!("listening on {}", listener.local_addr().unwrap());
    axum::serve(listener, app).await.unwrap();
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

    while let Some(Ok(msg)) = receiver.next().await {
        if let Message::Text(text) = msg {
            // Attempt to parse as SignalingMessage
            if let Ok(signal) = serde_json::from_str::<SignalingMessage>(&text) {
                match signal {
                    SignalingMessage::Identify { user_id } => {
                        tracing::info!("User identified: {}", user_id);
                        state.peers.insert(user_id.clone(), tx.clone());
                        my_id = Some(user_id);
                    }
                    SignalingMessage::Offer { target_id, sdp: _ } |
                    SignalingMessage::Answer { target_id, sdp: _ } |
                    SignalingMessage::Candidate { target_id, .. } => {
                        // Route to target
                        if let Some(peer_tx) = state.peers.get(&target_id) {
                            // Forward the original text message to preserve format
                            let _ = peer_tx.send(Message::Text(text));
                        } else {
                            tracing::warn!("Target peer {} not found", target_id);
                        }
                    }
                }
            }
        }
    }

    // Cleanup
    if let Some(id) = my_id {
        state.peers.remove(&id);
        tracing::info!("User disconnected: {}", id);
    }
}
