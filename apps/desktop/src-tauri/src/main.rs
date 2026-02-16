// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod api;
mod backoff;
mod config;
mod error;
mod messaging;
mod observability;
mod protocol;
mod signaling;
mod updater;

use api::ApiState;
use error::AppResult;
use media::{AudioSettings, IceServerConfig, MediaEngine};
use messaging::service::MessagingService;
use shared_proto::signaling::SignalingMessage;
use signaling::WsSender;
use std::sync::Arc;
use tauri::{Emitter, Manager, State};
use tokio::sync::Mutex;

struct AppState {
    media: Arc<Mutex<MediaEngine>>,
    ws_sender: WsSender,
}

#[derive(Clone)]
pub struct MessagingState {
    pub service: MessagingService,
}

fn parse_csv_env(name: &str) -> Vec<String> {
    std::env::var(name)
        .ok()
        .map(|raw| {
            raw.split(',')
                .map(|v| v.trim().to_string())
                .filter(|v| !v.is_empty())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default()
}

fn load_ice_servers_from_env() -> Vec<IceServerConfig> {
    if let Ok(raw_json) = std::env::var("ICE_SERVERS_JSON") {
        if let Ok(parsed) = serde_json::from_str::<Vec<IceServerConfig>>(&raw_json) {
            if !parsed.is_empty() {
                return parsed;
            }
        }
        eprintln!("[ICE] Failed to parse ICE_SERVERS_JSON, falling back to STUN/TURN vars");
    }

    let mut servers: Vec<IceServerConfig> = Vec::new();

    let mut stun_urls = parse_csv_env("STUN_URLS");
    if stun_urls.is_empty() {
        stun_urls.push("stun:stun.l.google.com:19302".to_string());
    }
    servers.push(IceServerConfig {
        urls: stun_urls,
        username: None,
        credential: None,
    });

    let turn_urls = parse_csv_env("TURN_URLS");
    if !turn_urls.is_empty() {
        let username = std::env::var("TURN_USERNAME")
            .ok()
            .filter(|v| !v.trim().is_empty());
        let credential = std::env::var("TURN_PASSWORD")
            .or_else(|_| std::env::var("TURN_CREDENTIAL"))
            .ok()
            .filter(|v| !v.trim().is_empty());

        servers.push(IceServerConfig {
            urls: turn_urls,
            username,
            credential,
        });
    }

    servers
}

#[tauri::command]
async fn identify_user(
    state: State<'_, AppState>,
    api_state: State<'_, ApiState>,
    user_id: String,
) -> AppResult<()> {
    tracing::info!(
        component = "ws.identify",
        user_id = %user_id,
        protocol_version = protocol::PROTOCOL_VERSION,
        "identifying websocket user"
    );
    let token = api_state.bearer_token().await?;
    signaling::send_identify(&state.ws_sender, &user_id, &token).await?;
    Ok(())
}

#[tauri::command]
async fn send_offer(
    state: State<'_, AppState>,
    target_id: String,
    sdp: String,
) -> AppResult<()> {
    let msg = SignalingMessage::Offer {
        version: protocol::PROTOCOL_VERSION,
        trace_id: Some(observability::trace_id().to_string()),
        target_id,
        sdp,
    };
    signaling::send_signal(&state.ws_sender, msg).await
}

#[tauri::command]
async fn send_answer(
    state: State<'_, AppState>,
    target_id: String,
    sdp: String,
) -> AppResult<()> {
    let msg = SignalingMessage::Answer {
        version: protocol::PROTOCOL_VERSION,
        trace_id: Some(observability::trace_id().to_string()),
        target_id,
        sdp,
    };
    signaling::send_signal(&state.ws_sender, msg).await
}

// === Call Commands ===

/// Start a call to a friend - generates keypair and sends CallInitiate
#[tauri::command]
async fn start_call(state: State<'_, AppState>, target_id: String) -> AppResult<String> {
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
    println!(
        "📞 [CALL-DEBUG] Generated public key: {}...",
        &public_key[..30.min(public_key.len())]
    );

    // Send call initiate signal
    println!("📞 [CALL-DEBUG] Sending CallInitiate signal...");
    let msg = SignalingMessage::CallInitiate {
        version: protocol::PROTOCOL_VERSION,
        trace_id: Some(observability::trace_id().to_string()),
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
) -> AppResult<String> {
    println!("✅ [CALL-DEBUG] ===== ACCEPTING CALL =====");
    println!("✅ [CALL-DEBUG] Caller ID: {}", caller_id);
    println!(
        "✅ [CALL-DEBUG] Caller public key: {}...",
        &caller_public_key[..30.min(caller_public_key.len())]
    );

    // Generate our keypair and complete key exchange
    println!("✅ [CALL-DEBUG] Generating keypair and completing key exchange...");
    let public_key = {
        let mut engine = state.media.lock().await;
        let pk = engine.generate_keypair().map_err(|e| {
            println!("✅ [CALL-DEBUG] ❌ Failed to generate keypair: {}", e);
            e.to_string()
        })?;
        engine
            .complete_key_exchange(&caller_public_key)
            .map_err(|e| {
                println!("✅ [CALL-DEBUG] ❌ Key exchange failed: {}", e);
                e.to_string()
            })?;
        pk
    };
    println!(
        "✅ [CALL-DEBUG] Generated our public key: {}...",
        &public_key[..30.min(public_key.len())]
    );

    // Send accept signal with our public key
    println!("✅ [CALL-DEBUG] Sending CallAccept signal...");
    let msg = SignalingMessage::CallAccept {
        version: protocol::PROTOCOL_VERSION,
        trace_id: Some(observability::trace_id().to_string()),
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
) -> AppResult<()> {
    println!("🔐 [CALL-DEBUG] ===== COMPLETING E2EE HANDSHAKE =====");
    println!(
        "🔐 [CALL-DEBUG] Peer public key (first 30 chars): {}...",
        &peer_public_key[..30.min(peer_public_key.len())]
    );

    {
        let mut engine = state.media.lock().await;
        println!("🔐 [CALL-DEBUG] Got media engine lock");
        engine
            .complete_key_exchange(&peer_public_key)
            .map_err(|e| {
                println!("🔐 [CALL-DEBUG] ❌ Key exchange failed: {}", e);
                e.to_string()
            })?;
    }

    println!("🔐 [CALL-DEBUG] ✅ Key exchange completed successfully");
    Ok(())
}

/// Decline incoming call
#[tauri::command]
async fn decline_call(state: State<'_, AppState>, caller_id: String) -> AppResult<()> {
    println!("❌ Declining call from {}", caller_id);

    // Reset media engine (may have generated keypair)
    {
        let mut engine = state.media.lock().await;
        engine.reset().await;
    }

    let msg = SignalingMessage::CallDecline {
        version: protocol::PROTOCOL_VERSION,
        trace_id: Some(observability::trace_id().to_string()),
        caller_id,
    };
    signaling::send_signal(&state.ws_sender, msg).await
}

/// End active call
#[tauri::command]
async fn end_call(state: State<'_, AppState>, peer_id: String) -> AppResult<()> {
    println!("📴 Ending call with {}", peer_id);

    // Reset media engine for next call
    {
        let mut engine = state.media.lock().await;
        engine.reset().await;
    }

    let msg = SignalingMessage::CallEnd {
        version: protocol::PROTOCOL_VERSION,
        trace_id: Some(observability::trace_id().to_string()),
        peer_id,
    };
    signaling::send_signal(&state.ws_sender, msg).await
}

/// Cancel outgoing call before answer
#[tauri::command]
async fn cancel_call(state: State<'_, AppState>, target_id: String) -> AppResult<()> {
    println!("🚫 Cancelling call to {}", target_id);

    // Reset media engine
    {
        let mut engine = state.media.lock().await;
        engine.reset().await;
    }

    let msg = SignalingMessage::CallCancel {
        version: protocol::PROTOCOL_VERSION,
        trace_id: Some(observability::trace_id().to_string()),
        target_id,
    };
    signaling::send_signal(&state.ws_sender, msg).await
}

/// Reset local call media state without sending signaling
#[tauri::command]
async fn reset_call_media(state: State<'_, AppState>) -> AppResult<()> {
    let mut engine = state.media.lock().await;
    engine.reset().await;
    Ok(())
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
async fn list_audio_devices() -> AppResult<Vec<AudioDevice>> {
    Ok(
        MediaEngine::list_input_devices()
            .map(|devices| {
                devices
                    .into_iter()
                    .map(|(id, name)| AudioDevice { id, name })
                    .collect()
            })
            .map_err(|e| e.to_string())?,
    )
}

#[tauri::command]
async fn get_default_audio_device() -> AppResult<AudioDevice> {
    Ok(
        MediaEngine::default_input_device_name()
            .map(|name| AudioDevice {
                id: name.clone(),
                name,
            })
            .map_err(|e| e.to_string())?,
    )
}

#[tauri::command]
async fn get_selected_audio_device(
    state: State<'_, AppState>,
) -> AppResult<Option<AudioDevice>> {
    let engine = state.media.lock().await;
    Ok(engine.selected_input_device().map(|name| AudioDevice {
        id: name.clone(),
        name,
    }))
}

#[tauri::command]
async fn set_audio_device(state: State<'_, AppState>, device_id: String) -> AppResult<()> {
    let mut engine = state.media.lock().await;
    engine
        .set_input_device(Some(device_id.clone()))
        .map_err(|e| format!("Failed to set audio device: {}", e))?;
    println!("🔊 [AUDIO] Input device set to: {}", device_id);
    Ok(())
}

#[tauri::command]
async fn list_output_devices() -> AppResult<Vec<AudioDevice>> {
    Ok(
        MediaEngine::list_output_devices()
            .map(|devices| {
                devices
                    .into_iter()
                    .map(|(id, name)| AudioDevice { id, name })
                    .collect()
            })
            .map_err(|e| e.to_string())?,
    )
}

#[tauri::command]
async fn get_default_output_device() -> AppResult<AudioDevice> {
    Ok(
        MediaEngine::default_output_device_name()
            .map(|name| AudioDevice {
                id: name.clone(),
                name,
            })
            .map_err(|e| e.to_string())?,
    )
}

#[tauri::command]
async fn get_selected_output_device(
    state: State<'_, AppState>,
) -> AppResult<Option<AudioDevice>> {
    let engine = state.media.lock().await;
    Ok(engine.selected_output_device().map(|name| AudioDevice {
        id: name.clone(),
        name,
    }))
}

#[tauri::command]
async fn set_output_device(state: State<'_, AppState>, device_id: String) -> AppResult<()> {
    let mut engine = state.media.lock().await;
    engine
        .set_output_device(Some(device_id.clone()))
        .map_err(|e| format!("Failed to set output device: {}", e))?;
    println!("🔊 [AUDIO] Output device set to: {}", device_id);
    Ok(())
}

#[tauri::command]
async fn get_audio_settings(state: State<'_, AppState>) -> AppResult<AudioSettings> {
    let engine = state.media.lock().await;
    Ok(engine.get_audio_settings())
}

#[tauri::command]
async fn update_audio_settings(
    state: State<'_, AppState>,
    settings: AudioSettings,
) -> AppResult<()> {
    let mut engine = state.media.lock().await;
    engine.update_audio_settings(settings);
    Ok(())
}

#[tauri::command]
async fn set_ptt_active(state: State<'_, AppState>, active: bool) -> AppResult<()> {
    let engine = state.media.lock().await;
    engine.set_ptt_active(active);
    Ok(())
}

#[tauri::command]
async fn set_remote_user_volume(state: State<'_, AppState>, volume: f32) -> AppResult<()> {
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
async fn init_audio_call(state: State<'_, AppState>, target_id: String) -> AppResult<()> {
    println!("📞 [WEBRTC] Initializing audio call to {}", target_id);

    // 1. Initialize WebRTC
    let mut ice_rx = {
        let mut engine = state.media.lock().await;
        if !engine.is_ready_for_audio() {
            return Err("E2EE handshake not completed".to_string().into());
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
                    version: protocol::PROTOCOL_VERSION,
                    trace_id: Some(observability::trace_id().to_string()),
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
        engine
            .create_audio_channel()
            .await
            .map_err(|e| e.to_string())?;
    }

    // 4. Create Offer
    let sdp = {
        let engine = state.media.lock().await;
        engine.create_offer().await.map_err(|e| e.to_string())?
    };
    println!("📞 [WEBRTC] Offer created, sending...");

    // 5. Send Offer via WS
    let msg = SignalingMessage::Offer {
        version: protocol::PROTOCOL_VERSION,
        trace_id: Some(observability::trace_id().to_string()),
        target_id,
        sdp,
    };
    signaling::send_signal(&state.ws_sender, msg).await
}

/// Handle received Offer (Callee side)
/// Initializes PC, accepts offer, creates Answer, and sends it via WS.
#[tauri::command]
async fn handle_audio_offer(
    state: State<'_, AppState>,
    target_id: String,
    sdp: String,
) -> AppResult<()> {
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
                    version: protocol::PROTOCOL_VERSION,
                    trace_id: Some(observability::trace_id().to_string()),
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
    let msg = SignalingMessage::Answer {
        version: protocol::PROTOCOL_VERSION,
        trace_id: Some(observability::trace_id().to_string()),
        target_id,
        sdp: answer_sdp,
    };
    signaling::send_signal(&state.ws_sender, msg).await
}

/// Handle received Answer (Caller side)
#[tauri::command]
async fn handle_audio_answer(state: State<'_, AppState>, sdp: String) -> AppResult<()> {
    println!("📞 [WEBRTC] Handling Answer");
    let engine = state.media.lock().await;
    engine
        .set_remote_description(&sdp)
        .await
        .map_err(|e| e.to_string())?;
    Ok(())
}

/// Handle received ICE candidate
#[tauri::command]
async fn handle_ice_candidate(
    state: State<'_, AppState>,
    payload: IceCandidatePayload,
) -> AppResult<()> {
    // Reconstruct valid JSON for RTCIceCandidateInit
    let json = serde_json::json!({
        "candidate": payload.candidate,
        "sdpMid": payload.sdpMid,
        "sdpMLineIndex": payload.sdpMLineIndex
    });
    let candidate_str = json.to_string();

    let engine = state.media.lock().await;
    engine
        .add_ice_candidate(&candidate_str)
        .await
        .map_err(|e| e.to_string())?;
    Ok(())
}

/// Start audio capture on call (after E2EE handshake)
#[tauri::command]
async fn start_call_audio(state: State<'_, AppState>) -> AppResult<()> {
    println!("🔊 [AUDIO] Starting call audio...");

    let engine = state.media.lock().await;

    if !engine.is_ready_for_audio() {
        return Err("E2EE key exchange not completed".to_string().into());
    }

    // Create the audio DataChannel (Offerer side)
    // This sets up capture → encode → encrypt → send pipeline
    engine
        .create_audio_channel()
        .await
        .map_err(|e| format!("Failed to create audio channel: {}", e))?;

    println!("🔊 [AUDIO] ✅ Audio DataChannel created, capture will start when channel opens");

    Ok(())
}

/// Toggle mute/unmute for audio capture
#[tauri::command]
async fn toggle_mute(state: State<'_, AppState>) -> AppResult<bool> {
    let engine = state.media.lock().await;
    let muted = engine.toggle_mute();
    println!("🔊 [AUDIO] Mute toggled: {}", muted);
    Ok(muted)
}

/// Start VU meter — emits `vu-level` events to the frontend
#[tauri::command]
async fn start_vu_meter(app: tauri::AppHandle, state: State<'_, AppState>) -> AppResult<()> {
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

    let mut rms_rx =
        rms_rx.ok_or_else(|| "RMS receiver not available (capture not started)".to_string())?;

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
    observability::init_tracing();

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .setup(move |app| {
            let app_handle = app.handle().clone();

            app.manage(updater::PendingUpdate::default());

            // Initialize local messaging storage (SQLite in app data dir)
            let app_data_dir = app.path().app_data_dir().map_err(|e| {
                std::io::Error::new(
                    std::io::ErrorKind::Other,
                    format!("Failed to resolve app data directory: {e}"),
                )
            })?;
            std::fs::create_dir_all(&app_data_dir)?;
            let messaging_db_path = app_data_dir.join("messaging.sqlite");
            let messaging_service = tauri::async_runtime::block_on(MessagingService::new(
                messaging_db_path,
            ))
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
                let ice_servers = load_ice_servers_from_env();

                let configured_urls = ice_servers
                    .iter()
                    .flat_map(|s| s.urls.iter().cloned())
                    .collect::<Vec<_>>();
                println!("[ICE] Configured ICE servers: {:?}", configured_urls);

                match signaling::connect(config::SERVER_URL, app_handle.clone()).await {
                    Ok(sender) => {
                        let mut media_engine = MediaEngine::new();
                        media_engine.set_ice_servers(ice_servers.clone());

                        // Store the sender in app state
                        let state = AppState {
                            media: Arc::new(Mutex::new(media_engine)),
                            ws_sender: sender,
                        };
                        app_handle.manage(state);
                        println!("WebSocket connected. Waiting for user login to identify...");
                    }
                    Err(e) => {
                        eprintln!("Warning: Could not connect to signaling server: {}", e);
                        eprintln!("Server URL: {}", config::SERVER_URL);

                        let mut media_engine = MediaEngine::new();
                        media_engine.set_ice_servers(ice_servers.clone());

                        // Manage with empty sender
                        let state = AppState {
                            media: Arc::new(Mutex::new(media_engine)),
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
            send_offer,
            send_answer,
            identify_user,
            start_call,
            accept_call,
            complete_call_handshake,
            decline_call,
            end_call,
            cancel_call,
            reset_call_media,
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
            api::users::api_fetch_my_profile,
            api::users::api_update_my_profile,
            api::users::api_fetch_my_settings,
            api::users::api_update_my_settings,
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
            api::chat::api_search_messages,
            api::chat::api_fetch_message_reactions,
            api::chat::api_add_message_reaction,
            api::chat::api_remove_message_reaction,
            api::chat::api_fetch_thread_messages,
            api::chat::api_send_thread_message,
            api::servers::api_fetch_servers,
            api::servers::api_create_server,
            api::servers::api_join_server,
            api::servers::api_leave_server,
            api::servers::api_delete_server,
            api::servers::api_regenerate_server_invite,
            api::servers::api_fetch_server_details,
            api::servers::api_create_channel,
            api::servers::api_update_member_role,
            api::servers::api_kick_member,
            api::servers::api_ban_member,
            api::servers::api_list_server_bans,
            api::servers::api_unban_member,
            api::servers::api_fetch_server_members,
            api::servers::api_fetch_channel_messages,
            api::servers::api_search_channel_messages,
            api::servers::api_send_channel_message,
            api::servers::api_fetch_channel_message_reactions,
            api::servers::api_add_channel_message_reaction,
            api::servers::api_remove_channel_message_reaction,
            api::servers::api_fetch_channel_thread_messages,
            api::servers::api_send_channel_thread_message,
            api::servers::api_send_channel_typing,
            api::servers::api_fetch_voice_channel_presence,
            api::servers::api_join_voice_channel,
            api::servers::api_leave_voice_channel,
            updater::app_check_for_updates,
            updater::app_download_and_install_update,
            updater::app_restart_after_update,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
