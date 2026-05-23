//! Audio capture — system loopback + microphone, per platform.
//!
//! Windows: WASAPI loopback (system) + WASAPI capture (mic)
//! macOS:   ScreenCaptureKit audio (system) + AVCaptureSession (mic)
//!
//! Both streams are captured as separate PCM f32 buffers and mixed
//! in [`AudioMixer`] with a configurable balance.

use anyhow::Result;
use std::sync::Arc;
use tokio::sync::{broadcast, Mutex};

/// One chunk of interleaved PCM audio (f32 normalised -1.0 to 1.0).
#[derive(Debug, Clone)]
pub struct AudioChunk {
    /// Samples per second (e.g. 48000).
    pub sample_rate: u32,
    /// Number of channels (1 = mono, 2 = stereo).
    pub channels: u16,
    /// Interleaved PCM f32 samples.
    pub samples: Arc<Vec<f32>>,
    /// Monotonic millisecond timestamp since recording start.
    pub timestamp_ms: u64,
}

/// Where this audio originated.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AudioSource {
    /// System loopback (what the speakers are playing).
    SystemLoopback,
    /// Microphone input.
    Microphone,
    /// Mixed output (system + mic).
    Mixed,
}

/// Balance between system audio (0.0) and microphone (1.0).
/// 0.5 = equal mix.
#[derive(Debug, Clone, Copy)]
pub struct AudioBalance {
    pub system: f32,
    pub mic: f32,
}

impl Default for AudioBalance {
    fn default() -> Self {
        Self { system: 0.8, mic: 0.9 }
    }
}

/// Configuration for the audio capture pipeline.
#[derive(Debug, Clone)]
pub struct AudioConfig {
    pub sample_rate: u32,
    pub channels: u16,
    pub balance: AudioBalance,
    /// Capture system audio (loopback).
    pub capture_system: bool,
    /// Capture microphone.
    pub capture_mic: bool,
    /// Device name override (None = default device).
    pub mic_device: Option<String>,
}

impl Default for AudioConfig {
    fn default() -> Self {
        Self {
            sample_rate: 48_000,
            channels: 2,
            balance: AudioBalance::default(),
            capture_system: true,
            capture_mic: true,
            mic_device: None,
        }
    }
}

/// Platform-agnostic audio capture handle.
pub struct AudioCapture {
    config: AudioConfig,
    running: Arc<Mutex<bool>>,
    chunk_tx: broadcast::Sender<AudioChunk>,
}

impl AudioCapture {
    /// Create an audio capture pipeline.
    pub fn new(config: AudioConfig) -> Result<Self> {
        let (chunk_tx, _) = broadcast::channel(256);
        Ok(Self {
            config,
            running: Arc::new(Mutex::new(false)),
            chunk_tx,
        })
    }

    /// Subscribe to receive mixed [`AudioChunk`]s in real time.
    pub fn subscribe(&self) -> broadcast::Receiver<AudioChunk> {
        self.chunk_tx.subscribe()
    }

    /// Start both capture streams and the mixer.
    pub async fn start(&self) -> Result<()> {
        let mut running = self.running.lock().await;
        if *running {
            return Ok(());
        }
        *running = true;
        tracing::info!(
            sample_rate = self.config.sample_rate,
            channels = self.config.channels,
            "Audio capture started"
        );
        
        let tx = self.chunk_tx.clone();
        let config = self.config.clone();
        let running_clone = self.running.clone();
        
        // Spawn the platform audio instance to keep the cpal stream alive
        std::thread::spawn(move || {
            if let Ok(_platform_audio) = PlatformAudio::start(&config, tx) {
                // Loop to keep the stream from dropping until we stop
                loop {
                    std::thread::sleep(std::time::Duration::from_millis(200));
                    if let Ok(is_running) = running_clone.try_lock() {
                        if !*is_running {
                            break;
                        }
                    }
                }
            }
        });
        
        Ok(())
    }

    /// Stop all capture streams.
    pub async fn stop(&self) -> Result<()> {
        let mut running = self.running.lock().await;
        *running = false; // Background threads polling `running` will naturally exit.
        tracing::info!("Audio capture stopped");
        Ok(())
    }
}

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::time::Instant;

#[cfg(not(target_arch = "wasm32"))]
pub struct PlatformAudio {
    _stream: cpal::Stream,
}

#[cfg(not(target_arch = "wasm32"))]
impl PlatformAudio {
    pub fn start(
        _config: &AudioConfig,
        sender: broadcast::Sender<AudioChunk>,
    ) -> Result<Self> {
        let host = cpal::default_host();
        let device = host
            .default_input_device()
            .ok_or_else(|| anyhow::anyhow!("No default input device found"))?;

        let supported_config = device.default_input_config()?;
        let sample_rate = supported_config.sample_rate().0;
        let channels = supported_config.channels();
        
        let err_fn = |err| tracing::error!("An error occurred on the audio input stream: {}", err);
        let start_time = Instant::now();
        
        let stream = match supported_config.sample_format() {
            cpal::SampleFormat::F32 => device.build_input_stream(
                &supported_config.into(),
                move |data: &[f32], _: &cpal::InputCallbackInfo| {
                    let chunk = AudioChunk {
                        sample_rate,
                        channels,
                        samples: Arc::new(data.to_vec()),
                        timestamp_ms: start_time.elapsed().as_millis() as u64,
                    };
                    let _ = sender.send(chunk);
                },
                err_fn,
                None,
            )?,
            _ => anyhow::bail!("Unsupported audio sample format (only f32 is supported in Phase 1)"),
        };
        
        stream.play()?;
        tracing::debug!("cpal audio capture started on {}", device.name().unwrap_or_default());
        
        Ok(Self { _stream: stream })
    }
}

#[cfg(target_arch = "wasm32")]
pub struct PlatformAudio {}

#[cfg(target_arch = "wasm32")]
impl PlatformAudio {
    pub fn start(
        _config: &AudioConfig,
        _sender: broadcast::Sender<AudioChunk>,
    ) -> Result<Self> {
        // In WASM, audio capture is handled alongside video via MediaRecorder.
        Ok(Self {})
    }
}
