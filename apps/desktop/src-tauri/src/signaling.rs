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

/// Track WebSocket connection state
pub struct WsState {
    pub connected: Arc<std::sync::atomic::AtomicBool>,
}

impl WsState {
    pub fn new() -> Self {
        Self {
            connected: Arc::new(std::sync::atomic::AtomicBool::new(false)),
        }
    }
    
    pub fn set_connected(&self, connected: bool) {
        self.connected.store(connected, std::sync::atomic::Ordering::SeqCst);
    }
    
    pub fn is_connected(&self) -> bool {
        self.connected.load(std::sync::atomic::Ordering::SeqCst)
    }
}

/// Connect to the signaling server (without identifying)
pub async fn connect(server_url: &str, app_handle: tauri::AppHandle) -> Result<WsSender, String> {
    let url = url::Url::parse(server_url).map_err(|e| e.to_string())?;
    
    let (ws_stream, _) = connect_async(url)
        .await
        .map_err(|e| format!("Failed to connect: {}", e))?;
    
    println!("Connected to signaling server: {}", server_url);
    
    let (write, mut read) = ws_stream.split();
    
    let sender = Arc::new(Mutex::new(Some(write)));
    let sender_clone = sender.clone();
    
    // Emit connected status
    let _ = app_handle.emit("ws-status", true);
    
    // Spawn task to handle incoming messages
    let app_handle_clone = app_handle.clone();
    tokio::spawn(async move {
        println!("📡 WebSocket message handler started");
        
        while let Some(msg) = read.next().await {
            match msg {
                Ok(Message::Text(text)) => {
                    println!("📨 [WS] Received from server: {}", text);
                    
                    // Emit to frontend for chat messages
                    match app_handle_clone.emit("ws-message", text.clone()) {
                        Ok(_) => println!("✅ [WS] Emitted 'ws-message' event to frontend"),
                        Err(e) => eprintln!("❌ [WS] Failed to emit ws-message event: {}", e),
                    }
                    
                    // Also handle WebRTC signaling
                    if let Ok(signal) = serde_json::from_str::<SignalingMessage>(&text) {
                        match signal {
                            SignalingMessage::Offer { target_id: _, sdp } => {
                                println!("🎯 Received Offer SDP: {}...", &sdp[..50.min(sdp.len())]);
                            }
                            SignalingMessage::Answer { target_id: _, sdp } => {
                                println!("🎯 Received Answer SDP: {}...", &sdp[..50.min(sdp.len())]);
                            }
                            SignalingMessage::Candidate { target_id: _, candidate, .. } => {
                                println!("🎯 Received ICE Candidate: {}", candidate);
                            }
                            _ => {}
                        }
                    }
                }
                Ok(Message::Ping(data)) => {
                    println!("🏓 Received ping, connection is alive");
                    // Pong is handled automatically by tungstenite
                }
                Ok(Message::Close(_)) => {
                    println!("🔌 Server closed connection");
                    let _ = app_handle_clone.emit("ws-status", false);
                    break;
                }
                Err(e) => {
                    eprintln!("❌ WebSocket error: {}", e);
                    let _ = app_handle_clone.emit("ws-status", false);
                    break;
                }
                _ => {}
            }
        }
        
        println!("📡 WebSocket message handler stopped");
        let _ = app_handle_clone.emit("ws-status", false);
    });
    
    Ok(sender_clone)
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
