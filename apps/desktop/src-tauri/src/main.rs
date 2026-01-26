// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod config;
mod signaling;

use tauri::{State, Manager};
use media::MediaEngine;
use std::sync::Arc;
use tokio::sync::Mutex;
use signaling::WsSender;
use uuid::Uuid;

struct AppState {
    media: Arc<std::sync::Mutex<MediaEngine>>,
    ws_sender: WsSender,
    user_id: String,
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

    Ok(format!("Joined channel {} as {}", channel, state.user_id))
}

#[tauri::command]
async fn get_user_id(state: State<'_, AppState>) -> Result<String, String> {
    Ok(state.user_id.clone())
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
    let user_id = Uuid::new_v4().to_string();
    let user_id_clone = user_id.clone();

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(move |app| {
            let app_handle = app.handle().clone();
            let user_id_for_connect = user_id_clone.clone();
            
            // Connect to signaling server with AppHandle
            tauri::async_runtime::spawn(async move {
                match signaling::connect(config::SERVER_URL, &user_id_for_connect, app_handle.clone()).await {
                    Ok(sender) => {
                        // Store the sender in app state
                        let state = AppState {
                            media: Arc::new(std::sync::Mutex::new(MediaEngine::new())),
                            ws_sender: sender,
                            user_id: user_id_for_connect.clone(),
                        };
                        app_handle.manage(state);
                    }
                    Err(e) => {
                        eprintln!("Warning: Could not connect to signaling server: {}", e);
                        eprintln!("Server URL: {}", config::SERVER_URL);
                        // Manage with empty sender
                        let state = AppState {
                            media: Arc::new(std::sync::Mutex::new(MediaEngine::new())),
                            ws_sender: Arc::new(Mutex::new(None)),
                            user_id: user_id_for_connect,
                        };
                        app_handle.manage(state);
                    }
                }
            });
            
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![join_call, get_user_id, send_offer, send_answer])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
