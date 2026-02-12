use crate::crypto::CryptoContext;
use anyhow::Result;
use audiopus::{
    coder::Decoder, coder::Encoder, packet::Packet, Application, Channels, MutSignals, SampleRate,
};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{SampleFormat, StreamConfig, SupportedStreamConfig};
use std::collections::VecDeque;
use std::sync::{
    atomic::{AtomicBool, AtomicU64, Ordering},
    Arc, Mutex,
};
use std::thread;
use tokio::sync::mpsc;

/// Audio configuration
pub const SAMPLE_RATE: u32 = 48000;
pub const CHANNELS: u16 = 1; // Mono
pub const FRAME_SIZE: usize = 960; // 20ms at 48kHz

struct CapturePipelineState {
    sample_buffer: Vec<i16>,
    resample_pos: f64,
}

impl CapturePipelineState {
    fn new() -> Self {
        Self {
            sample_buffer: Vec::with_capacity(FRAME_SIZE * 3),
            resample_pos: 0.0,
        }
    }
}

/// Opus encoder wrapper
pub struct OpusEncoder {
    encoder: Encoder,
}

impl OpusEncoder {
    pub fn new() -> Result<Self> {
        let encoder = Encoder::new(SampleRate::Hz48000, Channels::Mono, Application::Voip)
            .map_err(|e| anyhow::anyhow!("Failed to create Opus encoder: {:?}", e))?;

        Ok(Self { encoder })
    }

    /// Encode audio samples to Opus
    pub fn encode(&mut self, samples: &[i16]) -> Result<Vec<u8>> {
        let mut output = vec![0u8; 1024]; // Max Opus packet size
        let len = self
            .encoder
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
        let decoder = Decoder::new(SampleRate::Hz48000, Channels::Mono)
            .map_err(|e| anyhow::anyhow!("Failed to create Opus decoder: {:?}", e))?;

        Ok(Self { decoder })
    }

    /// Decode Opus packet to audio samples
    pub fn decode(&mut self, packet: &[u8]) -> Result<Vec<i16>> {
        let mut output = vec![0i16; FRAME_SIZE];
        let opus_packet =
            Packet::try_from(packet).map_err(|e| anyhow::anyhow!("Invalid packet: {:?}", e))?;
        let signals = MutSignals::try_from(&mut output[..])
            .map_err(|e| anyhow::anyhow!("Signal buffer error: {:?}", e))?;
        let len = self
            .decoder
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
    // Monotonic token used to invalidate old capture threads
    run_token: Arc<AtomicU64>,
    // Mute flag - when true, send silence instead of mic data
    muted: Arc<AtomicBool>,
    // VU meter RMS emission
    rms_tx: mpsc::UnboundedSender<f32>,
    rms_rx: Arc<Mutex<Option<mpsc::UnboundedReceiver<f32>>>>,
}

impl AudioCapture {
    pub fn new(crypto: Arc<CryptoContext>) -> Result<Self> {
        let (packet_tx, packet_rx) = mpsc::unbounded_channel();
        let (rms_tx, rms_rx) = mpsc::unbounded_channel();
        Ok(Self {
            encoder: Arc::new(Mutex::new(OpusEncoder::new()?)),
            crypto,
            packet_tx,
            packet_rx: Arc::new(Mutex::new(Some(packet_rx))),
            seq: Arc::new(std::sync::atomic::AtomicU32::new(0)),
            running: Arc::new(AtomicBool::new(false)),
            run_token: Arc::new(AtomicU64::new(0)),
            muted: Arc::new(AtomicBool::new(false)),
            rms_tx,
            rms_rx: Arc::new(Mutex::new(Some(rms_rx))),
        })
    }

    pub fn take_packet_receiver(&self) -> Option<mpsc::UnboundedReceiver<AudioPacket>> {
        self.packet_rx.lock().unwrap().take()
    }

    pub fn take_rms_receiver(&self) -> Option<mpsc::UnboundedReceiver<f32>> {
        self.rms_rx.lock().unwrap().take()
    }

    pub fn set_muted(&self, muted: bool) {
        self.muted.store(muted, Ordering::SeqCst);
        tracing::info!("Audio capture muted: {}", muted);
    }

    pub fn is_muted(&self) -> bool {
        self.muted.load(Ordering::SeqCst)
    }

    /// Start capture with the default input device
    pub fn start(&self) -> Result<()> {
        self.start_with_device(None)
    }

    /// Start capture with a specific device by name, or default if None
    pub fn start_with_device(&self, device_name: Option<&str>) -> Result<()> {
        if self.running.swap(true, Ordering::SeqCst) {
            return Ok(());
        }

        let encoder = self.encoder.clone();
        let crypto = self.crypto.clone();
        let packet_tx = self.packet_tx.clone();
        let seq = self.seq.clone();
        let running = self.running.clone();
        let run_token = self.run_token.clone();
        let muted = self.muted.clone();
        let rms_tx = self.rms_tx.clone();
        let device_name_owned = device_name.map(|s| s.to_string());
        let current_token = run_token.fetch_add(1, Ordering::SeqCst).wrapping_add(1);

        thread::spawn(move || {
            let host = cpal::default_host();
            let device = if let Some(ref name) = device_name_owned {
                match host.input_devices() {
                    Ok(devices) => {
                        if let Some(device) = devices
                            .into_iter()
                            .find(|d| d.name().map(|n| n == *name).unwrap_or(false))
                        {
                            device
                        } else {
                            tracing::warn!("Input device '{}' not found, using default", name);
                            match host.default_input_device() {
                                Some(d) => d,
                                None => {
                                    tracing::error!("No input device available");
                                    if run_token.load(Ordering::SeqCst) == current_token {
                                        running.store(false, Ordering::SeqCst);
                                    }
                                    return;
                                }
                            }
                        }
                    }
                    Err(e) => {
                        tracing::error!("Failed to enumerate input devices: {}", e);
                        match host.default_input_device() {
                            Some(d) => d,
                            None => {
                                tracing::error!("No input device available");
                                if run_token.load(Ordering::SeqCst) == current_token {
                                    running.store(false, Ordering::SeqCst);
                                }
                                return;
                            }
                        }
                    }
                }
            } else {
                match host.default_input_device() {
                    Some(d) => d,
                    None => {
                        tracing::error!("No input device available");
                        if run_token.load(Ordering::SeqCst) == current_token {
                            running.store(false, Ordering::SeqCst);
                        }
                        return;
                    }
                }
            };

            let config = match pick_input_config(&device) {
                Ok(c) => c,
                Err(e) => {
                    tracing::error!("Failed to pick input config: {}", e);
                    if run_token.load(Ordering::SeqCst) == current_token {
                        running.store(false, Ordering::SeqCst);
                    }
                    return;
                }
            };

            let sample_format = config.sample_format();
            let stream_config: StreamConfig = config.into();
            let input_channels = stream_config.channels as usize;
            let input_rate = stream_config.sample_rate.0;

            tracing::info!(
                "Using input device '{}' ({:?}, {}ch @ {}Hz)",
                device.name().unwrap_or_else(|_| "unknown".to_string()),
                sample_format,
                input_channels,
                input_rate
            );

            let pipeline_state = Arc::new(Mutex::new(CapturePipelineState::new()));

            let stream_result = match sample_format {
                SampleFormat::F32 => {
                    let encoder = encoder.clone();
                    let crypto = crypto.clone();
                    let packet_tx = packet_tx.clone();
                    let seq = seq.clone();
                    let muted = muted.clone();
                    let rms_tx = rms_tx.clone();
                    let pipeline_state = pipeline_state.clone();
                    device.build_input_stream(
                        &stream_config,
                        move |data: &[f32], _info| {
                            let mono = downmix_f32(data, input_channels);
                            if let Ok(mut state) = pipeline_state.lock() {
                                process_mono_samples(
                                    &mono,
                                    input_rate,
                                    muted.load(Ordering::Relaxed),
                                    &rms_tx,
                                    &encoder,
                                    &crypto,
                                    &seq,
                                    &packet_tx,
                                    &mut state,
                                );
                            }
                        },
                        |err| tracing::error!("Capture stream error: {}", err),
                        None,
                    )
                }
                SampleFormat::F64 => {
                    let encoder = encoder.clone();
                    let crypto = crypto.clone();
                    let packet_tx = packet_tx.clone();
                    let seq = seq.clone();
                    let muted = muted.clone();
                    let rms_tx = rms_tx.clone();
                    let pipeline_state = pipeline_state.clone();
                    device.build_input_stream(
                        &stream_config,
                        move |data: &[f64], _info| {
                            let mono = downmix_f64_to_f32(data, input_channels);
                            if let Ok(mut state) = pipeline_state.lock() {
                                process_mono_samples(
                                    &mono,
                                    input_rate,
                                    muted.load(Ordering::Relaxed),
                                    &rms_tx,
                                    &encoder,
                                    &crypto,
                                    &seq,
                                    &packet_tx,
                                    &mut state,
                                );
                            }
                        },
                        |err| tracing::error!("Capture stream error: {}", err),
                        None,
                    )
                }
                SampleFormat::I16 => {
                    let encoder = encoder.clone();
                    let crypto = crypto.clone();
                    let packet_tx = packet_tx.clone();
                    let seq = seq.clone();
                    let muted = muted.clone();
                    let rms_tx = rms_tx.clone();
                    let pipeline_state = pipeline_state.clone();
                    device.build_input_stream(
                        &stream_config,
                        move |data: &[i16], _info| {
                            let mono = downmix_i16_to_f32(data, input_channels);
                            if let Ok(mut state) = pipeline_state.lock() {
                                process_mono_samples(
                                    &mono,
                                    input_rate,
                                    muted.load(Ordering::Relaxed),
                                    &rms_tx,
                                    &encoder,
                                    &crypto,
                                    &seq,
                                    &packet_tx,
                                    &mut state,
                                );
                            }
                        },
                        |err| tracing::error!("Capture stream error: {}", err),
                        None,
                    )
                }
                SampleFormat::I8 => {
                    let encoder = encoder.clone();
                    let crypto = crypto.clone();
                    let packet_tx = packet_tx.clone();
                    let seq = seq.clone();
                    let muted = muted.clone();
                    let rms_tx = rms_tx.clone();
                    let pipeline_state = pipeline_state.clone();
                    device.build_input_stream(
                        &stream_config,
                        move |data: &[i8], _info| {
                            let mono = downmix_i8_to_f32(data, input_channels);
                            if let Ok(mut state) = pipeline_state.lock() {
                                process_mono_samples(
                                    &mono,
                                    input_rate,
                                    muted.load(Ordering::Relaxed),
                                    &rms_tx,
                                    &encoder,
                                    &crypto,
                                    &seq,
                                    &packet_tx,
                                    &mut state,
                                );
                            }
                        },
                        |err| tracing::error!("Capture stream error: {}", err),
                        None,
                    )
                }
                SampleFormat::I32 => {
                    let encoder = encoder.clone();
                    let crypto = crypto.clone();
                    let packet_tx = packet_tx.clone();
                    let seq = seq.clone();
                    let muted = muted.clone();
                    let rms_tx = rms_tx.clone();
                    let pipeline_state = pipeline_state.clone();
                    device.build_input_stream(
                        &stream_config,
                        move |data: &[i32], _info| {
                            let mono = downmix_i32_to_f32(data, input_channels);
                            if let Ok(mut state) = pipeline_state.lock() {
                                process_mono_samples(
                                    &mono,
                                    input_rate,
                                    muted.load(Ordering::Relaxed),
                                    &rms_tx,
                                    &encoder,
                                    &crypto,
                                    &seq,
                                    &packet_tx,
                                    &mut state,
                                );
                            }
                        },
                        |err| tracing::error!("Capture stream error: {}", err),
                        None,
                    )
                }
                SampleFormat::U16 => {
                    let encoder = encoder.clone();
                    let crypto = crypto.clone();
                    let packet_tx = packet_tx.clone();
                    let seq = seq.clone();
                    let muted = muted.clone();
                    let rms_tx = rms_tx.clone();
                    let pipeline_state = pipeline_state.clone();
                    device.build_input_stream(
                        &stream_config,
                        move |data: &[u16], _info| {
                            let mono = downmix_u16_to_f32(data, input_channels);
                            if let Ok(mut state) = pipeline_state.lock() {
                                process_mono_samples(
                                    &mono,
                                    input_rate,
                                    muted.load(Ordering::Relaxed),
                                    &rms_tx,
                                    &encoder,
                                    &crypto,
                                    &seq,
                                    &packet_tx,
                                    &mut state,
                                );
                            }
                        },
                        |err| tracing::error!("Capture stream error: {}", err),
                        None,
                    )
                }
                SampleFormat::U8 => {
                    let encoder = encoder.clone();
                    let crypto = crypto.clone();
                    let packet_tx = packet_tx.clone();
                    let seq = seq.clone();
                    let muted = muted.clone();
                    let rms_tx = rms_tx.clone();
                    let pipeline_state = pipeline_state.clone();
                    device.build_input_stream(
                        &stream_config,
                        move |data: &[u8], _info| {
                            let mono = downmix_u8_to_f32(data, input_channels);
                            if let Ok(mut state) = pipeline_state.lock() {
                                process_mono_samples(
                                    &mono,
                                    input_rate,
                                    muted.load(Ordering::Relaxed),
                                    &rms_tx,
                                    &encoder,
                                    &crypto,
                                    &seq,
                                    &packet_tx,
                                    &mut state,
                                );
                            }
                        },
                        |err| tracing::error!("Capture stream error: {}", err),
                        None,
                    )
                }
                SampleFormat::U32 => {
                    let encoder = encoder.clone();
                    let crypto = crypto.clone();
                    let packet_tx = packet_tx.clone();
                    let seq = seq.clone();
                    let muted = muted.clone();
                    let rms_tx = rms_tx.clone();
                    let pipeline_state = pipeline_state.clone();
                    device.build_input_stream(
                        &stream_config,
                        move |data: &[u32], _info| {
                            let mono = downmix_u32_to_f32(data, input_channels);
                            if let Ok(mut state) = pipeline_state.lock() {
                                process_mono_samples(
                                    &mono,
                                    input_rate,
                                    muted.load(Ordering::Relaxed),
                                    &rms_tx,
                                    &encoder,
                                    &crypto,
                                    &seq,
                                    &packet_tx,
                                    &mut state,
                                );
                            }
                        },
                        |err| tracing::error!("Capture stream error: {}", err),
                        None,
                    )
                }
                _ => {
                    tracing::error!("Unsupported input sample format: {:?}", sample_format);
                    if run_token.load(Ordering::SeqCst) == current_token {
                        running.store(false, Ordering::SeqCst);
                    }
                    return;
                }
            };

            let stream = match stream_result {
                Ok(s) => s,
                Err(e) => {
                    tracing::error!("Failed to build input stream: {}", e);
                    if run_token.load(Ordering::SeqCst) == current_token {
                        running.store(false, Ordering::SeqCst);
                    }
                    return;
                }
            };

            if let Err(e) = stream.play() {
                tracing::error!("Failed to play input stream: {}", e);
                if run_token.load(Ordering::SeqCst) == current_token {
                    running.store(false, Ordering::SeqCst);
                }
                return;
            }

            while running.load(Ordering::SeqCst)
                && run_token.load(Ordering::SeqCst) == current_token
            {
                thread::sleep(std::time::Duration::from_millis(100));
            }

            if run_token.load(Ordering::SeqCst) == current_token {
                running.store(false, Ordering::SeqCst);
            }
        });

        Ok(())
    }

    pub fn stop(&self) {
        self.running.store(false, Ordering::SeqCst);
        self.run_token.fetch_add(1, Ordering::SeqCst);
    }

    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }
}

fn pick_input_config(device: &cpal::Device) -> Result<SupportedStreamConfig> {
    let mut best_48k: Option<SupportedStreamConfig> = None;

    if let Ok(configs) = device.supported_input_configs() {
        for cfg in configs {
            if cfg.min_sample_rate().0 <= SAMPLE_RATE && cfg.max_sample_rate().0 >= SAMPLE_RATE {
                let candidate = cfg.with_sample_rate(cpal::SampleRate(SAMPLE_RATE));
                let better = match &best_48k {
                    None => true,
                    Some(best) => {
                        if candidate.channels() == CHANNELS {
                            best.channels() != CHANNELS
                        } else {
                            false
                        }
                    }
                };
                if better {
                    best_48k = Some(candidate);
                }
            }
        }
    }

    if let Some(cfg) = best_48k {
        return Ok(cfg);
    }

    device
        .default_input_config()
        .map_err(|e| anyhow::anyhow!("No usable input config: {}", e))
}

fn downmix_f32(input: &[f32], channels: usize) -> Vec<f32> {
    if channels <= 1 {
        return input.to_vec();
    }

    input
        .chunks_exact(channels)
        .map(|frame| frame.iter().sum::<f32>() / channels as f32)
        .collect()
}

fn downmix_f64_to_f32(input: &[f64], channels: usize) -> Vec<f32> {
    if channels <= 1 {
        return input.iter().map(|&s| s as f32).collect();
    }

    input
        .chunks_exact(channels)
        .map(|frame| (frame.iter().sum::<f64>() / channels as f64) as f32)
        .collect()
}

fn downmix_i8_to_f32(input: &[i8], channels: usize) -> Vec<f32> {
    if channels <= 1 {
        return input.iter().map(|&s| s as f32 / 128.0).collect();
    }

    input
        .chunks_exact(channels)
        .map(|frame| {
            let sum: f32 = frame.iter().map(|&s| s as f32 / 128.0).sum();
            sum / channels as f32
        })
        .collect()
}

fn downmix_i16_to_f32(input: &[i16], channels: usize) -> Vec<f32> {
    if channels <= 1 {
        return input.iter().map(|&s| s as f32 / 32768.0).collect();
    }

    input
        .chunks_exact(channels)
        .map(|frame| {
            let sum: f32 = frame.iter().map(|&s| s as f32 / 32768.0).sum();
            sum / channels as f32
        })
        .collect()
}

fn downmix_i32_to_f32(input: &[i32], channels: usize) -> Vec<f32> {
    if channels <= 1 {
        return input.iter().map(|&s| s as f32 / 2147483648.0).collect();
    }

    input
        .chunks_exact(channels)
        .map(|frame| {
            let sum: f32 = frame.iter().map(|&s| s as f32 / 2147483648.0).sum();
            sum / channels as f32
        })
        .collect()
}

fn downmix_u8_to_f32(input: &[u8], channels: usize) -> Vec<f32> {
    if channels <= 1 {
        return input
            .iter()
            .map(|&s| (s as f32 / 255.0) * 2.0 - 1.0)
            .collect();
    }

    input
        .chunks_exact(channels)
        .map(|frame| {
            let sum: f32 = frame.iter().map(|&s| (s as f32 / 255.0) * 2.0 - 1.0).sum();
            sum / channels as f32
        })
        .collect()
}

fn downmix_u16_to_f32(input: &[u16], channels: usize) -> Vec<f32> {
    if channels <= 1 {
        return input
            .iter()
            .map(|&s| (s as f32 / 65535.0) * 2.0 - 1.0)
            .collect();
    }

    input
        .chunks_exact(channels)
        .map(|frame| {
            let sum: f32 = frame
                .iter()
                .map(|&s| (s as f32 / 65535.0) * 2.0 - 1.0)
                .sum();
            sum / channels as f32
        })
        .collect()
}

fn downmix_u32_to_f32(input: &[u32], channels: usize) -> Vec<f32> {
    if channels <= 1 {
        return input
            .iter()
            .map(|&s| (s as f64 / 4294967295.0 * 2.0 - 1.0) as f32)
            .collect();
    }

    input
        .chunks_exact(channels)
        .map(|frame| {
            let sum: f64 = frame
                .iter()
                .map(|&s| s as f64 / 4294967295.0 * 2.0 - 1.0)
                .sum();
            (sum / channels as f64) as f32
        })
        .collect()
}

fn resample_to_48k(input: &[f32], input_rate: u32, pos: &mut f64) -> Vec<f32> {
    if input.is_empty() {
        return Vec::new();
    }
    if input_rate == SAMPLE_RATE {
        return input.to_vec();
    }

    let step = input_rate as f64 / SAMPLE_RATE as f64;
    let mut out = Vec::with_capacity(
        ((input.len() as u64 * SAMPLE_RATE as u64) / input_rate as u64 + 2) as usize,
    );

    while *pos < input.len() as f64 {
        let idx = (*pos).floor() as usize;
        out.push(input[idx]);
        *pos += step;
    }

    *pos -= input.len() as f64;
    out
}

fn process_mono_samples(
    mono_samples: &[f32],
    input_rate: u32,
    muted: bool,
    rms_tx: &mpsc::UnboundedSender<f32>,
    encoder: &Arc<Mutex<OpusEncoder>>,
    crypto: &Arc<CryptoContext>,
    seq: &Arc<std::sync::atomic::AtomicU32>,
    packet_tx: &mpsc::UnboundedSender<AudioPacket>,
    state: &mut CapturePipelineState,
) {
    let rms = calculate_rms(mono_samples);
    let _ = rms_tx.send(rms);

    let resampled = resample_to_48k(mono_samples, input_rate, &mut state.resample_pos);
    if resampled.is_empty() {
        return;
    }

    if muted {
        state.sample_buffer.extend(vec![0i16; resampled.len()]);
    } else {
        state.sample_buffer.extend(resampled.into_iter().map(|s| {
            let clamped = s.clamp(-1.0, 1.0);
            (clamped * 32767.0) as i16
        }));
    }

    while state.sample_buffer.len() >= FRAME_SIZE {
        let frame: Vec<i16> = state.sample_buffer.drain(..FRAME_SIZE).collect();
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
}

/// Audio playback pipeline (Channel -> Decrypt -> Opus -> Speaker)
pub struct AudioPlayback {
    decoder: Arc<Mutex<OpusDecoder>>,
    crypto: Arc<CryptoContext>,
    // Buffer for decoded samples waiting to be played
    sample_queue: Arc<Mutex<VecDeque<i16>>>,
    // Flag to keep playback thread alive
    running: Arc<AtomicBool>,
}

impl AudioPlayback {
    pub fn new(crypto: Arc<CryptoContext>) -> Result<Self> {
        Ok(Self {
            decoder: Arc::new(Mutex::new(OpusDecoder::new()?)),
            crypto,
            sample_queue: Arc::new(Mutex::new(VecDeque::with_capacity(FRAME_SIZE * 10))),
            running: Arc::new(AtomicBool::new(false)),
        })
    }

    /// Process incoming encrypted packet
    pub fn process_packet(&self, packet: AudioPacket) -> Result<()> {
        let decrypted = self
            .crypto
            .decrypt(&packet.data)
            .map_err(|e| anyhow::anyhow!("Decrypt error: {:?}", e))?;

        let mut decoder = self
            .decoder
            .lock()
            .map_err(|_| anyhow::anyhow!("Lock error"))?;
        let samples = decoder.decode(&decrypted)?;

        let mut queue = self
            .sample_queue
            .lock()
            .map_err(|_| anyhow::anyhow!("Lock error"))?;

        // Simple buffer management - avoid unlimited growth
        if queue.len() > FRAME_SIZE * 50 {
            // ~1s buffer max
            // If too full, drain half to catch up (latency optimization)
            queue.drain(..FRAME_SIZE * 25);
        }

        queue.extend(samples);
        Ok(())
    }

    pub fn start(&self) -> Result<()> {
        if self.running.swap(true, Ordering::SeqCst) {
            return Ok(());
        }

        let sample_queue = self.sample_queue.clone();
        let running = self.running.clone();

        thread::spawn(move || {
            let host = cpal::default_host();
            let device = match host.default_output_device() {
                Some(d) => d,
                None => {
                    tracing::error!("No output device");
                    running.store(false, Ordering::SeqCst);
                    return;
                }
            };

            let config = match pick_output_config(&device) {
                Ok(c) => c,
                Err(e) => {
                    tracing::error!("Failed to pick output config: {}", e);
                    running.store(false, Ordering::SeqCst);
                    return;
                }
            };

            let sample_format = config.sample_format();
            let stream_config: StreamConfig = config.into();
            let output_channels = stream_config.channels as usize;

            tracing::info!(
                "Using output device '{}' ({:?}, {}ch @ {}Hz)",
                device.name().unwrap_or_else(|_| "unknown".to_string()),
                sample_format,
                output_channels,
                stream_config.sample_rate.0
            );

            let stream_result = match sample_format {
                SampleFormat::F32 => {
                    let sample_queue = sample_queue.clone();
                    device.build_output_stream(
                        &stream_config,
                        move |data: &mut [f32], _info| {
                            fill_output_f32(data, output_channels, &sample_queue);
                        },
                        |err| tracing::error!("Playback stream error: {}", err),
                        None,
                    )
                }
                SampleFormat::F64 => {
                    let sample_queue = sample_queue.clone();
                    device.build_output_stream(
                        &stream_config,
                        move |data: &mut [f64], _info| {
                            fill_output_f64(data, output_channels, &sample_queue);
                        },
                        |err| tracing::error!("Playback stream error: {}", err),
                        None,
                    )
                }
                SampleFormat::I16 => {
                    let sample_queue = sample_queue.clone();
                    device.build_output_stream(
                        &stream_config,
                        move |data: &mut [i16], _info| {
                            fill_output_i16(data, output_channels, &sample_queue);
                        },
                        |err| tracing::error!("Playback stream error: {}", err),
                        None,
                    )
                }
                SampleFormat::I32 => {
                    let sample_queue = sample_queue.clone();
                    device.build_output_stream(
                        &stream_config,
                        move |data: &mut [i32], _info| {
                            fill_output_i32(data, output_channels, &sample_queue);
                        },
                        |err| tracing::error!("Playback stream error: {}", err),
                        None,
                    )
                }
                SampleFormat::U16 => {
                    let sample_queue = sample_queue.clone();
                    device.build_output_stream(
                        &stream_config,
                        move |data: &mut [u16], _info| {
                            fill_output_u16(data, output_channels, &sample_queue);
                        },
                        |err| tracing::error!("Playback stream error: {}", err),
                        None,
                    )
                }
                SampleFormat::U32 => {
                    let sample_queue = sample_queue.clone();
                    device.build_output_stream(
                        &stream_config,
                        move |data: &mut [u32], _info| {
                            fill_output_u32(data, output_channels, &sample_queue);
                        },
                        |err| tracing::error!("Playback stream error: {}", err),
                        None,
                    )
                }
                _ => {
                    tracing::error!("Unsupported output sample format: {:?}", sample_format);
                    running.store(false, Ordering::SeqCst);
                    return;
                }
            };

            let stream = match stream_result {
                Ok(s) => s,
                Err(e) => {
                    tracing::error!("Failed to build output stream: {}", e);
                    running.store(false, Ordering::SeqCst);
                    return;
                }
            };

            if let Err(e) = stream.play() {
                tracing::error!("Failed to play output stream: {}", e);
                running.store(false, Ordering::SeqCst);
                return;
            }

            while running.load(Ordering::SeqCst) {
                thread::sleep(std::time::Duration::from_millis(100));
            }
        });

        Ok(())
    }

    pub fn stop(&self) {
        self.running.store(false, Ordering::SeqCst);
        if let Ok(mut queue) = self.sample_queue.lock() {
            queue.clear();
        }
    }
}

fn pick_output_config(device: &cpal::Device) -> Result<SupportedStreamConfig> {
    let mut best_48k: Option<SupportedStreamConfig> = None;

    if let Ok(configs) = device.supported_output_configs() {
        for cfg in configs {
            if cfg.min_sample_rate().0 <= SAMPLE_RATE && cfg.max_sample_rate().0 >= SAMPLE_RATE {
                let candidate = cfg.with_sample_rate(cpal::SampleRate(SAMPLE_RATE));
                let better = match &best_48k {
                    None => true,
                    Some(best) => {
                        if candidate.channels() >= CHANNELS {
                            best.channels() < CHANNELS
                        } else {
                            false
                        }
                    }
                };
                if better {
                    best_48k = Some(candidate);
                }
            }
        }
    }

    if let Some(cfg) = best_48k {
        return Ok(cfg);
    }

    device
        .default_output_config()
        .map_err(|e| anyhow::anyhow!("No usable output config: {}", e))
}

fn next_i16_sample(queue: &mut VecDeque<i16>) -> i16 {
    queue.pop_front().unwrap_or(0)
}

fn fill_output_f32(data: &mut [f32], channels: usize, sample_queue: &Arc<Mutex<VecDeque<i16>>>) {
    let mut queue = match sample_queue.lock() {
        Ok(q) => q,
        Err(_) => return,
    };

    if channels <= 1 {
        for sample in data.iter_mut() {
            *sample = next_i16_sample(&mut queue) as f32 / 32767.0;
        }
        return;
    }

    for frame in data.chunks_mut(channels) {
        let value = next_i16_sample(&mut queue) as f32 / 32767.0;
        for out in frame.iter_mut() {
            *out = value;
        }
    }
}

fn fill_output_f64(data: &mut [f64], channels: usize, sample_queue: &Arc<Mutex<VecDeque<i16>>>) {
    let mut queue = match sample_queue.lock() {
        Ok(q) => q,
        Err(_) => return,
    };

    if channels <= 1 {
        for sample in data.iter_mut() {
            *sample = next_i16_sample(&mut queue) as f64 / 32767.0;
        }
        return;
    }

    for frame in data.chunks_mut(channels) {
        let value = next_i16_sample(&mut queue) as f64 / 32767.0;
        for out in frame.iter_mut() {
            *out = value;
        }
    }
}

fn fill_output_i16(data: &mut [i16], channels: usize, sample_queue: &Arc<Mutex<VecDeque<i16>>>) {
    let mut queue = match sample_queue.lock() {
        Ok(q) => q,
        Err(_) => return,
    };

    if channels <= 1 {
        for sample in data.iter_mut() {
            *sample = next_i16_sample(&mut queue);
        }
        return;
    }

    for frame in data.chunks_mut(channels) {
        let value = next_i16_sample(&mut queue);
        for out in frame.iter_mut() {
            *out = value;
        }
    }
}

fn fill_output_i32(data: &mut [i32], channels: usize, sample_queue: &Arc<Mutex<VecDeque<i16>>>) {
    let mut queue = match sample_queue.lock() {
        Ok(q) => q,
        Err(_) => return,
    };

    if channels <= 1 {
        for sample in data.iter_mut() {
            *sample = (next_i16_sample(&mut queue) as i32) << 16;
        }
        return;
    }

    for frame in data.chunks_mut(channels) {
        let value = (next_i16_sample(&mut queue) as i32) << 16;
        for out in frame.iter_mut() {
            *out = value;
        }
    }
}

fn fill_output_u16(data: &mut [u16], channels: usize, sample_queue: &Arc<Mutex<VecDeque<i16>>>) {
    let mut queue = match sample_queue.lock() {
        Ok(q) => q,
        Err(_) => return,
    };

    if channels <= 1 {
        for sample in data.iter_mut() {
            *sample = (next_i16_sample(&mut queue) as i32 + 32768) as u16;
        }
        return;
    }

    for frame in data.chunks_mut(channels) {
        let value = (next_i16_sample(&mut queue) as i32 + 32768) as u16;
        for out in frame.iter_mut() {
            *out = value;
        }
    }
}

fn fill_output_u32(data: &mut [u32], channels: usize, sample_queue: &Arc<Mutex<VecDeque<i16>>>) {
    let mut queue = match sample_queue.lock() {
        Ok(q) => q,
        Err(_) => return,
    };

    if channels <= 1 {
        for sample in data.iter_mut() {
            *sample = ((next_i16_sample(&mut queue) as i32 + 32768) as u32) << 16;
        }
        return;
    }

    for frame in data.chunks_mut(channels) {
        let value = ((next_i16_sample(&mut queue) as i32 + 32768) as u32) << 16;
        for out in frame.iter_mut() {
            *out = value;
        }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crypto::KeyPair;
    use std::f32::consts::PI;

    #[test]
    fn downmix_stereo_f32_to_mono() {
        let stereo = vec![1.0f32, -1.0f32, 0.5f32, 0.5f32];
        let mono = downmix_f32(&stereo, 2);
        assert_eq!(mono.len(), 2);
        assert!((mono[0] - 0.0).abs() < 1e-6);
        assert!((mono[1] - 0.5).abs() < 1e-6);
    }

    #[test]
    fn resample_44100_to_48000_produces_expected_count() {
        let input = vec![0.0f32; 441]; // ~10ms at 44.1kHz
        let mut pos = 0.0;
        let out = resample_to_48k(&input, 44_100, &mut pos);
        assert!((470..=490).contains(&out.len()));
    }

    #[test]
    fn fill_output_duplicates_mono_to_stereo() {
        let queue = Arc::new(Mutex::new(VecDeque::from(vec![1000i16, -1000i16])));
        let mut out = vec![0.0f32; 4]; // 2 frames, 2 channels
        fill_output_f32(&mut out, 2, &queue);

        assert!((out[0] - out[1]).abs() < 1e-6);
        assert!((out[2] - out[3]).abs() < 1e-6);
        assert!(out[0] > 0.0);
        assert!(out[2] < 0.0);
    }

    #[test]
    fn process_pipeline_produces_decryptable_opus_packet() {
        let alice = KeyPair::generate().expect("alice keypair");
        let bob = KeyPair::generate().expect("bob keypair");
        let alice_pub = alice.public_key_bytes.clone();
        let bob_pub = bob.public_key_bytes.clone();

        let sender_ctx = Arc::new(alice.derive_shared_secret(&bob_pub).expect("sender ctx"));
        let receiver_ctx = bob.derive_shared_secret(&alice_pub).expect("receiver ctx");

        let encoder = Arc::new(Mutex::new(OpusEncoder::new().expect("opus encoder")));
        let (packet_tx, mut packet_rx) = mpsc::unbounded_channel();
        let (rms_tx, _rms_rx) = mpsc::unbounded_channel();
        let seq = Arc::new(std::sync::atomic::AtomicU32::new(0));
        let mut state = CapturePipelineState::new();

        let input: Vec<f32> = (0..FRAME_SIZE)
            .map(|i| ((i as f32 * 2.0 * PI) / FRAME_SIZE as f32).sin() * 0.2)
            .collect();

        process_mono_samples(
            &input,
            SAMPLE_RATE,
            false,
            &rms_tx,
            &encoder,
            &sender_ctx,
            &seq,
            &packet_tx,
            &mut state,
        );

        let packet = packet_rx.try_recv().expect("expected one packet");
        let decrypted = receiver_ctx
            .decrypt(&packet.data)
            .expect("packet decryptable by peer");

        let mut decoder = OpusDecoder::new().expect("opus decoder");
        let decoded = decoder.decode(&decrypted).expect("opus decodes");
        assert!(!decoded.is_empty());
    }
}
