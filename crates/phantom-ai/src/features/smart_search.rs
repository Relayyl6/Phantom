//! Smart Search — hybrid local FTS5 + semantic embedding search.
//!
//! Free tier:  SQLite FTS5 full-text search (fast, local, no API calls).
//! Pro tier:   Gemini `text-embedding-004` semantic search across all recordings.
//!
//! On the Pro path:
//!   1. Query is embedded via Gemini Embeddings API.
//!   2. Stored embeddings are loaded from the DB and cosine-ranked.
//!   3. FTS5 results are merged (reciprocal rank fusion) for a hybrid result.

use anyhow::Result;
use serde::{Deserialize, Serialize};
use reqwest::Client;

const EMBEDDING_MODEL: &str = "text-embedding-004";
#[allow(dead_code)]
const EMBEDDING_DIM: usize = 768;

/// A single search result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    pub recording_id: uuid::Uuid,
    pub title: String,
    pub snippet: String,
    /// Combined ranking score (higher is better).
    pub score: f32,
    /// Estimated timestamp in the recording where the match occurs (ms).
    pub timestamp_ms: Option<u64>,
}

// ─── Embedding helpers ─────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
struct EmbedRequest<'a> {
    model: &'a str,
    content: EmbedContent<'a>,
    #[serde(rename = "taskType")]
    task_type: &'a str,
}

#[derive(Debug, Serialize)]
struct EmbedContent<'a> {
    parts: Vec<EmbedPart<'a>>,
}

#[derive(Debug, Serialize)]
struct EmbedPart<'a> {
    text: &'a str,
}

#[derive(Debug, Deserialize)]
struct EmbedResponse {
    embedding: EmbedValues,
}

#[derive(Debug, Deserialize)]
struct EmbedValues {
    values: Vec<f32>,
}

/// Fetch an embedding vector for `text` using the Gemini Embeddings API.
pub async fn embed_text(api_key: &str, text: &str) -> Result<Vec<f32>> {
    let http = Client::new();
    let url = format!(
        "https://generativelanguage.googleapis.com/v1beta/models/{EMBEDDING_MODEL}:embedContent?key={api_key}"
    );
    let body = EmbedRequest {
        model: &format!("models/{EMBEDDING_MODEL}"),
        content: EmbedContent {
            parts: vec![EmbedPart { text }],
        },
        task_type: "RETRIEVAL_QUERY",
    };
    let resp: EmbedResponse = http.post(&url).json(&body).send().await?.error_for_status()?.json().await?;
    Ok(resp.embedding.values)
}

/// Cosine similarity between two equal-length vectors.
pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }
    let dot: f32 = a.iter().zip(b).map(|(x, y)| x * y).sum();
    let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm_a == 0.0 || norm_b == 0.0 {
        0.0
    } else {
        dot / (norm_a * norm_b)
    }
}

// ─── Stored embedding ──────────────────────────────────────────────────────

/// A stored embedding for a recording (persisted in the DB alongside the recording).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecordingEmbedding {
    pub recording_id: uuid::Uuid,
    pub title: String,
    pub snippet: String,
    /// Serialised as JSON blob in the DB.
    pub vector: Vec<f32>,
}

impl RecordingEmbedding {
    /// Build a RecordingEmbedding by embedding the recording title + summary.
    pub async fn build(
        api_key: &str,
        recording_id: uuid::Uuid,
        title: &str,
        transcript_snippet: &str,
    ) -> Result<Self> {
        let text = format!("Title: {title}\n\nTranscript: {transcript_snippet}");
        let vector = embed_text(api_key, &text).await?;
        Ok(Self {
            recording_id,
            title: title.to_owned(),
            snippet: transcript_snippet.chars().take(200).collect(),
            vector,
        })
    }
}

// ─── Semantic searcher ─────────────────────────────────────────────────────

/// In-memory semantic search index loaded from stored embeddings.
pub struct SemanticSearchIndex {
    embeddings: Vec<RecordingEmbedding>,
}

impl SemanticSearchIndex {
    /// Load from a collection of stored embeddings.
    pub fn new(embeddings: Vec<RecordingEmbedding>) -> Self {
        Self { embeddings }
    }

    /// Rank recordings by cosine similarity to `query_vec`.
    /// Returns up to `top_k` results.
    pub fn search(&self, query_vec: &[f32], top_k: usize) -> Vec<SearchResult> {
        let mut scored: Vec<(f32, &RecordingEmbedding)> = self
            .embeddings
            .iter()
            .map(|e| (cosine_similarity(query_vec, &e.vector), e))
            .collect();
        scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
        scored
            .into_iter()
            .take(top_k)
            .filter(|(score, _)| *score > 0.1) // minimum relevance threshold
            .map(|(score, e)| SearchResult {
                recording_id: e.recording_id,
                title: e.title.clone(),
                snippet: e.snippet.clone(),
                score,
                timestamp_ms: None,
            })
            .collect()
    }
}

// ─── Hybrid search ─────────────────────────────────────────────────────────

/// Merge FTS results and semantic results using Reciprocal Rank Fusion.
/// `fts_results` and `semantic_results` are already ordered (best first).
pub fn reciprocal_rank_fusion(
    fts_results: &[SearchResult],
    semantic_results: &[SearchResult],
    top_k: usize,
) -> Vec<SearchResult> {
    use std::collections::HashMap;

    const K: f32 = 60.0;
    let mut scores: HashMap<uuid::Uuid, f32> = HashMap::new();
    let mut meta: HashMap<uuid::Uuid, SearchResult> = HashMap::new();

    for (rank, r) in fts_results.iter().enumerate() {
        *scores.entry(r.recording_id).or_default() += 1.0 / (K + rank as f32 + 1.0);
        meta.entry(r.recording_id).or_insert_with(|| r.clone());
    }
    for (rank, r) in semantic_results.iter().enumerate() {
        *scores.entry(r.recording_id).or_default() += 1.0 / (K + rank as f32 + 1.0);
        meta.entry(r.recording_id).or_insert_with(|| r.clone());
    }

    let mut results: Vec<SearchResult> = scores
        .into_iter()
        .filter_map(|(id, score)| {
            meta.get(&id).map(|r| SearchResult {
                score,
                ..r.clone()
            })
        })
        .collect();
    results.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
    results.truncate(top_k);
    results
}
