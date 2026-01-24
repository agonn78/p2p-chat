use tokio_tungstenite::{connect_async, tungstenite::protocol::Message};
use futures_util::{SinkExt, StreamExt};
use std::sync::Arc;
use tokio::sync::Mutex;
use shared_proto::signaling::SignalingMessage;

pub type WsSender = Arc<Mutex<Option<futures_util::stream::SplitSink<
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>,
    Message
>>>>;

/// Connect to the signaling server
pub async fn connect(server_url: &str, user_id: &str) -> Result<WsSender, String> {
    let url = url::Url::parse(server_url).map_err(|e| e.to_string())?;
    
    let (ws_stream, _) = connect_async(url)
        .await
        .map_err(|e| format!("Failed to connect: {}", e))?;
    
    println!("Connected to signaling server: {}", server_url);
    
    let (mut write, mut read) = ws_stream.split();
    
    // Send identify message
    let identify = SignalingMessage::Identify { user_id: user_id.to_string() };
    let msg = serde_json::to_string(&identify).map_err(|e| e.to_string())?;
    write.send(Message::Text(msg)).await.map_err(|e| e.to_string())?;
    println!("Identified as: {}", user_id);
    
    let sender = Arc::new(Mutex::new(Some(write)));
    let sender_clone = sender.clone();
    
    // Spawn task to handle incoming messages
    tokio::spawn(async move {
        while let Some(msg) = read.next().await {
            match msg {
                Ok(Message::Text(text)) => {
                    println!("Received from server: {}", text);
                    // TODO: Handle incoming signaling messages (Offer/Answer/Candidate)
                    if let Ok(signal) = serde_json::from_str::<SignalingMessage>(&text) {
                        match signal {
                            SignalingMessage::Offer { target_id: _, sdp } => {
                                println!("Received Offer SDP: {}...", &sdp[..50.min(sdp.len())]);
                            }
                            SignalingMessage::Answer { target_id: _, sdp } => {
                                println!("Received Answer SDP: {}...", &sdp[..50.min(sdp.len())]);
                            }
                            SignalingMessage::Candidate { target_id: _, candidate, .. } => {
                                println!("Received ICE Candidate: {}", candidate);
                            }
                            _ => {}
                        }
                    }
                }
                Ok(Message::Close(_)) => {
                    println!("Server closed connection");
                    break;
                }
                Err(e) => {
                    eprintln!("WebSocket error: {}", e);
                    break;
                }
                _ => {}
            }
        }
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
