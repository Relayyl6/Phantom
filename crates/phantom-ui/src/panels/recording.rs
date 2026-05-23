use eframe::egui;
use crate::app_state::AppState;

pub fn show(state: &mut AppState, ui: &mut egui::Ui) {
    ui.vertical(|ui| {
        ui.add_space(8.0);
        ui.heading("⏺  New Recording");
        ui.separator();
        ui.add_space(12.0);

        egui::Grid::new("rec_config").num_columns(2).spacing([16.0, 10.0]).show(ui, |ui| {
            ui.label("Title:");
            ui.text_edit_singleline(&mut state.rec_title);
            ui.end_row();

            ui.label("System audio:");
            ui.checkbox(&mut state.rec_audio_system, "Capture speaker output (WASAPI loopback)");
            ui.end_row();

            ui.label("Microphone:");
            ui.checkbox(&mut state.rec_audio_mic, "Capture microphone input");
            ui.end_row();

            ui.label("Duration limit:");
            ui.horizontal(|ui| {
                let mut limited = state.rec_duration_limit.is_some();
                if ui.checkbox(&mut limited, "").clicked() {
                    state.rec_duration_limit = if limited { Some(300) } else { None };
                }
                if let Some(ref mut secs) = state.rec_duration_limit {
                    ui.add(egui::DragValue::new(secs).range(30..=3600).suffix("s"));
                } else {
                    ui.label(egui::RichText::new("Unlimited").color(egui::Color32::GRAY));
                }
            });
            ui.end_row();
        });

        ui.add_space(24.0);

        ui.vertical_centered(|ui| {
            if state.is_recording {
                let elapsed = if let Some(start) = state.rec_start_time {
                    start.elapsed().as_secs()
                } else {
                    0
                };
                let m = elapsed / 60; let s = elapsed % 60;
                ui.label(egui::RichText::new(format!("● REC  {m}:{s:02}"))
                    .size(20.0).color(egui::Color32::RED).strong());
                ui.add_space(12.0);
                
                // Show live preview
                if let Some(tex) = &state.preview_texture {
                    let avail = ui.available_width();
                    let preview_w = avail.min(720.0);
                    let preview_h = preview_w * 9.0 / 16.0; // 16:9 aspect
                    ui.add(
                        egui::Image::new(tex)
                            .max_width(preview_w)
                            .max_height(preview_h)
                            .fit_to_exact_size(egui::vec2(preview_w, preview_h))
                    );
                    ui.add_space(4.0);
                    ui.label(
                        egui::RichText::new("🔴 LIVE — screen capture active")
                            .color(egui::Color32::from_rgb(255, 80, 80))
                            .size(11.0)
                    );
                    ui.add_space(6.0);
                } else if state.preview_rx.is_some() {
                    // Waiting for first frame
                    let (rect, _) = ui.allocate_exact_size(egui::vec2(480.0, 270.0), egui::Sense::hover());
                    ui.painter().rect_filled(rect, 8.0, egui::Color32::from_rgb(10, 12, 18));
                    ui.painter().text(
                        rect.center(), egui::Align2::CENTER_CENTER,
                        "📡 Initialising capture…",
                        egui::FontId::proportional(16.0),
                        egui::Color32::GRAY,
                    );
                    ui.add_space(6.0);
                }

                if ui.button(egui::RichText::new("  ⏹  Stop Recording  ").size(16.0)).clicked() {
                    state.is_recording = false;
                    if let Some(tx) = state.stop_recording_tx.take() {
                        let _ = tx.send(());
                    }
                    state.status_msg = "Recording stopped. File saved.".to_string();
                    
                    // We can reload the library here to show the new file
                    if let Some(ref db) = state.db {
                        if let Ok(recs) = db.list_all() {
                            state.recordings = recs;
                        }
                    }
                }
            } else {
                if ui.button(egui::RichText::new("  ⏺  Start Recording  ").size(18.0)).clicked() {
                    state.is_recording = true;
                    state.rec_start_time = Some(std::time::Instant::now());
                    
                    let _ = std::fs::create_dir_all("recordings");
                    let out_path = format!("recordings/{}.mp4", uuid::Uuid::new_v4());
                    let title = state.rec_title.clone();
                    
                    let (tx, rx) = tokio::sync::oneshot::channel();
                    state.stop_recording_tx = Some(tx);
                    
                    let (preview_tx, preview_rx) = std::sync::mpsc::channel();
                    state.preview_rx = Some(preview_rx);
                    state.preview_texture = None;
                    
                    crate::app_recorder::spawn_recorder(
                        crate::app_recorder::RecordingTask {
                            duration_limit: state.rec_duration_limit,
                            output_path: out_path.clone(),
                            title: title.clone(),
                            preview_tx: Some(preview_tx),
                        },
                        rx,
                    );
                    
                    // Pre-insert a placeholder record in the DB so it shows up
                    if let Some(ref db) = state.db {
                        let rec = phantom_storage::db::Recording {
                            id: uuid::Uuid::new_v4(),
                            title: title.clone(),
                            file_path: out_path,
                            thumb_path: None,
                            duration_secs: 0,
                            created_at: chrono::Utc::now(),
                            tags: vec![],
                            share_token: None,
                            share_password_hash: None,
                            share_download_enabled: false,
                            share_expires_at: None,
                            summary: None,
                        };
                        let _ = db.insert(&rec);
                        if let Ok(recs) = db.list_all() {
                            state.recordings = recs;
                        }
                    }
                    
                    state.status_msg = format!("Recording \"{}\"…", title);
                }
            }
        });

        ui.add_space(24.0);
        ui.separator();
        ui.add_space(8.0);

        ui.label(egui::RichText::new("Capture engine").strong());
        ui.add_space(4.0);
        egui::Frame::none()
            .fill(egui::Color32::from_rgb(22, 22, 32))
            .rounding(6.0)
            .inner_margin(egui::Margin::same(10.0))
            .show(ui, |ui| {
                ui.label("🖥  Screen: DXGI zero-copy capture (Windows)  |  60fps, GPU-direct");
                ui.label("🎙  Audio:  WASAPI loopback + microphone  |  48kHz stereo f32");
                ui.label("🎬  Encoder:  ffmpeg H.264/AAC  |  ultrafast preset  |  CRF 18");
            });

        if state.is_recording {
            // Read preview frames if they arrive
            if let Some(rx) = &state.preview_rx {
                while let Ok(img) = rx.try_recv() {
                    if let Some(tex) = &mut state.preview_texture {
                        tex.set(img, egui::TextureOptions::LINEAR);
                    } else {
                        state.preview_texture = Some(ui.ctx().load_texture(
                            "preview",
                            img,
                            egui::TextureOptions::LINEAR,
                        ));
                    }
                }
            }
            // Request repaint frequently to keep the timer and preview updating smoothly
            ui.ctx().request_repaint();
        }
    });
}
