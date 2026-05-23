//! Code Intelligence — Phase 4 feature.
//!
//! Uses Gemini Pro Vision to analyze screen context (keyframes) and extract
//! code snippets, detect programming languages, and OCR terminal errors.
//! This allows developers to easily copy-paste code from a recorded video.

use anyhow::Result;
use serde::{Deserialize, Serialize};
use crate::gemini_client::GeminiClient;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeSnippet {
    pub language: String,
    pub code: String,
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OcrResult {
    pub text: String,
    pub has_code: bool,
    pub snippets: Vec<CodeSnippet>,
}

/// Analyze a keyframe (provided as base64 JPEG) to extract code snippets and text.
pub async fn analyze_keyframe_for_code(
    gemini_client: &GeminiClient,
    base64_image: &str,
) -> Result<OcrResult> {
    let prompt = "Analyze this screen recording keyframe. Extract any visible text. \
                  If there is source code or terminal output visible, extract it exactly as written, \
                  identify the programming language, and provide a brief description.\n\
                  Respond ONLY with a JSON object in this format:\n\
                  {\"text\": \"...\", \"has_code\": true/false, \"snippets\": [{\"language\": \"...\", \"code\": \"...\", \"description\": \"...\"}]}";

    let response_text = gemini_client
        .summarise_multimodal(prompt, "image/jpeg", base64_image, 1024)
        .await?;

    let json_str = response_text.trim().trim_start_matches("```json").trim_end_matches("```").trim();
    
    let result: OcrResult = serde_json::from_str(json_str).unwrap_or(OcrResult {
        text: "Could not parse AI response.".to_string(),
        has_code: false,
        snippets: vec![],
    });

    Ok(result)
}
