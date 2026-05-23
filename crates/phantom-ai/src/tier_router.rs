//! AI Tier Router — routes requests to Gemini Nano (free) or Gemini Pro (paid).
//!
//! Free tier:  Gemini Nano on-device, hard-capped at 3 summaries / day.
//! Pro tier:   Gemini 2.5 Pro / Flash via API — unlimited, caller pays Stripe.
//!
//! The router reads the current entitlement from `phantom-billing` and
//! transparently selects the right backend. Callers never need to know
//! which backend ran.

use anyhow::Result;
use phantom_billing::entitlement::{EntitlementStore, Tier};
use serde::{Deserialize, Serialize};
use tracing::instrument;

/// A request to the AI tier router.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiRequest {
    pub kind: AiRequestKind,
    /// Session ID for logging / correlation.
    pub session_id: uuid::Uuid,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AiRequestKind {
    /// Transcribe audio at the given path.
    Transcribe { audio_path: String },
    /// Summarise a recording (transcript + optional keyframes).
    Summarise { transcript: String },
    /// Generate chapter markers from a transcript.
    Chapters { transcript: String },
    /// Extract action items from a transcript.
    ActionItems { transcript: String },
    /// Answer a question about a video.
    VideoQa { transcript: String, question: String },
    /// Send a message to the live assistant (streaming).
    LiveAssistant { message: String, screen_context: Option<String> },
}

/// The AI response returned to callers.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiResponse {
    /// Which backend actually ran this request.
    pub backend: AiBackend,
    /// The text result.
    pub text: String,
    /// Structured data for chapter/action-item responses.
    pub structured: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AiBackend {
    /// Ran locally on-device via Gemini Nano.
    GeminiNano,
    /// Ran against Gemini Pro API.
    GeminiPro,
    /// Ran locally via Whisper.cpp (transcription only).
    WhisperLocal,
}

/// The central routing hub.
pub struct AiTierRouter {
    entitlement: EntitlementStore,
    gemini: crate::gemini_client::GeminiClient,
    nano: crate::nano_client::NanoClient,
}

impl AiTierRouter {
    pub fn new(
        entitlement: EntitlementStore,
        gemini_api_key: String,
    ) -> Self {
        Self {
            entitlement,
            gemini: crate::gemini_client::GeminiClient::new(gemini_api_key),
            nano: crate::nano_client::NanoClient::new(),
        }
    }

    /// Dispatch a request to the appropriate backend.
    #[instrument(skip(self), fields(kind = ?req.kind))]
    pub async fn dispatch(&self, req: AiRequest) -> Result<AiResponse> {
        let tier = self.entitlement.current_tier().await?;

        match &req.kind {
            // Transcription: always use local Whisper first (free + offline).
            AiRequestKind::Transcribe { audio_path } => {
                tracing::debug!("Routing transcription to local Whisper");
                crate::features::transcription::transcribe_local(audio_path).await
            }

            // Summarise: free tier → Nano (with daily cap check).
            AiRequestKind::Summarise { transcript } => {
                match tier {
                    Tier::Free => {
                        self.entitlement.check_and_increment_daily_ai().await?;
                        self.nano.summarise(transcript).await
                    }
                    Tier::Pro | Tier::Team => {
                        self.gemini.summarise(transcript).await
                    }
                }
            }

            // Chapters, ActionItems, VideoQa, LiveAssistant → Pro only.
            AiRequestKind::Chapters { .. }
            | AiRequestKind::ActionItems { .. }
            | AiRequestKind::VideoQa { .. }
            | AiRequestKind::LiveAssistant { .. } => {
                if tier == Tier::Free {
                    anyhow::bail!(
                        "This feature requires a Phantom Pro subscription. \
                         Upgrade at https://phantom.app/upgrade"
                    );
                }
                self.gemini.dispatch_generic(&req).await
            }
        }
    }
}
