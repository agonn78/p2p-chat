use tokio_tungstenite::{connect_async, tungstenite::protocol::Message};
use futures_util::{SinkExt, StreamExt};
use std::sync::Arc;
use tokio::sync::Mutex;
use shared_proto::signaling::SignalingMessage;
use tauri::Emitter;

pub type WsSender = Arc<Mutex<Option<futures_util::stream::SplitSink<
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>,
    Message
>>>>;

/// Connect to the signaling server with automatic reconnection
pub async fn connect(server_url: &str, app_handle: tauri::AppHandle) -> Result<WsSender, String> {
    let url = url::Url::parse(server_url).map_err(|e| e.to_string())?;
    
    let (ws_stream, _) = connect_async(url.clone())
        .await
        .map_err(|e| format!("Failed to connect: {}", e))?;
    
    println!("Connected to signaling server: {}", server_url);
    
    let (write, mut read) = ws_stream.split();
    
    let sender = Arc::new(Mutex::new(Some(write)));
    let sender_clone = sender.clone();
    
    // Emit connected status
    let _ = app_handle.emit("ws-status", true);
    
    // Spawn task to handle incoming messages with reconnection
    let app_handle_clone = app_handle.clone();
    let sender_for_reconnect = sender.clone();
    let server_url_owned = server_url.to_string();
    tokio::spawn(async move {
        println!("📡 WebSocket message handler started");
        
        // Process messages from current connection
        handle_ws_messages(&mut read, &app_handle_clone).await;
        
        // Connection dropped — start reconnection loop
        println!("🔄 WebSocket disconnected, starting reconnection...");
        let _ = app_handle_clone.emit("ws-status", false);
        
        let mut backoff_secs = 1u64;
        let max_backoff = 30u64;
        
        loop {
            println!("🔄 Reconnecting in {}s...", backoff_secs);
            tokio::time::sleep(tokio::time::Duration::from_secs(backoff_secs)).await;
            
            let reconnect_url = match url::Url::parse(&server_url_owned) {
                Ok(url) => url,
                Err(e) => {
                    eprintln!("❌ Invalid reconnect URL: {}", e);
                    backoff_secs = (backoff_secs * 2).min(max_backoff);
                    continue;
                }
            };

            match connect_async(reconnect_url).await {
                Ok((new_ws_stream, _)) => {
                    println!("✅ WebSocket reconnected!");
                    let (new_write, mut new_read) = new_ws_stream.split();
                    
                    // Update the sender with new write half
                    {
                        let mut guard = sender_for_reconnect.lock().await;
                        *guard = Some(new_write);
                    }
                    
                    let _ = app_handle_clone.emit("ws-status", true);
                    // Signal frontend to re-identify
                    let _ = app_handle_clone.emit("ws-reconnected", true);
                    backoff_secs = 1; // Reset backoff
                    
                    // Handle messages from new connection
                    handle_ws_messages(&mut new_read, &app_handle_clone).await;
                    
                    // If we get here, connection dropped again
                    println!("🔄 WebSocket disconnected again, reconnecting...");
                    let _ = app_handle_clone.emit("ws-status", false);
                }
                Err(e) => {
                    eprintln!("❌ Reconnection failed: {}", e);
                    backoff_secs = (backoff_secs * 2).min(max_backoff);
                }
            }
        }
    });
    
    Ok(sender_clone)
}

/// Handle incoming WebSocket messages (extracted for reconnection reuse)
async fn handle_ws_messages(
    read: &mut futures_util::stream::SplitStream<
        tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>
    >,
    app_handle: &tauri::AppHandle,
) {
    while let Some(msg) = read.next().await {
        match msg {
            Ok(Message::Text(text)) => {
                println!("📨 [WS] Received from server: {}", text);
                
                // Emit to frontend for chat messages
                match app_handle.emit("ws-message", text.clone()) {
                    Ok(_) => println!("✅ [WS] Emitted 'ws-message' event to frontend"),
                    Err(e) => eprintln!("❌ [WS] Failed to emit ws-message event: {}", e),
                }
                
                // Also handle WebRTC signaling
                if let Ok(signal) = serde_json::from_str::<SignalingMessage>(&text) {
                    match signal {
                        SignalingMessage::Offer { target_id, sdp } => {
                            println!("🎯 Received Offer SDP from {}", target_id);
                            let payload = serde_json::json!({
                                "peerId": target_id,
                                "sdp": sdp,
                            });
                            let _ = app_handle.emit("webrtc-offer", payload);
                        }
                        SignalingMessage::Answer { target_id, sdp } => {
                            println!("🎯 Received Answer SDP from {}", target_id);
                            let payload = serde_json::json!({
                                "peerId": target_id,
                                "sdp": sdp,
                            });
                            let _ = app_handle.emit("webrtc-answer", payload);
                        }
                        SignalingMessage::Candidate { target_id, candidate, sdp_mid, sdp_m_line_index } => {
                            println!("🎯 Received ICE Candidate from {}", target_id);
                            let payload = serde_json::json!({
                                "peerId": target_id,
                                "candidate": candidate,
                                "sdpMid": sdp_mid,
                                "sdpMLineIndex": sdp_m_line_index,
                            });
                            let _ = app_handle.emit("webrtc-candidate", payload);
                        }
                        
                        // === Call Events ===
                        SignalingMessage::IncomingCall { caller_id, caller_name, public_key } => {
                            println!("📞 Incoming call from {} ({})", caller_name, caller_id);
                            let payload = serde_json::json!({
                                "callerId": caller_id,
                                "callerName": caller_name,
                                "publicKey": public_key,
                            });
                            let _ = app_handle.emit("incoming-call", payload);
                        }
                        SignalingMessage::CallAccepted { target_id, public_key } => {
                            println!("✅ Call accepted by {}", target_id);
                            let payload = serde_json::json!({
                                "peerId": target_id,
                                "publicKey": public_key,
                            });
                            let _ = app_handle.emit("call-accepted", payload);
                        }
                        SignalingMessage::CallDeclined { target_id } => {
                            println!("❌ Call declined by {}", target_id);
                            let _ = app_handle.emit("call-declined", target_id);
                        }
                        SignalingMessage::CallEnded { peer_id } => {
                            println!("📴 Call ended by {}", peer_id);
                            let _ = app_handle.emit("call-ended", peer_id);
                        }
                        SignalingMessage::CallBusy { caller_id: busy_user } => {
                            println!("📳 User {} is busy", busy_user);
                            let _ = app_handle.emit("call-busy", busy_user);
                        }
                        SignalingMessage::CallCancelled { caller_id } => {
                            println!("🚫 Call cancelled by {}", caller_id);
                            let _ = app_handle.emit("call-cancelled", caller_id);
                        }
                        SignalingMessage::CallUnavailable { target_id, reason } => {
                            println!("⚠️ Call unavailable for {} ({})", target_id, reason);
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
            Ok(Message::Ping(_data)) => {
                println!("🏓 Received ping, connection is alive");
            }
            Ok(Message::Close(_)) => {
                println!("🔌 Server closed connection");
                break;
            }
            Err(e) => {
                eprintln!("❌ WebSocket error: {}", e);
                break;
            }
            _ => {}
        }
    }
}

/// Send a signaling message to a peer via the server
pub async fn send_signal(sender: &WsSender, message: SignalingMessage) -> Result<(), String> {
    let msg = serde_json::to_string(&message).map_err(|e| e.to_string())?;
    
    let mut guard = sender.lock().await;
    if let Some(ref mut write) = *guard {
        write.send(Message::Text(msg)).await.map_err(|e| e.to_string())?;
        Ok(())
    } else {
        Err("WebSocket not connected".to_string())
    }
}

/// Send identification message to server
pub async fn send_identify(sender: &WsSender, user_id: &str) -> Result<(), String> {
    let identify = SignalingMessage::Identify { user_id: user_id.to_string() };
    send_signal(sender, identify).await
}
