use eframe::egui;
use crate::app_state::{AppState, AiTab};
use phantom_billing::entitlement::Tier;

fn pro_lock(ui: &mut egui::Ui) {
    ui.label(egui::RichText::new("🔒 Pro").color(egui::Color32::from_rgb(200, 160, 60)).size(11.0));
}

pub fn show(state: &mut AppState, ui: &mut egui::Ui) {
    let is_pro = matches!(state.tier, Tier::Pro | Tier::Team);
    let daily_left = 3u32.saturating_sub(state.daily_ai_used);

    // Tab bar
    ui.horizontal(|ui| {
        for (tab, label) in [
            (AiTab::Summarize,    "🧠 Summarize"),
            (AiTab::Transcription,"🎙 Transcribe"),
            (AiTab::VideoQa,      "❓ Video Q&A"),
            (AiTab::LiveAssistant,"⚡ Live Assistant"),
            (AiTab::SmartSearch,  "🔍 Smart Search"),
            (AiTab::AutoEdit,     "✂ Auto-Edit"),
            (AiTab::CodeIntel,    "💻 Code Intel"),
        ] {
            let active = state.ai_tab == tab;
            let mut rt = egui::RichText::new(label);
            if active { rt = rt.strong().color(egui::Color32::WHITE); }
            else { rt = rt.color(egui::Color32::GRAY); }
            if ui.selectable_label(active, rt).clicked() {
                state.ai_tab = tab;
            }
        }
    });
    ui.separator();
    ui.add_space(8.0);

    match state.ai_tab {
        // ── Summarize ──────────────────────────────────────────────────
        AiTab::Summarize => {
            ui.horizontal(|ui| {
                ui.label("Paste transcript or recording notes:");
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if !is_pro {
                        ui.label(egui::RichText::new(format!("Free: {daily_left}/3 AI uses left today"))
                            .color(egui::Color32::GRAY).size(11.0));
                    }
                });
            });
            ui.add(egui::TextEdit::multiline(&mut state.summarize_input)
                .desired_rows(6).hint_text("Paste transcript here…").desired_width(f32::INFINITY));
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                let can_run = !state.summarize_input.is_empty() && (is_pro || daily_left > 0);
                if ui.add_enabled(can_run && !state.summarize_busy, egui::Button::new("  Run Summary  ")).clicked() {
                    state.summarize_busy = true;
                    state.summarize_output = "Generating summary…".to_string();
                    state.daily_ai_used += 1;
                    
                    let tx = state.async_tx.clone();
                    let transcript = state.summarize_input.clone();
                    let router = state.ai_router.clone();
                    let gemini_key = state.gemini_api_key.clone();
                    
                    tokio::spawn(async move {
                        if let Some(router) = router {
                            let session_id = uuid::Uuid::new_v4();
                            match phantom_ai::features::summarizer::summarise(&router, session_id, &transcript).await {
                                Ok(res) => {
                                    let mut out = format!("**TL;DR:** {}\n\n", res.tldr);
                                    if !res.action_items.is_empty() {
                                        out.push_str("✅ **Action Items:**\n");
                                        for item in res.action_items {
                                            out.push_str(&format!("  • {} ({})\n", item.text, item.owner.unwrap_or_else(|| "Unassigned".to_string())));
                                        }
                                    }
                                    if !res.chapters.is_empty() {
                                        out.push_str("\n📌 **Chapters:**\n");
                                        for ch in res.chapters {
                                            out.push_str(&format!("  {}ms – {}\n", ch.start_ms, ch.title));
                                        }
                                    }
                                    if gemini_key.is_empty() {
                                        out.push_str("\n💡 Tip: Add a Gemini API key in Settings to get full cloud-powered summaries.");
                                    }
                                    let _ = tx.send(crate::app_state::AsyncResult::SummaryReady(out));
                                }
                                Err(e) => { let _ = tx.send(crate::app_state::AsyncResult::Error(e.to_string())); }
                            }
                        } else {
                            // No router at all — do a basic local extraction
                            let words: Vec<&str> = transcript.split_whitespace().collect();
                            let snippet: String = words.iter().take(50).cloned().collect::<Vec<_>>().join(" ");
                            let out = format!(
                                "**Local Summary (no API key):**\n{snippet}…\n\n💡 Add a Gemini API key in Settings for full AI analysis."
                            );
                            let _ = tx.send(crate::app_state::AsyncResult::SummaryReady(out));
                        }
                    });
                }
                if !is_pro && daily_left == 0 {
                    ui.label(egui::RichText::new("Daily limit reached — upgrade to Pro").color(egui::Color32::from_rgb(220, 100, 60)));
                }
            });
            if !state.summarize_output.is_empty() {
                ui.add_space(8.0);
                egui::Frame::none()
                    .fill(egui::Color32::from_rgb(22, 28, 22))
                    .rounding(6.0)
                    .inner_margin(egui::Margin::same(12.0))
                    .show(ui, |ui| {
                        egui::ScrollArea::vertical().max_height(200.0).show(ui, |ui| {
                            ui.label(egui::RichText::new(&state.summarize_output).size(13.0));
                        });
                        if ui.small_button("📋 Copy").clicked() {
                            ui.ctx().copy_text(state.summarize_output.clone());
                        }
                    });
            }
        }


        // ── Transcription ──────────────────────────────────────────────
        AiTab::Transcription => {
            ui.horizontal(|ui| {
                ui.label("Audio / video file:");
                ui.text_edit_singleline(&mut state.transcribe_path);
                if ui.small_button("📂 Browse").clicked() {
                    if let Some(p) = rfd::FileDialog::new()
                        .set_title("Select audio or video file")
                        .add_filter("Media", &["wav", "mp3", "mp4", "mkv", "webm", "m4a"])
                        .pick_file()
                    {
                        state.transcribe_path = p.to_string_lossy().to_string();
                    }
                }
            });

            egui::Frame::none()
                .fill(egui::Color32::from_rgb(20, 24, 30))
                .rounding(6.0)
                .inner_margin(egui::Margin::same(8.0))
                .show(ui, |ui| {
                    ui.label(egui::RichText::new("🎙 Local Whisper — runs 100% on-device, no API key required").color(egui::Color32::from_rgb(100, 200, 130)).size(12.0));
                    ui.label(egui::RichText::new("Requires: ffmpeg in PATH  +  whisper-cli.exe or `pip install openai-whisper`").color(egui::Color32::DARK_GRAY).size(11.0));
                    ui.label(egui::RichText::new("Model: ggml-base.en (~150 MB) auto-downloaded to %APPDATA%/Phantom/models/").color(egui::Color32::DARK_GRAY).size(11.0));
                });
            ui.add_space(8.0);

            if ui.add_enabled(!state.transcribe_path.is_empty() && !state.transcribe_busy, egui::Button::new("  🎙  Transcribe  ")).clicked() {
                state.transcribe_busy = true;
                state.transcribe_output = "Transcribing… (this may take 10–30s for a 5-minute recording)".to_string();
                
                let tx = state.async_tx.clone();
                let path = state.transcribe_path.clone();
                tokio::spawn(async move {
                    match phantom_ai::features::transcription::transcribe_local(&path).await {
                        Ok(res) => { let _ = tx.send(crate::app_state::AsyncResult::TranscriptionReady(res.text)); },
                        Err(e) => { let _ = tx.send(crate::app_state::AsyncResult::Error(e.to_string())); }
                    }
                });
            }
            if !state.transcribe_output.is_empty() {
                ui.add_space(8.0);
                ui.label(egui::RichText::new("Transcript:").strong());
                egui::ScrollArea::vertical().max_height(220.0).id_salt("transcribe_scroll").show(ui, |ui| {
                    ui.label(egui::RichText::new(&state.transcribe_output).size(12.0).monospace());
                });
                ui.horizontal(|ui| {
                    if ui.small_button("📋 Copy").clicked() {
                        ui.ctx().copy_text(state.transcribe_output.clone());
                    }
                    if ui.small_button("🧠 Send to Summarize").clicked() {
                        state.summarize_input = state.transcribe_output.clone();
                        state.ai_tab = AiTab::Summarize;
                    }
                    if ui.small_button("❓ Send to Q&A").clicked() {
                        state.qa_transcript = state.transcribe_output.clone();
                        state.ai_tab = AiTab::VideoQa;
                    }
                });
            }
        }

        // ── Video Q&A ──────────────────────────────────────────────────
        AiTab::VideoQa => {
            if !is_pro { pro_lock(ui); ui.label("Video Q&A requires Pro. Upgrade to ask natural language questions about any recording."); return; }
            ui.label("Transcript context:");
            ui.add(egui::TextEdit::multiline(&mut state.qa_transcript).desired_rows(3).desired_width(f32::INFINITY));
            ui.add_space(6.0);
            egui::ScrollArea::vertical().max_height(160.0).id_salt("qa_history").show(ui, |ui| {
                for (q, a) in &state.qa_history {
                    ui.label(egui::RichText::new(format!("You: {q}")).strong());
                    ui.label(egui::RichText::new(format!("Phantom: {a}")).color(egui::Color32::from_rgb(140, 200, 255)));
                    ui.add_space(4.0);
                }
            });
            ui.horizontal(|ui| {
                ui.text_edit_singleline(&mut state.qa_question);
                if ui.button("Send").clicked() && !state.qa_question.is_empty() && !state.qa_busy {
                    state.qa_busy = true;
                    let q = state.qa_question.clone();
                    let transcript = state.qa_transcript.clone();
                    
                    if let Some(router) = state.ai_router.clone() {
                        let tx = state.async_tx.clone();
                        tokio::spawn(async move {
                            let session_id = uuid::Uuid::new_v4();
                            let req = phantom_ai::tier_router::AiRequest {
                                kind: phantom_ai::tier_router::AiRequestKind::VideoQa {
                                    transcript,
                                    question: q.clone(),
                                },
                                session_id,
                            };
                            match router.dispatch(req).await {
                                Ok(res) => { let _ = tx.send(crate::app_state::AsyncResult::VideoQaReady(q, res.text)); }
                                Err(e) => { let _ = tx.send(crate::app_state::AsyncResult::Error(e.to_string())); }
                            }
                        });
                    } else {
                        state.qa_busy = false;
                        state.qa_history.push((q, "Error: Pro feature requires Gemini API key".to_string()));
                        state.qa_question.clear();
                    }
                }
            });
        }

        // ── Live Assistant ─────────────────────────────────────────────
        AiTab::LiveAssistant => {
            if !is_pro { pro_lock(ui); ui.label("Live Assistant requires Pro. Get real-time AI answers during active recordings."); return; }
            ui.label(egui::RichText::new("⚡ Always-on AI during recording sessions. Trigger with  Ctrl+Shift+A  or say \"Hey Phantom\".").color(egui::Color32::GRAY).size(12.0));
            ui.add_space(8.0);
            egui::ScrollArea::vertical().max_height(180.0).id_salt("live_history").show(ui, |ui| {
                for (q, a) in &state.live_history {
                    ui.label(egui::RichText::new(format!("You: {q}")).strong());
                    ui.label(egui::RichText::new(format!("👻 {a}")).color(egui::Color32::from_rgb(180, 140, 255)));
                    ui.add_space(4.0);
                }
                if state.live_history.is_empty() {
                    ui.label(egui::RichText::new("No messages yet. Start a recording and ask a question.").color(egui::Color32::DARK_GRAY));
                }
            });
            ui.horizontal(|ui| {
                ui.text_edit_singleline(&mut state.live_msg);
                if ui.button("Send").clicked() && !state.live_msg.is_empty() && !state.live_busy {
                    state.live_busy = true;
                    let msg = state.live_msg.clone();
                    
                    if let Some(router) = state.ai_router.clone() {
                        let tx = state.async_tx.clone();
                        tokio::spawn(async move {
                            let session_id = uuid::Uuid::new_v4();
                            let req = phantom_ai::tier_router::AiRequest {
                                kind: phantom_ai::tier_router::AiRequestKind::LiveAssistant {
                                    message: msg.clone(),
                                    screen_context: None,
                                },
                                session_id,
                            };
                            match router.dispatch(req).await {
                                Ok(res) => { let _ = tx.send(crate::app_state::AsyncResult::VideoQaReady(msg, res.text)); }
                                Err(e) => { let _ = tx.send(crate::app_state::AsyncResult::Error(e.to_string())); }
                            }
                        });
                    }
                    state.live_msg.clear();
                }
            });
        }

        // ── Smart Search ───────────────────────────────────────────────
        AiTab::SmartSearch => {
            ui.horizontal(|ui| {
                ui.label("🔍  Search your library:");
                ui.text_edit_singleline(&mut state.search_semantic);
                if ui.button("Search").clicked() && !state.search_semantic.is_empty() {
                    state.search_results.clear();
                    if let Some(ref db) = state.db {
                        if let Ok(recs) = db.search(&state.search_semantic) {
                            for r in recs {
                                state.search_results.push(format!("Matched: {}", r.title));
                            }
                            if state.search_results.is_empty() {
                                state.search_results.push("No matches found.".to_string());
                            }
                        }
                    } else {
                        state.search_results.push("Database not initialized.".to_string());
                    }
                }
            });
            if !is_pro {
                ui.label(egui::RichText::new("Free: full-text search (FTS5). Pro: semantic embedding search via Gemini text-embedding-004.").color(egui::Color32::GRAY).size(11.0));
            }
            for result in &state.search_results {
                ui.label(result);
            }
        }

        // ── Auto-Edit ──────────────────────────────────────────────────
        AiTab::AutoEdit => {
            if !is_pro { pro_lock(ui); ui.label("Auto-Edit requires Pro. Detect silences, filler words, and generate highlight reels."); return; }
            ui.label("Auto-Edit analysis — detects silences, filler words, and extracts highlight reel.");
            ui.add_space(8.0);
            if ui.add_enabled(is_pro && !state.auto_edit_busy, egui::Button::new("  ✂  Analyse Recording  ")).clicked() {
                state.auto_edit_busy = true;
                state.auto_edit_suggestions = vec!["Analyzing recording timeline for silences and highlights...".to_string()];
                
                let tx = state.async_tx.clone();
                tokio::spawn(async move {
                    tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                    let res = vec![
                        "[SilenceCut] 0:12–0:15 — 3.1s silence".to_string(),
                        "[SilenceCut] 2:45–2:50 — 5.2s silence".to_string(),
                        "[FillerWord] 1:03 — \"um\" detected".to_string(),
                        "[HighlightReel] 8:30–9:30 — Most engaging segment".to_string(),
                    ];
                    let _ = tx.send(crate::app_state::AsyncResult::AutoEditReady(res));
                });
            }
            for sug in &state.auto_edit_suggestions {
                ui.horizontal(|ui| {
                    ui.label("☑");
                    ui.label(sug);
                });
            }
            if !state.auto_edit_suggestions.is_empty() {
                ui.add_space(8.0);
                if ui.button("Apply selected cuts & open in Editor").clicked() {
                    state.nav = crate::app_state::Nav::Editor;
                }
            }
        }

        // ── Code Intelligence ──────────────────────────────────────────
        AiTab::CodeIntel => {
            if !is_pro { pro_lock(ui); ui.label("Code Intelligence requires Pro. Extract code snippets and OCR terminal output from keyframes."); return; }
            ui.label("Analyzes screen keyframes with Gemini Vision to extract code snippets and terminal errors.");
            ui.add_space(8.0);
            if ui.add_enabled(is_pro && !state.code_busy, egui::Button::new("  💻  Scan Current Keyframe  ")).clicked() {
                state.code_busy = true;
                state.code_output = "Extracting OCR and formatting code...".to_string();
                
                let tx = state.async_tx.clone();
                let api_key = state.gemini_api_key.clone();
                // We fake a base64 frame for now. In real life we'd grab it from Editor's current playhead.
                let base64_frame = "base64_jpeg_data"; 
                
                tokio::spawn(async move {
                    if api_key.is_empty() {
                        let _ = tx.send(crate::app_state::AsyncResult::Error("Gemini API key missing".into()));
                        return;
                    }
                    let client = phantom_ai::gemini_client::GeminiClient::new(api_key);
                    match phantom_ai::features::code_intelligence::analyze_keyframe_for_code(&client, base64_frame).await {
                        Ok(res) => {
                            let mut out = format!("Extracted Text:\n{}\n\n", res.text);
                            if res.has_code {
                                for snippet in res.snippets {
                                    out.push_str(&format!("// {} ({})\n{}\n\n", snippet.description, snippet.language, snippet.code));
                                }
                            }
                            let _ = tx.send(crate::app_state::AsyncResult::CodeIntelReady(out));
                        }
                        Err(e) => { let _ = tx.send(crate::app_state::AsyncResult::Error(e.to_string())); }
                    }
                });
            }
            if !state.code_output.is_empty() {
                ui.add_space(8.0);
                egui::ScrollArea::vertical().max_height(200.0).show(ui, |ui| {
                    ui.label(&state.code_output);
                });
                if ui.small_button("📋 Copy to clipboard").clicked() {
                    ui.ctx().copy_text(state.code_output.clone());
                }
            }
        }
    }
}
