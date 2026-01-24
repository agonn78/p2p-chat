//! P2P Nitro Media Engine
//! 
//! Provides E2EE audio/video communication over WebRTC.
//! 
//! Pipeline: cpal (capture) → audiopus (encode) → ring (encrypt) → webrtc-rs (send)

pub mod audio;
pub mod crypto;

use anyhow::Result;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::sync::Arc;
use tokio::sync::mpsc;

pub use audio::{AudioPacket, AudioPipeline};
pub use crypto::{CryptoContext, KeyPair};

/// Media engine state
pub struct MediaEngine {
    /// Our key pair for E2EE
    keypair: Option<crypto::KeyPair>,
    /// Derived crypto context after key exchange
    crypto_ctx: Option<Arc<CryptoContext>>,
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

    /// Start the audio capture pipeline
    /// Returns a receiver for encrypted audio packets
    pub async fn start_audio_pipeline(&self) -> Result<(cpal::Stream, mpsc::UnboundedReceiver<AudioPacket>)> {
        let crypto = self.crypto_ctx.clone()
            .ok_or_else(|| anyhow::anyhow!("Key exchange not completed"))?;
        
        let (tx, rx) = mpsc::unbounded_channel();
        let pipeline = AudioPipeline::new(crypto, tx)?;
        let stream = pipeline.start_capture()?;
        
        Ok((stream, rx))
    }

    /// Legacy start method for basic audio capture (non-encrypted)
    pub async fn start(&self) -> Result<()> {
        tracing::info!("Media Engine Starting (basic mode)...");
        
        let host = cpal::default_host();
        let device = host.default_input_device()
            .ok_or_else(|| anyhow::anyhow!("No input device"))?;
        tracing::info!("Using input device: {}", device.name()?);

        let config = device.default_input_config()?;
        tracing::info!("Default input config: {:?}", config);

        let err_fn = move |err| {
            tracing::error!("Audio stream error: {}", err);
        };

        let stream = device.build_input_stream(
            &config.into(),
            move |data: &[f32], _: &cpal::InputCallbackInfo| {
                let rms = audio::calculate_rms(data);
                if rms > 0.01 {
                    let db = audio::rms_to_db(rms);
                    tracing::debug!("Audio level: {:.1}dB", db);
                }
            },
            err_fn,
            None,
        )?;

        stream.play()?;
        
        // Keep the stream alive
        tokio::time::sleep(tokio::time::Duration::from_secs(3600)).await;
        
        Ok(())
    }
}

