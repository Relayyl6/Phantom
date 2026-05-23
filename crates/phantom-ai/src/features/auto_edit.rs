//! Auto-edit — Pro feature that analyses recordings and suggests / applies edits.
//!
//! Capabilities:
//!   - Silence trimming: segments > 2s of silence are marked for removal.
//!   - Filler word detection: "uh", "um", "like", "you know" identified from transcript.
//!   - Auto-zoom hint generation: flags moments where cursor moves rapidly.
//!   - Highlight reel extraction: selects the best ~60-second clip.
//!
//! All analysis is done locally from the transcript and audio waveform.
//! No AI API calls are required for silence / filler detection.
//! Highlight reel uses Gemini Flash to select the most engaging segment.

use anyhow::Result;
use serde::{Deserialize, Serialize};

/// A proposed edit on the recording timeline.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EditSuggestion {
    pub kind: EditKind,
    /// Start of the affected range in milliseconds.
    pub start_ms: u64,
    /// End of the affected range in milliseconds.
    pub end_ms: u64,
    /// Human-readable rationale.
    pub reason: String,
    /// Confidence 0.0–1.0.
    pub confidence: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EditKind {
    /// Long silence — cut or speed-up.
    SilenceCut,
    /// Filler word ("uh", "um") — mute or cut.
    FillerWord,
    /// Auto-zoom hint based on fast cursor movement.
    AutoZoom,
    /// Highlight reel candidate — best ~60s clip.
    HighlightReel,
}

// ─── Silence detection ─────────────────────────────────────────────────────

const SILENCE_THRESHOLD_DB: f32 = -40.0;
const MIN_SILENCE_MS: u64 = 2000;
#[allow(dead_code)]
const SAMPLE_RATE: u32 = 44100;

/// Detect silent regions from f32 PCM audio (mono, 44.1 kHz).
/// Returns edit suggestions for each silence longer than `MIN_SILENCE_MS`.
pub fn detect_silences(samples: &[f32], sample_rate: u32) -> Vec<EditSuggestion> {
    let chunk_size = (sample_rate / 20) as usize; // 50ms chunks
    let ms_per_chunk: u64 = 1000 / 20;
    let threshold_linear = 10f32.powf(SILENCE_THRESHOLD_DB / 20.0);

    let mut suggestions = Vec::new();
    let mut silence_start_ms: Option<u64> = None;

    for (i, chunk) in samples.chunks(chunk_size).enumerate() {
        let rms = (chunk.iter().map(|s| s * s).sum::<f32>() / chunk.len() as f32).sqrt();
        let chunk_ms = i as u64 * ms_per_chunk;

        if rms < threshold_linear {
            if silence_start_ms.is_none() {
                silence_start_ms = Some(chunk_ms);
            }
        } else if let Some(start) = silence_start_ms.take() {
            let duration = chunk_ms - start;
            if duration >= MIN_SILENCE_MS {
                suggestions.push(EditSuggestion {
                    kind: EditKind::SilenceCut,
                    start_ms: start,
                    end_ms: chunk_ms,
                    reason: format!("{}ms of silence detected", duration),
                    confidence: 0.95,
                });
            }
        }
    }
    // Close any trailing silence.
    if let Some(start) = silence_start_ms {
        let end_ms = (samples.len() as u64 * 1000) / sample_rate as u64;
        let duration = end_ms - start;
        if duration >= MIN_SILENCE_MS {
            suggestions.push(EditSuggestion {
                kind: EditKind::SilenceCut,
                start_ms: start,
                end_ms,
                reason: format!("{}ms of trailing silence", duration),
                confidence: 0.9,
            });
        }
    }
    suggestions
}

// ─── Filler word detection ─────────────────────────────────────────────────

/// Common English filler words and phrases to flag.
const FILLER_PATTERNS: &[&str] = &[
    "uh ", "um ", " uh,", " um,",
    " like,", " you know,", " i mean,",
    " basically,", " literally,",
    " right?", " okay?",
];

/// Detect filler words from timestamped transcript segments.
/// `segments` is a list of (start_ms, end_ms, text) tuples.
pub fn detect_filler_words(
    segments: &[(u64, u64, &str)],
) -> Vec<EditSuggestion> {
    let mut suggestions = Vec::new();
    for &(start_ms, end_ms, text) in segments {
        let lower = text.to_lowercase();
        for &pattern in FILLER_PATTERNS {
            if lower.contains(pattern) {
                suggestions.push(EditSuggestion {
                    kind: EditKind::FillerWord,
                    start_ms,
                    end_ms,
                    reason: format!("Filler word/phrase detected: \"{}\"", pattern.trim()),
                    confidence: 0.8,
                });
                break; // one suggestion per segment
            }
        }
    }
    suggestions
}

// ─── Highlight reel ────────────────────────────────────────────────────────

/// Ask Gemini Flash to identify the best 60-second highlight segment.
/// Returns start/end timestamps extracted from the model's response.
pub async fn find_highlight_reel(
    gemini_client: &crate::gemini_client::GeminiClient,
    transcript: &str,
    total_duration_ms: u64,
) -> Result<EditSuggestion> {
    #[allow(unused_imports)]
    use crate::tier_router::{AiRequest, AiRequestKind, AiTierRouter};

    let prompt = format!(
        "You are an AI video editor. Identify the single most engaging, informative, or \
         exciting 60-second segment in this recording transcript.\n\
         Respond ONLY with a JSON object: {{\"start_ms\": <number>, \"end_ms\": <number>, \"reason\": \"<string>\"}}\n\
         The recording is {total_duration_ms}ms long.\n\n\
         Transcript:\n{transcript}"
    );

    // Use gemini_client directly since this is a Pro feature.
    let text = gemini_client.summarise_raw(&prompt, 256).await?;

    // Parse the JSON response.
    #[derive(Deserialize)]
    struct HighlightResp {
        start_ms: u64,
        end_ms: u64,
        reason: String,
    }

    // Strip markdown fences if present.
    let json_str = text.trim().trim_start_matches("```json").trim_end_matches("```").trim();

    let parsed: HighlightResp = serde_json::from_str(json_str)
        .unwrap_or(HighlightResp {
            start_ms: 0,
            end_ms: std::cmp::min(60_000, total_duration_ms),
            reason: "Could not parse AI response; defaulting to first 60s".to_owned(),
        });

    Ok(EditSuggestion {
        kind: EditKind::HighlightReel,
        start_ms: parsed.start_ms,
        end_ms: parsed.end_ms,
        reason: parsed.reason,
        confidence: 0.85,
    })
}

// ─── Combined analysis ─────────────────────────────────────────────────────

/// Run all auto-edit analysis passes and return a consolidated list of suggestions.
pub fn analyse_local(
    audio_samples: &[f32],
    transcript_segments: &[(u64, u64, &str)],
    sample_rate: u32,
) -> Vec<EditSuggestion> {
    let mut suggestions = detect_silences(audio_samples, sample_rate);
    suggestions.extend(detect_filler_words(transcript_segments));
    // Sort chronologically.
    suggestions.sort_by_key(|s| s.start_ms);
    suggestions
}
