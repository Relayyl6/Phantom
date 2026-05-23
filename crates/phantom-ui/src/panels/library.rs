use eframe::egui;
use crate::app_state::{AppState, Nav};

pub fn show(state: &mut AppState, ui: &mut egui::Ui) {
    let q = state.search_query.clone();
    let recs: Vec<_> = if q.is_empty() {
        state.recordings.iter().enumerate().collect()
    } else {
        state.recordings.iter().enumerate()
            .filter(|(_, r)| r.title.to_lowercase().contains(&q.to_lowercase()))
            .collect()
    };

    if recs.is_empty() {
        ui.centered_and_justified(|ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(60.0);
                ui.label(egui::RichText::new("🎬").size(48.0));
                ui.add_space(12.0);
                ui.label(egui::RichText::new("No recordings yet").size(20.0).color(egui::Color32::GRAY));
                ui.add_space(8.0);
                ui.label(egui::RichText::new("Click  New Recording  to capture your screen").color(egui::Color32::DARK_GRAY));
                ui.add_space(24.0);
                if ui.button(egui::RichText::new("  ⏺  Start Recording  ").size(16.0)).clicked() {
                    state.nav = Nav::NewRecording;
                }
            });
        });
        return;
    }

    let mut delete_id = None;

    egui::ScrollArea::vertical().show(ui, |ui| {
        for (idx, rec) in recs {
            let selected = state.selected_recording == Some(idx);
            let frame = egui::Frame::none()
                .fill(if selected { egui::Color32::from_rgb(40, 40, 70) } else { egui::Color32::from_rgb(28, 28, 36) })
                .rounding(8.0)
                .inner_margin(egui::Margin::same(12.0));

            frame.show(ui, |ui| {
                ui.horizontal(|ui| {
                    // Thumbnail placeholder
                    let (rect, _) = ui.allocate_exact_size(egui::vec2(80.0, 52.0), egui::Sense::hover());
                    ui.painter().rect_filled(rect, 4.0, egui::Color32::from_rgb(20, 20, 30));
                    ui.painter().text(
                        rect.center(), egui::Align2::CENTER_CENTER,
                        "🎬", egui::FontId::proportional(22.0), egui::Color32::DARK_GRAY,
                    );

                    ui.vertical(|ui| {
                        ui.horizontal(|ui| {
                            ui.label(egui::RichText::new(&rec.title).strong().size(14.0));
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                let mins = rec.duration_secs / 60;
                                let secs = rec.duration_secs % 60;
                                ui.label(egui::RichText::new(format!("{mins}:{secs:02}")).color(egui::Color32::GRAY).size(12.0));
                            });
                        });

                        let date_str = rec.created_at.format("%b %d, %Y  %H:%M").to_string();
                        ui.label(egui::RichText::new(&date_str).color(egui::Color32::DARK_GRAY).size(11.0));

                        if !rec.tags.is_empty() {
                            ui.horizontal(|ui| {
                                for tag in &rec.tags {
                                    ui.label(egui::RichText::new(format!("#{tag}"))
                                        .color(egui::Color32::from_rgb(100, 120, 200)).size(11.0));
                                }
                            });
                        }

                        if let Some(summary) = &rec.summary {
                            let snippet: String = summary.chars().take(100).collect();
                            ui.label(egui::RichText::new(format!("💡 {snippet}…")).color(egui::Color32::GRAY).size(11.0).italics());
                        }

                        if rec.share_token.is_some() {
                            ui.label(egui::RichText::new("🔗 Shared").color(egui::Color32::from_rgb(100, 200, 130)).size(11.0));
                        }

                        ui.add_space(4.0);
                        ui.horizontal(|ui| {
                            if ui.small_button("✂  Edit").clicked() {
                                state.selected_recording = Some(idx);
                                state.nav = Nav::Editor;
                            }
                            if ui.small_button("🔗  Share").clicked() {
                                state.selected_recording = Some(idx);
                                state.nav = Nav::Share;
                            }
                            if ui.small_button("🧠  Summarize").clicked() {
                                state.selected_recording = Some(idx);
                                state.ai_tab = crate::app_state::AiTab::Summarize;
                                if let Some(sum) = &rec.summary {
                                    state.summarize_input = sum.clone();
                                }
                                state.nav = Nav::AiFeatures;
                            }
                            if ui.small_button("🎙  Transcribe").clicked() {
                                state.selected_recording = Some(idx);
                                state.transcribe_path = rec.file_path.clone();
                                state.ai_tab = crate::app_state::AiTab::Transcription;
                                state.nav = Nav::AiFeatures;
                            }
                            if ui.small_button("🗑  Delete").clicked() {
                                delete_id = Some(rec.id);
                            }
                        });
                    });
                });
            });
            ui.add_space(8.0);
        }
    });

    if let Some(id) = delete_id {
        if let Some(ref db) = state.db {
            let _ = db.delete(id);
            if let Ok(new_recs) = db.list_all() {
                state.recordings = new_recs;
            }
        }
    }
}
