//! P2P Nitro Media Engine
//! 
//! Provides E2EE audio/video communication over WebRTC.
//! 
//! Pipeline: cpal (capture) → audiopus (encode) → ring (encrypt) → webrtc-rs (send)

mod audio;
mod crypto;

use anyhow::Result;
use cpal::traits::{DeviceTrait, HostTrait};
use std::sync::{Arc, atomic::{AtomicBool, Ordering}};
use tokio::sync::mpsc;
use webrtc::api::media_engine::MediaEngine as WebRtcMediaEngine;
use webrtc::api::APIBuilder;
use webrtc::data_channel::data_channel_init::RTCDataChannelInit;
use webrtc::data_channel::data_channel_message::DataChannelMessage;
use webrtc::data_channel::RTCDataChannel;
use webrtc::ice_transport::ice_candidate::{RTCIceCandidate, RTCIceCandidateInit};
use webrtc::ice_transport::ice_server::RTCIceServer;
use webrtc::peer_connection::configuration::RTCConfiguration;
use webrtc::peer_connection::peer_connection_state::RTCPeerConnectionState;
use webrtc::peer_connection::sdp::session_description::RTCSessionDescription;
use webrtc::peer_connection::RTCPeerConnection;
// Required for ICE candidate methods
use webrtc::peer_connection::policy::ice_transport_policy::RTCIceTransportPolicy;

pub use audio::{AudioCapture, AudioPacket, AudioPlayback, VoiceMode};
pub use crypto::{CryptoContext, KeyPair};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AudioMode {
    Headphones,
    Speakers,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct IceServerConfig {
    pub urls: Vec<String>,
    pub username: Option<String>,
    pub credential: Option<String>,
}

impl Default for IceServerConfig {
    fn default() -> Self {
        Self {
            urls: vec!["stun:stun.l.google.com:19302".to_string()],
            username: None,
            credential: None,
        }
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AudioSettings {
    pub mic_gain: f32,
    pub output_volume: f32,
    pub remote_user_volume: f32,
    pub voice_mode: String,
    pub vad_threshold: f32,
    pub noise_suppression: bool,
    pub aec: bool,
    pub agc: bool,
    pub noise_gate: bool,
    pub noise_gate_threshold: f32,
    pub limiter: bool,
    pub deafen: bool,
    pub ptt_key: String,
    pub audio_mode: AudioMode,
}

impl Default for AudioSettings {
    fn default() -> Self {
        Self {
            mic_gain: 1.0,
            output_volume: 1.0,
            remote_user_volume: 1.0,
            voice_mode: "voice_activity".to_string(),
            vad_threshold: 0.02,
            noise_suppression: true,
            aec: true,
            agc: true,
            noise_gate: true,
            noise_gate_threshold: 0.01,
            limiter: true,
            deafen: false,
            ptt_key: "V".to_string(),
            audio_mode: AudioMode::Headphones,
        }
    }
}

/// Media engine state
pub struct MediaEngine {
    /// Our key pair for E2EE
    keypair: Option<crypto::KeyPair>,
    /// Derived crypto context after key exchange
    crypto_ctx: Option<Arc<CryptoContext>>,
    
    // WebRTC components
    rtc_connection: Option<Arc<RTCPeerConnection>>,
    
    // Audio components
    audio_capture: Option<Arc<AudioCapture>>,
    audio_playback: Option<Arc<AudioPlayback>>,
    // Preferred input device name chosen by user
    selected_input_device: Option<String>,
    // Preferred output device name chosen by user
    selected_output_device: Option<String>,
    // Runtime audio settings
    audio_settings: AudioSettings,
    // Runtime ICE server configuration
    ice_servers: Vec<IceServerConfig>,
    /// Track whether playback stream has been started
    playback_started: Arc<AtomicBool>,
}

impl Default for MediaEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl MediaEngine {
    pub fn new() -> Self {
        Self {
            keypair: None,
            crypto_ctx: None,
            rtc_connection: None,
            audio_capture: None,
            audio_playback: None,
            selected_input_device: None,
            selected_output_device: None,
            audio_settings: AudioSettings::default(),
            ice_servers: vec![IceServerConfig::default()],
            playback_started: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn set_ice_servers(&mut self, ice_servers: Vec<IceServerConfig>) {
        self.ice_servers = if ice_servers.is_empty() {
            vec![IceServerConfig::default()]
        } else {
            ice_servers
        };
    }

    pub fn get_ice_servers(&self) -> Vec<IceServerConfig> {
        self.ice_servers.clone()
    }

    /// Reset the media engine for a new call
    /// Must be called when a call ends to clean up all state
    pub async fn reset(&mut self) {
        // Stop audio capture
        if let Some(capture) = &self.audio_capture {
            capture.stop();
        }

        // Stop audio playback
        if let Some(playback) = &self.audio_playback {
            playback.stop();
        }
        
        // Close WebRTC connection
        if let Some(pc) = self.rtc_connection.take() {
            let _ = pc.close().await;
            tracing::info!("WebRTC connection closed");
        }
        
        self.keypair = None;
        self.crypto_ctx = None;
        self.audio_capture = None;
        self.audio_playback = None;
        self.playback_started.store(false, Ordering::SeqCst);
        tracing::info!("MediaEngine reset for next call");
    }

    /// Generate a new key pair for E2EE
    /// Returns the public key as base64 to send to the peer
    pub fn generate_keypair(&mut self) -> Result<String> {
        let keypair = crypto::KeyPair::generate()
            .map_err(|_| anyhow::anyhow!("Failed to generate keypair"))?;
        let public_key = keypair.public_key_base64();
        self.keypair = Some(keypair);
        Ok(public_key)
    }

    /// Complete key exchange with peer's public key
    pub fn complete_key_exchange(&mut self, peer_public_key_base64: &str) -> Result<()> {
        let keypair = self.keypair.take()
            .ok_or_else(|| anyhow::anyhow!("No keypair generated"))?;
        
        let peer_key_bytes = crypto::parse_public_key(peer_public_key_base64)
            .map_err(|e| anyhow::anyhow!(e))?;
        let ctx = keypair.derive_shared_secret(&peer_key_bytes)
            .map_err(|e| anyhow::anyhow!(e))?;
        
        self.crypto_ctx = Some(Arc::new(ctx));
        tracing::info!("E2EE key exchange completed successfully");
        Ok(())
    }

    /// Check if we have completed key exchange and are ready for audio
    pub fn is_ready_for_audio(&self) -> bool {
        self.crypto_ctx.is_some()
    }

    /// Toggle mute on/off. Returns the new mute state.
    pub fn toggle_mute(&self) -> bool {
        if let Some(capture) = &self.audio_capture {
            let new_state = !capture.is_muted();
            capture.set_muted(new_state);
            new_state
        } else {
            false
        }
    }

    /// Get current mute state
    pub fn is_muted(&self) -> bool {
        self.audio_capture.as_ref().map(|c| c.is_muted()).unwrap_or(false)
    }

    /// Take the RMS receiver for VU meter updates
    pub fn take_rms_receiver(&self) -> Option<tokio::sync::mpsc::UnboundedReceiver<f32>> {
        self.audio_capture.as_ref().and_then(|c| c.take_rms_receiver())
    }

    /// List available input (microphone) devices
    pub fn list_input_devices() -> Result<Vec<(String, String)>> {
        let host = cpal::default_host();
        let mut devices = Vec::new();
        
        for device in host.input_devices()? {
            if let Ok(name) = device.name() {
                // Use name as both ID and display name for now
                devices.push((name.clone(), name));
            }
        }
        
        Ok(devices)
    }

    /// Get the default input device name
    pub fn default_input_device_name() -> Result<String> {
        let host = cpal::default_host();
        let device = host.default_input_device()
            .ok_or_else(|| anyhow::anyhow!("No default input device"))?;
        device.name().map_err(|e| anyhow::anyhow!(e))
    }

    /// Return currently selected input device (if set by user)
    pub fn selected_input_device(&self) -> Option<String> {
        self.selected_input_device.clone()
    }

    /// Set preferred input device and hot-switch capture if already running
    pub fn set_input_device(&mut self, device_name: Option<String>) -> Result<()> {
        let normalized = device_name
            .map(|d| d.trim().to_string())
            .filter(|d| !d.is_empty());

        self.selected_input_device = normalized;

        if let Some(capture) = &self.audio_capture {
            let was_running = capture.is_running();
            if was_running {
                capture.stop();
                std::thread::sleep(std::time::Duration::from_millis(120));
                capture.start_with_device(self.selected_input_device.as_deref())?;
                tracing::info!(
                    "Input device switched to {:?}",
                    self.selected_input_device.as_deref().unwrap_or("default")
                );
            }
        }

        Ok(())
    }

    /// List available output (speaker/headphone) devices
    pub fn list_output_devices() -> Result<Vec<(String, String)>> {
        let host = cpal::default_host();
        let mut devices = Vec::new();

        for device in host.output_devices()? {
            if let Ok(name) = device.name() {
                devices.push((name.clone(), name));
            }
        }

        Ok(devices)
    }

    /// Get the default output device name
    pub fn default_output_device_name() -> Result<String> {
        let host = cpal::default_host();
        let device = host.default_output_device()
            .ok_or_else(|| anyhow::anyhow!("No default output device"))?;
        device.name().map_err(|e| anyhow::anyhow!(e))
    }

    /// Return currently selected output device (if set by user)
    pub fn selected_output_device(&self) -> Option<String> {
        self.selected_output_device.clone()
    }

    /// Set preferred output device and hot-switch playback if already running
    pub fn set_output_device(&mut self, device_name: Option<String>) -> Result<()> {
        let normalized = device_name
            .map(|d| d.trim().to_string())
            .filter(|d| !d.is_empty());

        self.selected_output_device = normalized;

        if let Some(playback) = &self.audio_playback {
            let was_running = playback.is_running();
            if was_running {
                playback.stop();
                std::thread::sleep(std::time::Duration::from_millis(120));
                playback.start_with_device(self.selected_output_device.as_deref())?;
                tracing::info!(
                    "Output device switched to {:?}",
                    self.selected_output_device.as_deref().unwrap_or("default")
                );
            }
        }

        Ok(())
    }

    fn parse_voice_mode(mode: &str) -> VoiceMode {
        match mode {
            "mute" => VoiceMode::Mute,
            "push_to_talk" => VoiceMode::PushToTalk,
            _ => VoiceMode::VoiceActivity,
        }
    }

    fn apply_audio_settings_to_runtime(&self) {
        let effective_aec = match self.audio_settings.audio_mode {
            AudioMode::Headphones => false,
            AudioMode::Speakers => self.audio_settings.aec,
        };

        if let Some(capture) = &self.audio_capture {
            capture.set_input_gain(self.audio_settings.mic_gain);
            capture.set_voice_mode(Self::parse_voice_mode(&self.audio_settings.voice_mode));
            capture.set_vad_threshold(self.audio_settings.vad_threshold);
            capture.set_noise_suppression(self.audio_settings.noise_suppression);
            capture.set_aec_enabled(effective_aec);
            capture.set_agc_enabled(self.audio_settings.agc);
            capture.set_noise_gate_enabled(self.audio_settings.noise_gate);
            capture.set_noise_gate_threshold(self.audio_settings.noise_gate_threshold);
            capture.set_muted(self.audio_settings.deafen || self.audio_settings.voice_mode == "mute");
        }

        if let Some(playback) = &self.audio_playback {
            playback.set_output_volume(self.audio_settings.output_volume);
            playback.set_remote_volume(self.audio_settings.remote_user_volume);
            playback.set_limiter_enabled(self.audio_settings.limiter);
            playback.set_muted(self.audio_settings.deafen);
        }
    }

    pub fn get_audio_settings(&self) -> AudioSettings {
        self.audio_settings.clone()
    }

    pub fn update_audio_settings(&mut self, settings: AudioSettings) {
        self.audio_settings = settings;
        self.apply_audio_settings_to_runtime();
    }

    pub fn set_ptt_active(&self, active: bool) {
        if let Some(capture) = &self.audio_capture {
            capture.set_ptt_active(active);
        }
    }

    pub fn set_remote_user_volume(&mut self, volume: f32) {
        self.audio_settings.remote_user_volume = volume.clamp(0.0, 2.0);
        if let Some(playback) = &self.audio_playback {
            playback.set_remote_volume(self.audio_settings.remote_user_volume);
        }
    }

    // === WebRTC Implementation ===

    /// Initialize WebRTC PeerConnection
    /// Returns a receiver for local ICE candidates that must be sent to the peer
    pub async fn init_webrtc(&mut self) -> Result<mpsc::Receiver<String>> {
        let mut media_engine = WebRtcMediaEngine::default();
        media_engine.register_default_codecs()?;

        let api = APIBuilder::new()
            .with_media_engine(media_engine)
            .build();

        let ice_servers = self
            .ice_servers
            .iter()
            .map(|cfg| {
                let mut server = RTCIceServer {
                    urls: cfg.urls.clone(),
                    ..Default::default()
                };
                if let Some(username) = &cfg.username {
                    server.username = username.clone();
                }
                if let Some(credential) = &cfg.credential {
                    server.credential = credential.clone();
                }
                server
            })
            .collect::<Vec<_>>();

        let config = RTCConfiguration {
            ice_servers,
            ice_transport_policy: RTCIceTransportPolicy::All, // Allow both UDP and TCP
            ..Default::default()
        };

        let pc = Arc::new(api.new_peer_connection(config).await?);
        let (ice_tx, ice_rx) = mpsc::channel(10);

        // Handle ICE candidates
        pc.on_ice_candidate(Box::new(move |candidate: Option<RTCIceCandidate>| {
            let ice_tx = ice_tx.clone();
            Box::pin(async move {
                if let Some(candidate) = candidate {
                    // candidate.to_json() returns Result<RTCIceCandidateInit, webrtc::Error>
                    // parameters. webrtc::Error is not serializable, so we must unwrap the result first.
                    if let Ok(ice_candidate_init) = candidate.to_json() {
                        if let Ok(json) = serde_json::to_string(&ice_candidate_init) {
                            let _ = ice_tx.send(json).await;
                        }
                    }
                }
            })
        }));

        pc.on_peer_connection_state_change(Box::new(move |s: RTCPeerConnectionState| {
            tracing::info!("Peer Connection State has changed: {}", s);
            Box::pin(async {})
        }));

        // Initialize Audio Components if we have crypto context
        if let Some(ctx) = &self.crypto_ctx {
            // Setup Playback
            let playback = Arc::new(AudioPlayback::new(ctx.clone())?);
            self.audio_playback = Some(playback.clone());
            let shared_playback_rms = playback.output_rms_shared();
             
            // Setup Capture
            let capture = Arc::new(AudioCapture::new(ctx.clone(), shared_playback_rms)?);
            self.audio_capture = Some(capture.clone());

            self.apply_audio_settings_to_runtime();

            // Clone for on_data_channel closures
            let playback_started_clone = self.playback_started.clone();
            let preferred_input_device = self.selected_input_device.clone();
            let preferred_output_device = self.selected_output_device.clone();

            // Handle incoming DataChannel (Answerer side receives channel created by Offerer)
            let playback_clone = playback.clone();
            let capture_clone = capture.clone();
            let preferred_input_device_clone = preferred_input_device.clone();
            let preferred_output_device_clone = preferred_output_device.clone();
            
            pc.on_data_channel(Box::new(move |d_channel: Arc<RTCDataChannel>| {
                let playback = playback_clone.clone();
                let capture = capture_clone.clone();
                let playback_started = playback_started_clone.clone();
                let preferred_input_device = preferred_input_device_clone.clone();
                let preferred_output_device = preferred_output_device_clone.clone();
                
                Box::pin(async move {
                    tracing::info!("New DataChannel {} {}", d_channel.label(), d_channel.id());
                    
                    let d_channel_clone = d_channel.clone();
                    let playback_for_open = playback.clone();
                    let ps_for_open = playback_started.clone();
                    let preferred_input_for_open = preferred_input_device.clone();
                    let preferred_output_for_open = preferred_output_device.clone();
                    d_channel.on_open(Box::new(move || {
                        tracing::info!("Data channel opened (Answerer)");
                        let dc = d_channel_clone.clone();
                        let capture = capture.clone();
                        let playback = playback_for_open.clone();
                        let ps = ps_for_open.clone();
                        let preferred_input = preferred_input_for_open.clone();
                        let preferred_output = preferred_output_for_open.clone();
                        Box::pin(async move {
                            // Start playback stream once
                            if !ps.swap(true, Ordering::SeqCst) {
                                match playback.start_with_device(preferred_output.as_deref()) {
                                    Ok(()) => tracing::info!("Playback stream started (Answerer)"),
                                    Err(e) => tracing::error!("Failed to start playback: {}", e),
                                }
                            }
                            
                            // Start capture
                            if let Err(e) = capture.start_with_device(preferred_input.as_deref()) {
                                tracing::error!("Failed to start capture: {}", e);
                            }
                            
                            // Pipe capture -> DC
                            if let Some(mut rx) = capture.take_packet_receiver() {
                                tokio::spawn(async move {
                                    while let Some(packet) = rx.recv().await {
                                        if let Ok(bytes) = bincode::serialize(&packet) {
                                            if let Err(e) = dc.send(&bytes.into()).await {
                                                tracing::warn!("Failed to send audio packet (Answerer): {}", e);
                                            }
                                        }
                                    }
                                });
                            }
                        })
                    }));

                    d_channel.on_message(Box::new(move |msg: DataChannelMessage| {
                        let playback = playback.clone();
                        Box::pin(async move {
                           if let Ok(packet) = bincode::deserialize::<AudioPacket>(&msg.data) {
                               if let Err(e) = playback.process_packet(packet) {
                                   tracing::warn!("Failed to process incoming audio packet (Answerer): {}", e);
                               }
                           }
                        })
                    }));
                })
            }));
        }

        self.rtc_connection = Some(pc);
        Ok(ice_rx)
    }

    /// Create an offer for a WebRTC connection
    pub async fn create_offer(&self) -> Result<String> {
        let pc = self.rtc_connection.as_ref()
            .ok_or_else(|| anyhow::anyhow!("WebRTC not initialized"))?;

        let offer = pc.create_offer(None).await?;
        pc.set_local_description(offer).await?;

        // Send the SDP immediately and rely on trickle ICE via on_ice_candidate.
        let local_desc = pc.local_description().await
            .ok_or_else(|| anyhow::anyhow!("Failed to get local description"))?;
        
        Ok(serde_json::to_string(&local_desc)?)
    }

    /// Accept an offer from a peer and create an answer
    pub async fn accept_offer(&self, offer_sdp: &str) -> Result<String> {
        let pc = self.rtc_connection.as_ref()
            .ok_or_else(|| anyhow::anyhow!("WebRTC not initialized"))?;

        let offer = serde_json::from_str::<RTCSessionDescription>(offer_sdp)?;
        pc.set_remote_description(offer).await?;

        let answer = pc.create_answer(None).await?;
        pc.set_local_description(answer).await?;

        // Send the SDP immediately and rely on trickle ICE via on_ice_candidate.
        let local_desc = pc.local_description().await
            .ok_or_else(|| anyhow::anyhow!("Failed to get local description"))?;
        
        Ok(serde_json::to_string(&local_desc)?)
    }

    /// Set the remote description (answer or offer)
    pub async fn set_remote_description(&self, sdp: &str) -> Result<()> {
        let pc = self.rtc_connection.as_ref()
            .ok_or_else(|| anyhow::anyhow!("WebRTC not initialized"))?;
        
        let remote_desc = serde_json::from_str::<RTCSessionDescription>(sdp)?;
        pc.set_remote_description(remote_desc).await?;
        Ok(())
    }

    /// Add a remote ICE candidate
    pub async fn add_ice_candidate(&self, candidate_json: &str) -> Result<()> {
        let pc = self.rtc_connection.as_ref()
            .ok_or_else(|| anyhow::anyhow!("WebRTC not initialized"))?;
        
        let ice_candidate_init: RTCIceCandidateInit = serde_json::from_str(candidate_json)?;
        pc.add_ice_candidate(ice_candidate_init).await?;
        Ok(())
    }

    /// Create DataChannel for audio (Offerer side) and start capture
    pub async fn create_audio_channel(&self) -> Result<()> {
        let pc = self.rtc_connection.as_ref()
             .ok_or_else(|| anyhow::anyhow!("WebRTC not initialized"))?;
             
        let mut options = RTCDataChannelInit::default();
        options.ordered = Some(false);
        options.max_retransmits = Some(0); // Unreliable (UDP-like) for audio

        let dc = pc.create_data_channel("audio", Some(options)).await?;
        
        let audio_capture = self.audio_capture.clone()
            .ok_or_else(|| anyhow::anyhow!("Audio capture not initialized"))?;
        let audio_playback = self.audio_playback.clone()
            .ok_or_else(|| anyhow::anyhow!("Audio playback not initialized"))?;
        let preferred_input_device = self.selected_input_device.clone();
        let preferred_output_device = self.selected_output_device.clone();

        // Handle incoming messages on this channel too (Answerer audio)
        let playback_clone = audio_playback.clone();
        dc.on_message(Box::new(move |msg: DataChannelMessage| {
            let playback = playback_clone.clone();
            Box::pin(async move {
                if let Ok(packet) = bincode::deserialize::<AudioPacket>(&msg.data) {
                    if let Err(e) = playback.process_packet(packet) {
                        tracing::warn!("Failed to process incoming audio packet (Offerer): {}", e);
                    }
                }
            })
        }));

        let dc_clone = dc.clone();
        let playback_started = self.playback_started.clone();
        let preferred_input_for_open = preferred_input_device.clone();
        let preferred_output_for_open = preferred_output_device.clone();
        dc.on_open(Box::new(move || {
            tracing::info!("DataChannel 'audio' opened (Offerer)");
            let dc = dc_clone.clone();
            let capture = audio_capture.clone();
            let playback = audio_playback.clone();
            let ps = playback_started.clone();
            let preferred_input = preferred_input_for_open.clone();
            let preferred_output = preferred_output_for_open.clone();
            
            Box::pin(async move {
                // Start playback stream once (Offerer side)
                if !ps.swap(true, Ordering::SeqCst) {
                    match playback.start_with_device(preferred_output.as_deref()) {
                        Ok(()) => tracing::info!("Playback stream started (Offerer)"),
                        Err(e) => tracing::error!("Failed to start playback: {}", e),
                    }
                }

                // Start capture stream locally
                if let Err(e) = capture.start_with_device(preferred_input.as_deref()) {
                    tracing::error!("Failed to start capture: {}", e);
                    return;
                }
                
                // Pipe captured audio packets to the DataChannel
                if let Some(mut rx) = capture.take_packet_receiver() {
                    tokio::spawn(async move {
                        while let Some(packet) = rx.recv().await {
                            if let Ok(bytes) = bincode::serialize(&packet) {
                                if let Err(e) = dc.send(&bytes.into()).await {
                                    tracing::warn!("Failed to send audio packet (Offerer): {}", e);
                                }
                            }
                        }
                    });
                } else {
                    tracing::error!("Failed to take packet receiver - already taken?");
                }
            })
        }));
        
        Ok(())
    }
}
