//! phantom-voice — VAD, noise suppression, mic pipeline, voice commands.

pub mod mic_pipeline;
pub mod vad;
pub mod voice_commands;

pub use mic_pipeline::MicPipeline;
pub use vad::VoiceActivityDetector;
pub use voice_commands::VoiceCommandDetector;
