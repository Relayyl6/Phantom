//! Live AI Assistant — always-on overlay during recording (Pro only).
//!
//! Uses `gemini-2.0-flash` to give real-time responses during an active session.
//! Activated by voice command "Hey Phantom" or keyboard shortcut Ctrl+Shift+A.
//!
//! The assistant receives:
//!   - The user's spoken query (transcribed by Whisper in <500ms)
//!   - A rolling 30-second screen context summary (low-FPS keyframe analysis)
//!   - The running transcript of the current session

use anyhow::Result;
use serde::{Deserialize, Serialize};
use crate::tier_router::{AiRequest, AiRequestKind, AiTierRouter};
use uuid::Uuid;

/// A single interaction with the live assistant.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssistantTurn {
    pub user_message: String,
    pub assistant_reply: String,
    /// Screen context (keyframe description) that was provided.
    pub screen_context: Option<String>,
    /// Timestamp of this turn (seconds since session start).
    pub session_offset_secs: u64,
}

/// Stateful live assistant session for a single recording.
pub struct LiveAssistant<'a> {
    router: &'a AiTierRouter,
    session_id: Uuid,
    /// Rolling session transcript (updated as recording progresses).
    rolling_transcript: String,
    /// History of turns for multi-turn coherence (capped at 5 turns).
    pub history: Vec<AssistantTurn>,
    /// Monotonic session clock (seconds elapsed since recording start).
    session_secs: u64,
}

impl<'a> LiveAssistant<'a> {
    pub fn new(router: &'a AiTierRouter, session_id: Uuid) -> Self {
        Self {
            router,
            session_id,
            rolling_transcript: String::new(),
            history: Vec::new(),
            session_secs: 0,
        }
    }

    /// Append new transcript text as the recording progresses.
    pub fn push_transcript(&mut self, text: &str) {
        if !self.rolling_transcript.is_empty() {
            self.rolling_transcript.push(' ');
        }
        self.rolling_transcript.push_str(text);
        // Keep the rolling transcript at most 4000 chars to limit tokens.
        if self.rolling_transcript.len() > 4000 {
            let trim_at = self.rolling_transcript.len() - 4000;
            // Trim at word boundary.
            let trim_at = self.rolling_transcript[trim_at..]
                .find(' ')
                .map(|p| trim_at + p + 1)
                .unwrap_or(trim_at);
            self.rolling_transcript = self.rolling_transcript[trim_at..].to_owned();
        }
    }

    /// Advance the session clock (call this every second from the event loop).
    pub fn tick(&mut self) {
        self.session_secs += 1;
    }

    /// Ask the assistant a question, optionally with screen context.
    ///
    /// `message`        — the user's spoken/typed query.
    /// `screen_context` — optional keyframe description from the screen analyser.
    pub async fn ask(
        &mut self,
        message: &str,
        screen_context: Option<String>,
    ) -> Result<AssistantTurn> {
        // Prepend short history for multi-turn coherence (last 3 turns).
        let history_ctx = self
            .history
            .iter()
            .rev()
            .take(3)
            .rev()
            .map(|t| format!("User: {}\nPhantom: {}", t.user_message, t.assistant_reply))
            .collect::<Vec<_>>()
            .join("\n");

        let augmented_message = if history_ctx.is_empty() {
            message.to_owned()
        } else {
            format!(
                "Previous assistant turns:\n{history_ctx}\n\nUser's new question: {message}"
            )
        };

        // Include the current rolling transcript as part of the context prompt.
        let full_screen_ctx = if self.rolling_transcript.is_empty() {
            screen_context.clone()
        } else {
            let transcript_snippet = &self.rolling_transcript
                [self.rolling_transcript.len().saturating_sub(1500)..];
            let base = format!("Current session transcript (last segment): {transcript_snippet}");
            Some(match screen_context.as_deref() {
                Some(s) => format!("{base}\nScreen: {s}"),
                None => base,
            })
        };

        let resp = self
            .router
            .dispatch(AiRequest {
                kind: AiRequestKind::LiveAssistant {
                    message: augmented_message,
                    screen_context: full_screen_ctx.clone(),
                },
                session_id: self.session_id,
            })
            .await?;

        let turn = AssistantTurn {
            user_message: message.to_owned(),
            assistant_reply: resp.text,
            screen_context: full_screen_ctx,
            session_offset_secs: self.session_secs,
        };

        // Keep history bounded at 5 turns.
        if self.history.len() >= 5 {
            self.history.remove(0);
        }
        self.history.push(turn.clone());
        Ok(turn)
    }
}
