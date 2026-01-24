use anyhow::Result;
use cpal::traits::{HostTrait, DeviceTrait, StreamTrait};

pub struct MediaEngine {
    // runtime: tokio::runtime::Runtime,
}

impl MediaEngine {
    pub fn new() -> Self {
        Self {
            // runtime: tokio::runtime::Runtime::new().unwrap(),
        }
    }

    pub async fn start(&self) -> Result<()> {
        println!("Media Engine Starting...");
        
        let host = cpal::default_host();
        let device = host.default_input_device().ok_or_else(|| anyhow::anyhow!("No input device"))?;
        println!("Using input device: {}", device.name()?);

        let config = device.default_input_config()?;
        println!("Default input config: {:?}", config);

        let err_fn = move |err| {
            eprintln!("an error occurred on stream: {}", err);
        };

        let stream = device.build_input_stream(
            &config.into(),
            move |data: &[f32], _: &cpal::InputCallbackInfo| {
                // In a real app, we would encode 'data' with Audiopus here
                // and send it via WebRTC track.
                // For now, calculating RMS volume for the VU meter would happen here.
                let mut sum = 0.0;
                for &sample in data {
                    sum += sample * sample;
                }
                let rms = (sum / data.len() as f32).sqrt();
                if rms > 0.01 {
                    // println!("RMS: {:.4}", rms); // debug
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
