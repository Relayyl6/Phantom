use eframe::egui;
use std::sync::mpsc;
use phantom_billing::entitlement::{Tier, EntitlementStore};
use phantom_storage::RecordingDb;
use phantom_ai::AiTierRouter;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Nav { Library, NewRecording, AiFeatures, Editor, Share, Settings }

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AiTab { Summarize, Transcription, VideoQa, LiveAssistant, SmartSearch, AutoEdit, CodeIntel }

pub enum AsyncResult {
    TranscriptionReady(String),
    SummaryReady(String),
    VideoQaReady(String, String), // (Question, Answer)
    CodeIntelReady(String),
    AutoEditReady(Vec<String>),
    Error(String),
}

pub struct AppState {
    pub nav: Nav,
    pub tier: Tier,
    pub daily_ai_used: u32,

    pub db: Option<RecordingDb>,
    pub entitlement: Option<EntitlementStore>,
    pub ai_router: Option<std::sync::Arc<AiTierRouter>>,

    // Async tasks
    pub async_tx: mpsc::Sender<AsyncResult>,
    pub async_rx: mpsc::Receiver<AsyncResult>,

    // Library
    pub search_query: String,
    pub recordings: Vec<phantom_storage::db::Recording>,
    pub selected_recording: Option<usize>,

    // New Recording
    pub rec_title: String,
    pub rec_audio_system: bool,
    pub rec_audio_mic: bool,
    pub rec_duration_limit: Option<u64>,
    pub is_recording: bool,
    pub rec_start_time: Option<std::time::Instant>,
    pub stop_recording_tx: Option<tokio::sync::oneshot::Sender<()>>,
    pub preview_rx: Option<mpsc::Receiver<egui::ColorImage>>,
    pub preview_texture: Option<egui::TextureHandle>,

    // AI
    pub ai_tab: AiTab,
    pub summarize_input: String,
    pub summarize_output: String,
    pub summarize_busy: bool,
    pub qa_transcript: String,
    pub qa_question: String,
    pub qa_history: Vec<(String, String)>,
    pub qa_busy: bool,
    pub live_msg: String,
    pub live_history: Vec<(String, String)>,
    pub live_busy: bool,
    pub search_semantic: String,
    pub search_results: Vec<String>,
    pub search_busy: bool,
    pub transcribe_path: String,
    pub transcribe_output: String,
    pub transcribe_busy: bool,
    pub code_output: String,
    pub code_busy: bool,
    pub auto_edit_suggestions: Vec<String>,
    pub auto_edit_busy: bool,

    // Share
    pub share_url: String,
    pub share_expiry_days: u32,
    pub share_allow_download: bool,
    pub share_password: String,
    pub share_use_password: bool,

    // Settings
    pub gemini_api_key: String,
    pub supabase_url: String,
    pub supabase_anon_key: String,
    pub show_api_key: bool,

    pub status_msg: String,
    
    // Editor embedding
    pub editor_app: Option<crate::EditorApp>,
}

impl Default for AppState {
    fn default() -> Self {
        let (async_tx, async_rx) = mpsc::channel();
        Self {
            nav: Nav::Library, tier: Tier::Free, daily_ai_used: 0,
            db: None, entitlement: None, ai_router: None,
            async_tx, async_rx,
            search_query: String::new(), recordings: vec![], selected_recording: None,
            rec_title: "My Recording".to_string(),
            rec_audio_system: true, rec_audio_mic: true,
            rec_duration_limit: None, is_recording: false,
            rec_start_time: None, stop_recording_tx: None,
            preview_rx: None, preview_texture: None,
            ai_tab: AiTab::Summarize,
            summarize_input: String::new(), summarize_output: String::new(), summarize_busy: false,
            qa_transcript: String::new(), qa_question: String::new(), qa_history: vec![], qa_busy: false,
            live_msg: String::new(), live_history: vec![], live_busy: false,
            search_semantic: String::new(), search_results: vec![], search_busy: false,
            transcribe_path: String::new(), transcribe_output: String::new(), transcribe_busy: false,
            code_output: String::new(), code_busy: false,
            auto_edit_suggestions: vec![], auto_edit_busy: false,
            share_url: String::new(), share_expiry_days: 7,
            share_allow_download: false, share_password: String::new(), share_use_password: false,
            gemini_api_key: std::env::var("GEMINI_API_KEY").unwrap_or_default(),
            supabase_url: std::env::var("SUPABASE_URL").unwrap_or_default(),
            supabase_anon_key: std::env::var("SUPABASE_ANON_KEY").unwrap_or_default(),
            show_api_key: false,
            status_msg: "Initializing...".to_string(),
            editor_app: None,
        }
    }
}
