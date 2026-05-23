use eframe::egui;
use crate::app_state::AppState;
use phantom_billing::entitlement::Tier;

pub fn show(state: &mut AppState, ui: &mut egui::Ui) {
    let is_pro = matches!(state.tier, Tier::Pro | Tier::Team);

    ui.heading("🔗  Share Recording");
    ui.separator();
    ui.add_space(8.0);

    // Show currently selected recording
    let (rec_name, rec_path, rec_id) = if let Some(i) = state.selected_recording {
        if let Some(rec) = state.recordings.get(i) {
            (rec.title.clone(), rec.file_path.clone(), Some(rec.id))
        } else {
            ("(no recording selected)".to_string(), String::new(), None)
        }
    } else {
        ("(no recording selected)".to_string(), String::new(), None)
    };

    ui.horizontal(|ui| {
        ui.label("Recording:");
        ui.label(egui::RichText::new(&rec_name).strong());
        if rec_id.is_none() {
            ui.label(egui::RichText::new("← Select a recording from Library first").color(egui::Color32::from_rgb(220, 180, 60)).size(11.0));
        }
    });
    ui.add_space(12.0);

    if rec_id.is_none() {
        if ui.button("  📚  Go to Library  ").clicked() {
            state.nav = crate::app_state::Nav::Library;
        }
        return;
    }

    egui::Grid::new("share_grid").num_columns(2).spacing([12.0, 8.0]).show(ui, |ui| {
        // Allow download — Pro only
        ui.label("Allow download:");
        ui.horizontal(|ui| {
            if is_pro {
                ui.checkbox(&mut state.share_allow_download, "");
            } else {
                ui.add_enabled(false, egui::Checkbox::new(&mut state.share_allow_download, ""));
                pro_badge(ui);
            }
        });
        ui.end_row();

        // Password — Pro only
        ui.label("Password protect:");
        ui.horizontal(|ui| {
            if is_pro {
                ui.checkbox(&mut state.share_use_password, "");
                if state.share_use_password {
                    ui.add(egui::TextEdit::singleline(&mut state.share_password).password(true).desired_width(120.0));
                }
            } else {
                pro_badge(ui);
            }
        });
        ui.end_row();

        // Expiry — Pro only
        ui.label("Expires in:");
        ui.horizontal(|ui| {
            if is_pro {
                ui.add(egui::DragValue::new(&mut state.share_expiry_days).range(1..=365).suffix(" days"));
            } else {
                ui.label(egui::RichText::new("Never (Free tier — view-only)").color(egui::Color32::GRAY).size(11.0));
            }
        });
        ui.end_row();
    });

    ui.add_space(12.0);

    ui.horizontal(|ui| {
        if ui.button("  🔗  Generate Share Link  ").clicked() && !rec_path.is_empty() {
            let token = uuid::Uuid::new_v4().to_string().replace('-', "").chars().take(12).collect::<String>();

            // Determine absolute path to web server recordings folder
            let exe_dir = std::env::current_exe()
                .ok()
                .and_then(|p| p.parent().map(|p| p.to_path_buf()))
                .unwrap_or_else(|| std::path::PathBuf::from("."));

            // Try multiple possible locations for the web server public folder
            let web_recordings = [
                exe_dir.join("crates").join("web").join("public").join("recordings"),
                std::path::PathBuf::from("crates/web/public/recordings"),
                std::path::PathBuf::from("web/public/recordings"),
            ];

            let mut copied = false;
            for dest_dir in &web_recordings {
                let _ = std::fs::create_dir_all(dest_dir);
                let dest = dest_dir.join(format!("{token}.mp4"));
                if std::fs::copy(&rec_path, &dest).is_ok() {
                    tracing::info!("Copied recording to {:?}", dest);
                    copied = true;
                    break;
                }
            }

            // Update share token in DB
            if let (Some(db), Some(id)) = (&state.db, rec_id) {
                let expires = if is_pro {
                    Some(chrono::Utc::now() + chrono::Duration::days(state.share_expiry_days as i64))
                } else {
                    None
                };
                // Persist key to env for this session (safe single-threaded UI context)
                #[allow(unused_unsafe)]
                unsafe { std::env::set_var("GEMINI_API_KEY", &state.gemini_api_key); }
                let _ = db.set_share(id, &token, state.share_allow_download && is_pro, expires);
                if let Ok(recs) = db.list_all() {
                    state.recordings = recs;
                }
            }

            state.share_url = format!("http://localhost:3000/recordings/{token}.mp4");
            state.status_msg = if copied {
                "Share link generated and file copied to web server!".to_string()
            } else {
                format!("Share link generated (file at: {})", rec_path)
            };
        }

        ui.add_space(12.0);

        if ui.button(egui::RichText::new("  🗑  Delete Recording  ").color(egui::Color32::from_rgb(255, 80, 80))).clicked() {
            if let (Some(db), Some(id)) = (&state.db, rec_id) {
                let _ = db.delete(id);
                if let Ok(new_recs) = db.list_all() {
                    state.recordings = new_recs;
                }
                state.selected_recording = None;
                state.share_url.clear();
                state.nav = crate::app_state::Nav::Library;
                state.status_msg = "Recording deleted.".to_string();
            }
        }
    });

    if !state.share_url.is_empty() {
        ui.add_space(8.0);
        egui::Frame::none()
            .fill(egui::Color32::from_rgb(15, 30, 20))
            .rounding(6.0)
            .inner_margin(egui::Margin::same(10.0))
            .show(ui, |ui| {
                ui.label(egui::RichText::new("Share Link:").strong());
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new(&state.share_url).monospace().color(egui::Color32::from_rgb(100, 220, 130)));
                    if ui.small_button("📋 Copy").clicked() {
                        ui.ctx().copy_text(state.share_url.clone());
                        state.status_msg = "Link copied to clipboard!".to_string();
                    }
                    if state.share_allow_download && is_pro {
                        if ui.small_button("⬇ Open in Browser").clicked() {
                            let _ = std::process::Command::new("cmd")
                                .args(["/c", "start", "", &state.share_url])
                                .spawn();
                        }
                    }
                });
                let flags: Vec<&str> = [
                    if state.share_allow_download && is_pro { Some("📥 Download enabled") } else { None },
                    if state.share_use_password && is_pro { Some("🔑 Password protected") } else { None },
                    if is_pro { Some("⏰ Custom expiry") } else { Some("⚡ Free: view-only") },
                ].iter().flatten().copied().collect();
                for f in flags { ui.label(egui::RichText::new(f).color(egui::Color32::GRAY).size(11.0)); }
            });
    }

    if !is_pro {
        ui.add_space(12.0);
        egui::Frame::none()
            .fill(egui::Color32::from_rgb(40, 32, 10))
            .rounding(6.0)
            .inner_margin(egui::Margin::same(10.0))
            .show(ui, |ui| {
                ui.label(egui::RichText::new("🔒 Upgrade to Pro to unlock:").strong().color(egui::Color32::from_rgb(230, 180, 60)));
                ui.label("  • Download-enabled share links");
                ui.label("  • Password protection");
                ui.label("  • Configurable expiry (1–365 days)");
            });
    }
}

fn pro_badge(ui: &mut egui::Ui) {
    ui.label(egui::RichText::new("🔒 Pro").color(egui::Color32::from_rgb(200, 160, 60)).size(11.0));
}

