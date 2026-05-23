//! Local transcription via whisper.cpp CLI subprocess.
//!
//! Strategy (in order):
//!   1. Convert audio to 16-kHz mono WAV with ffmpeg.
//!   2. Try `whisper-cli` / `whisper-cpp` / `main` in PATH (whisper.cpp builds).
//!   3. Try `python -m whisper` (openai-whisper Python package).
//!   4. Bail with clear installation instructions.
//!
//! The whisper.cpp model (`ggml-base.en.bin`) is auto-downloaded on first run
//! from HuggingFace to `<data_dir>/phantom/models/` (~150 MB, English-only).
//!
//! No Rust build-time dependencies on whisper-rs-sys — always compiles.

use anyhow::Result;
use crate::tier_router::{AiBackend, AiResponse};

const MODEL_URL: &str =
    "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-base.en.bin";

/// Returns the path to the Phantom model directory, creating it if needed.
fn model_dir() -> std::path::PathBuf {
    let base = dirs::data_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."));
    let dir = base.join("Phantom").join("models");
    let _ = std::fs::create_dir_all(&dir);
    dir
}

/// Download the model file if it isn't already on disk.
fn ensure_model(model_path: &std::path::Path) -> Result<()> {
    if model_path.exists() {
        return Ok(());
    }
    tracing::info!("Downloading Whisper base.en model (~150 MB) to {:?}", model_path);
    let response = reqwest::blocking::get(MODEL_URL)?.error_for_status()?;
    let bytes = response.bytes()?;
    std::fs::write(model_path, &bytes)?;
    tracing::info!("Model downloaded successfully ({} MB)", bytes.len() / 1_048_576);
    Ok(())
}

/// Convert any audio/video file to 16-kHz mono WAV using ffmpeg.
fn convert_to_wav(input: &str, wav_out: &std::path::Path) -> Result<()> {
    let ffmpeg_cmd = if std::path::Path::new("ffmpeg.exe").exists() { "ffmpeg.exe" } else { "ffmpeg" };
    let status = std::process::Command::new(ffmpeg_cmd)
        .args([
            "-y", "-i", input,
            "-ar", "16000",
            "-ac", "1",
            "-f", "wav",
            wav_out.to_str().unwrap_or("out.wav"),
        ])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()?;
    if !status.success() {
        anyhow::bail!("ffmpeg failed to convert '{}' to WAV", input);
    }
    Ok(())
}

/// Try whisper.cpp CLI binaries in order.
fn try_whisper_cpp(wav: &std::path::Path, model: &std::path::Path) -> Option<String> {
    let model_str = model.to_str().unwrap_or("ggml-base.en.bin");
    let wav_str   = wav.to_str().unwrap_or("audio.wav");

    for binary in &["whisper-cli", "whisper-cpp", "main"] {
        tracing::debug!("Trying whisper.cpp binary: {binary}");
        if let Ok(out) = std::process::Command::new(binary)
            .args([
                "-m", model_str,
                "-f", wav_str,
                "--output-txt",
                "--no-prints",
                "--no-timestamps",
            ])
            .output()
        {
            if out.status.success() {
                let text = String::from_utf8_lossy(&out.stdout).trim().to_string();
                if !text.is_empty() {
                    tracing::info!("whisper.cpp transcription succeeded ({binary})");
                    return Some(text);
                }
                // whisper.cpp writes to a .txt sidecar file when --output-txt is used
                let txt_path = wav.with_extension("txt");
                if let Ok(t) = std::fs::read_to_string(&txt_path) {
                    let t = t.trim().to_string();
                    if !t.is_empty() {
                        tracing::info!("whisper.cpp txt sidecar read ({binary})");
                        return Some(t);
                    }
                }
            }
        }
    }
    None
}

/// Try `python -m whisper` (openai-whisper pip package).
fn try_python_whisper(wav: &std::path::Path) -> Option<String> {
    let tmp_dir = std::env::temp_dir();
    let wav_str = wav.to_str().unwrap_or("audio.wav");

    for py in &["python", "python3"] {
        tracing::debug!("Trying Python whisper via {py}");
        if let Ok(out) = std::process::Command::new(py)
            .args([
                "-m", "whisper", wav_str,
                "--model", "base.en",
                "--output_format", "txt",
                "--output_dir", tmp_dir.to_str().unwrap_or("."),
                "--fp16", "False",
            ])
            .output()
        {
            if out.status.success() {
                let stem = wav.file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("audio");
                let txt_path = tmp_dir.join(format!("{stem}.txt"));
                if let Ok(t) = std::fs::read_to_string(&txt_path) {
                    let t = t.trim().to_string();
                    if !t.is_empty() {
                        tracing::info!("Python whisper transcription succeeded");
                        return Some(t);
                    }
                }
            }
        }
    }
    None
}

/// Transcribe a WAV/MP4/MP3 audio file using the local Whisper model.
///
/// `audio_path` can be any format ffmpeg understands (WAV, MP4, MP3, etc.).
pub async fn transcribe_local(audio_path: &str) -> Result<AiResponse> {
    tracing::info!(path = audio_path, "Starting local Whisper transcription");

    let input_path = audio_path.to_owned();

    let text = tokio::task::spawn_blocking(move || -> Result<String> {
        let tmp  = std::env::temp_dir();
        let wav  = tmp.join("phantom_whisper_in.wav");
        let model_path = model_dir().join("ggml-base.en.bin");

        // Step 1: Convert to 16 kHz mono WAV
        convert_to_wav(&input_path, &wav)?;

        // Step 2: Ensure model is present
        if let Err(e) = ensure_model(&model_path) {
            tracing::warn!("Could not download Whisper model: {e}");
        }

        // Step 3: Try whisper.cpp CLI
        if model_path.exists() {
            if let Some(t) = try_whisper_cpp(&wav, &model_path) {
                return Ok(t);
            }
        }

        // Step 4: Try Python whisper (uses its own model cache)
        if let Some(t) = try_python_whisper(&wav) {
            return Ok(t);
        }

        // Step 5: Helpful error message
        anyhow::bail!(
            "No Whisper backend found.\n\n\
             To enable local transcription, install one of:\n\
             \n\
             Option A — whisper.cpp (recommended, fast native binary):\n\
             1. Download: https://github.com/ggerganov/whisper.cpp/releases\n\
             2. Put whisper-cli.exe in your PATH\n\
             3. Model auto-downloads to: {}\n\
             \n\
             Option B — Python whisper:\n\
             1. pip install openai-whisper\n\
             2. Ensure python.exe is in PATH",
            model_dir().display()
        )
    })
    .await??;

    Ok(AiResponse {
        backend: AiBackend::WhisperLocal,
        text,
        structured: None,
    })
}

/// Timestamped transcript segment (returned by higher-level callers).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TranscriptSegment {
    pub start_ms: u64,
    pub end_ms: u64,
    pub text: String,
    pub confidence: Option<f32>,
}
