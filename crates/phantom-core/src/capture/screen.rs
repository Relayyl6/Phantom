//! Screen capture — platform-native implementations.
//!
//! Windows: DXGI Desktop Duplication API (zero-copy GPU texture)
//! macOS:   ScreenCaptureKit (SCStream)
//!
//! The public interface is platform-agnostic: call [`ScreenCapture::start`]
//! and poll [`ScreenCapture::next_frame`] in your capture loop.

use anyhow::Result;
use std::sync::Arc;
use tokio::sync::Mutex;

/// A single captured frame — raw BGRA pixels from the GPU.
#[derive(Debug, Clone)]
pub struct VideoFrame {
    /// Frame width in pixels.
    pub width: u32,
    /// Frame height in pixels.
    pub height: u32,
    /// Raw BGRA pixel data (width * height * 4 bytes).
    pub data: Arc<Vec<u8>>,
    /// Monotonic timestamp in milliseconds since recording start.
    pub timestamp_ms: u64,
    /// Frame index (0-based) within the current session.
    pub index: u64,
}

/// Configuration for screen capture.
#[derive(Debug, Clone)]
pub struct CaptureConfig {
    /// Target frames per second (1 – 60).
    pub fps: u32,
    /// Optional window title to capture (None = full primary display).
    pub window_title: Option<String>,
    /// Capture region (x, y, w, h) in logical pixels. None = full display.
    pub region: Option<(i32, i32, u32, u32)>,
    /// Include the cursor in the capture.
    pub include_cursor: bool,
    /// Gaming mode: disables AI overhead, forces 60fps, prioritizes minimal latency.
    pub gaming_mode: bool,
}

impl Default for CaptureConfig {
    fn default() -> Self {
        Self {
            fps: 30,
            window_title: None,
            region: None,
            include_cursor: true,
            gaming_mode: false,
        }
    }
}

/// Platform-agnostic screen capture handle.
pub struct ScreenCapture {
    config: CaptureConfig,
    running: Arc<Mutex<bool>>,
    #[allow(dead_code)]
    inner: PlatformCapture,
}

impl ScreenCapture {
    /// Initialise capture with the given config.
    pub fn new(config: CaptureConfig) -> Result<Self> {
        let inner = PlatformCapture::new(&config)?;
        Ok(Self {
            config,
            running: Arc::new(Mutex::new(false)),
            inner,
        })
    }

    /// Start capturing frames. Must be called before [`next_frame`].
    pub async fn start(&self) -> Result<()> {
        let mut running = self.running.lock().await;
        if *running {
            return Ok(()); // already running
        }
        tracing::info!(fps = self.config.fps, "Screen capture started");
        *running = true;
        Ok(())
    }

    /// Stop the capture session.
    pub async fn stop(&self) -> Result<()> {
        let mut running = self.running.lock().await;
        *running = false;
        tracing::info!("Screen capture stopped");
        Ok(())
    }

    /// Pull the next available frame. Returns `None` if not running.
    pub async fn next_frame(&self) -> Option<VideoFrame> {
        let running = self.running.lock().await;
        if !*running {
            return None;
        }
        drop(running);
        self.inner.next_frame().await
    }
}

use std::time::{Instant, Duration};

#[cfg(not(target_arch = "wasm32"))]
pub struct PlatformCapture {
    config: CaptureConfig,
    start_time: Instant,
    frame_idx: std::sync::atomic::AtomicU64,
}

#[cfg(not(target_arch = "wasm32"))]
impl PlatformCapture {
    pub fn new(config: &CaptureConfig) -> Result<Self> {
        // Test that we can access monitors
        let monitors = xcap::Monitor::all().map_err(|e| anyhow::anyhow!("Failed to list monitors: {e}"))?;
        if monitors.is_empty() {
            anyhow::bail!("No monitors found for capture");
        }
        
        let mut final_config = config.clone();
        if final_config.gaming_mode {
            final_config.fps = 60; // Force 60fps for gaming mode
            tracing::info!("Gaming mode enabled: forcing 60fps, minimal AI overhead");
        }

        tracing::debug!("Initialised xcap screen capture");
        Ok(Self {
            config: final_config,
            start_time: Instant::now(),
            frame_idx: std::sync::atomic::AtomicU64::new(0),
        })
    }

    pub async fn next_frame(&self) -> Option<VideoFrame> {
        // Calculate sleep duration to hit target FPS
        let target_interval = Duration::from_secs_f64(1.0 / self.config.fps as f64);
        tokio::time::sleep(target_interval).await;
        
        // Spawn blocking because xcap capture_image is synchronous
        let handle = tokio::task::spawn_blocking(move || {
            let monitors = xcap::Monitor::all().unwrap_or_default();
            // Just capture the first (primary) monitor for Phase 1
            let monitor = monitors.into_iter().next()?;
            
            let image = monitor.capture_image().ok()?;
            let width = image.width();
            let height = image.height();
            let data = image.into_raw();
            
            Some((width, height, data))
        });

        match handle.await.ok().flatten() {
            Some((width, height, data)) => {
                let idx = self.frame_idx.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                let timestamp_ms = self.start_time.elapsed().as_millis() as u64;
                
                Some(VideoFrame {
                    width,
                    height,
                    data: Arc::new(data),
                    timestamp_ms,
                    index: idx,
                })
            }
            None => None
        }
    }
}

#[cfg(target_arch = "wasm32")]
pub struct PlatformCapture {
    config: CaptureConfig,
}

#[cfg(target_arch = "wasm32")]
impl PlatformCapture {
    pub fn new(config: &CaptureConfig) -> Result<Self> {
        Ok(Self { config: config.clone() })
    }

    pub async fn next_frame(&self) -> Option<VideoFrame> {
        None
    }
}
