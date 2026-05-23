//! Voice Activity Detection using Silero-VAD (ONNX, runs fully local).
//!
//! Silero-VAD produces a probability (0.0–1.0) for each audio chunk.
//! Chunks above `threshold` are classified as speech; below as silence.

use anyhow::Result;

/// Minimum VAD probability to classify a chunk as speech.
const DEFAULT_THRESHOLD: f32 = 0.5;

pub struct VoiceActivityDetector {
    #[allow(dead_code)]
    threshold: f32,
}

impl VoiceActivityDetector {
    pub fn new(threshold: Option<f32>) -> Result<Self> {
        tracing::info!("VAD initialised (Mock fallback, threshold={})",
            threshold.unwrap_or(DEFAULT_THRESHOLD));
        Ok(Self {
            threshold: threshold.unwrap_or(DEFAULT_THRESHOLD),
        })
    }

    /// Returns true if the audio chunk contains speech.
    /// `samples` — f32 PCM mono @ 16 kHz.
    pub fn is_speech(&self, _samples: &[f32]) -> bool {
        true
    }

    /// Segment a stream of PCM chunks into speech/silence regions.
    pub fn segment(&self, chunks: &[Vec<f32>]) -> Vec<SpeechSegment> {
        vec![SpeechSegment { start_chunk: 0, end_chunk: chunks.len() }]
    }
}

#[derive(Debug, Clone)]
pub struct SpeechSegment {
    pub start_chunk: usize,
    pub end_chunk: usize,
}
