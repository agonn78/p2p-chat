// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod api;
mod config;
mod signaling;

use tauri::{State, Manager};
use media::MediaEngine;
use std::sync::Arc;
use tokio::sync::Mutex;
use signaling::WsSender;
use shared_proto::signaling::SignalingMessage;
use api::ApiState;

struct AppState {
    media: Arc<std::sync::Mutex<MediaEngine>>,
    ws_sender: WsSender,
}

#[tauri::command]
async fn join_call(state: State<'_, AppState>, channel: String) -> Result<String, String> {
    println!("Joining call in channel: {}", channel);
    
    // Start media engine in background
    let media = state.media.clone();
    std::thread::spawn(move || {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let engine = media.lock().unwrap();
            if let Err(e) = engine.start().await {
                eprintln!("Media engine error: {}", e);
            }
        });
    });

    Ok(format!("Joined channel {}", channel))
}

#[tauri::command]
async fn identify_user(state: State<'_, AppState>, user_id: String) -> Result<(), String> {
    println!("Identifying user: {}", user_id);
    signaling::send_identify(&state.ws_sender, &user_id).await?;
    Ok(())
}

#[tauri::command]
async fn send_offer(state: State<'_, AppState>, target_id: String, sdp: String) -> Result<(), String> {
    let msg = SignalingMessage::Offer { target_id, sdp };
    signaling::send_signal(&state.ws_sender, msg).await
}

#[tauri::command]
async fn send_answer(state: State<'_, AppState>, target_id: String, sdp: String) -> Result<(), String> {
    let msg = SignalingMessage::Answer { target_id, sdp };
    signaling::send_signal(&state.ws_sender, msg).await
}

// === Call Commands ===

/// Start a call to a friend - generates keypair and sends CallInitiate
#[tauri::command]
async fn start_call(state: State<'_, AppState>, target_id: String) -> Result<String, String> {
    println!("📞 [CALL-DEBUG] ===== STARTING CALL =====");
    println!("📞 [CALL-DEBUG] Target ID: {}", target_id);
    
    // Generate keypair for E2EE
    println!("📞 [CALL-DEBUG] Generating keypair...");
    let public_key = {
        let mut engine = state.media.lock().map_err(|e| {
            println!("📞 [CALL-DEBUG] ❌ Failed to lock media engine: {}", e);
            e.to_string()
        })?;
        engine.generate_keypair().map_err(|e| {
            println!("📞 [CALL-DEBUG] ❌ Failed to generate keypair: {}", e);
            e.to_string()
        })?
    };
    println!("📞 [CALL-DEBUG] Generated public key: {}...", &public_key[..30.min(public_key.len())]);
    
    // Send call initiate signal
    println!("📞 [CALL-DEBUG] Sending CallInitiate signal...");
    let msg = SignalingMessage::CallInitiate { 
        target_id: target_id.clone(), 
        public_key: public_key.clone(),
    };
    signaling::send_signal(&state.ws_sender, msg).await?;
    println!("📞 [CALL-DEBUG] ✅ CallInitiate sent successfully");
    
    Ok(public_key)
}

/// Accept incoming call - generates keypair, completes key exchange
#[tauri::command]
async fn accept_call(
    state: State<'_, AppState>, 
    caller_id: String,
    caller_public_key: String,
) -> Result<String, String> {
    println!("✅ [CALL-DEBUG] ===== ACCEPTING CALL =====");
    println!("✅ [CALL-DEBUG] Caller ID: {}", caller_id);
    println!("✅ [CALL-DEBUG] Caller public key: {}...", &caller_public_key[..30.min(caller_public_key.len())]);
    
    // Generate our keypair and complete key exchange
    println!("✅ [CALL-DEBUG] Generating keypair and completing key exchange...");
    let public_key = {
        let mut engine = state.media.lock().map_err(|e| {
            println!("✅ [CALL-DEBUG] ❌ Failed to lock media engine: {}", e);
            e.to_string()
        })?;
        let pk = engine.generate_keypair().map_err(|e| {
            println!("✅ [CALL-DEBUG] ❌ Failed to generate keypair: {}", e);
            e.to_string()
        })?;
        engine.complete_key_exchange(&caller_public_key).map_err(|e| {
            println!("✅ [CALL-DEBUG] ❌ Key exchange failed: {}", e);
            e.to_string()
        })?;
        pk
    };
    println!("✅ [CALL-DEBUG] Generated our public key: {}...", &public_key[..30.min(public_key.len())]);
    
    // Send accept signal with our public key
    println!("✅ [CALL-DEBUG] Sending CallAccept signal...");
    let msg = SignalingMessage::CallAccept { 
        caller_id, 
        public_key: public_key.clone(),
    };
    signaling::send_signal(&state.ws_sender, msg).await?;
    println!("✅ [CALL-DEBUG] ✅ CallAccept sent successfully");
    
    Ok(public_key)
}

/// Complete key exchange after call is accepted (caller side)
#[tauri::command]
async fn complete_call_handshake(
    state: State<'_, AppState>,
    peer_public_key: String,
) -> Result<(), String> {
    println!("🔐 [CALL-DEBUG] ===== COMPLETING E2EE HANDSHAKE =====");
    println!("🔐 [CALL-DEBUG] Peer public key (first 30 chars): {}...", &peer_public_key[..30.min(peer_public_key.len())]);
    
    {
        let mut engine = state.media.lock().map_err(|e| {
            println!("🔐 [CALL-DEBUG] ❌ Failed to lock media engine: {}", e);
            e.to_string()
        })?;
        println!("🔐 [CALL-DEBUG] Got media engine lock");
        engine.complete_key_exchange(&peer_public_key).map_err(|e| {
            println!("🔐 [CALL-DEBUG] ❌ Key exchange failed: {}", e);
            e.to_string()
        })?;
    }
    
    println!("🔐 [CALL-DEBUG] ✅ Key exchange completed successfully");
    Ok(())
}

/// Decline incoming call
#[tauri::command]
async fn decline_call(state: State<'_, AppState>, caller_id: String) -> Result<(), String> {
    println!("❌ Declining call from {}", caller_id);
    
    let msg = SignalingMessage::CallDecline { caller_id };
    signaling::send_signal(&state.ws_sender, msg).await
}

/// End active call
#[tauri::command]
async fn end_call(state: State<'_, AppState>, peer_id: String) -> Result<(), String> {
    println!("📴 Ending call with {}", peer_id);
    
    let msg = SignalingMessage::CallEnd { peer_id };
    signaling::send_signal(&state.ws_sender, msg).await
}

/// Cancel outgoing call before answer
#[tauri::command]
async fn cancel_call(state: State<'_, AppState>, target_id: String) -> Result<(), String> {
    println!("🚫 Cancelling call to {}", target_id);
    
    let msg = SignalingMessage::CallCancel { target_id };
    signaling::send_signal(&state.ws_sender, msg).await
}

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(move |app| {
            let app_handle = app.handle().clone();
            
            // Initialize API state for HTTP requests
            let api_state = ApiState::new(config::API_URL.to_string());
            app.manage(api_state);
            
            // Connect to signaling server (without identifying yet)
            tauri::async_runtime::spawn(async move {
                match signaling::connect(config::SERVER_URL, app_handle.clone()).await {
                    Ok(sender) => {
                        // Store the sender in app state
                        let state = AppState {
                            media: Arc::new(std::sync::Mutex::new(MediaEngine::new())),
                            ws_sender: sender,
                        };
                        app_handle.manage(state);
                        println!("WebSocket connected. Waiting for user login to identify...");
                    }
                    Err(e) => {
                        eprintln!("Warning: Could not connect to signaling server: {}", e);
                        eprintln!("Server URL: {}", config::SERVER_URL);
                        // Manage with empty sender
                        let state = AppState {
                            media: Arc::new(std::sync::Mutex::new(MediaEngine::new())),
                            ws_sender: Arc::new(Mutex::new(None)),
                        };
                        app_handle.manage(state);
                    }
                }
            });
            
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            // WebRTC/Call commands
            join_call, 
            send_offer, 
            send_answer, 
            identify_user,
            start_call,
            accept_call,
            complete_call_handshake,
            decline_call,
            end_call,
            cancel_call,
            // API commands
            api::auth::api_login,
            api::auth::api_register,
            api::auth::api_logout,
            api::auth::api_health_check,
            api::users::api_upload_public_key,
            api::users::api_fetch_user_public_key,
            api::friends::api_fetch_friends,
            api::friends::api_fetch_pending_requests,
            api::friends::api_send_friend_request,
            api::friends::api_accept_friend,
            api::chat::api_create_or_get_dm,
            api::chat::api_fetch_messages,
            api::chat::api_send_message,
            api::chat::api_delete_message,
            api::chat::api_delete_all_messages,
            api::servers::api_fetch_servers,
            api::servers::api_create_server,
            api::servers::api_join_server,
            api::servers::api_leave_server,
            api::servers::api_fetch_server_details,
            api::servers::api_create_channel,
            api::servers::api_fetch_server_members,
            api::servers::api_fetch_channel_messages,
            api::servers::api_send_channel_message,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}


