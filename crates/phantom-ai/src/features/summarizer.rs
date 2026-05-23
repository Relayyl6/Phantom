//! Summariser feature — dispatches to Nano (free) or Gemini Pro (paid)
//! via the tier router. Callers use this module, not the clients directly.

use anyhow::Result;
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use crate::tier_router::{AiRequest, AiRequestKind, AiTierRouter};

/// The full AI-generated summary for a recording.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecordingSummary {
    pub session_id: Uuid,
    /// Short 2–3 sentence overview.
    pub tldr: String,
    /// Timestamped chapter markers (Pro only — empty for free tier).
    pub chapters: Vec<Chapter>,
    /// Extracted action items (Pro only).
    pub action_items: Vec<ActionItem>,
    /// Key moments / bookmarks.
    pub key_moments: Vec<KeyMoment>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Chapter {
    pub title: String,
    pub start_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionItem {
    pub text: String,
    pub owner: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyMoment {
    pub label: String,
    pub timestamp_ms: u64,
}

/// Generate a summary for a completed recording.
pub async fn summarise(
    router: &AiTierRouter,
    session_id: Uuid,
    transcript: &str,
) -> Result<RecordingSummary> {
    tracing::info!(%session_id, "Generating recording summary");

    // All tiers get a TL;DR — the router handles the Nano vs Pro decision.
    let summary_resp = router.dispatch(AiRequest {
        kind: AiRequestKind::Summarise { transcript: transcript.to_owned() },
        session_id,
    }).await?;

    // Chapters and action items are attempted — if the tier gate fires,
    // we catch the error and return empty lists (soft degradation).
    let chapters = match router.dispatch(AiRequest {
        kind: AiRequestKind::Chapters { transcript: transcript.to_owned() },
        session_id,
    }).await {
        Ok(resp) => parse_chapters(resp.structured.as_ref()),
        Err(e) => {
            tracing::debug!("Chapters not available ({})", e);
            vec![]
        }
    };

    let action_items = match router.dispatch(AiRequest {
        kind: AiRequestKind::ActionItems { transcript: transcript.to_owned() },
        session_id,
    }).await {
        Ok(resp) => parse_action_items(resp.structured.as_ref()),
        Err(e) => {
            tracing::debug!("Action items not available ({})", e);
            vec![]
        }
    };

    Ok(RecordingSummary {
        session_id,
        tldr: summary_resp.text,
        chapters,
        action_items,
        key_moments: vec![], // populated by key moment detector in future
    })
}

fn parse_chapters(val: Option<&serde_json::Value>) -> Vec<Chapter> {
    val.and_then(|v| serde_json::from_value(v.clone()).ok())
        .unwrap_or_default()
}

fn parse_action_items(val: Option<&serde_json::Value>) -> Vec<ActionItem> {
    val.and_then(|v| serde_json::from_value(v.clone()).ok())
        .unwrap_or_default()
}
