//! phantom-core — capture pipeline, async event bus, and encoder.
//!
//! Platform targets:
//!   Windows — DXGI Desktop Duplication + WASAPI
//!   macOS   — ScreenCaptureKit + CoreAudio

pub mod bus;
pub mod capture;
pub mod encoder;
pub mod plugin;

pub use bus::{Bus, PhantomEvent};
