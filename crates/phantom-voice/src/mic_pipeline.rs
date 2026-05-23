//! Microphone pipeline — capture, noise suppression, and level metering.
//!
//! Uses `cpal` for cross-platform audio input and `nnnoiseless` (RNNoise)
//! for real-time noise suppression.

use anyhow::Result;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::sync::broadcast;

/// A processed (noise-suppressed) audio chunk from the microphone.
#[derive(Debug, Clone)]
pub struct MicChunk {
    /// f32 mono PCM @ 48 kHz.
    pub samples: Vec<f32>,
    /// RMS level for the UI meter (0.0–1.0).
    pub level: f32,
    pub timestamp_ms: u64,
}

pub struct MicPipeline {
    chunk_tx: broadcast::Sender<MicChunk>,
    running: bool,
    stop_flag: Arc<AtomicBool>,
}

impl MicPipeline {
    pub fn new() -> Self {
        let (chunk_tx, _) = broadcast::channel(128);
        Self {
            chunk_tx,
            running: false,
            stop_flag: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn subscribe(&self) -> broadcast::Receiver<MicChunk> {
        self.chunk_tx.subscribe()
    }

    pub async fn start(&mut self) -> Result<()> {
        if self.running {
            return Ok(());
        }
        self.running = true;
        self.stop_flag.store(false, Ordering::Relaxed);
        tracing::info!("Mic pipeline started");

        let chunk_tx = self.chunk_tx.clone();
        let stop_flag = self.stop_flag.clone();
        let session_start = std::time::Instant::now();

        tokio::task::spawn_blocking(move || -> Result<()> {
            use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

            let host = cpal::default_host();
            let device = host
                .default_input_device()
                .ok_or_else(|| anyhow::anyhow!("No default input device available"))?;

            tracing::debug!("Mic device: {}", device.name().unwrap_or_default());
            let config = device.default_input_config()?;

            // nnnoiseless DenoiseState must be accessed from within the callback.
            // We use a Mutex to allow move into the closure.
            let denoise = std::sync::Mutex::new(nnnoiseless::DenoiseState::new());

            let stop_flag_cb = stop_flag.clone();
            let stream = device.build_input_stream(
                &config.into(),
                move |data: &[f32], _: &cpal::InputCallbackInfo| {
                    if stop_flag_cb.load(Ordering::Relaxed) {
                        return;
                    }

                    let mut output = vec![0.0f32; data.len()];

                    if let Ok(mut d) = denoise.lock() {
                        // Process in 480-sample blocks (10ms @ 48kHz).
                        for (in_chunk, out_chunk) in data.chunks(480).zip(output.chunks_mut(480)) {
                            if in_chunk.len() == 480 {
                                let mut block = [0.0f32; 480];
                                d.process_frame(&mut block, in_chunk);
                                out_chunk.copy_from_slice(&block);
                            } else {
                                // Partial chunk — copy as-is
                                out_chunk.copy_from_slice(in_chunk);
                            }
                        }
                    }

                    let n = output.len() as f32;
                    let rms: f32 = (output.iter().map(|s| s * s).sum::<f32>() / n).sqrt();
                    let timestamp_ms = session_start.elapsed().as_millis() as u64;

                    let _ = chunk_tx.send(MicChunk {
                        samples: output,
                        level: rms,
                        timestamp_ms,
                    });
                },
                |err| tracing::error!("Mic stream error: {err}"),
                None,
            )?;

            stream.play()?;
            tracing::debug!("Mic stream is playing");

            // Park this thread keeping the stream alive until stop is requested.
            loop {
                std::thread::sleep(std::time::Duration::from_millis(100));
                if stop_flag.load(Ordering::Relaxed) {
                    break;
                }
            }

            drop(stream); // triggers cpal cleanup
            tracing::debug!("Mic stream closed");
            Ok(())
        });

        Ok(())
    }

    pub async fn stop(&mut self) -> Result<()> {
        self.stop_flag.store(true, Ordering::Relaxed);
        self.running = false;
        tracing::info!("Mic pipeline stopped");
        Ok(())
    }
}

impl Default for MicPipeline {
    fn default() -> Self {
        Self::new()
    }
}
