//! Audio processing pipeline: Capture → Encode (Opus) → Encrypt → Send
//! 
//! This module handles the complete audio pipeline for P2P voice calls.

use crate::crypto::CryptoContext;
use anyhow::Result;
use audiopus::{coder::Encoder, Channels, SampleRate, Application};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc;

/// Audio configuration
pub const SAMPLE_RATE: u32 = 48000;
pub const CHANNELS: u16 = 1; // Mono
pub const FRAME_SIZE: usize = 960; // 20ms at 48kHz

/// Opus encoder wrapper
pub struct OpusEncoder {
    encoder: Encoder,
}

impl OpusEncoder {
    pub fn new() -> Result<Self> {
        let encoder = Encoder::new(
            SampleRate::Hz48000,
            Channels::Mono,
            Application::Voip,
        ).map_err(|e| anyhow::anyhow!("Failed to create Opus encoder: {:?}", e))?;
        
        Ok(Self { encoder })
    }

    /// Encode audio samples to Opus
    pub fn encode(&mut self, samples: &[i16]) -> Result<Vec<u8>> {
        let mut output = vec![0u8; 1024]; // Max Opus packet size
        let len = self.encoder
            .encode(samples, &mut output)
            .map_err(|e| anyhow::anyhow!("Encode error: {:?}", e))?;
        output.truncate(len);
        Ok(output)
    }
}

/// Audio packet ready for transmission
#[derive(Debug, Clone)]
pub struct AudioPacket {
    /// Sequence number
    pub seq: u32,
    /// Encrypted Opus data (nonce + ciphertext)
    pub data: Vec<u8>,
}

/// Audio capture and processing pipeline
pub struct AudioPipeline {
    /// Opus encoder
    encoder: Arc<Mutex<OpusEncoder>>,
    /// Crypto context for E2EE
    crypto: Arc<CryptoContext>,
    /// Channel for sending encoded packets
    packet_tx: mpsc::UnboundedSender<AudioPacket>,
    /// Sequence counter
    seq: Arc<std::sync::atomic::AtomicU32>,
}

impl AudioPipeline {
    pub fn new(
        crypto: Arc<CryptoContext>,
        packet_tx: mpsc::UnboundedSender<AudioPacket>,
    ) -> Result<Self> {
        Ok(Self {
            encoder: Arc::new(Mutex::new(OpusEncoder::new()?)),
            crypto,
            packet_tx,
            seq: Arc::new(std::sync::atomic::AtomicU32::new(0)),
        })
    }

    /// Start capturing audio from the microphone
    pub fn start_capture(&self) -> Result<cpal::Stream> {
        let host = cpal::default_host();
        let device = host
            .default_input_device()
            .ok_or_else(|| anyhow::anyhow!("No input device"))?;

        tracing::info!("Using input device: {:?}", device.name());

        let config = cpal::StreamConfig {
            channels: CHANNELS,
            sample_rate: cpal::SampleRate(SAMPLE_RATE),
            buffer_size: cpal::BufferSize::Fixed(FRAME_SIZE as u32),
        };

        let encoder = self.encoder.clone();
        let crypto = self.crypto.clone();
        let packet_tx = self.packet_tx.clone();
        let seq = self.seq.clone();

        // Buffer for accumulating samples
        let sample_buffer = Arc::new(Mutex::new(Vec::with_capacity(FRAME_SIZE)));

        let stream = device.build_input_stream(
            &config,
            move |data: &[f32], _info| {
                // Convert f32 to i16
                let samples: Vec<i16> = data.iter()
                    .map(|&s| (s * 32767.0) as i16)
                    .collect();

                let mut buffer = sample_buffer.lock().unwrap();
                buffer.extend_from_slice(&samples);

                // Process complete frames
                while buffer.len() >= FRAME_SIZE {
                    let frame: Vec<i16> = buffer.drain(..FRAME_SIZE).collect();
                    
                    // Encode with Opus
                    if let Ok(mut enc) = encoder.lock() {
                        if let Ok(encoded) = enc.encode(&frame) {
                            // Encrypt the encoded data
                            if let Ok(encrypted) = crypto.encrypt(&encoded) {
                                let sequence = seq.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                                let packet = AudioPacket {
                                    seq: sequence,
                                    data: encrypted,
                                };
                                let _ = packet_tx.send(packet);
                            }
                        }
                    }
                }
            },
            |err| {
                tracing::error!("Audio stream error: {}", err);
            },
            None,
        )?;

        stream.play()?;
        tracing::info!("Audio capture started");

        Ok(stream)
    }
}

/// Calculate RMS volume from samples (for VU meter)
pub fn calculate_rms(samples: &[f32]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    let sum: f32 = samples.iter().map(|&s| s * s).sum();
    (sum / samples.len() as f32).sqrt()
}

/// Convert RMS to dB
pub fn rms_to_db(rms: f32) -> f32 {
    if rms <= 0.0 {
        return -100.0;
    }
    20.0 * rms.log10()
}
