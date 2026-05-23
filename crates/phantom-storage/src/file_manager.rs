//! File manager — recording directory layout and metadata serialisation.
//!
//! Each session gets its own directory:
//!   ~/Phantom/recordings/2026-05-06_00-01-00_{session_id_short}/
//!     video.mp4
//!     audio_raw.wav        (optional, for re-transcription)
//!     transcript.json
//!     summary.json
//!     meta.json

use anyhow::Result;
use chrono::{DateTime, Utc};
use directories::UserDirs;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use uuid::Uuid;

/// Metadata saved alongside every recording.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecordingMeta {
    pub session_id: Uuid,
    pub title: String,
    pub created_at: DateTime<Utc>,
    pub duration_secs: u64,
    pub width: u32,
    pub height: u32,
    pub fps: u32,
    pub has_webcam: bool,
    pub has_audio: bool,
    pub phantom_version: String,
}

pub struct FileManager {
    base_dir: PathBuf,
}

impl FileManager {
    /// Initialise, creating the base Phantom recordings directory if needed.
    pub fn new() -> Result<Self> {
        let base_dir = UserDirs::new()
            .map(|u| u.home_dir().join("Phantom").join("recordings"))
            .unwrap_or_else(|| PathBuf::from("Phantom/recordings"));
        std::fs::create_dir_all(&base_dir)?;
        tracing::info!(path = %base_dir.display(), "Recordings directory ready");
        Ok(Self { base_dir })
    }

    /// Create the session directory and return its path.
    pub fn session_dir(&self, session_id: Uuid, started_at: &DateTime<Utc>) -> Result<PathBuf> {
        let short_id = &session_id.to_string()[..8];
        let dir_name = format!(
            "{}_{short_id}",
            started_at.format("%Y-%m-%d_%H-%M-%S")
        );
        let path = self.base_dir.join(dir_name);
        std::fs::create_dir_all(&path)?;
        Ok(path)
    }

    /// Path where the MP4 will be written.
    pub fn video_path(&self, session_dir: &PathBuf) -> PathBuf {
        session_dir.join("video.mp4")
    }

    /// Path where the raw WAV will be written.
    pub fn audio_raw_path(&self, session_dir: &PathBuf) -> PathBuf {
        session_dir.join("audio_raw.wav")
    }

    /// Write `meta.json` for a completed recording.
    pub fn write_meta(&self, session_dir: &PathBuf, meta: &RecordingMeta) -> Result<()> {
        let path = session_dir.join("meta.json");
        let json = serde_json::to_string_pretty(meta)?;
        std::fs::write(&path, json)?;
        Ok(())
    }

    /// Write the transcript JSON.
    pub fn write_transcript(&self, session_dir: &PathBuf, transcript: &serde_json::Value) -> Result<()> {
        let path = session_dir.join("transcript.json");
        std::fs::write(path, serde_json::to_string_pretty(transcript)?)?;
        Ok(())
    }

    /// Write the AI summary JSON.
    pub fn write_summary(&self, session_dir: &PathBuf, summary: &serde_json::Value) -> Result<()> {
        let path = session_dir.join("summary.json");
        std::fs::write(path, serde_json::to_string_pretty(summary)?)?;
        Ok(())
    }

    /// Return total storage used by the recordings directory in bytes.
    pub fn total_storage_bytes(&self) -> u64 {
        dir_size(&self.base_dir)
    }
}

fn dir_size(path: &PathBuf) -> u64 {
    std::fs::read_dir(path)
        .ok()
        .map(|entries| {
            entries.filter_map(|e| e.ok()).map(|e| {
                let meta = e.metadata().ok();
                if e.path().is_dir() {
                    dir_size(&e.path())
                } else {
                    meta.map(|m| m.len()).unwrap_or(0)
                }
            }).sum()
        })
        .unwrap_or(0)
}
