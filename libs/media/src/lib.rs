//! P2P Nitro Media Engine
//! 
//! Provides E2EE audio/video communication over WebRTC.
//! 
//! Pipeline: cpal (capture) → audiopus (encode) → ring (encrypt) → webrtc-rs (send)

mod audio;
mod crypto;

use anyhow::Result;
use cpal::traits::{DeviceTrait, HostTrait};
use std::sync::Arc;
use tokio::sync::{mpsc, Mutex};
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

pub use audio::{AudioCapture, AudioPacket, AudioPlayback};
pub use crypto::{CryptoContext, KeyPair};

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
        }
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

    // === WebRTC Implementation ===

    /// Initialize WebRTC PeerConnection
    /// Returns a receiver for local ICE candidates that must be sent to the peer
    pub async fn init_webrtc(&mut self) -> Result<mpsc::Receiver<String>> {
        let mut media_engine = WebRtcMediaEngine::default();
        media_engine.register_default_codecs()?;

        let api = APIBuilder::new()
            .with_media_engine(media_engine)
            .build();

        let config = RTCConfiguration {
            ice_servers: vec![RTCIceServer {
                urls: vec!["stun:stun.l.google.com:19302".to_owned()],
                ..Default::default()
            }],
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
                    // candidate.to_json() returns RTCIceCandidateInit which is Serializable
                    if let Ok(json) = serde_json::to_string(&candidate.to_json()) {
                        let _ = ice_tx.send(json).await;
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
            
            // Setup Capture
            let capture = Arc::new(AudioCapture::new(ctx.clone())?);
            self.audio_capture = Some(capture.clone());

            // Handle incoming DataChannel (Answerer side receives channel created by Offerer)
            let playback_clone = playback.clone();
            let capture_clone = capture.clone();
            
            pc.on_data_channel(Box::new(move |d_channel: Arc<RTCDataChannel>| {
                let playback = playback_clone.clone();
                let capture = capture_clone.clone();
                
                Box::pin(async move {
                    tracing::info!("New DataChannel {} {}", d_channel.label(), d_channel.id());
                    
                    let d_channel_clone = d_channel.clone();
                    d_channel.on_open(Box::new(move || {
                        tracing::info!("Data channel opened (Answerer)");
                        let dc = d_channel_clone.clone();
                        let capture = capture.clone();
                         Box::pin(async move {
                            // Start capture
                            if let Err(e) = capture.start() {
                                tracing::error!("Failed to start capture: {}", e);
                            }
                            
                            // Pipe capture -> DC
                            if let Some(mut rx) = capture.take_packet_receiver() {
                                tokio::spawn(async move {
                                    while let Some(packet) = rx.recv().await {
                                        if let Ok(bytes) = bincode::serialize(&packet) {
                                            let _ = dc.send(&bytes.into()).await;
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
                               let _ = playback.process_packet(packet);
                               let _ = playback.start(); // Ensure playback is running
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
        pc.set_local_description(offer.clone()).await?;

        // Block until ICE gathering is complete
        let mut gather_complete = pc.gathering_complete_promise().await;
        let _ = gather_complete.recv().await;

        let local_desc = pc.local_description().await
            .ok_or_else(|| anyhow::anyhow!("Failed to get local description after ICE gathering"))?;
        
        Ok(serde_json::to_string(&local_desc)?)
    }

    /// Accept an offer from a peer and create an answer
    pub async fn accept_offer(&self, offer_sdp: &str) -> Result<String> {
        let pc = self.rtc_connection.as_ref()
            .ok_or_else(|| anyhow::anyhow!("WebRTC not initialized"))?;

        let offer = serde_json::from_str::<RTCSessionDescription>(offer_sdp)?;
        pc.set_remote_description(offer).await?;

        let answer = pc.create_answer(None).await?;
        pc.set_local_description(answer.clone()).await?;

        // Block until ICE gathering is complete
        let mut gather_complete = pc.gathering_complete_promise().await;
        let _ = gather_complete.recv().await;

        let local_desc = pc.local_description().await
            .ok_or_else(|| anyhow::anyhow!("Failed to get local description after ICE gathering"))?;
        
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

        // Handle incoming messages on this channel too (Answerer audio)
        let playback_clone = audio_playback.clone();
        dc.on_message(Box::new(move |msg: DataChannelMessage| {
            let playback = playback_clone.clone();
            Box::pin(async move {
                if let Ok(packet) = bincode::deserialize::<AudioPacket>(&msg.data) {
                    let _ = playback.process_packet(packet);
                }
            })
        }));

        let dc_clone = dc.clone();
        dc.on_open(Box::new(move || {
            tracing::info!("DataChannel 'audio' opened (Offerer)");
            let dc = dc_clone.clone();
            let capture = audio_capture.clone();
            
            Box::pin(async move {
                // Start capture stream locally
                if let Err(e) = capture.start() {
                    tracing::error!("Failed to start capture: {}", e);
                    return;
                }
                
                // Now, pipe packets from AudioCapture to the DataChannel
                // This requires a way to get the packet_rx from AudioCapture.
                // The current AudioCapture::new takes a tx, so we need to get the rx from it.
                // This is a design challenge.
                //
                // RE-THINK: packet_rx can't be stored easily because it's not Clone/Sync compatible in Arc<Mutex> easily.
                // Better: `init_webrtc` should SPAWN the capture->channel loop?
                // But we don't have the DC yet.
                
                // Solution: Use a broadcast channel or a shared buffer?
                // OR: `AudioCapture` takes a callback `OnPacket` instead of a channel?
                // Yes! `AudioCapture` should take a `Arc<dyn Fn(AudioPacket) + Send + Sync>`.
                // Then that callback calls `dc.send()`.
                //
                // For now, this part is a placeholder, as the `packet_rx` is not accessible here.
                // A refactor of `AudioCapture` to accept a callback or a shared queue would be needed.
                //
                // Example of how it *would* work if `AudioCapture` provided a `packet_rx` or callback:
                // while let Some(packet) = capture.get_packet_receiver().await.recv().await {
                //     if let Ok(encoded) = bincode::serialize(&packet) {
                //         let _ = dc.send(&encoded).await;
                //     }
                // }
            })
        }));
        
        Ok(())
    }
}
