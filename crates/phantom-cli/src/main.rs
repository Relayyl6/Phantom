#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
//! Phantom CLI — headless record/transcribe/summarise/upload.
//!
//! Usage:
//!   phantom record [--duration <secs>] [--output <path>]
//!   phantom transcribe <file>
//!   phantom summarise <file>
//!   phantom upload <recording-id> [--share] [--expires <days>]

use anyhow::Result;
use clap::{Parser, Subcommand};
use tracing_subscriber::EnvFilter;

#[derive(Parser)]
#[command(name = "phantom", version, about = "Phantom — AI-native screen recorder")]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Start a screen recording session.
    Record {
        /// Stop automatically after this many seconds.
        #[arg(short, long)]
        duration: Option<u64>,
        /// Output file path (default: ~/Phantom/recordings/…/video.mp4).
        #[arg(short, long)]
        output: Option<String>,
    },
    /// Transcribe an audio or video file.
    Transcribe {
        /// Path to the file.
        file: String,
    },
    /// Generate an AI summary for a recording.
    Summarise {
        /// Path to the recording directory or video file.
        file: String,
        /// Output format: text (default) or markdown.
        #[arg(long, default_value = "text")]
        format: String,
    },
    /// Upload a recording to Supabase and optionally generate a share link.
    Upload {
        /// Recording ID (UUID) to upload.
        recording_id: String,
        /// Generate a share link after uploading.
        #[arg(long)]
        share: bool,
        /// Share link expiry in days (Pro only).
        #[arg(long)]
        expires: Option<u64>,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    let cli = Cli::parse();

    match cli.command {
        Some(Commands::Record { duration, output }) => {
            let out = output.unwrap_or_else(|| "video.mp4".to_string());
            println!("▶ Starting recording (duration={duration:?}, output={out:?})");
            
            use phantom_core::{
                capture::{AudioCapture, AudioConfig, ScreenCapture, CaptureConfig},
                encoder::{Encoder, EncoderConfig},
            };
            
            let mut encoder = Encoder::new(EncoderConfig::default(), std::path::PathBuf::from(&out))?;
            encoder.start()?;
            
            let audio_cap = AudioCapture::new(AudioConfig::default())?;
            audio_cap.start().await?;
            
            let screen_cap = ScreenCapture::new(CaptureConfig::default())?;
            screen_cap.start().await?;
            
            let start_time = std::time::Instant::now();
            let mut audio_rx = audio_cap.subscribe();

            loop {
                if let Some(d) = duration {
                    if start_time.elapsed().as_secs() >= d {
                        break;
                    }
                }

                tokio::select! {
                    // Poll screen at target FPS (next_frame() sleeps internally)
                    maybe_frame = screen_cap.next_frame() => {
                        if let Some(frame) = maybe_frame {
                            let _ = encoder.push_video_frame(
                                &frame.data, frame.width, frame.height, frame.timestamp_ms
                            );
                        } else {
                            // Capture stopped — exit the loop
                            break;
                        }
                    }
                    // Drain audio chunks as they arrive
                    result = audio_rx.recv() => {
                        match result {
                            Ok(chunk) => {
                                let _ = encoder.push_audio_chunk(
                                    &chunk.samples, chunk.sample_rate, chunk.channels, chunk.timestamp_ms
                                );
                            }
                            Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                                tracing::warn!("Audio ring buffer lagged by {n} chunks");
                            }
                            Err(_) => break,
                        }
                    }
                }
            }

            
            screen_cap.stop().await?;
            audio_cap.stop().await?;
            encoder.finish()?;
            println!("✅ Recording saved to {}", out);
        }
        Some(Commands::Transcribe { file }) => {
            println!("🎙 Transcribing: {file}");
            let res = phantom_ai::features::transcription::transcribe_local(&file).await?;
            println!("\nTranscript:\n{}", res.text);
        }
        Some(Commands::Summarise { file, format }) => {
            println!("🧠 Summarising: {file} (format={format})");
            
            // Initialise Entitlement store and tier router
            let db_path = std::env::temp_dir().join("phantom_entitlements.db");
            let entitlement = phantom_billing::entitlement::EntitlementStore::open(db_path)?;
            
            let api_key = std::env::var("GEMINI_API_KEY")
                .map_err(|_| anyhow::anyhow!("GEMINI_API_KEY environment variable is required"))?;
            let router = phantom_ai::AiTierRouter::new(entitlement, api_key);
            
            let transcript = std::fs::read_to_string(&file)?;
            let session_id = uuid::Uuid::new_v4();
            
            let summary = phantom_ai::features::summarizer::summarise(&router, session_id, &transcript).await?;
            
            if format == "json" {
                println!("{}", serde_json::to_string_pretty(&summary)?);
            } else {
                println!("\nTL;DR:\n{}\n", summary.tldr);
                
                if !summary.action_items.is_empty() {
                    println!("Action Items:");
                    for item in summary.action_items {
                        println!("- {} (Owner: {})", item.text, item.owner.unwrap_or_else(|| "Unassigned".to_string()));
                    }
                }
            }
        }
        Some(Commands::Upload { recording_id, share, expires }) => {
            println!("☁ Uploading recording {recording_id} (share={share}, expires={expires:?})");
            
            let mut supabase = phantom_uplink::SupabaseClient::with_defaults();
            
            // Read user creds from keychain
            let entry = keyring::Entry::new("phantom-app", "supabase-creds")?;
            let (email, password) = match entry.get_password() {
                Ok(creds) => {
                    let parts: Vec<&str> = creds.split(':').collect();
                    if parts.len() == 2 {
                        (parts[0].to_string(), parts[1].to_string())
                    } else {
                        anyhow::bail!("Invalid keychain format. Please re-authenticate with `phantom login`.");
                    }
                }
                Err(_) => {
                    anyhow::bail!("No credentials found in keychain. Please authenticate with `phantom login` first.");
                }
            };
            
            let _ = supabase.sign_in(&email, &password).await; 
            
            let obj_path = format!("sessions/{}/video.mp4", recording_id);
            let url = supabase.upload_file("recordings", &obj_path, vec![], "video/mp4").await?;
            println!("Upload complete! Raw URL: {}", url);
            
            if share {
                let db_path = std::env::temp_dir().join("phantom_entitlements.db");
                let entitlement = phantom_billing::entitlement::EntitlementStore::open(db_path)?;
                let base_url = std::env::var("PHANTOM_APP_URL").unwrap_or_else(|_| "https://phantom.app".to_string());
                let manager = phantom_uplink::ShareManager::new(supabase, entitlement, base_url);
                
                let link = manager.generate(uuid::Uuid::parse_str(&recording_id)?, &obj_path, None, expires, true).await?;
                println!("Share Link: {}", link.url);
            }
        }
        None => {
            println!("Starting Phantom UI...");
            let options = eframe::NativeOptions {
                viewport: eframe::egui::ViewportBuilder::default()
                    .with_inner_size([1200.0, 800.0])
                    .with_title("Phantom"),
                ..Default::default()
            };
            let _ = eframe::run_native(
                "Phantom",
                options,
                Box::new(|_cc| {
                    Ok(Box::new(phantom_ui::DashboardApp::default()))
                }),
            );
        }
    }

    Ok(())
}
