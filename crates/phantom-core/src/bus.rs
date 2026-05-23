//! Async event bus — the nervous system of Phantom.
//!
//! All subsystems communicate exclusively through [`PhantomEvent`] messages
//! broadcast on a Tokio broadcast channel. This keeps every crate fully
//! decoupled and makes adding new listeners (plugins, UI, AI) trivial.

use std::sync::Arc;
use tokio::sync::broadcast;
use uuid::Uuid;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// The channel capacity — 1 024 events in flight before oldest are dropped.
const BUS_CAPACITY: usize = 1_024;

/// Every significant action in Phantom emits one of these events.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", content = "payload")]
pub enum PhantomEvent {
    /// A new recording session has begun.
    RecordingStarted {
        session_id: Uuid,
        started_at: DateTime<Utc>,
    },

    /// The active recording has been stopped.
    RecordingStopped {
        session_id: Uuid,
        stopped_at: DateTime<Utc>,
        /// Duration in seconds.
        duration_secs: u64,
        /// Path to the output MP4 on disk.
        output_path: String,
    },

    /// A raw video frame is available (not serialised — passed by Arc).
    #[serde(skip)]
    FrameCaptured {
        session_id: Uuid,
        /// Frame index within the session.
        frame_idx: u64,
        /// Timestamp relative to recording start (ms).
        timestamp_ms: u64,
    },

    /// A chunk of interleaved PCM audio is ready.
    AudioChunkReady {
        session_id: Uuid,
        /// Chunk index.
        chunk_idx: u64,
        /// Sample rate (e.g. 44100).
        sample_rate: u32,
        /// Channel count.
        channels: u16,
        /// Total PCM samples in this chunk.
        sample_count: usize,
    },

    /// The AI engine has finished processing a result.
    AiResultReady {
        session_id: Uuid,
        result_kind: AiResultKind,
    },

    /// A recording has been uploaded to Supabase Storage.
    UploadComplete {
        session_id: Uuid,
        share_url: String,
    },

    /// Upload progress (0.0 – 1.0).
    UploadProgress {
        session_id: Uuid,
        progress: f32,
    },

    /// The user's billing entitlement has changed (e.g. subscription renewed).
    EntitlementChanged {
        new_tier: crate::bus::Tier,
    },

    /// A voice command was recognised.
    VoiceCommand {
        command: String,
    },

    /// Application is shutting down cleanly.
    Shutdown,
}

/// Billing tier carried in entitlement events.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Tier {
    Free,
    Pro,
    Team,
}

/// What kind of AI result was produced.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AiResultKind {
    Transcript,
    Summary,
    Chapters,
    ActionItems,
    VideoQa { question: String },
    LiveAssistant { response: String },
}

/// The shared bus handle — cheap to clone, backed by an Arc.
#[derive(Debug, Clone)]
pub struct Bus {
    inner: Arc<broadcast::Sender<PhantomEvent>>,
}

impl Bus {
    /// Create a new bus. Hold on to the returned [`Bus`]; drop it to shut down.
    pub fn new() -> Self {
        let (tx, _) = broadcast::channel(BUS_CAPACITY);
        Self { inner: Arc::new(tx) }
    }

    /// Publish an event. Returns the number of active receivers.
    pub fn publish(&self, event: PhantomEvent) -> usize {
        self.inner.send(event).unwrap_or(0)
    }

    /// Subscribe to receive all future events.
    pub fn subscribe(&self) -> broadcast::Receiver<PhantomEvent> {
        self.inner.subscribe()
    }
}

impl Default for Bus {
    fn default() -> Self {
        Self::new()
    }
}
