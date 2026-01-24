// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use tauri::State;
use media::MediaEngine;
use std::sync::{Arc, Mutex};

struct AppState {
    media: Arc<Mutex<MediaEngine>>,
}

#[tauri::command]
async fn join_call(state: State<'_, AppState>, channel: String) -> Result<(), String> {
    println!("Joining call in channel: {}", channel);
    let media = state.media.clone();
    
    // In a real app, this would be spawned or async handled better
    std::thread::spawn(move || {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let engine = media.lock().unwrap();
            if let Err(e) = engine.start().await {
                eprintln!("Media engine error: {}", e);
            }
        });
    });

    Ok(())
}

fn main() {
    let media_engine = MediaEngine::new();

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        .manage(AppState {
            media: Arc::new(Mutex::new(media_engine)),
        })
        .invoke_handler(tauri::generate_handler![join_call])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
