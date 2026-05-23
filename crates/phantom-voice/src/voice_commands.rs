//! Voice command detection — hotword "Hey Phantom" + command parsing.

use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;

/// Recognised voice commands.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum VoiceCommand {
    StartRecording,
    StopRecording,
    MarkChapter { name: Option<String> },
    AddBookmark,
    SummariseNow,
    /// Passthrough for free-form Pro NL commands.
    NaturalLanguage { text: String },
}

pub struct VoiceCommandDetector {
    cmd_tx: broadcast::Sender<VoiceCommand>,
}

impl VoiceCommandDetector {
    pub fn new() -> Self {
        let (cmd_tx, _) = broadcast::channel(32);
        Self { cmd_tx }
    }

    pub fn subscribe(&self) -> broadcast::Receiver<VoiceCommand> {
        self.cmd_tx.subscribe()
    }

    /// Feed a transcribed phrase; attempt to parse a command.
    /// Returns `Some(VoiceCommand)` if recognised, broadcasts it on the channel.
    pub fn process_phrase(&self, phrase: &str) -> Option<VoiceCommand> {
        let lower = phrase.to_lowercase();

        // Simple keyword matching — replace with a real keyword spotter in Phase 3.
        let cmd = if lower.contains("start recording") {
            Some(VoiceCommand::StartRecording)
        } else if lower.contains("stop recording") || lower.contains("stop and summarise") {
            Some(VoiceCommand::StopRecording)
        } else if lower.contains("mark chapter") {
            let name = lower.replace("mark chapter", "").trim().to_owned();
            Some(VoiceCommand::MarkChapter {
                name: if name.is_empty() { None } else { Some(name) },
            })
        } else if lower.contains("add bookmark") || lower.contains("bookmark this") {
            Some(VoiceCommand::AddBookmark)
        } else if lower.contains("summarise now") || lower.contains("summarize now") {
            Some(VoiceCommand::SummariseNow)
        } else {
            None
        };

        if let Some(ref c) = cmd {
            tracing::info!(?c, "Voice command recognised");
            let _ = self.cmd_tx.send(c.clone());
        }
        cmd
    }
}

impl Default for VoiceCommandDetector {
    fn default() -> Self { Self::new() }
}
