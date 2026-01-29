use crate::crypto::CryptoContext;
use anyhow::Result;
use audiopus::{coder::Decoder, coder::Encoder, packet::Packet, Channels, MutSignals, SampleRate, Application};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::sync::{Arc, Mutex, atomic::{AtomicBool, Ordering}};
use std::collections::VecDeque;
use std::thread;
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
            .encode(samples, &mut output[..])
            .map_err(|e| anyhow::anyhow!("Encode error: {:?}", e))?;
        output.truncate(len);
        Ok(output)
    }
}

/// Opus decoder wrapper
pub struct OpusDecoder {
    decoder: Decoder,
}

impl OpusDecoder {
    pub fn new() -> Result<Self> {
        let decoder = Decoder::new(
            SampleRate::Hz48000,
            Channels::Mono,
        ).map_err(|e| anyhow::anyhow!("Failed to create Opus decoder: {:?}", e))?;
        
        Ok(Self { decoder })
    }

    /// Decode Opus packet to audio samples
    pub fn decode(&mut self, packet: &[u8]) -> Result<Vec<i16>> {
        let mut output = vec![0i16; FRAME_SIZE];
        let opus_packet = Packet::try_from(packet)
            .map_err(|e| anyhow::anyhow!("Invalid packet: {:?}", e))?;
        let signals = MutSignals::try_from(&mut output[..])
            .map_err(|e| anyhow::anyhow!("Signal buffer error: {:?}", e))?;
        let len = self.decoder
            .decode(Some(opus_packet), signals, false)
            .map_err(|e| anyhow::anyhow!("Decode error: {:?}", e))?;
        output.truncate(len);
        Ok(output)
    }
}

/// Audio packet ready for transmission
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AudioPacket {
    /// Sequence number
    pub seq: u32,
    /// Encrypted Opus data (nonce + ciphertext)
    pub data: Vec<u8>,
}

/// Audio capture pipeline (Mic -> Opus -> Encrypt -> Channel)
pub struct AudioCapture {
    encoder: Arc<Mutex<OpusEncoder>>,
    crypto: Arc<CryptoContext>,
    packet_tx: mpsc::UnboundedSender<AudioPacket>,
    // We hold the receiver until it's taken by the WebRTC engine
    packet_rx: Arc<Mutex<Option<mpsc::UnboundedReceiver<AudioPacket>>>>,
    seq: Arc<std::sync::atomic::AtomicU32>,
    // Flag to signal the capture thread to stop
    running: Arc<AtomicBool>,
    // Thread handle (not stored in struct since we don't need to join)
}

impl AudioCapture {
    pub fn new(
        crypto: Arc<CryptoContext>,
    ) -> Result<Self> {
        let (packet_tx, packet_rx) = mpsc::unbounded_channel();
        Ok(Self {
            encoder: Arc::new(Mutex::new(OpusEncoder::new()?)),
            crypto,
            packet_tx,
            packet_rx: Arc::new(Mutex::new(Some(packet_rx))),
            seq: Arc::new(std::sync::atomic::AtomicU32::new(0)),
            running: Arc::new(AtomicBool::new(false)),
        })
    }

    pub fn take_packet_receiver(&self) -> Option<mpsc::UnboundedReceiver<AudioPacket>> {
         self.packet_rx.lock().unwrap().take()
    }

    pub fn start(&self) -> Result<()> {
        // Prevent starting twice
        if self.running.swap(true, Ordering::SeqCst) {
            return Ok(()); // Already running
        }

        let encoder = self.encoder.clone();
        let crypto = self.crypto.clone();
        let packet_tx = self.packet_tx.clone();
        let seq = self.seq.clone();
        let running = self.running.clone();

        // Spawn audio capture in a dedicated thread (cpal::Stream is not Send)
        thread::spawn(move || {
            let host = cpal::default_host();
            let device = match host.default_input_device() {
                Some(d) => d,
                None => {
                    tracing::error!("No input device");
                    return;
                }
            };

            tracing::info!("Using input device: {:?}", device.name());

            let config = cpal::StreamConfig {
                channels: CHANNELS,
                sample_rate: cpal::SampleRate(SAMPLE_RATE),
                buffer_size: cpal::BufferSize::Fixed(FRAME_SIZE as u32),
            };

            let sample_buffer = Arc::new(Mutex::new(Vec::with_capacity(FRAME_SIZE * 2)));

            let stream = match device.build_input_stream(
                &config,
                move |data: &[f32], _info| {
                    // Convert f32 to i16
                    let samples: Vec<i16> = data.iter()
                        .map(|&s| (s * 32767.0) as i16)
                        .collect();

                    let mut buffer = sample_buffer.lock().unwrap();
                    buffer.extend_from_slice(&samples);

                    // Process frames
                    while buffer.len() >= FRAME_SIZE {
                        let frame: Vec<i16> = buffer.drain(..FRAME_SIZE).collect();
                        
                        if let Ok(mut enc) = encoder.lock() {
                            if let Ok(encoded) = enc.encode(&frame) {
                                if let Ok(encrypted) = crypto.encrypt(&encoded) {
                                    let sequence = seq.fetch_add(1, Ordering::SeqCst);
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
                |err| tracing::error!("Capture stream error: {}", err),
                None,
            ) {
                Ok(s) => s,
                Err(e) => {
                    tracing::error!("Failed to build input stream: {}", e);
                    return;
                }
            };

            if let Err(e) = stream.play() {
                tracing::error!("Failed to play stream: {}", e);
                return;
            }

            // Keep the thread alive while running is true
            while running.load(Ordering::SeqCst) {
                thread::sleep(std::time::Duration::from_millis(100));
            }
            // Stream is dropped here, stopping capture
        });

        Ok(())
    }

    pub fn stop(&self) {
        self.running.store(false, Ordering::SeqCst);
    }
}

/// Audio playback pipeline (Channel -> Decrypt -> Opus -> Speaker)
pub struct AudioPlayback {
    decoder: Arc<Mutex<OpusDecoder>>,
    crypto: Arc<CryptoContext>,
    // Buffer for decoded samples waiting to be played
    sample_queue: Arc<Mutex<VecDeque<i16>>>,
}

impl AudioPlayback {
    pub fn new(crypto: Arc<CryptoContext>) -> Result<Self> {
        Ok(Self {
            decoder: Arc::new(Mutex::new(OpusDecoder::new()?)),
            crypto,
            sample_queue: Arc::new(Mutex::new(VecDeque::with_capacity(FRAME_SIZE * 10))),
        })
    }

    /// Process incoming encrypted packet
    pub fn process_packet(&self, packet: AudioPacket) -> Result<()> {
        let decrypted = self.crypto.decrypt(&packet.data)
            .map_err(|e| anyhow::anyhow!("Decrypt error: {:?}", e))?;
            
        let mut decoder = self.decoder.lock().map_err(|_| anyhow::anyhow!("Lock error"))?;
        let samples = decoder.decode(&decrypted)?;
        
        let mut queue = self.sample_queue.lock().map_err(|_| anyhow::anyhow!("Lock error"))?;
        
        // Simple buffer management - avoid unlimited growth
        if queue.len() > FRAME_SIZE * 50 { // ~1s buffer max
             // If too full, drain half to catch up (latency optimization)
             queue.drain(..FRAME_SIZE * 25);
        }
        
        queue.extend(samples);
        Ok(())
    }

    pub fn start(&self) -> Result<cpal::Stream> {
        let host = cpal::default_host();
        let device = host.default_output_device()
            .ok_or_else(|| anyhow::anyhow!("No output device"))?;

        tracing::info!("Using output device: {:?}", device.name());

        let config = cpal::StreamConfig {
            channels: CHANNELS,
            sample_rate: cpal::SampleRate(SAMPLE_RATE),
            buffer_size: cpal::BufferSize::Fixed(FRAME_SIZE as u32),
        };

        let sample_queue = self.sample_queue.clone();

        let stream = device.build_output_stream(
            &config,
            move |data: &mut [f32], _info| {
                let mut queue = match sample_queue.lock() {
                    Ok(q) => q,
                    Err(_) => return, // Should not happen often
                };

                for sample in data.iter_mut() {
                    if let Some(s) = queue.pop_front() {
                        *sample = (s as f32) / 32767.0;
                    } else {
                        *sample = 0.0; // Silence if underflow
                    }
                }
            },
            |err| tracing::error!("Playback stream error: {}", err),
            None,
        )?;

        stream.play()?;
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
