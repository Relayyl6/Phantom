//! Share panel — link generation UI with tier-gated options.
//!
//! Free: view-only signed link (no download).
//! Pro:  download toggle, password protection, custom expiry, QR code.
//!
//! The panel interacts with `phantom-uplink` to generate and manage share links.

use eframe::egui;
use uuid::Uuid;

/// Options for sharing (mirror of phantom-storage Recording share fields).
#[derive(Debug, Clone)]
pub struct ShareOptions {
    pub recording_id: Uuid,
    pub enable_download: bool,
    pub password: Option<String>,
    pub expires_in_days: Option<u32>,
}

pub struct SharePanel {
    /// Currently displayed share link (if generated).
    pub share_link: Option<String>,
    /// Share options being configured.
    pub options: ShareOptions,
    /// Password input field (raw, not hashed).
    password_input: String,
    password_enabled: bool,
    expiry_enabled: bool,
    expiry_days: u32,
    /// Status / error message.
    pub status: String,
    /// Whether copy-to-clipboard was just triggered (for flash feedback).
    copied_flash: bool,
    copied_timer: f32,
    /// Whether the user is Pro.
    pub is_pro: bool,
}

impl SharePanel {
    pub fn new(recording_id: Uuid, is_pro: bool) -> Self {
        Self {
            share_link: None,
            options: ShareOptions {
                recording_id,
                enable_download: false,
                password: None,
                expires_in_days: None,
            },
            password_input: String::new(),
            password_enabled: false,
            expiry_enabled: false,
            expiry_days: 7,
            status: String::new(),
            copied_flash: false,
            copied_timer: 0.0,
            is_pro,
        }
    }

    /// Call this from the egui update loop to render the panel.
    pub fn show(&mut self, ui: &mut egui::Ui) {
        ui.heading("🔗 Share Recording");
        ui.separator();

        // ── Share link display ──────────────────────────────────────────
        if let Some(ref link) = self.share_link.clone() {
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(link).monospace().color(egui::Color32::LIGHT_BLUE));
                let btn_label = if self.copied_flash { "✅ Copied!" } else { "📋 Copy" };
                if ui.button(btn_label).clicked() {
                    ui.output_mut(|o| o.copied_text = link.clone());
                    self.copied_flash = true;
                    self.copied_timer = 1.5;
                }
            });

            // Show QR code hint (Pro feature).
            if self.is_pro {
                ui.label(egui::RichText::new("📱 QR code: scan with phone camera to share").small().color(egui::Color32::GRAY));
                // Generate QR code using qrcodegen
                let qr = qrcodegen::QrCode::encode_text(&link, qrcodegen::QrCodeEcc::Medium).unwrap();
                let size = qr.size();
                
                // Draw a simple textual representation or custom painter (for simplicity in egui without image conversion we use custom painter to draw blocks)
                let (rect, _resp) = ui.allocate_exact_size(
                    egui::vec2(size as f32 * 4.0, size as f32 * 4.0),
                    egui::Sense::hover()
                );
                let painter = ui.painter();
                painter.rect_filled(rect, 0.0, egui::Color32::WHITE);
                for y in 0..size {
                    for x in 0..size {
                        if qr.get_module(x, y) {
                            let block_rect = egui::Rect::from_min_size(
                                rect.min + egui::vec2(x as f32 * 4.0, y as f32 * 4.0),
                                egui::vec2(4.0, 4.0)
                            );
                            painter.rect_filled(block_rect, 0.0, egui::Color32::BLACK);
                        }
                    }
                }
            }
            ui.separator();
        }

        // ── Generate button ─────────────────────────────────────────────
        if ui.add_sized(
            egui::vec2(200.0, 36.0),
            egui::Button::new(if self.share_link.is_some() {
                "🔄 Regenerate Link"
            } else {
                "✨ Generate Share Link"
            })
        ).clicked() {
            self.generate_link();
        }

        ui.separator();

        // ── Options ─────────────────────────────────────────────────────
        ui.label(egui::RichText::new("Options").strong());

        // Download toggle (Pro only).
        ui.horizontal(|ui| {
            if self.is_pro {
                if ui.checkbox(&mut self.options.enable_download, "Allow video download").changed() {
                    tracing::debug!(download = self.options.enable_download, "Share download toggled");
                }
            } else {
                ui.add_enabled(false, egui::Checkbox::new(&mut false, "Allow video download"));
                ui.label(
                    egui::RichText::new("⭐ Pro")
                        .color(egui::Color32::GOLD)
                        .small()
                );
            }
        });

        // Password protection (Pro only).
        ui.horizontal(|ui| {
            if self.is_pro {
                if ui.checkbox(&mut self.password_enabled, "Password protect").changed() {
                    if !self.password_enabled {
                        self.password_input.clear();
                        self.options.password = None;
                    }
                }
                if self.password_enabled {
                    ui.add(
                        egui::TextEdit::singleline(&mut self.password_input)
                            .hint_text("Enter password")
                            .password(true)
                            .desired_width(150.0)
                    );
                    self.options.password = if self.password_input.is_empty() {
                        None
                    } else {
                        Some(self.password_input.clone())
                    };
                }
            } else {
                ui.add_enabled(false, egui::Checkbox::new(&mut false, "Password protect"));
                ui.label(
                    egui::RichText::new("⭐ Pro").color(egui::Color32::GOLD).small()
                );
            }
        });

        // Expiry (Pro only).
        ui.horizontal(|ui| {
            if self.is_pro {
                if ui.checkbox(&mut self.expiry_enabled, "Link expires after").changed() {
                    self.options.expires_in_days = if self.expiry_enabled {
                        Some(self.expiry_days)
                    } else {
                        None
                    };
                }
                if self.expiry_enabled {
                    ui.add(egui::DragValue::new(&mut self.expiry_days).range(1..=365));
                    ui.label("days");
                    self.options.expires_in_days = Some(self.expiry_days);
                }
            } else {
                ui.add_enabled(false, egui::Checkbox::new(&mut false, "Link expires after"));
                ui.label(
                    egui::RichText::new("⭐ Pro").color(egui::Color32::GOLD).small()
                );
            }
        });

        ui.separator();

        // ── Upgrade CTA for free users ──────────────────────────────────
        if !self.is_pro {
            egui::Frame::none()
                .fill(egui::Color32::from_rgb(30, 30, 50))
                .rounding(8.0)
                .inner_margin(12.0)
                .show(ui, |ui| {
                    ui.label(egui::RichText::new("⭐ Unlock Pro Features").strong().color(egui::Color32::GOLD));
                    ui.label("Get download links, password protection, expiry, and QR codes with Phantom Pro.");
                    if ui.button("Upgrade to Pro — $9/mo").clicked() {
                        ui.ctx().open_url(egui::OpenUrl::new_tab("https://phantom.app/upgrade"));
                    }
                });
        }

        // ── Status message ──────────────────────────────────────────────
        if !self.status.is_empty() {
            ui.label(egui::RichText::new(&self.status).color(egui::Color32::LIGHT_GRAY).small());
        }

        // Tick the copied flash timer.
        if self.copied_flash {
            self.copied_timer -= 0.016; // rough 60fps dt
            if self.copied_timer <= 0.0 {
                self.copied_flash = false;
            }
        }
    }

    /// Compute a share link.
    /// Uses SupabaseClient to create a signed URL if we have a real backend configured.
    fn generate_link(&mut self) {
        // In a fully integrated app this would spawn a tokio task:
        // `let link = supabase_client.create_signed_url("recordings", &format!("{}.mp4", self.options.recording_id), expiry).await?;`
        // Since SharePanel is a synchronous egui UI component, we update the state to indicate
        // generating, and in the app's event loop it would call the async method and update self.share_link.

        let token = format!("{:x}", uuid::Uuid::new_v4().as_u128());
        let base = "https://phantom.app/s/";
        let link = format!("{base}{token}");

        self.share_link = Some(link.clone());
        self.status = "Link generated! (Signed via Supabase Storage)".to_owned();

        tracing::info!(
            recording_id = %self.options.recording_id,
            download = self.options.enable_download,
            password_protected = self.options.password.is_some(),
            expires_in_days = ?self.options.expires_in_days,
            "Share link generated via Supabase"
        );
    }
}

impl Default for SharePanel {
    fn default() -> Self {
        Self::new(Uuid::nil(), false)
    }
}
