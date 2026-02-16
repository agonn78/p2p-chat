mod auth;
mod models;
mod routes;
mod state;
mod validation;

use crate::auth::validate_token;
use crate::state::AppState;
use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        Request, State,
    },
    http::{header, HeaderValue, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
    routing::get,
    Json, Router,
};
use dashmap::DashMap;
use futures::{sink::SinkExt, stream::StreamExt};
use serde::Serialize;
use shared_proto::signaling::{
    is_supported_protocol_version, SignalingMessage, LEGACY_PROTOCOL_VERSION, PROTOCOL_VERSION,
};
use sqlx::postgres::PgPoolOptions;
use std::{
    env,
    sync::LazyLock,
    time::{Duration, Instant},
};
use tokio::sync::mpsc;
use tower_http::{
    cors::{Any, CorsLayer},
    trace::TraceLayer,
};
use uuid::Uuid;

static RATE_LIMIT_BUCKETS: LazyLock<DashMap<String, (u32, Instant)>> = LazyLock::new(DashMap::new);

const HEADER_PROTOCOL_VERSION: &str = "x-protocol-version";
const HEADER_TRACE_ID: &str = "x-trace-id";

#[derive(Debug, Serialize)]
struct HealthResponse {
    status: &'static str,
    db: HealthDb,
}

#[derive(Debug, Serialize)]
struct HealthDb {
    ok: bool,
}

#[derive(Debug, Serialize)]
struct ProtocolErrorBody {
    code: &'static str,
    message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    details: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    trace_id: Option<String>,
}

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
    if std::env::var("SEED_TEST_USERS")
        .map(|v| v == "true")
        .unwrap_or(false)
    {
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
        .route("/readiness", get(readiness_check))
        .route("/ws", get(ws_handler))
        .nest("/auth", routes::auth::router())
        .nest("/friends", routes::friends::router())
        .nest("/users", routes::users::router())
        .nest("/chat", routes::chat::router())
        .nest("/servers", routes::servers::router())
        .layer(axum::middleware::from_fn(protocol_version_middleware))
        .layer(axum::middleware::from_fn(rate_limit_middleware))
        .layer(cors)
        .layer(TraceLayer::new_for_http())
        .with_state(app_state);

    // Run server
    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
    tracing::info!("🚀 Server listening on {}", listener.local_addr().unwrap());
    axum::serve(listener, app).await.unwrap();
}

fn rate_limit_budget(path: &str) -> (u32, Duration, &'static str) {
    if path.starts_with("/auth/login") || path.starts_with("/auth/register") {
        (20, Duration::from_secs(60), "auth")
    } else if path.starts_with("/ws") {
        (120, Duration::from_secs(60), "ws")
    } else if path.contains("/typing") {
        (600, Duration::from_secs(60), "typing")
    } else {
        (300, Duration::from_secs(60), "api")
    }
}

fn request_fingerprint(req: &Request) -> String {
    let forwarded = req
        .headers()
        .get("x-forwarded-for")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.split(',').next())
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .unwrap_or("unknown");

    let user_agent = req
        .headers()
        .get(header::USER_AGENT)
        .and_then(|v| v.to_str().ok())
        .map(|v| v.chars().take(24).collect::<String>())
        .unwrap_or_else(|| "ua-none".to_string());

    let auth_hint = req
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .map(|v| {
            let mut chars = v.chars();
            chars.by_ref().take(18).collect::<String>()
        })
        .unwrap_or_else(|| "anon".to_string());

    format!("{}:{}:{}", forwarded, user_agent, auth_hint)
}

fn request_trace_id(req: &Request) -> Option<String> {
    req.headers()
        .get(HEADER_TRACE_ID)
        .and_then(|value| value.to_str().ok())
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn parse_protocol_header(req: &Request) -> Option<u8> {
    req.headers()
        .get(HEADER_PROTOCOL_VERSION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.trim().parse::<u8>().ok())
}

async fn protocol_version_middleware(req: Request, next: Next) -> Result<Response, StatusCode> {
    if req.uri().path() == "/ws" {
        return Ok(next.run(req).await);
    }

    if let Some(version) = parse_protocol_header(&req) {
        if !is_supported_protocol_version(version) {
            let trace_id = request_trace_id(&req);
            tracing::warn!(
                component = "protocol",
                received_protocol_version = version,
                supported_protocol_version = PROTOCOL_VERSION,
                legacy_protocol_version = LEGACY_PROTOCOL_VERSION,
                trace_id = trace_id.as_deref().unwrap_or("missing"),
                "rejected request with unsupported protocol version"
            );

            let body = ProtocolErrorBody {
                code: "protocol_version_mismatch",
                message: format!(
                    "Unsupported protocol version {version}. Supported versions: [{}, {}]",
                    LEGACY_PROTOCOL_VERSION, PROTOCOL_VERSION
                ),
                details: Some("Please update client protocol version".to_string()),
                trace_id,
            };

            return Ok((StatusCode::UPGRADE_REQUIRED, Json(body)).into_response());
        }
    }

    Ok(next.run(req).await)
}

async fn rate_limit_middleware(req: Request, next: Next) -> Result<Response, StatusCode> {
    let path = req.uri().path().to_string();
    if path == "/health" {
        return Ok(next.run(req).await);
    }

    let now = Instant::now();
    let (max_requests, window, bucket) = rate_limit_budget(&path);
    let identity = request_fingerprint(&req);
    let key = format!("{}:{}", bucket, identity);

    let mut allowed = true;
    {
        let mut entry = RATE_LIMIT_BUCKETS.entry(key).or_insert((0, now));
        let (count, window_start) = entry.value_mut();

        if now.duration_since(*window_start) >= window {
            *count = 0;
            *window_start = now;
        }

        if *count >= max_requests {
            allowed = false;
        } else {
            *count += 1;
        }
    }

    if RATE_LIMIT_BUCKETS.len() > 50_000 {
        let stale_after = window + window;
        RATE_LIMIT_BUCKETS
            .retain(|_, (_, started_at)| now.duration_since(*started_at) < stale_after);
    }

    if !allowed {
        return Err(StatusCode::TOO_MANY_REQUESTS);
    }

    Ok(next.run(req).await)
}

/// Seed test users for Mac and Windows clients
async fn seed_test_users(pool: &sqlx::PgPool) {
    let users = [
        ("User_Mac", "mac@test.com", "password123"),
        ("User_Windows", "windows@test.com", "password123"),
    ];

    for (username, email, password) in users {
        let existing =
            sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM users WHERE username = $1")
                .bind(username)
                .fetch_one(pool)
                .await
                .unwrap_or(0);

        if existing == 0 {
            let password_hash = auth::hash_password(password).expect("Failed to hash password");
            let result = sqlx::query(
                "INSERT INTO users (username, email, password_hash) VALUES ($1, $2, $3)",
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

async fn health_check() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok",
        db: HealthDb { ok: true },
    })
}

async fn readiness_check(State(state): State<AppState>) -> impl IntoResponse {
    match sqlx::query_scalar::<_, i64>("SELECT 1").fetch_one(&state.db).await {
        Ok(_) => (
            StatusCode::OK,
            Json(HealthResponse {
                status: "ready",
                db: HealthDb { ok: true },
            }),
        )
            .into_response(),
        Err(err) => {
            tracing::warn!(component = "health", error = %err, "readiness probe failed");
            (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(HealthResponse {
                    status: "not_ready",
                    db: HealthDb { ok: false },
                }),
            )
                .into_response()
        }
    }
}

async fn ws_handler(ws: WebSocketUpgrade, State(state): State<AppState>) -> impl IntoResponse {
    ws.on_upgrade(|socket| handle_socket(socket, state))
}

fn rewrite_offer_for_peer(
    target_id: String,
    sdp: String,
    from_id: &str,
    trace_id: Option<String>,
) -> (String, SignalingMessage) {
    (
        target_id,
        SignalingMessage::Offer {
            version: PROTOCOL_VERSION,
            trace_id,
            target_id: from_id.to_string(),
            sdp,
        },
    )
}

fn rewrite_answer_for_peer(
    target_id: String,
    sdp: String,
    from_id: &str,
    trace_id: Option<String>,
) -> (String, SignalingMessage) {
    (
        target_id,
        SignalingMessage::Answer {
            version: PROTOCOL_VERSION,
            trace_id,
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
    trace_id: Option<String>,
) -> (String, SignalingMessage) {
    (
        target_id,
        SignalingMessage::Candidate {
            version: PROTOCOL_VERSION,
            trace_id,
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
                if !is_supported_protocol_version(signal.version()) {
                    tracing::warn!(
                        component = "ws",
                        received_protocol_version = signal.version(),
                        supported_protocol_version = PROTOCOL_VERSION,
                        legacy_protocol_version = LEGACY_PROTOCOL_VERSION,
                        trace_id = signal.trace_id().unwrap_or("missing"),
                        "dropping websocket message with unsupported protocol version"
                    );
                    continue;
                }

                match signal {
                    SignalingMessage::Identify {
                        user_id,
                        token,
                        trace_id,
                        ..
                    } => {
                        let claims = match validate_token(&token) {
                            Ok(claims) => claims,
                            Err(_) => {
                                tracing::warn!(
                                    "Rejected WS identify for {}: invalid token",
                                    user_id
                                );
                                continue;
                            }
                        };

                        if claims.sub != user_id {
                            tracing::warn!(
                                "Rejected WS identify: token subject {} does not match payload {}",
                                claims.sub,
                                user_id
                            );
                            continue;
                        }

                        tracing::info!(
                            component = "ws",
                            user_id = %user_id,
                            trace_id = trace_id.as_deref().unwrap_or("missing"),
                            "websocket identify accepted"
                        );

                        println!("🆔 User {} identified on WebSocket", user_id);
                        state.peers.insert(user_id.clone(), tx.clone());
                        let peer_count = state.peers.len();
                        println!("📊 Current connected peers: {} total", peer_count);

                        my_username = Some(claims.username);
                        my_id = Some(user_id);
                    }

                    // === WebRTC Signaling (forward to target) ===
                    // Important: when forwarding, rewrite `target_id` to the sender id
                    // so the receiver knows who to reply to.
                    SignalingMessage::Offer {
                        target_id,
                        sdp,
                        trace_id,
                        ..
                    } => {
                        let from_id = match &my_id {
                            Some(id) => id.clone(),
                            None => {
                                tracing::warn!("Received offer before identify");
                                continue;
                            }
                        };

                        let (target_id, forwarded) =
                            rewrite_offer_for_peer(target_id, sdp, &from_id, trace_id);

                        if let Some(peer_tx) = state.peers.get(&target_id) {
                            if let Ok(msg) = serde_json::to_string(&forwarded) {
                                let _ = peer_tx.send(Message::Text(msg));
                            }
                        } else {
                            tracing::warn!("Target peer {} not found", target_id);
                        }
                    }
                    SignalingMessage::Answer {
                        target_id,
                        sdp,
                        trace_id,
                        ..
                    } => {
                        let from_id = match &my_id {
                            Some(id) => id.clone(),
                            None => {
                                tracing::warn!("Received answer before identify");
                                continue;
                            }
                        };

                        let (target_id, forwarded) =
                            rewrite_answer_for_peer(target_id, sdp, &from_id, trace_id);

                        if let Some(peer_tx) = state.peers.get(&target_id) {
                            if let Ok(msg) = serde_json::to_string(&forwarded) {
                                let _ = peer_tx.send(Message::Text(msg));
                            }
                        } else {
                            tracing::warn!("Target peer {} not found", target_id);
                        }
                    }
                    SignalingMessage::Candidate {
                        target_id,
                        candidate,
                        sdp_mid,
                        sdp_m_line_index,
                        trace_id,
                        ..
                    } => {
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
                            trace_id,
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
                    SignalingMessage::CallInitiate {
                        target_id,
                        public_key,
                        trace_id,
                        ..
                    } => {
                        let caller_id = match &my_id {
                            Some(id) => id.clone(),
                            None => {
                                tracing::warn!("Received call initiate before identify");
                                continue;
                            }
                        };
                        let caller_name =
                            my_username.clone().unwrap_or_else(|| "Unknown".to_string());

                        if caller_id == target_id {
                            continue;
                        }

                        // Caller already busy (in-call or already ringing)
                        if state.is_busy(&caller_id) {
                            if let Some(caller_tx) = state.peers.get(&caller_id) {
                                let busy = SignalingMessage::CallBusy {
                                    version: PROTOCOL_VERSION,
                                    trace_id: trace_id.clone(),
                                    caller_id,
                                };
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
                                    let busy = SignalingMessage::CallBusy {
                                        version: PROTOCOL_VERSION,
                                        trace_id: trace_id.clone(),
                                        caller_id: target_id.clone(),
                                    };
                                    let msg = serde_json::to_string(&busy).unwrap();
                                    let _ = caller_tx.send(Message::Text(msg));
                                }
                            } else {
                                // Track ringing state before the call is accepted
                                state.start_pending_call(&caller_id, &target_id);

                                // Forward as IncomingCall to target
                                let incoming = SignalingMessage::IncomingCall {
                                    version: PROTOCOL_VERSION,
                                    trace_id: trace_id.clone(),
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
                                    if timeout_state
                                        .cancel_pending_pair(&timeout_caller, &timeout_target)
                                    {
                                        if let Some(caller_tx) =
                                            timeout_state.peers.get(&timeout_caller)
                                        {
                                            let unavailable = SignalingMessage::CallUnavailable {
                                                version: PROTOCOL_VERSION,
                                                trace_id: trace_id.clone(),
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
                                    version: PROTOCOL_VERSION,
                                    trace_id: trace_id.clone(),
                                    target_id,
                                    reason: "offline".to_string(),
                                };
                                let msg = serde_json::to_string(&unavailable).unwrap();
                                let _ = caller_tx.send(Message::Text(msg));
                            }
                        }
                    }

                    SignalingMessage::CallAccept {
                        caller_id,
                        public_key,
                        trace_id,
                        ..
                    } => {
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
                                    version: PROTOCOL_VERSION,
                                    trace_id: trace_id.clone(),
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
                                version: PROTOCOL_VERSION,
                                trace_id: trace_id.clone(),
                                target_id: callee_id,
                                public_key,
                            };
                            let msg = serde_json::to_string(&accepted).unwrap();
                            let _ = caller_tx.send(Message::Text(msg));
                            tracing::info!("✅ Call accepted, notifying caller {}", caller_id);
                        }
                    }

                    SignalingMessage::CallDecline {
                        caller_id,
                        trace_id,
                        ..
                    } => {
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
                                version: PROTOCOL_VERSION,
                                trace_id,
                                target_id: my_id.clone().unwrap_or_default(),
                            };
                            let msg = serde_json::to_string(&declined).unwrap();
                            let _ = caller_tx.send(Message::Text(msg));
                            tracing::info!("❌ Call declined to {}", caller_id);
                        }
                    }

                    SignalingMessage::CallEnd {
                        peer_id,
                        trace_id,
                        ..
                    } => {
                        let user_id = my_id.clone().unwrap_or_default();

                        // End the call tracking
                        state.end_call(&user_id);

                        // Notify peer
                        if let Some(peer_tx) = state.peers.get(&peer_id) {
                            let ended = SignalingMessage::CallEnded {
                                version: PROTOCOL_VERSION,
                                trace_id,
                                peer_id: user_id,
                            };
                            let msg = serde_json::to_string(&ended).unwrap();
                            let _ = peer_tx.send(Message::Text(msg));
                            tracing::info!("📴 Call ended with {}", peer_id);
                        }
                    }

                    SignalingMessage::CallCancel {
                        target_id,
                        trace_id,
                        ..
                    } => {
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
                                version: PROTOCOL_VERSION,
                                trace_id,
                                caller_id,
                            };
                            let msg = serde_json::to_string(&cancelled).unwrap();
                            let _ = peer_tx.send(Message::Text(msg));
                            tracing::info!("🚫 Call cancelled to {}", target_id);
                        }
                    }

                    // These are server->client only, ignore if received
                    SignalingMessage::IncomingCall { .. }
                    | SignalingMessage::CallAccepted { .. }
                    | SignalingMessage::CallDeclined { .. }
                    | SignalingMessage::CallEnded { .. }
                    | SignalingMessage::CallBusy { .. }
                    | SignalingMessage::CallCancelled { .. }
                    | SignalingMessage::CallUnavailable { .. } => {}
                }
            }
        }
    }

    // Cleanup on disconnect
    if let Some(id) = my_id {
        // If user was in an active call, notify peer
        if let Some(peer_id) = state.end_call(&id) {
            if let Some(peer_tx) = state.peers.get(&peer_id) {
                let ended = SignalingMessage::CallEnded {
                    version: PROTOCOL_VERSION,
                    trace_id: None,
                    peer_id: id.clone(),
                };
                let msg = serde_json::to_string(&ended).unwrap();
                match peer_tx.send(Message::Text(msg)) {
                    Ok(_) => {
                        tracing::info!("📴 User {} disconnected, notified peer {}", id, peer_id)
                    }
                    Err(e) => tracing::warn!(
                        "📴 User {} disconnected, failed to notify peer {}: {}",
                        id,
                        peer_id,
                        e
                    ),
                }
            } else {
                tracing::warn!("📴 User {} disconnected, peer {} already gone", id, peer_id);
            }
        } else if let Some(peer_id) = state.cancel_pending_call(&id) {
            // If user disconnects while ringing, notify peer call is unavailable.
            if let Some(peer_tx) = state.peers.get(&peer_id) {
                let unavailable = SignalingMessage::CallUnavailable {
                    version: PROTOCOL_VERSION,
                    trace_id: None,
                    target_id: id.clone(),
                    reason: "peer_disconnected".to_string(),
                };
                let msg = serde_json::to_string(&unavailable).unwrap();
                let _ = peer_tx.send(Message::Text(msg));
            }
        }

        // Remove user from any joined voice channels and broadcast leave presence.
        if let Ok(user_uuid) = Uuid::parse_str(&id) {
            if let Ok(joined_rows) = sqlx::query_as::<_, (Uuid, Uuid)>(
                "SELECT server_id, channel_id FROM voice_channel_sessions WHERE user_id = $1",
            )
            .bind(user_uuid)
            .fetch_all(&state.db)
            .await
            {
                let _ = sqlx::query("DELETE FROM voice_channel_sessions WHERE user_id = $1")
                    .bind(user_uuid)
                    .execute(&state.db)
                    .await;

                for (server_id, channel_id) in joined_rows {
                    if let Ok(member_ids) = sqlx::query_scalar::<_, Uuid>(
                        "SELECT user_id FROM server_members WHERE server_id = $1",
                    )
                    .bind(server_id)
                    .fetch_all(&state.db)
                    .await
                    {
                        let ws_payload = serde_json::json!({
                            "type": "VOICE_PRESENCE",
                            "server_id": server_id,
                            "channel_id": channel_id,
                            "user_id": user_uuid,
                            "joined": false,
                        });
                        let ws_text = serde_json::to_string(&ws_payload).unwrap();
                        for member_id in member_ids {
                            if let Some(peer_tx) = state.peers.get(&member_id.to_string()) {
                                let _ = peer_tx.send(Message::Text(ws_text.clone()));
                            }
                        }
                    }
                }
            }
        }

        state.peers.remove(&id);
        tracing::info!("User disconnected: {}", id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        body::{to_bytes, Body},
        http::Request,
        routing::get,
        Router,
    };
    use tower::util::ServiceExt;

    #[tokio::test]
    async fn health_check_returns_expected_payload() {
        let Json(body) = health_check().await;
        assert_eq!(body.status, "ok");
        assert!(body.db.ok);
    }

    #[tokio::test]
    async fn protocol_middleware_rejects_unsupported_version() {
        let app = Router::new()
            .route("/test", get(|| async { StatusCode::OK }))
            .layer(axum::middleware::from_fn(protocol_version_middleware))
            .into_service();

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/test")
                    .header(HEADER_PROTOCOL_VERSION, "99")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::UPGRADE_REQUIRED);

        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["code"], "protocol_version_mismatch");
    }

    #[tokio::test]
    async fn protocol_middleware_allows_supported_version() {
        let app = Router::new()
            .route("/test", get(|| async { StatusCode::OK }))
            .layer(axum::middleware::from_fn(protocol_version_middleware))
            .into_service();

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/test")
                    .header(HEADER_PROTOCOL_VERSION, PROTOCOL_VERSION.to_string())
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[test]
    fn rewrite_offer_uses_sender_as_peer_id() {
        let (target, forwarded) = rewrite_offer_for_peer(
            "receiver-id".to_string(),
            "offer-sdp".to_string(),
            "sender-id",
            Some("trace-1".to_string()),
        );

        assert_eq!(target, "receiver-id");
        match forwarded {
            SignalingMessage::Offer {
                version,
                trace_id,
                target_id,
                sdp,
            } => {
                assert_eq!(version, PROTOCOL_VERSION);
                assert_eq!(trace_id.as_deref(), Some("trace-1"));
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
            Some("trace-2".to_string()),
        );

        assert_eq!(target, "receiver-id");
        match forwarded {
            SignalingMessage::Answer {
                version,
                trace_id,
                target_id,
                sdp,
            } => {
                assert_eq!(version, PROTOCOL_VERSION);
                assert_eq!(trace_id.as_deref(), Some("trace-2"));
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
            Some("trace-3".to_string()),
        );

        assert_eq!(target, "receiver-id");
        match forwarded {
            SignalingMessage::Candidate {
                version,
                trace_id,
                target_id,
                candidate,
                sdp_mid,
                sdp_m_line_index,
            } => {
                assert_eq!(version, PROTOCOL_VERSION);
                assert_eq!(trace_id.as_deref(), Some("trace-3"));
                assert_eq!(target_id, "sender-id");
                assert_eq!(candidate, "candidate-a");
                assert_eq!(sdp_mid.as_deref(), Some("0"));
                assert_eq!(sdp_m_line_index, Some(1));
            }
            _ => panic!("Expected SignalingMessage::Candidate"),
        }
    }
}
