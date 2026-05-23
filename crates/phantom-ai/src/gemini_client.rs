//! Gemini Pro API client — streaming SSE, retry logic, model selection.
//!
//! Uses models:
//!   gemini-2.5-pro      — deep analysis, chapters, action items
//!   gemini-2.0-flash    — fast summaries
//!   gemini-2.0-flash-live — real-time voice (Live Assistant)

use anyhow::Result;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use crate::tier_router::{AiBackend, AiRequest, AiRequestKind, AiResponse};

const GEMINI_BASE: &str = "https://generativelanguage.googleapis.com/v1beta";

/// Model identifiers.
mod models {
    pub const PRO:   &str = "gemini-2.5-pro";
    pub const FLASH: &str = "gemini-2.0-flash";
    // Live API uses a different endpoint — handled separately.
}

#[derive(Debug, Serialize)]
struct GenerateRequest<'a> {
    contents: Vec<Content<'a>>,
    #[serde(rename = "generationConfig")]
    generation_config: GenerationConfig,
}

#[derive(Debug, Serialize)]
struct Content<'a> {
    role: &'a str,
    parts: Vec<Part<'a>>,
}

#[derive(Debug, Serialize)]
struct Part<'a> {
    #[serde(skip_serializing_if = "Option::is_none")]
    text: Option<&'a str>,
    #[serde(rename = "inlineData", skip_serializing_if = "Option::is_none")]
    inline_data: Option<InlineData<'a>>,
}

#[derive(Debug, Serialize)]
struct InlineData<'a> {
    #[serde(rename = "mimeType")]
    mime_type: &'a str,
    data: &'a str,
}

#[derive(Debug, Serialize)]
struct GenerationConfig {
    temperature: f32,
    #[serde(rename = "maxOutputTokens")]
    max_output_tokens: u32,
}

#[derive(Debug, Deserialize)]
struct GenerateResponse {
    candidates: Vec<Candidate>,
}

#[derive(Debug, Deserialize)]
struct Candidate {
    content: CandidateContent,
}

#[derive(Debug, Deserialize)]
struct CandidateContent {
    parts: Vec<CandidatePart>,
}

#[derive(Debug, Deserialize)]
struct CandidatePart {
    text: String,
}

pub struct GeminiClient {
    api_key: String,
    http: Client,
}

impl GeminiClient {
    pub fn new(api_key: String) -> Self {
        Self {
            api_key,
            http: Client::builder()
                .timeout(std::time::Duration::from_secs(120))
                .build()
                .expect("reqwest client"),
        }
    }

    /// Generate a summary using gemini-2.0-flash (fast).
    pub async fn summarise(&self, transcript: &str) -> Result<AiResponse> {
        let prompt = format!(
            "You are an AI assistant embedded in a screen recording app called Phantom.\n\
             Analyse the following recording transcript and produce:\n\
             1. A 2–3 sentence TL;DR summary.\n\
             2. 3–5 bullet-point key moments.\n\
             3. A list of action items (tasks, decisions, follow-ups).\n\n\
             Transcript:\n{transcript}"
        );
        let text = self.generate(models::FLASH, &prompt, 0.3, 1024).await?;
        Ok(AiResponse {
            backend: AiBackend::GeminiPro,
            text,
            structured: None,
        })
    }

    /// Dispatch a generic request to the appropriate Gemini model.
    pub async fn dispatch_generic(&self, req: &AiRequest) -> Result<AiResponse> {
        let (model, prompt, max_tokens) = match &req.kind {
            AiRequestKind::Chapters { transcript } => (
                models::PRO,
                format!(
                    "Analyse this recording transcript and generate a list of chapters.\n\
                     Format: JSON array of {{\"title\": string, \"start_ms\": number}}.\n\n\
                     Transcript:\n{transcript}"
                ),
                2048,
            ),
            AiRequestKind::ActionItems { transcript } => (
                models::FLASH,
                format!(
                    "Extract all action items, decisions, and follow-ups from this transcript.\n\
                     Format: JSON array of {{\"item\": string, \"owner\": string|null}}.\n\n\
                     Transcript:\n{transcript}"
                ),
                1024,
            ),
            AiRequestKind::VideoQa { transcript, question } => (
                models::PRO,
                format!(
                    "Answer the following question about a screen recording, using the transcript below.\n\
                     Question: {question}\n\nTranscript:\n{transcript}"
                ),
                2048,
            ),
            AiRequestKind::LiveAssistant { message, screen_context } => {
                let ctx = screen_context.as_deref().unwrap_or("(no screen context)");
                (
                    models::FLASH,
                    format!(
                        "You are Phantom's live AI assistant. The user is currently recording their screen.\n\
                         Screen context: {ctx}\n\nUser says: {message}\n\
                         Respond concisely (1–3 sentences max)."
                    ),
                    512,
                )
            }
            _ => anyhow::bail!("Unsupported request kind for dispatch_generic"),
        };

        let text = self.generate(model, &prompt, 0.4, max_tokens).await?;
        // Attempt to parse structured JSON if the response looks like JSON.
        let structured = serde_json::from_str(&text).ok();
        Ok(AiResponse {
            backend: AiBackend::GeminiPro,
            text,
            structured,
        })
    }

    /// Low-level generate call with exponential backoff on 429.
    async fn generate(
        &self,
        model: &str,
        prompt: &str,
        temperature: f32,
        max_tokens: u32,
    ) -> Result<String> {
        let url = format!(
            "{GEMINI_BASE}/models/{model}:generateContent?key={}",
            self.api_key
        );
        let body = GenerateRequest {
            contents: vec![Content {
                role: "user",
                parts: vec![Part { text: Some(prompt), inline_data: None }],
            }],
            generation_config: GenerationConfig {
                temperature,
                max_output_tokens: max_tokens,
            },
        };

        let mut delay = std::time::Duration::from_millis(500);
        for attempt in 0..4u8 {
            let resp = self.http.post(&url).json(&body).send().await?;
            if resp.status() == reqwest::StatusCode::TOO_MANY_REQUESTS {
                tracing::warn!(attempt, "Gemini 429 — backing off {delay:?}");
                tokio::time::sleep(delay).await;
                delay *= 2;
                continue;
            }
            let parsed: GenerateResponse = resp.error_for_status()?.json().await?;
            let text = parsed
                .candidates
                .into_iter()
                .next()
                .and_then(|c| c.content.parts.into_iter().next())
                .map(|p| p.text)
                .unwrap_or_default();
            return Ok(text);
        }
        anyhow::bail!("Gemini API request failed after 4 attempts (rate limited)")
    }

    /// Public raw generation interface — used by `auto_edit` and other feature modules
    /// that need to send a custom prompt directly to Gemini Flash.
    pub async fn summarise_raw(&self, prompt: &str, max_tokens: u32) -> Result<String> {
        self.generate(models::FLASH, prompt, 0.2, max_tokens).await
    }

    /// Public multimodal generation interface — used by `code_intelligence` to send
    /// image keyframes along with text prompts.
    pub async fn summarise_multimodal(
        &self,
        prompt: &str,
        mime_type: &str,
        base64_data: &str,
        max_tokens: u32,
    ) -> Result<String> {
        let url = format!(
            "{GEMINI_BASE}/models/{}:generateContent?key={}",
            models::PRO,
            self.api_key
        );
        let body = GenerateRequest {
            contents: vec![Content {
                role: "user",
                parts: vec![
                    Part { text: Some(prompt), inline_data: None },
                    Part { text: None, inline_data: Some(InlineData { mime_type, data: base64_data }) },
                ],
            }],
            generation_config: GenerationConfig {
                temperature: 0.2,
                max_output_tokens: max_tokens,
            },
        };

        let mut delay = std::time::Duration::from_millis(500);
        for attempt in 0..4u8 {
            let resp = self.http.post(&url).json(&body).send().await?;
            if resp.status() == reqwest::StatusCode::TOO_MANY_REQUESTS {
                tracing::warn!(attempt, "Gemini 429 — backing off {delay:?}");
                tokio::time::sleep(delay).await;
                delay *= 2;
                continue;
            }
            let parsed: GenerateResponse = resp.error_for_status()?.json().await?;
            let text = parsed
                .candidates
                .into_iter()
                .next()
                .and_then(|c| c.content.parts.into_iter().next())
                .map(|p| p.text)
                .unwrap_or_default();
            return Ok(text);
        }
        anyhow::bail!("Gemini API request failed after 4 attempts (rate limited)")
    }
}
