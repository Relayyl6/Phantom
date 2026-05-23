use anyhow::Result;
use tokio::sync::oneshot;
use std::sync::{Arc, Mutex};

pub struct RecordingTask {
    pub duration_limit: Option<u64>,
    pub output_path: String,
    pub title: String,
    pub preview_tx: Option<std::sync::mpsc::Sender<eframe::egui::ColorImage>>,
}

pub fn spawn_recorder(
    task: RecordingTask,
    mut stop_rx: oneshot::Receiver<()>,
) -> tokio::task::JoinHandle<Result<String>> {
    tokio::spawn(async move {
        use phantom_core::{
            capture::{AudioCapture, AudioConfig, ScreenCapture, CaptureConfig},
            encoder::{Encoder, EncoderConfig},
        };

        let out_path = std::path::PathBuf::from(&task.output_path);
        if let Some(parent) = out_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        // Shared encoder protected by a mutex so both tasks can write to it
        let encoder = Arc::new(Mutex::new(
            Encoder::new(EncoderConfig::default(), out_path.clone())?
        ));
        encoder.lock().unwrap().start()?;

        // Start audio capture
        let audio_cap = AudioCapture::new(AudioConfig::default())?;
        audio_cap.start().await?;
        let mut audio_rx = audio_cap.subscribe();

        // Start screen capture
        let screen_cap = Arc::new(ScreenCapture::new(CaptureConfig::default())?);
        screen_cap.start().await?;

        let start_time = std::time::Instant::now();
        let preview_tx = task.preview_tx.map(Arc::new);
        let duration_limit = task.duration_limit;

        // ── Spawn dedicated video task ─────────────────────────────────
        // This runs INDEPENDENTLY of audio so audio events can't starve video.
        // The critical bug fix: previously both audio+video were in a single
        // select!, and because audio arrives every ~5ms, the video future
        // (which needs 33ms to produce a frame) was dropped and restarted
        // on every audio event — meaning video frames were never produced.
        let screen_cap2 = screen_cap.clone();
        let encoder_video = encoder.clone();
        let preview_tx2 = preview_tx.clone();
        let (vid_stop_tx, mut vid_stop_rx) = tokio::sync::oneshot::channel::<()>();

        tokio::spawn(async move {
            let mut last_preview_ms = 0u64;

            loop {
                // Respect stop signal
                if vid_stop_rx.try_recv().is_ok() { break; }

                let maybe_frame = screen_cap2.next_frame().await;
                match maybe_frame {
                    Some(frame) => {
                        // Write frame to encoder
                        if let Ok(mut enc) = encoder_video.lock() {
                            let _ = enc.push_video_frame(
                                &frame.data, frame.width, frame.height, frame.timestamp_ms,
                            );
                        }

                        // Send a downscaled preview every 250ms
                        if frame.timestamp_ms.saturating_sub(last_preview_ms) >= 250 {
                            if let Some(ref tx) = preview_tx2 {
                                // xcap returns RGBA via the image crate — no channel swap needed
                                // Use stride=4 for ~480x270 preview on a 1920x1080 source
                                let stride = 4usize;
                                let w = (frame.width as usize) / stride;
                                let h = (frame.height as usize) / stride;
                                let mut rgba = Vec::with_capacity(w * h * 4);

                                for y in 0..h {
                                    for x in 0..w {
                                        let sx = x * stride;
                                        let sy = y * stride;
                                        let idx = (sy * frame.width as usize + sx) * 4;
                                        if idx + 3 < frame.data.len() {
                                            rgba.push(frame.data[idx]);      // R
                                            rgba.push(frame.data[idx + 1]);  // G
                                            rgba.push(frame.data[idx + 2]);  // B
                                            rgba.push(255);                   // A
                                        } else {
                                            rgba.extend_from_slice(&[10, 12, 20, 255]);
                                        }
                                    }
                                }

                                if w > 0 && h > 0 {
                                    let img = eframe::egui::ColorImage::from_rgba_unmultiplied(
                                        [w, h], &rgba,
                                    );
                                    let _ = tx.send(img);
                                }
                                last_preview_ms = frame.timestamp_ms;
                            }
                        }
                    }
                    None => break, // capture stopped
                }
            }
        });

        // ── Audio loop (main task) ─────────────────────────────────────
        loop {
            // Check stop signal
            if stop_rx.try_recv().is_ok() { break; }

            // Check duration limit
            if let Some(limit) = duration_limit {
                if start_time.elapsed().as_secs() >= limit { break; }
            }

            // Receive audio with a timeout so we also check stop regularly
            match tokio::time::timeout(
                std::time::Duration::from_millis(50),
                audio_rx.recv(),
            ).await {
                Ok(Ok(chunk)) => {
                    if let Ok(mut enc) = encoder.lock() {
                        let _ = enc.push_audio_chunk(
                            &chunk.samples, chunk.sample_rate, chunk.channels, chunk.timestamp_ms,
                        );
                    }
                }
                Ok(Err(_)) => break,  // audio channel closed
                Err(_) => {}          // timeout — loop back and check stop
            }
        }

        // ── Clean up ──────────────────────────────────────────────────
        let _ = vid_stop_tx.send(());
        screen_cap.stop().await?;
        audio_cap.stop().await?;

        if let Ok(mut enc) = encoder.lock() {
            enc.finish()?;
        }

        Ok(task.output_path)
    })
}
