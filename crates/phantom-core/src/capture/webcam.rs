//! Webcam capture with configurable PiP overlay.
//!
//! Windows: MediaFoundation IMFSourceReader (via nokhwa)
//! macOS:   AVCaptureSession (via nokhwa)
//!
//! Background removal (blur/replace) is applied via a local ONNX model
//! (MediaPipe Selfie Segmentation) — no API cost.

use anyhow::Result;
use std::sync::{Arc, Mutex};

/// Where the PiP overlay is rendered relative to the main recording.
#[derive(Debug, Clone, Copy, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PipPosition {
    #[default]
    BottomRight,
    BottomLeft,
    TopRight,
    TopLeft,
    /// Fullscreen webcam (no screen content).
    Fullscreen,
}

/// Background treatment for the webcam feed.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BackgroundMode {
    /// Show the raw webcam feed.
    #[default]
    None,
    /// Gaussian blur behind the subject (ONNX local inference).
    Blur { radius: u8 },
    /// Replace background with a static colour.
    SolidColour { r: u8, g: u8, b: u8 },
    /// Replace background with an image file.
    Image { path: String },
}

/// Webcam capture configuration.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct WebcamConfig {
    pub enabled: bool,
    pub position: PipPosition,
    /// Width of the PiP overlay as fraction of total width (0.0 – 1.0).
    pub size_fraction: f32,
    pub background: BackgroundMode,
    /// Device index (0 = first/default camera)
    pub device_index: usize,
}

impl Default for WebcamConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            position: PipPosition::BottomRight,
            size_fraction: 0.22,
            background: BackgroundMode::Blur { radius: 12 },
            device_index: 0,
        }
    }
}

/// A single webcam frame (BGRA).
#[derive(Debug, Clone)]
pub struct WebcamFrame {
    pub width: u32,
    pub height: u32,
    pub data: Vec<u8>,
    pub timestamp_ms: u64,
}

/// Webcam capture handle — wraps a nokhwa Camera in a background thread.
pub struct WebcamCapture {
    pub config: WebcamConfig,
    /// Latest frame from the background capture thread.
    latest_frame: Arc<Mutex<Option<WebcamFrame>>>,
    stop_flag: Arc<std::sync::atomic::AtomicBool>,
}

impl WebcamCapture {
    pub fn new(config: WebcamConfig) -> Result<Self> {
        Ok(Self {
            config,
            latest_frame: Arc::new(Mutex::new(None)),
            stop_flag: Arc::new(std::sync::atomic::AtomicBool::new(false)),
        })
    }

    pub async fn start(&self) -> Result<()> {
        if !self.config.enabled {
            return Ok(());
        }

        let device_index = self.config.device_index as u32;
        let latest_frame = self.latest_frame.clone();
        let stop_flag = self.stop_flag.clone();
        let session_start = std::time::Instant::now();

        self.stop_flag.store(false, std::sync::atomic::Ordering::Relaxed);

        std::thread::spawn(move || {
            use nokhwa::utils::{CameraIndex, RequestedFormat, RequestedFormatType};
            use nokhwa::pixel_format::RgbFormat;

            let index = CameraIndex::Index(device_index);
            let format = RequestedFormat::new::<RgbFormat>(RequestedFormatType::AbsoluteHighestFrameRate);

            let mut camera = match nokhwa::Camera::new(index, format) {
                Ok(c) => c,
                Err(e) => {
                    tracing::error!("Failed to open webcam device {device_index}: {e}");
                    return;
                }
            };

            if let Err(e) = camera.open_stream() {
                tracing::error!("Failed to open webcam stream: {e}");
                return;
            }

            tracing::info!("Webcam stream opened on device index {device_index}");

            loop {
                if stop_flag.load(std::sync::atomic::Ordering::Relaxed) {
                    break;
                }

                match camera.frame() {
                    Ok(frame) => {
                        let width = frame.resolution().width();
                        let height = frame.resolution().height();
                        let data = frame.buffer().to_vec();
                        let timestamp_ms = session_start.elapsed().as_millis() as u64;

                        if let Ok(mut lock) = latest_frame.lock() {
                            *lock = Some(WebcamFrame { width, height, data, timestamp_ms });
                        }
                    }
                    Err(e) => {
                        tracing::warn!("Webcam frame error: {e}");
                        std::thread::sleep(std::time::Duration::from_millis(33));
                    }
                }
            }

            let _ = camera.stop_stream();
            tracing::info!("Webcam stream closed");
        });

        tracing::info!("Webcam capture started via nokhwa");
        Ok(())
    }

    pub async fn stop(&self) -> Result<()> {
        self.stop_flag.store(true, std::sync::atomic::Ordering::Relaxed);
        tracing::info!("Webcam capture stopped");
        Ok(())
    }

    /// Returns the most recent frame, or None if not streaming.
    pub async fn next_frame(&self) -> Option<WebcamFrame> {
        self.latest_frame.lock().ok()?.clone()
    }
}
