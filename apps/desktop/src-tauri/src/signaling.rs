use std::sync::{Arc, OnceLock};

use futures_util::{SinkExt, StreamExt};
use serde::Serialize;
use shared_proto::signaling::SignalingMessage;
use tauri::Emitter;
use tokio::sync::Mutex;
use tokio_tungstenite::{connect_async, tungstenite::protocol::Message};

use crate::backoff::{compute_backoff_delay, BackoffConfig};
use crate::error::{AppError, AppResult};
use crate::observability;
use crate::protocol;

pub type WsSender = Arc<
    Mutex<
        Option<
            futures_util::stream::SplitSink<
                tokio_tungstenite::WebSocketStream<
                    tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
                >,
                Message,
            >,
        >,
    >,
>;

type WsReadHalf = futures_util::stream::SplitStream<
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>,
>;

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
enum WsLifecycleState {
    Disconnected,
    Connecting,
    Identifying,
    Ready,
    Reconnecting,
}

#[derive(Debug, Clone)]
struct IdentifyContext {
    user_id: String,
    token: String,
}

type IdentifySlot = Arc<Mutex<Option<IdentifyContext>>>;

static IDENTIFY_SLOT: OnceLock<IdentifySlot> = OnceLock::new();

fn identify_slot() -> IdentifySlot {
    IDENTIFY_SLOT
        .get_or_init(|| Arc::new(Mutex::new(None)))
        .clone()
}

/// Connect to the signaling server with automatic reconnection.
pub async fn connect(server_url: &str, app_handle: tauri::AppHandle) -> AppResult<WsSender> {
    let url = url::Url::parse(server_url)?;

    let sender = Arc::new(Mutex::new(None));
    let state = Arc::new(Mutex::new(WsLifecycleState::Disconnected));

    transition_ws_state(&app_handle, &state, WsLifecycleState::Connecting, "initial connect").await;

    let (ws_stream, _) = connect_async(url.clone())
        .await
        .map_err(|e| AppError::network("Failed to connect signaling websocket").with_details(e.to_string()))?;

    let (write, read) = ws_stream.split();
    {
        let mut guard = sender.lock().await;
        *guard = Some(write);
    }

    transition_ws_state(&app_handle, &state, WsLifecycleState::Ready, "connected").await;

    let sender_clone = sender.clone();
    let state_clone = state.clone();
    let app_handle_clone = app_handle.clone();
    let reconnect_url = server_url.to_string();
    let identify = identify_slot();

    tauri::async_runtime::spawn(async move {
        run_ws_loop(
            reconnect_url,
            read,
            sender_clone,
            state_clone,
            identify,
            app_handle_clone,
        )
        .await;
    });

    Ok(sender)
}

async fn run_ws_loop(
    server_url: String,
    mut read: WsReadHalf,
    sender: WsSender,
    state: Arc<Mutex<WsLifecycleState>>,
    identify: IdentifySlot,
    app_handle: tauri::AppHandle,
) {
    let backoff = BackoffConfig::websocket_default();
    let mut reconnect_attempt: u32 = 0;

    loop {
        handle_ws_messages(&mut read, &app_handle).await;

        transition_ws_state(
            &app_handle,
            &state,
            WsLifecycleState::Reconnecting,
            "connection dropped",
        )
        .await;

        loop {
            let delay = compute_backoff_delay(backoff, reconnect_attempt);
            reconnect_attempt = reconnect_attempt.saturating_add(1);

            tracing::warn!(
                component = "ws",
                ws_state = "reconnecting",
                attempt = reconnect_attempt,
                delay_ms = delay.as_millis() as u64,
                trace_id = observability::trace_id(),
                protocol_version = protocol::PROTOCOL_VERSION,
                "websocket reconnect scheduled"
            );

            tokio::time::sleep(delay).await;

            transition_ws_state(&app_handle, &state, WsLifecycleState::Connecting, "retry connect").await;

            let reconnect_url = match url::Url::parse(&server_url) {
                Ok(url) => url,
                Err(err) => {
                    tracing::error!(
                        component = "ws",
                        ws_state = "connecting",
                        trace_id = observability::trace_id(),
                        protocol_version = protocol::PROTOCOL_VERSION,
                        error = %err,
                        "invalid signaling reconnect URL"
                    );
                    transition_ws_state(
                        &app_handle,
                        &state,
                        WsLifecycleState::Reconnecting,
                        "invalid reconnect url",
                    )
                    .await;
                    continue;
                }
            };

            match connect_async(reconnect_url).await {
                Ok((new_ws_stream, _)) => {
                    reconnect_attempt = 0;
                    let (new_write, new_read) = new_ws_stream.split();
                    {
                        let mut guard = sender.lock().await;
                        *guard = Some(new_write);
                    }

                    let identify_result = maybe_identify_after_reconnect(
                        &sender,
                        identify.clone(),
                        &state,
                        &app_handle,
                    )
                    .await;

                    if let Err(err) = identify_result {
                        tracing::warn!(
                            component = "ws",
                            ws_state = "identifying",
                            trace_id = observability::trace_id(),
                            protocol_version = protocol::PROTOCOL_VERSION,
                            error = %err,
                            "automatic identify after reconnect failed"
                        );
                        transition_ws_state(
                            &app_handle,
                            &state,
                            WsLifecycleState::Reconnecting,
                            "identify failed",
                        )
                        .await;
                        continue;
                    }

                    transition_ws_state(
                        &app_handle,
                        &state,
                        WsLifecycleState::Ready,
                        "reconnected",
                    )
                    .await;

                    emit_resync(&app_handle);
                    read = new_read;
                    break;
                }
                Err(err) => {
                    tracing::warn!(
                        component = "ws",
                        ws_state = "connecting",
                        trace_id = observability::trace_id(),
                        protocol_version = protocol::PROTOCOL_VERSION,
                        error = %err,
                        "websocket reconnect failed"
                    );
                    transition_ws_state(
                        &app_handle,
                        &state,
                        WsLifecycleState::Reconnecting,
                        "connect failed",
                    )
                    .await;
                }
            }
        }
    }
}

async fn transition_ws_state(
    app_handle: &tauri::AppHandle,
    state: &Arc<Mutex<WsLifecycleState>>,
    next: WsLifecycleState,
    reason: &str,
) {
    {
        let mut guard = state.lock().await;
        *guard = next;
    }

    let ws_connected = matches!(next, WsLifecycleState::Identifying | WsLifecycleState::Ready);

    let payload = serde_json::json!({
        "state": next,
        "reason": reason,
        "traceId": observability::trace_id(),
        "protocolVersion": protocol::PROTOCOL_VERSION,
    });

    let _ = app_handle.emit("ws-state", payload);
    let _ = app_handle.emit("ws-status", ws_connected);

    tracing::info!(
        component = "ws",
        ws_state = ?next,
        trace_id = observability::trace_id(),
        protocol_version = protocol::PROTOCOL_VERSION,
        reason,
        "websocket state transition"
    );
}

async fn maybe_identify_after_reconnect(
    sender: &WsSender,
    identify: IdentifySlot,
    state: &Arc<Mutex<WsLifecycleState>>,
    app_handle: &tauri::AppHandle,
) -> AppResult<()> {
    let identify_ctx = identify.lock().await.clone();
    if let Some(ctx) = identify_ctx {
        transition_ws_state(
            app_handle,
            state,
            WsLifecycleState::Identifying,
            "auto identify",
        )
        .await;
        send_identify(sender, &ctx.user_id, &ctx.token).await?;
    }
    Ok(())
}

fn emit_resync(app_handle: &tauri::AppHandle) {
    let payload = serde_json::json!({
        "traceId": observability::trace_id(),
        "protocolVersion": protocol::PROTOCOL_VERSION,
    });
    let _ = app_handle.emit("ws-reconnected", true);
    let _ = app_handle.emit("ws-resync", payload);
}

/// Handle incoming WebSocket messages.
async fn handle_ws_messages(read: &mut WsReadHalf, app_handle: &tauri::AppHandle) {
    while let Some(msg) = read.next().await {
        match msg {
            Ok(Message::Text(text)) => {
                let _ = app_handle.emit("ws-message", text.clone());

                if let Ok(signal) = serde_json::from_str::<SignalingMessage>(&text) {
                    if !protocol::is_supported_protocol_version(signal.version()) {
                        tracing::warn!(
                            component = "ws",
                            ws_state = "ready",
                            trace_id = signal.trace_id().unwrap_or(observability::trace_id()),
                            protocol_version = signal.version(),
                            "received unsupported protocol version"
                        );
                        let _ = app_handle.emit(
                            "ws-protocol-error",
                            serde_json::json!({
                                "receivedVersion": signal.version(),
                                "supported": [protocol::LEGACY_PROTOCOL_VERSION, protocol::PROTOCOL_VERSION],
                            }),
                        );
                        continue;
                    }

                    match signal {
                        SignalingMessage::Offer {
                            target_id, sdp, ..
                        } => {
                            let payload = serde_json::json!({
                                "peerId": target_id,
                                "sdp": sdp,
                            });
                            let _ = app_handle.emit("webrtc-offer", payload);
                        }
                        SignalingMessage::Answer {
                            target_id, sdp, ..
                        } => {
                            let payload = serde_json::json!({
                                "peerId": target_id,
                                "sdp": sdp,
                            });
                            let _ = app_handle.emit("webrtc-answer", payload);
                        }
                        SignalingMessage::Candidate {
                            target_id,
                            candidate,
                            sdp_mid,
                            sdp_m_line_index,
                            ..
                        } => {
                            let payload = serde_json::json!({
                                "peerId": target_id,
                                "candidate": candidate,
                                "sdpMid": sdp_mid,
                                "sdpMLineIndex": sdp_m_line_index,
                            });
                            let _ = app_handle.emit("webrtc-candidate", payload);
                        }
                        SignalingMessage::IncomingCall {
                            caller_id,
                            caller_name,
                            public_key,
                            ..
                        } => {
                            let payload = serde_json::json!({
                                "callerId": caller_id,
                                "callerName": caller_name,
                                "publicKey": public_key,
                            });
                            let _ = app_handle.emit("incoming-call", payload);
                        }
                        SignalingMessage::CallAccepted {
                            target_id,
                            public_key,
                            ..
                        } => {
                            let payload = serde_json::json!({
                                "peerId": target_id,
                                "publicKey": public_key,
                            });
                            let _ = app_handle.emit("call-accepted", payload);
                        }
                        SignalingMessage::CallDeclined { target_id, .. } => {
                            let _ = app_handle.emit("call-declined", target_id);
                        }
                        SignalingMessage::CallEnded { peer_id, .. } => {
                            let _ = app_handle.emit("call-ended", peer_id);
                        }
                        SignalingMessage::CallBusy {
                            caller_id: busy_user,
                            ..
                        } => {
                            let _ = app_handle.emit("call-busy", busy_user);
                        }
                        SignalingMessage::CallCancelled { caller_id, .. } => {
                            let _ = app_handle.emit("call-cancelled", caller_id);
                        }
                        SignalingMessage::CallUnavailable {
                            target_id, reason, ..
                        } => {
                            let payload = serde_json::json!({
                                "targetId": target_id,
                                "reason": reason,
                            });
                            let _ = app_handle.emit("call-unavailable", payload);
                        }
                        _ => {}
                    }
                }
            }
            Ok(Message::Ping(_)) => {
                tracing::debug!(component = "ws", ws_state = "ready", "received ping");
            }
            Ok(Message::Close(_)) => {
                tracing::warn!(component = "ws", ws_state = "disconnected", "server closed websocket");
                break;
            }
            Err(err) => {
                tracing::warn!(
                    component = "ws",
                    ws_state = "disconnected",
                    trace_id = observability::trace_id(),
                    protocol_version = protocol::PROTOCOL_VERSION,
                    error = %err,
                    "websocket error"
                );
                break;
            }
            _ => {}
        }
    }
}

/// Send a signaling message to a peer via the server.
pub async fn send_signal(sender: &WsSender, message: SignalingMessage) -> AppResult<()> {
    let message = with_message_metadata(message);
    if !protocol::is_supported_protocol_version(message.version()) {
        return Err(AppError::protocol(format!(
            "Unsupported protocol version {} for websocket message",
            message.version()
        )));
    }

    let payload = serde_json::to_string(&message)?;

    let mut guard = sender.lock().await;
    if let Some(ref mut write) = *guard {
        write
            .send(Message::Text(payload))
            .await
            .map_err(|err| AppError::network("Failed to send websocket message").with_details(err.to_string()))?;
        Ok(())
    } else {
        Err(AppError::network("WebSocket not connected"))
    }
}

/// Send identification message to server.
pub async fn send_identify(sender: &WsSender, user_id: &str, token: &str) -> AppResult<()> {
    {
        let slot = identify_slot();
        let mut guard = slot.lock().await;
        *guard = Some(IdentifyContext {
            user_id: user_id.to_string(),
            token: token.to_string(),
        });
    }

    let identify = SignalingMessage::Identify {
        version: protocol::PROTOCOL_VERSION,
        trace_id: Some(observability::trace_id().to_string()),
        user_id: user_id.to_string(),
        token: token.to_string(),
    };
    send_signal(sender, identify).await
}

fn with_message_metadata(message: SignalingMessage) -> SignalingMessage {
    let trace = Some(observability::trace_id().to_string());

    match message {
        SignalingMessage::Offer {
            trace_id,
            target_id,
            sdp,
            ..
        } => SignalingMessage::Offer {
            version: protocol::PROTOCOL_VERSION,
            trace_id: trace_id.or(trace.clone()),
            target_id,
            sdp,
        },
        SignalingMessage::Answer {
            trace_id,
            target_id,
            sdp,
            ..
        } => SignalingMessage::Answer {
            version: protocol::PROTOCOL_VERSION,
            trace_id: trace_id.or(trace.clone()),
            target_id,
            sdp,
        },
        SignalingMessage::Candidate {
            trace_id,
            target_id,
            candidate,
            sdp_mid,
            sdp_m_line_index,
            ..
        } => SignalingMessage::Candidate {
            version: protocol::PROTOCOL_VERSION,
            trace_id: trace_id.or(trace.clone()),
            target_id,
            candidate,
            sdp_mid,
            sdp_m_line_index,
        },
        SignalingMessage::Identify {
            trace_id,
            user_id,
            token,
            ..
        } => SignalingMessage::Identify {
            version: protocol::PROTOCOL_VERSION,
            trace_id: trace_id.or(trace.clone()),
            user_id,
            token,
        },
        SignalingMessage::CallInitiate {
            trace_id,
            target_id,
            public_key,
            ..
        } => SignalingMessage::CallInitiate {
            version: protocol::PROTOCOL_VERSION,
            trace_id: trace_id.or(trace.clone()),
            target_id,
            public_key,
        },
        SignalingMessage::IncomingCall {
            trace_id,
            caller_id,
            caller_name,
            public_key,
            ..
        } => SignalingMessage::IncomingCall {
            version: protocol::PROTOCOL_VERSION,
            trace_id: trace_id.or(trace.clone()),
            caller_id,
            caller_name,
            public_key,
        },
        SignalingMessage::CallAccept {
            trace_id,
            caller_id,
            public_key,
            ..
        } => SignalingMessage::CallAccept {
            version: protocol::PROTOCOL_VERSION,
            trace_id: trace_id.or(trace.clone()),
            caller_id,
            public_key,
        },
        SignalingMessage::CallAccepted {
            trace_id,
            target_id,
            public_key,
            ..
        } => SignalingMessage::CallAccepted {
            version: protocol::PROTOCOL_VERSION,
            trace_id: trace_id.or(trace.clone()),
            target_id,
            public_key,
        },
        SignalingMessage::CallDecline {
            trace_id,
            caller_id,
            ..
        } => SignalingMessage::CallDecline {
            version: protocol::PROTOCOL_VERSION,
            trace_id: trace_id.or(trace.clone()),
            caller_id,
        },
        SignalingMessage::CallDeclined {
            trace_id,
            target_id,
            ..
        } => SignalingMessage::CallDeclined {
            version: protocol::PROTOCOL_VERSION,
            trace_id: trace_id.or(trace.clone()),
            target_id,
        },
        SignalingMessage::CallEnd {
            trace_id, peer_id, ..
        } => SignalingMessage::CallEnd {
            version: protocol::PROTOCOL_VERSION,
            trace_id: trace_id.or(trace.clone()),
            peer_id,
        },
        SignalingMessage::CallEnded {
            trace_id, peer_id, ..
        } => SignalingMessage::CallEnded {
            version: protocol::PROTOCOL_VERSION,
            trace_id: trace_id.or(trace.clone()),
            peer_id,
        },
        SignalingMessage::CallBusy {
            trace_id,
            caller_id,
            ..
        } => SignalingMessage::CallBusy {
            version: protocol::PROTOCOL_VERSION,
            trace_id: trace_id.or(trace.clone()),
            caller_id,
        },
        SignalingMessage::CallCancel {
            trace_id,
            target_id,
            ..
        } => SignalingMessage::CallCancel {
            version: protocol::PROTOCOL_VERSION,
            trace_id: trace_id.or(trace.clone()),
            target_id,
        },
        SignalingMessage::CallCancelled {
            trace_id,
            caller_id,
            ..
        } => SignalingMessage::CallCancelled {
            version: protocol::PROTOCOL_VERSION,
            trace_id: trace_id.or(trace.clone()),
            caller_id,
        },
        SignalingMessage::CallUnavailable {
            trace_id,
            target_id,
            reason,
            ..
        } => SignalingMessage::CallUnavailable {
            version: protocol::PROTOCOL_VERSION,
            trace_id: trace_id.or(trace.clone()),
            target_id,
            reason,
        },
    }
}
