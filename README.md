# Phantom

Phantom is a high-performance, AI-native screen recorder built in Rust. It combines low-overhead native screen capture (DXGI on Windows) with an integrated AI pipeline for smart auto-editing, automatic transcription, and intelligent search.

## Features

- **Native Screen & Audio Capture**: High-performance DXGI capture and WASAPI loopback audio capture on Windows.
- **AI Auto-Edit**: Automatically detects silence and filler words, suggesting precise, non-destructive cuts via the integrated editor.
- **Local AI Transcription**: Built-in Whisper integration automatically transcribes your recordings to text, locally and securely.
- **AI Smart Search**: Ask questions about your recordings, and the AI will analyze transcripts and metadata to provide semantic answers.
- **Non-Destructive Editor**: Review auto-edits and manually trim footage instantly using a fast, responsive egui-based timeline.
- **Direct Cloud Sharing**: Seamlessly sync videos to the cloud to generate instant share links.

## Project Structure

The workspace is organized into independent crates:

- `phantom-core` - Cross-platform screen/audio capture pipelines and video encoders.
- `phantom-storage` - SQLite-backed recording library and full-text search.
- `phantom-ai` - AI features: whisper transcription, auto-edit, and summarization.
- `phantom-ui` - The primary desktop graphical interface built with `egui`.
- `phantom-cli` - Headless command-line interface for scripting captures and uploads.

## Prerequisites

- **Rust** (Edition 2021)
- **FFmpeg**: Required in your system `PATH` or adjacent to the `phantom` executable. Phantom uses the FFmpeg backend for blazing-fast H.264/AAC muxing and extracting editor frames.
- **Whisper**: Required for local AI transcription.

## Building & Running

1. **Clone the repository:**
   ```bash
   git clone https://github.com/Relayyl6/Phantom.git
   cd Phantom
   ```

2. **Run the UI Application:**
   ```bash
   cargo run --release -p phantom-cli -- gui
   ```

3. **Run the Headless CLI:**
   ```bash
   cargo run --release -p phantom-cli -- --help
   ```

## Known Limitations
- Nano AI inference (on-device NPU processing) is currently in development and falls back to a high-performance semantic heuristic on devices without AI accelerators.
- System loopback audio capture is fully optimized for Windows via WASAPI. macOS integration is coming soon.
