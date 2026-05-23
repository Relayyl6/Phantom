//! phantom-ai — Gemini client, local Whisper, Nano/Pro tier routing.

pub mod features;
pub mod gemini_client;
pub mod nano_client;
pub mod tier_router;

pub use tier_router::{AiTierRouter, AiRequest, AiResponse};
