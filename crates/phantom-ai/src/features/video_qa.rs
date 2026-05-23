//! Video Q&A — Pro feature that lets users ask natural language questions
//! about a recording. Sends the transcript + keyframe context to Gemini Pro Vision.

use anyhow::Result;
use serde::{Deserialize, Serialize};
use crate::tier_router::{AiRequest, AiRequestKind, AiTierRouter};
use uuid::Uuid;

/// A single Q&A exchange about a recording.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QaExchange {
    pub question: String,
    pub answer: String,
    /// Relevant timestamp in the recording (milliseconds), if detected.
    pub relevant_timestamp_ms: Option<u64>,
}

/// Session for answering multiple questions about one recording.
pub struct VideoQaSession<'a> {
    router: &'a AiTierRouter,
    session_id: Uuid,
    /// Full transcript text (pre-loaded once for the session).
    transcript: String,
    /// Conversation history for multi-turn context.
    pub history: Vec<QaExchange>,
}

impl<'a> VideoQaSession<'a> {
    pub fn new(router: &'a AiTierRouter, session_id: Uuid, transcript: String) -> Self {
        Self {
            router,
            session_id,
            transcript,
            history: Vec::new(),
        }
    }

    /// Ask a question about the recording. Builds multi-turn context from history.
    pub async fn ask(&mut self, question: &str) -> Result<QaExchange> {
        // Build a context string from the last 3 turns to keep token usage bounded.
        let history_context: String = self
            .history
            .iter()
            .rev()
            .take(3)
            .rev()
            .map(|ex| format!("Q: {}\nA: {}", ex.question, ex.answer))
            .collect::<Vec<_>>()
            .join("\n\n");

        let augmented_question = if history_context.is_empty() {
            question.to_owned()
        } else {
            format!("Previous conversation:\n{history_context}\n\nNew question: {question}")
        };

        let resp = self
            .router
            .dispatch(AiRequest {
                kind: AiRequestKind::VideoQa {
                    transcript: self.transcript.clone(),
                    question: augmented_question,
                },
                session_id: self.session_id,
            })
            .await?;

        // Try to extract a timestamp from the response (e.g. "at 2:34" or "around 154s").
        let relevant_timestamp_ms = extract_timestamp_hint(&resp.text);

        let exchange = QaExchange {
            question: question.to_owned(),
            answer: resp.text,
            relevant_timestamp_ms,
        };
        self.history.push(exchange.clone());
        Ok(exchange)
    }

    /// Clear conversation history (start a fresh session on the same recording).
    pub fn reset_history(&mut self) {
        self.history.clear();
    }
}

/// Best-effort extraction of a timestamp hint from a Gemini response.
/// Handles formats like "at 2:34", "around 154 seconds", "at the 1:30 mark".
fn extract_timestamp_hint(text: &str) -> Option<u64> {
    // Pattern: "M:SS" or "H:MM:SS"
    let colon_re = regex::Regex::new(r"(\d{1,2}):(\d{2})(?::(\d{2}))?").ok()?;
    if let Some(cap) = colon_re.captures(text) {
        let h: u64 = cap.get(3).map(|_| cap[1].parse().unwrap_or(0)).unwrap_or(0);
        let m: u64 = if cap.get(3).is_some() {
            cap[2].parse().unwrap_or(0)
        } else {
            cap[1].parse().unwrap_or(0)
        };
        let s: u64 = if cap.get(3).is_some() {
            cap.get(3).map(|c| c.as_str().parse().unwrap_or(0)).unwrap_or(0)
        } else {
            cap[2].parse().unwrap_or(0)
        };
        return Some((h * 3600 + m * 60 + s) * 1000);
    }

    // Pattern: "NNN seconds"
    let secs_re = regex::Regex::new(r"(\d+)\s*(?:second|sec)s?").ok()?;
    if let Some(cap) = secs_re.captures(&text.to_lowercase()) {
        let s: u64 = cap[1].parse().unwrap_or(0);
        return Some(s * 1000);
    }

    None
}
