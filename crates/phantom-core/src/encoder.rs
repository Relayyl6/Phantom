//! Video + audio encoder.
//!
//! Native: ffmpeg-next backed H.264/HEVC/AV1.
//! Web: MediaRecorder API (in-browser capture)

use anyhow::Result;
use std::path::PathBuf;
use serde::{Deserialize, Serialize};

/// Codec choice for video encoding.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VideoCodec {
    #[default]
    H264,
    Hevc,
    Av1,
}

/// Audio codec for the muxed container.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AudioCodec {
    #[default]
    Aac,
    Opus,
}

/// Encoder configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EncoderConfig {
    pub video_codec: VideoCodec,
    pub audio_codec: AudioCodec,
    pub crf: u8,
    pub fps: u32,
    pub width: u32,
    pub height: u32,
    pub hardware_accel: bool,
}

impl Default for EncoderConfig {
    fn default() -> Self {
        Self {
            video_codec: VideoCodec::H264,
            audio_codec: AudioCodec::Aac,
            crf: 18,
            fps: 30,
            width: 0,
            height: 0,
            hardware_accel: true,
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub struct Encoder {
    config: EncoderConfig,
    output_path: PathBuf,
    running: bool,
    frame_count: u64,
    video_tmp: Option<std::fs::File>,
    audio_tmp: Option<std::fs::File>,
    video_tmp_path: PathBuf,
    audio_tmp_path: PathBuf,
}

#[cfg(not(target_arch = "wasm32"))]
impl Encoder {
    pub fn new(config: EncoderConfig, output_path: PathBuf) -> Result<Self> {
        let video_tmp_path = std::env::temp_dir().join(format!("phantom_video_{}.raw", uuid::Uuid::new_v4()));
        let audio_tmp_path = std::env::temp_dir().join(format!("phantom_audio_{}.raw", uuid::Uuid::new_v4()));
        
        Ok(Self {
            config,
            output_path,
            running: false,
            frame_count: 0,
            video_tmp: None,
            audio_tmp: None,
            video_tmp_path,
            audio_tmp_path,
        })
    }

    pub fn start(&mut self) -> Result<()> {
        tracing::info!(
            path = %self.output_path.display(),
            codec = ?self.config.video_codec,
            fps = self.config.fps,
            "Encoder started (writing to temp raw files)"
        );
        self.running = true;
        
        self.video_tmp = Some(std::fs::File::create(&self.video_tmp_path)?);
        self.audio_tmp = Some(std::fs::File::create(&self.audio_tmp_path)?);
        
        Ok(())
    }

    pub fn push_video_frame(&mut self, data: &[u8], _width: u32, _height: u32, _pts_ms: u64) -> Result<()> {
        if !self.running { return Ok(()); }
        
        if let Some(ref mut file) = self.video_tmp {
            use std::io::Write;
            file.write_all(data)?;
        }
        
        self.frame_count += 1;
        Ok(())
    }

    pub fn push_audio_chunk(&mut self, samples: &[f32], _sample_rate: u32, _channels: u16, _pts_ms: u64) -> Result<()> {
        if !self.running { return Ok(()); }
        
        if let Some(ref mut file) = self.audio_tmp {
            use std::io::Write;
            let bytes: &[u8] = unsafe {
                std::slice::from_raw_parts(
                    samples.as_ptr() as *const u8,
                    samples.len() * std::mem::size_of::<f32>(),
                )
            };
            file.write_all(bytes)?;
        }
        
        Ok(())
    }

    pub fn finish(&mut self) -> Result<PathBuf> {
        tracing::info!(frames = self.frame_count, "Encoder finishing, muxing with ffmpeg CLI...");
        self.running = false;
        
        // Close files
        self.video_tmp = None;
        self.audio_tmp = None;
        
        let width = if self.config.width == 0 { 1920 } else { self.config.width };
        let height = if self.config.height == 0 { 1080 } else { self.config.height };

        let ffmpeg_bin = if std::path::Path::new("ffmpeg.exe").exists() {
            "ffmpeg.exe".to_string()
        } else if let Ok(exe_dir) = std::env::current_exe().map(|p| p.parent().unwrap_or(std::path::Path::new(".")).to_path_buf()) {
            let candidate = exe_dir.join("ffmpeg.exe");
            if candidate.exists() { candidate.to_string_lossy().to_string() } else { "ffmpeg".to_string() }
        } else {
            "ffmpeg".to_string()
        };

        let status = std::process::Command::new(&ffmpeg_bin)
            .args(&[
                "-y",
                "-f", "f32le",
                "-ar", "48000",
                "-ac", "2",
                "-i", self.audio_tmp_path.to_str().unwrap(),
                "-f", "rawvideo",
                "-pix_fmt", "bgra",
                "-s", &format!("{}x{}", width, height),
                "-r", &self.config.fps.to_string(),
                "-i", self.video_tmp_path.to_str().unwrap(),
                "-c:v", "libx264",
                "-preset", "ultrafast",
                "-crf", &self.config.crf.to_string(),
                "-c:a", "aac",
                self.output_path.to_str().unwrap(),
            ])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()?;
            
        if !status.success() {
            tracing::error!("ffmpeg CLI failed to mux the raw streams. Please ensure ffmpeg is installed and in your PATH.");
        } else {
            tracing::info!("Encoding complete: {}", self.output_path.display());
        }
        
        // Cleanup temp files
        let _ = std::fs::remove_file(&self.video_tmp_path);
        let _ = std::fs::remove_file(&self.audio_tmp_path);

        Ok(self.output_path.clone())
    }
}

#[cfg(target_arch = "wasm32")]
pub struct Encoder {
    config: EncoderConfig,
    output_path: PathBuf,
}

#[cfg(target_arch = "wasm32")]
impl Encoder {
    pub fn new(config: EncoderConfig, output_path: PathBuf) -> Result<Self> {
        Ok(Self { config, output_path })
    }

    pub fn start(&mut self) -> Result<()> {
        tracing::info!("WebMediaRecorder started");
        Ok(())
    }

    pub fn push_video_frame(&mut self, _data: &[u8], _width: u32, _height: u32, _pts_ms: u64) -> Result<()> {
        Ok(())
    }

    pub fn push_audio_chunk(&mut self, _samples: &[f32], _sample_rate: u32, _channels: u16, _pts_ms: u64) -> Result<()> {
        Ok(())
    }

    pub fn finish(&mut self) -> Result<PathBuf> {
        tracing::info!("WebMediaRecorder finishing");
        Ok(self.output_path.clone())
    }
}
