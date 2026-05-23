//! Capture subsystem — screen, audio, webcam, and input tracking.

pub mod audio;
pub mod input_tracker;
pub mod screen;
pub mod webcam;

// ── Public re-exports ────────────────────────────────────────────────────────
pub use audio::{AudioCapture, AudioChunk, AudioConfig, AudioSource};
pub use screen::{CaptureConfig, PlatformCapture, ScreenCapture, VideoFrame};
pub use webcam::{WebcamCapture, WebcamConfig};
pub use input_tracker::InputTracker;
