use eframe::egui;
use crate::app_state::{AppState, Nav};
use phantom_billing::entitlement::Tier;

pub struct DashboardApp {
    state: AppState,
}

impl Default for DashboardApp {
    fn default() -> Self {
        let mut state = AppState::default();
        
        // Use a persistent DB in AppData so recordings survive app restarts
        let db_dir = dirs::data_dir()
            .unwrap_or_else(|| std::env::temp_dir())
            .join("Phantom");
        let _ = std::fs::create_dir_all(&db_dir);
        let db_path = db_dir.join("recordings.db");
        
        if let Ok(db) = phantom_storage::RecordingDb::open(db_path) {
            if let Ok(recs) = db.list_all() {
                state.recordings = recs;
            }
            state.db = Some(db);
        }
        
        let ent_path = dirs::data_dir()
            .unwrap_or_else(|| std::env::temp_dir())
            .join("Phantom")
            .join("entitlements.db");
        
        if let Ok(ent) = phantom_billing::entitlement::EntitlementStore::open(ent_path) {
            // Always create a router — it can work with an empty key (will fail gracefully on API calls)
            let api_key = state.gemini_api_key.clone();
            let router = phantom_ai::AiTierRouter::new(ent.clone(), api_key);
            state.ai_router = Some(std::sync::Arc::new(router));
            state.entitlement = Some(ent);
        }
        
        Self { state }
    }
}

impl eframe::App for DashboardApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let s = &mut self.state;

        // Process async task results
        while let Ok(msg) = s.async_rx.try_recv() {
            match msg {
                crate::app_state::AsyncResult::TranscriptionReady(text) => {
                    s.transcribe_busy = false;
                    s.transcribe_output = text;
                    s.status_msg = "Transcription completed.".to_string();
                }
                crate::app_state::AsyncResult::SummaryReady(text) => {
                    s.summarize_busy = false;
                    s.summarize_output = text;
                    s.status_msg = "Summary generated.".to_string();
                }
                crate::app_state::AsyncResult::VideoQaReady(_q, a) => {
                    s.qa_busy = false;
                    s.qa_history.push((s.qa_question.clone(), a));
                    s.qa_question.clear();
                }
                crate::app_state::AsyncResult::AutoEditReady(suggestions) => {
                    s.auto_edit_busy = false;
                    s.auto_edit_suggestions = suggestions;
                }
                crate::app_state::AsyncResult::CodeIntelReady(code) => {
                    s.code_busy = false;
                    s.code_output = code;
                }
                crate::app_state::AsyncResult::Error(err) => {
                    s.status_msg = format!("Error: {err}");
                    let err_msg = format!("Failed: {err}");
                    if s.transcribe_busy { s.transcribe_output = err_msg.clone(); }
                    if s.summarize_busy { s.summarize_output = err_msg.clone(); }
                    if s.qa_busy { s.qa_history.push((s.qa_question.clone(), err_msg.clone())); }
                    if s.code_busy { s.code_output = err_msg.clone(); }
                    
                    s.transcribe_busy = false;
                    s.summarize_busy = false;
                    s.qa_busy = false;
                    s.auto_edit_busy = false;
                    s.code_busy = false;
                }
            }
        }

        // ── Top bar ──────────────────────────────────────────────────────
        egui::TopBottomPanel::top("top_bar").min_height(44.0).show(ctx, |ui| {
            ui.horizontal_centered(|ui| {
                // Logo
                ui.label(egui::RichText::new("👻  Phantom").size(18.0).strong());
                ui.separator();

                // Nav tabs
                for (nav, label) in [
                    (Nav::Library,      "📚 Library"),
                    (Nav::NewRecording, "⏺ Record"),
                    (Nav::AiFeatures,   "🧠 AI"),
                    (Nav::Share,        "🔗 Share"),
                    (Nav::Settings,     "⚙ Settings"),
                ] {
                    let active = s.nav == nav;
                    let rt = if active {
                        egui::RichText::new(label).strong()
                    } else {
                        egui::RichText::new(label).color(egui::Color32::GRAY)
                    };
                    if ui.selectable_label(active, rt).clicked() { s.nav = nav; }
                }

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    // Tier badge
                    let (tier_txt, tier_col) = match s.tier {
                        Tier::Free => ("FREE",  egui::Color32::GRAY),
                        Tier::Pro  => ("PRO",   egui::Color32::from_rgb(120, 100, 255)),
                        Tier::Team => ("TEAM",  egui::Color32::from_rgb(220, 180, 60)),
                    };
                    ui.label(egui::RichText::new(tier_txt).strong().color(tier_col).size(11.0));

                    // Recording indicator
                    if s.is_recording {
                        let secs = s.rec_start_time.map(|start| start.elapsed().as_secs()).unwrap_or(0);
                        ui.label(egui::RichText::new(format!("● REC  {}:{:02}", secs/60, secs%60))
                            .color(egui::Color32::RED).size(12.0));
                    }

                    // Search
                    if matches!(s.nav, Nav::Library) {
                        ui.text_edit_singleline(&mut s.search_query);
                        ui.label("🔍");
                    }

                    // Daily AI counter
                    if matches!(s.tier, Tier::Free) {
                        let left = 3u32.saturating_sub(s.daily_ai_used);
                        ui.label(egui::RichText::new(format!("AI {left}/3")).color(
                            if left == 0 { egui::Color32::RED } else { egui::Color32::GRAY }
                        ).size(11.0));
                    }
                });
            });
        });

        // ── Status bar ───────────────────────────────────────────────────
        egui::TopBottomPanel::bottom("status_bar").min_height(22.0).show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(&s.status_msg).color(egui::Color32::GRAY).size(11.0));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(egui::RichText::new("Phantom v0.1.0  |  DXGI · WASAPI · Whisper · ffmpeg 8.1").color(egui::Color32::DARK_GRAY).size(10.0));
                });
            });
        });

        // ── Main Content Area ─────────────────────────────────────────────
        if matches!(s.nav, Nav::Editor) {
            // Check if we need to initialize the editor for the selected recording
            if s.editor_app.is_none() {
                if let Some(idx) = s.selected_recording {
                    if let Some(rec) = s.recordings.get(idx) {
                        let path = std::path::PathBuf::from(&rec.file_path);
                        s.editor_app = Some(crate::EditorApp::new(path, rec.duration_secs * 1000));
                    }
                }
            }
            
            // If we have an editor, let it take over the screen.
            // We pass a dummy frame to the editor if we want, or we can just call update.
            if let Some(editor) = &mut s.editor_app {
                // To let the user go back, we draw a small overlay or they use the top bar
                // (The top bar is already drawn by DashboardApp, which is fine)
                editor.update(ctx, _frame);
            } else {
                egui::CentralPanel::default().show(ctx, |ui| {
                    ui.centered_and_justified(|ui| {
                        ui.label("No recording selected.");
                    });
                });
            }
        } else {
            // Drop editor instance to free memory/ffmpeg when not using it
            s.editor_app = None;

            egui::CentralPanel::default().show(ctx, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| {
                    match s.nav.clone() {
                        Nav::Library      => crate::panels::library::show(s, ui),
                        Nav::NewRecording => crate::panels::recording::show(s, ui),
                        Nav::AiFeatures   => crate::panels::ai_features::show(s, ui),
                        Nav::Share        => crate::panels::share::show(s, ui),
                        Nav::Settings     => crate::panels::settings::show(s, ui),
                        _ => {}
                    }
                });
            });
        }

        // Repaint frequently while recording (for live timer + preview)
        if s.is_recording {
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        }
    }
}
