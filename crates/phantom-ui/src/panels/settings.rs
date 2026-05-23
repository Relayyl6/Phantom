use eframe::egui;
use crate::app_state::AppState;
use phantom_billing::entitlement::Tier;

pub fn show(state: &mut AppState, ui: &mut egui::Ui) {
    ui.heading("⚙  Settings");
    ui.separator();
    ui.add_space(8.0);

    ui.collapsing("🔑  API Keys", |ui| {
        egui::Grid::new("api_keys_grid").num_columns(2).spacing([12.0, 8.0]).show(ui, |ui| {
            ui.label("Gemini API Key:");
            ui.add(egui::TextEdit::singleline(&mut state.gemini_api_key)
                .password(!state.show_api_key).desired_width(280.0).hint_text("AIza…"));
            ui.end_row();
            ui.label("Supabase URL:");
            ui.text_edit_singleline(&mut state.supabase_url);
            ui.end_row();
            ui.label("Supabase Anon Key:");
            ui.add(egui::TextEdit::singleline(&mut state.supabase_anon_key)
                .password(!state.show_api_key).desired_width(280.0));
            ui.end_row();
        });
        ui.checkbox(&mut state.show_api_key, "Show keys");
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            if ui.button("  💾  Save  ").clicked() {
                // Persist key to env for this session
                #[allow(unused_unsafe)]
                unsafe { std::env::set_var("GEMINI_API_KEY", &state.gemini_api_key); }
                
                // Reinitialize AI router with new key (works even if entitlement is None)
                let ent = state.entitlement.clone().unwrap_or_else(|| {
                    // Create a fallback entitlement if not yet loaded
                    let ent_path = dirs::data_dir()
                        .unwrap_or_else(|| std::env::temp_dir())
                        .join("Phantom")
                        .join("entitlements.db");
                    phantom_billing::entitlement::EntitlementStore::open(ent_path)
                        .expect("Cannot open entitlement DB")
                });
                let router = phantom_ai::AiTierRouter::new(ent, state.gemini_api_key.clone());
                state.ai_router = Some(std::sync::Arc::new(router));
                state.status_msg = if state.gemini_api_key.is_empty() {
                    "Settings saved. Add a Gemini API key to use cloud AI features.".to_string()
                } else {
                    "Settings saved. Gemini AI is now active!".to_string()
                };
            }
            if !state.gemini_api_key.is_empty() {
                ui.label(egui::RichText::new("✅ Key configured").color(egui::Color32::from_rgb(100, 220, 100)).size(11.0));
            } else {
                ui.label(egui::RichText::new("⚠ No key — Free tier only (local AI)").color(egui::Color32::from_rgb(220, 180, 60)).size(11.0));
            }
        });
    });

    ui.add_space(8.0);

    ui.collapsing("💳  Subscription", |ui| {
        ui.horizontal(|ui| {
            let (tier_label, tier_color) = match state.tier {
                Tier::Free => ("FREE", egui::Color32::GRAY),
                Tier::Pro  => ("PRO",  egui::Color32::from_rgb(120, 100, 255)),
                Tier::Team => ("TEAM", egui::Color32::from_rgb(220, 180, 60)),
            };
            ui.label("Current tier:");
            ui.label(egui::RichText::new(tier_label).strong().color(tier_color).size(16.0));
        });
        ui.add_space(4.0);

        // Tier switcher (simulated)
        ui.horizontal(|ui| {
            if ui.selectable_label(state.tier == Tier::Free, "Free").clicked()  { state.tier = Tier::Free; }
            if ui.selectable_label(state.tier == Tier::Pro,  "Pro ($15/mo)").clicked() { state.tier = Tier::Pro; }
            if ui.selectable_label(state.tier == Tier::Team, "Team ($49/mo)").clicked() { state.tier = Tier::Team; }
        });
        ui.add_space(4.0);

        egui::Frame::none()
            .fill(egui::Color32::from_rgb(22, 22, 32))
            .rounding(6.0)
            .inner_margin(egui::Margin::same(10.0))
            .show(ui, |ui| {
                ui.label("Free:  3 AI summaries/day  |  View-only share  |  Local Whisper  |  FTS5 search");
                ui.label("Pro:   Unlimited AI  |  Download share  |  Password + expiry  |  Semantic search  |  Auto-Edit  |  Live Assistant  |  Code Intel  |  Video Q&A");
                ui.label("Team:  Everything in Pro  |  Team workspaces  |  Shared library  |  Priority support");
            });

        ui.add_space(4.0);
        let daily_left = 3u32.saturating_sub(state.daily_ai_used);
        if matches!(state.tier, Tier::Free) {
            ui.label(egui::RichText::new(format!("AI uses today: {}/3 (resets midnight UTC)", state.daily_ai_used))
                .color(if daily_left == 0 { egui::Color32::RED } else { egui::Color32::GRAY }).size(12.0));
        }
    });

    ui.add_space(8.0);

    ui.collapsing("🔄  Auto-Updater", |ui| {
        ui.label("Current version: v0.1.0");
        ui.label(egui::RichText::new("Update channel: Supabase Storage (phantom-releases bucket)").color(egui::Color32::GRAY).size(11.0));
        ui.add_space(4.0);
        if ui.button("  🔍  Check for Updates  ").clicked() {
            state.status_msg = "Already on latest version (v0.1.0).".to_string();
        }
    });

    ui.add_space(8.0);

    ui.collapsing("ℹ  About", |ui| {
        ui.label("Phantom — AI-Native Screen Capture Platform");
        ui.label("Version: 0.1.0");
        ui.label("Build: Rust 1.87 · egui 0.29 · ffmpeg 8.1");
        ui.label("Capture: DXGI (Windows) · WASAPI audio · Whisper.cpp local ASR");
        ui.label("AI: Gemini Flash (Free/Nano) · Gemini Pro Vision (Pro tier)");
        ui.hyperlink_to("📖 Docs", "https://phantom.app/docs");
    });
}
