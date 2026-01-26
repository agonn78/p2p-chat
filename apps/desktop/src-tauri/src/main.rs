// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod config;
mod signaling;

use tauri::{State, Manager};
use media::MediaEngine;
use std::sync::Arc;
use tokio::sync::Mutex;
use signaling::WsSender;

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
    let msg = shared_proto::signaling::SignalingMessage::Offer { target_id, sdp };
    signaling::send_signal(&state.ws_sender, msg).await
}

#[tauri::command]
async fn send_answer(state: State<'_, AppState>, target_id: String, sdp: String) -> Result<(), String> {
    let msg = shared_proto::signaling::SignalingMessage::Answer { target_id, sdp };
    signaling::send_signal(&state.ws_sender, msg).await
}

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(move |app| {
            let app_handle = app.handle().clone();
            
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
        .invoke_handler(tauri::generate_handler![join_call, send_offer, send_answer, identify_user])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
