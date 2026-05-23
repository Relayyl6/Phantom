//! Floating always-on-top overlay toolbar.
//!
//! Rendered via egui + eframe (wgpu backend).
//! Dimensions: 300×64px collapsed, 300×200px expanded on hover.
//! Hotkey: Ctrl+Shift+R to show/hide.

use eframe::egui;

#[derive(Default)]
pub struct OverlayState {
    pub recording: bool,
    pub elapsed_secs: u64,
    pub mic_level: f32,     // 0.0–1.0
    pub ai_thinking: bool,
    pub expanded: bool,
}

pub struct OverlayApp {
    pub state: OverlayState,
    pub bus: Option<phantom_core::bus::Bus>,
}

impl OverlayApp {
    pub fn new(bus: Option<phantom_core::bus::Bus>) -> Self {
        Self {
            state: OverlayState::default(),
            bus,
        }
    }
}

impl eframe::App for OverlayApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Transparent background — only the panel is visible.
        let frame = egui::Frame::none()
            .fill(egui::Color32::from_rgba_unmultiplied(18, 18, 24, 220))
            .rounding(egui::Rounding::same(12.0))
            .inner_margin(egui::Margin::symmetric(12.0, 8.0));

        egui::CentralPanel::default().frame(frame).show(ctx, |ui| {
            ui.horizontal(|ui| {
                // ── Record / Stop button ──────────────────────────────────
                let rec_label = if self.state.recording { "⏹  Stop" } else { "⏺  Record" };
                let rec_color = if self.state.recording {
                    egui::Color32::from_rgb(220, 60, 60)
                } else {
                    egui::Color32::from_rgb(80, 200, 120)
                };

                if ui.add(
                    egui::Button::new(egui::RichText::new(rec_label).color(rec_color).size(14.0))
                        .fill(egui::Color32::from_rgba_unmultiplied(255, 255, 255, 15))
                        .rounding(8.0)
                ).clicked() {
                    self.state.recording = !self.state.recording;
                    
                    if let Some(bus) = &self.bus {
                        if self.state.recording {
                            bus.publish(phantom_core::bus::PhantomEvent::RecordingStarted {
                                session_id: uuid::Uuid::new_v4(),
                                started_at: chrono::Utc::now(),
                            });
                        } else {
                            bus.publish(phantom_core::bus::PhantomEvent::RecordingStopped {
                                session_id: uuid::Uuid::new_v4(),
                                stopped_at: chrono::Utc::now(),
                                duration_secs: self.state.elapsed_secs,
                                output_path: String::new(),
                            });
                        }
                    }
                }

                ui.separator();

                // ── Timer ────────────────────────────────────────────────
                if self.state.recording {
                    let m = self.state.elapsed_secs / 60;
                    let s = self.state.elapsed_secs % 60;
                    ui.label(
                        egui::RichText::new(format!("{m:02}:{s:02}"))
                            .color(egui::Color32::WHITE)
                            .monospace()
                            .size(14.0)
                    );
                }

                ui.separator();

                // ── Mic level meter ──────────────────────────────────────
                let level_w = 48.0 * self.state.mic_level.clamp(0.0, 1.0);
                let (rect, _) = ui.allocate_exact_size(
                    egui::vec2(48.0, 8.0), egui::Sense::hover()
                );
                ui.painter().rect_filled(
                    egui::Rect::from_min_size(rect.min, egui::vec2(48.0, 8.0)),
                    4.0,
                    egui::Color32::from_rgba_unmultiplied(255, 255, 255, 30),
                );
                ui.painter().rect_filled(
                    egui::Rect::from_min_size(rect.min, egui::vec2(level_w, 8.0)),
                    4.0,
                    egui::Color32::from_rgb(80, 200, 120),
                );

                // ── AI spinner ───────────────────────────────────────────
                if self.state.ai_thinking {
                    ui.spinner();
                }

                // ── Expand toggle ────────────────────────────────────────
                if ui.small_button("⋯").clicked() {
                    self.state.expanded = !self.state.expanded;
                }
            });

            // Expanded panel — quick actions.
            if self.state.expanded {
                ui.separator();
                ui.horizontal(|ui| {
                    if ui.button("📌 Bookmark").clicked() {
                        if let Some(bus) = &self.bus {
                            bus.publish(phantom_core::bus::PhantomEvent::VoiceCommand {
                                command: "bookmark".to_string(),
                            });
                        }
                    }
                    if ui.button("📋 Chapter").clicked() {
                        if let Some(bus) = &self.bus {
                            bus.publish(phantom_core::bus::PhantomEvent::VoiceCommand {
                                command: "chapter".to_string(),
                            });
                        }
                    }
                    if ui.button("📤 Share").clicked() {
                        if let Some(bus) = &self.bus {
                            bus.publish(phantom_core::bus::PhantomEvent::VoiceCommand {
                                command: "share".to_string(),
                            });
                        }
                    }
                    if ui.button("🧠 Summarise").clicked() {
                        if let Some(bus) = &self.bus {
                            bus.publish(phantom_core::bus::PhantomEvent::VoiceCommand {
                                command: "summarize".to_string(),
                            });
                        }
                    }
                });
            }
        });

        // Request repaint every frame when recording (for timer).
        if self.state.recording {
            ctx.request_repaint_after(std::time::Duration::from_secs(1));
        }
    }
}
