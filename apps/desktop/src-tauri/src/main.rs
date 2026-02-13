// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod api;
mod config;
mod messaging;
mod signaling;

use tauri::{State, Manager, Emitter};
use media::{MediaEngine, AudioSettings};
use std::sync::Arc;
use tokio::sync::Mutex;
use signaling::WsSender;
use shared_proto::signaling::SignalingMessage;
use api::ApiState;
use messaging::service::MessagingService;

struct AppState {
    media: Arc<Mutex<MediaEngine>>,
    ws_sender: WsSender,
}

#[derive(Clone)]
pub struct MessagingState {
    pub service: MessagingService,
}

#[tauri::command]
async fn join_call(_state: State<'_, AppState>, channel: String) -> Result<String, String> {
    println!("🔊 [CALL-DEBUG] Joining call in channel: {}", channel);
    // Note: Media engine will be started after E2EE handshake completes
    // Don't start it here to avoid holding the lock during async operations
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
        let mut engine = state.media.lock().await;
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
        let mut engine = state.media.lock().await;
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
        let mut engine = state.media.lock().await;
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
    
    // Reset media engine (may have generated keypair)
    {
        let mut engine = state.media.lock().await;
        engine.reset().await;
    }
    
    let msg = SignalingMessage::CallDecline { caller_id };
    signaling::send_signal(&state.ws_sender, msg).await
}

/// End active call
#[tauri::command]
async fn end_call(state: State<'_, AppState>, peer_id: String) -> Result<(), String> {
    println!("📴 Ending call with {}", peer_id);
    
    // Reset media engine for next call
    {
        let mut engine = state.media.lock().await;
        engine.reset().await;
    }
    
    let msg = SignalingMessage::CallEnd { peer_id };
    signaling::send_signal(&state.ws_sender, msg).await
}

/// Cancel outgoing call before answer
#[tauri::command]
async fn cancel_call(state: State<'_, AppState>, target_id: String) -> Result<(), String> {
    println!("🚫 Cancelling call to {}", target_id);
    
    // Reset media engine
    {
        let mut engine = state.media.lock().await;
        engine.reset().await;
    }
    
    let msg = SignalingMessage::CallCancel { target_id };
    signaling::send_signal(&state.ws_sender, msg).await
}

// === Audio Commands ===

/// Audio device info for frontend
#[derive(serde::Serialize)]
struct AudioDevice {
    id: String,
    name: String,
}

/// List available input (microphone) devices
#[tauri::command]
async fn list_audio_devices() -> Result<Vec<AudioDevice>, String> {
    MediaEngine::list_input_devices()
        .map(|devices| {
            devices.into_iter()
                .map(|(id, name)| AudioDevice { id, name })
                .collect()
        })
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn get_default_audio_device() -> Result<AudioDevice, String> {
    MediaEngine::default_input_device_name()
        .map(|name| AudioDevice { id: name.clone(), name })
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn get_selected_audio_device(state: State<'_, AppState>) -> Result<Option<AudioDevice>, String> {
    let engine = state.media.lock().await;
    Ok(engine
        .selected_input_device()
        .map(|name| AudioDevice { id: name.clone(), name }))
}

#[tauri::command]
async fn set_audio_device(state: State<'_, AppState>, device_id: String) -> Result<(), String> {
    let mut engine = state.media.lock().await;
    engine
        .set_input_device(Some(device_id.clone()))
        .map_err(|e| format!("Failed to set audio device: {}", e))?;
    println!("🔊 [AUDIO] Input device set to: {}", device_id);
    Ok(())
}

#[tauri::command]
async fn list_output_devices() -> Result<Vec<AudioDevice>, String> {
    MediaEngine::list_output_devices()
        .map(|devices| {
            devices
                .into_iter()
                .map(|(id, name)| AudioDevice { id, name })
                .collect()
        })
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn get_default_output_device() -> Result<AudioDevice, String> {
    MediaEngine::default_output_device_name()
        .map(|name| AudioDevice { id: name.clone(), name })
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn get_selected_output_device(state: State<'_, AppState>) -> Result<Option<AudioDevice>, String> {
    let engine = state.media.lock().await;
    Ok(engine
        .selected_output_device()
        .map(|name| AudioDevice { id: name.clone(), name }))
}

#[tauri::command]
async fn set_output_device(state: State<'_, AppState>, device_id: String) -> Result<(), String> {
    let mut engine = state.media.lock().await;
    engine
        .set_output_device(Some(device_id.clone()))
        .map_err(|e| format!("Failed to set output device: {}", e))?;
    println!("🔊 [AUDIO] Output device set to: {}", device_id);
    Ok(())
}

#[tauri::command]
async fn get_audio_settings(state: State<'_, AppState>) -> Result<AudioSettings, String> {
    let engine = state.media.lock().await;
    Ok(engine.get_audio_settings())
}

#[tauri::command]
async fn update_audio_settings(state: State<'_, AppState>, settings: AudioSettings) -> Result<(), String> {
    let mut engine = state.media.lock().await;
    engine.update_audio_settings(settings);
    Ok(())
}

#[tauri::command]
async fn set_ptt_active(state: State<'_, AppState>, active: bool) -> Result<(), String> {
    let engine = state.media.lock().await;
    engine.set_ptt_active(active);
    Ok(())
}

#[tauri::command]
async fn set_remote_user_volume(state: State<'_, AppState>, volume: f32) -> Result<(), String> {
    let mut engine = state.media.lock().await;
    engine.set_remote_user_volume(volume);
    Ok(())
}

#[derive(serde::Deserialize)]
struct IceCandidatePayload {
    candidate: String,
    sdpMid: Option<String>,
    sdpMLineIndex: Option<u16>,
}

/// Start WebRTC handshake (Caller side)
/// Initializes PC, DC, creates Offer, and sends it via WS.
#[tauri::command]
async fn init_audio_call(state: State<'_, AppState>, target_id: String) -> Result<(), String> {
    println!("📞 [WEBRTC] Initializing audio call to {}", target_id);
    
    // 1. Initialize WebRTC
    let mut ice_rx = {
        let mut engine = state.media.lock().await;
        if !engine.is_ready_for_audio() {
            return Err("E2EE handshake not completed".to_string());
        }
        engine.init_webrtc().await.map_err(|e| e.to_string())?
    };

    // 2. Spawn ICE candidate forwarder
    let ws_sender = state.ws_sender.clone();
    let target_id_clone = target_id.clone();
    tokio::spawn(async move {
        while let Some(candidate_json) = ice_rx.recv().await {
            // Parse JSON to extract fields for SignalingMessage
            if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&candidate_json) {
                let candidate = parsed["candidate"].as_str().unwrap_or("").to_string();
                let sdp_mid = parsed["sdpMid"].as_str().map(|s| s.to_string());
                let sdp_m_line_index = parsed["sdpMLineIndex"].as_u64().map(|n| n as u16);

                let msg = SignalingMessage::Candidate {
                    target_id: target_id_clone.clone(),
                    candidate,
                    sdp_mid,
                    sdp_m_line_index,
                };
                let _ = signaling::send_signal(&ws_sender, msg).await;
            }
        }
    });

    // 3. Create Audio DataChannel
    {
        let engine = state.media.lock().await;
        engine.create_audio_channel().await.map_err(|e| e.to_string())?;
    }

    // 4. Create Offer
    let sdp = {
        let engine = state.media.lock().await;
        engine.create_offer().await.map_err(|e| e.to_string())?
    };
    println!("📞 [WEBRTC] Offer created, sending...");

    // 5. Send Offer via WS
    let msg = SignalingMessage::Offer { target_id, sdp };
    signaling::send_signal(&state.ws_sender, msg).await
}

/// Handle received Offer (Callee side)
/// Initializes PC, accepts offer, creates Answer, and sends it via WS.
#[tauri::command]
async fn handle_audio_offer(state: State<'_, AppState>, target_id: String, sdp: String) -> Result<(), String> {
    println!("📞 [WEBRTC] Handling Offer from {}", target_id);

    // 1. Initialize WebRTC (Answerer)
    let mut ice_rx = {
        let mut engine = state.media.lock().await;
        // Note: is_ready_for_audio check might fail if E2EE not finished, but normally it is.
        engine.init_webrtc().await.map_err(|e| e.to_string())?
    };

    // 2. Spawn ICE candidate forwarder
    let ws_sender = state.ws_sender.clone();
    let target_id_clone = target_id.clone();
    tokio::spawn(async move {
        while let Some(candidate_json) = ice_rx.recv().await {
            if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&candidate_json) {
                let candidate = parsed["candidate"].as_str().unwrap_or("").to_string();
                let sdp_mid = parsed["sdpMid"].as_str().map(|s| s.to_string());
                let sdp_m_line_index = parsed["sdpMLineIndex"].as_u64().map(|n| n as u16);

                let msg = SignalingMessage::Candidate {
                    target_id: target_id_clone.clone(),
                    candidate,
                    sdp_mid,
                    sdp_m_line_index,
                };
                let _ = signaling::send_signal(&ws_sender, msg).await;
            }
        }
    });

    // 3. Accept Offer and Create Answer
    let answer_sdp = {
        let engine = state.media.lock().await;
        engine.accept_offer(&sdp).await.map_err(|e| e.to_string())?
    };
    println!("📞 [WEBRTC] Answer created, sending...");

    // 4. Send Answer
    let msg = SignalingMessage::Answer { target_id, sdp: answer_sdp };
    signaling::send_signal(&state.ws_sender, msg).await
}

/// Handle received Answer (Caller side)
#[tauri::command]
async fn handle_audio_answer(state: State<'_, AppState>, sdp: String) -> Result<(), String> {
    println!("📞 [WEBRTC] Handling Answer");
    let engine = state.media.lock().await;
    engine.set_remote_description(&sdp).await.map_err(|e| e.to_string())?;
    Ok(())
}

/// Handle received ICE candidate
#[tauri::command]
async fn handle_ice_candidate(state: State<'_, AppState>, payload: IceCandidatePayload) -> Result<(), String> {
    // Reconstruct valid JSON for RTCIceCandidateInit
    let json = serde_json::json!({
        "candidate": payload.candidate,
        "sdpMid": payload.sdpMid,
        "sdpMLineIndex": payload.sdpMLineIndex
    });
    let candidate_str = json.to_string();

    let engine = state.media.lock().await;
    engine.add_ice_candidate(&candidate_str).await.map_err(|e| e.to_string())?;
    Ok(())
}

/// Start audio capture on call (after E2EE handshake)
#[tauri::command]
async fn start_call_audio(state: State<'_, AppState>) -> Result<(), String> {
    println!("🔊 [AUDIO] Starting call audio...");
    
    let engine = state.media.lock().await;
    
    if !engine.is_ready_for_audio() {
        return Err("E2EE key exchange not completed".to_string());
    }
    
    // Create the audio DataChannel (Offerer side)
    // This sets up capture → encode → encrypt → send pipeline
    engine.create_audio_channel().await.map_err(|e| format!("Failed to create audio channel: {}", e))?;
    
    println!("🔊 [AUDIO] ✅ Audio DataChannel created, capture will start when channel opens");
    
    Ok(())
}

/// Toggle mute/unmute for audio capture
#[tauri::command]
async fn toggle_mute(state: State<'_, AppState>) -> Result<bool, String> {
    let engine = state.media.lock().await;
    let muted = engine.toggle_mute();
    println!("🔊 [AUDIO] Mute toggled: {}", muted);
    Ok(muted)
}

/// Start VU meter — emits `vu-level` events to the frontend
#[tauri::command]
async fn start_vu_meter(app: tauri::AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    let mut rms_rx = None;
    for _ in 0..20 {
        {
            let engine = state.media.lock().await;
            rms_rx = engine.take_rms_receiver();
        }

        if rms_rx.is_some() {
            break;
        }

        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    }

    let mut rms_rx = rms_rx
        .ok_or_else(|| "RMS receiver not available (capture not started)".to_string())?;

    // Spawn a background task to forward RMS levels to the frontend
    tauri::async_runtime::spawn(async move {
        use std::time::{Duration, Instant};
        let mut last_emit = Instant::now();
        let throttle = Duration::from_millis(50); // ~20 FPS for smooth animation

        while let Some(rms) = rms_rx.recv().await {
            if last_emit.elapsed() >= throttle {
                let _ = app.emit("vu-level", rms);
                last_emit = Instant::now();
            }
        }
    });
    
    Ok(())
}

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(move |app| {
            let app_handle = app.handle().clone();

            // Initialize local messaging storage (SQLite in app data dir)
            let app_data_dir = app.path().app_data_dir().map_err(|e| {
                std::io::Error::new(
                    std::io::ErrorKind::Other,
                    format!("Failed to resolve app data directory: {e}"),
                )
            })?;
            std::fs::create_dir_all(&app_data_dir)?;
            let messaging_db_path = app_data_dir.join("messaging.sqlite");
            let messaging_service = tauri::async_runtime::block_on(MessagingService::new(messaging_db_path))
                .map_err(|e| {
                    std::io::Error::new(
                        std::io::ErrorKind::Other,
                        format!("Failed to initialize messaging storage: {e}"),
                    )
                })?;
            app.manage(MessagingState {
                service: messaging_service,
            });
             
            // Initialize API state for HTTP requests
            let api_state = ApiState::new(config::API_URL.to_string());
            app.manage(api_state);
            
            // Connect to signaling server (without identifying yet)
            tauri::async_runtime::spawn(async move {
                match signaling::connect(config::SERVER_URL, app_handle.clone()).await {
                    Ok(sender) => {
                        // Store the sender in app state
                        let state = AppState {
                            media: Arc::new(Mutex::new(MediaEngine::new())),
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
                            media: Arc::new(Mutex::new(MediaEngine::new())),
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
            // Audio commands
            list_audio_devices,
            get_default_audio_device,
            get_selected_audio_device,
            set_audio_device,
            list_output_devices,
            get_default_output_device,
            get_selected_output_device,
            set_output_device,
            get_audio_settings,
            update_audio_settings,
            set_ptt_active,
            set_remote_user_volume,
            toggle_mute,
            start_vu_meter,
            start_call_audio,
            init_audio_call,
            handle_audio_offer,
            handle_audio_answer,
            handle_ice_candidate,
            // API commands
            api::auth::api_login,
            api::auth::api_register,
            api::auth::api_logout,
            api::auth::api_health_check,
            api::users::api_upload_public_key,
            api::users::api_fetch_user_public_key,
            api::friends::api_fetch_friends,
            api::friends::api_fetch_pending_requests,
            api::friends::api_fetch_online_friends,
            api::friends::api_send_friend_request,
            api::friends::api_accept_friend,
            api::chat::api_create_or_get_dm,
            api::chat::api_fetch_messages,
            api::chat::api_send_message,
            api::chat::api_drain_outbox,
            api::chat::api_cache_message_status,
            api::chat::api_send_typing,
            api::chat::api_mark_message_delivered,
            api::chat::api_mark_room_read,
            api::chat::api_delete_message,
            api::chat::api_delete_all_messages,
            api::chat::api_edit_message,
            api::servers::api_fetch_servers,
            api::servers::api_create_server,
            api::servers::api_join_server,
            api::servers::api_leave_server,
            api::servers::api_fetch_server_details,
            api::servers::api_create_channel,
            api::servers::api_fetch_server_members,
            api::servers::api_fetch_channel_messages,
            api::servers::api_send_channel_message,
            api::servers::api_send_channel_typing,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}


