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

    // Seed test users if they don't exist
    seed_test_users(&pool).await;

    // Initialize state
    let app_state = AppState::new(pool.clone());

    // CORS configuration
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    // Build router
    let app = Router::new()
        .route("/health", get(health_check))
        .route("/ws", get(ws_handler))
        .nest("/auth", routes::auth::router())
        .nest("/friends", routes::friends::router())
        .nest("/users", routes::users::router())
        .nest("/chat", routes::chat::router())
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
                        println!("🆔 User {} identified on WebSocket", user_id);
                        state.peers.insert(user_id.clone(), tx.clone());
                        let peer_count = state.peers.len();
                        println!("📊 Current connected peers: {} total", peer_count);
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

