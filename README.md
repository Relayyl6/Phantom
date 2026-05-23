# Phantom — AI-Native Screen Recorder

> **Architecture Audit** — last run: 2026-05-12

Phantom is a high-performance, Rust-native screen recording and AI intelligence platform. The workspace is structured as 8 independent crates plus a Next.js web frontend.

---

## Workspace Structure

```
phantom/
├── crates/
│   ├── phantom-core      # Capture pipeline (DXGI/ScreenCaptureKit), encoder, event bus
│   ├── phantom-ai        # Gemini client, Whisper transcription, AI tier router
│   ├── phantom-voice     # Mic pipeline, VAD (Silero), voice commands
│   ├── phantom-storage   # SQLite recording library + FTS5 search
│   ├── phantom-billing   # Stripe, entitlement store (SQLite), paywall
│   ├── phantom-uplink    # Supabase client, share links, auto-updater, team
│   ├── phantom-ui        # egui dashboard, non-destructive editor, share panel, overlay
│   └── phantom-cli       # Headless `phantom record|transcribe|summarise|upload`
└── web/                  # Next.js landing page + web dashboard
```

---

## ⚠️ System Audit — Known Issues

The following bugs prevent correct end-to-end operation. They are listed by severity.

---

### 🔴 CRITICAL — Compile Errors (Build will FAIL)

#### [BUG-001] `phantom-cli`: Wrong import path for `PlatformCapture` + missing `uuid` dep
**File:** `crates/phantom-cli/src/main.rs` · `crates/phantom-cli/Cargo.toml`
**Status:** 🔴 OPEN — Compile error

`main.rs` imports `PlatformCapture` directly from `phantom_core::capture` but `PlatformCapture` is an internal struct inside `capture::screen` and is not re-exported from `capture/mod.rs`. The correct public API is `ScreenCapture`. Additionally, `main.rs` calls `.stop()` on a `PlatformCapture` which has no such method. The `uuid` crate is also used (`Uuid::new_v4()`, `Uuid::parse_str()`) but not listed in `phantom-cli/Cargo.toml`.

**Fix:** Rewrite the `Record` command to use `ScreenCapture` (the real public API), add `uuid` to `phantom-cli/Cargo.toml`.

---

#### [BUG-002] `phantom-voice`: `reqwest` version conflict
**File:** `crates/phantom-voice/Cargo.toml`
**Status:** 🔴 OPEN — Compile error

`phantom-voice` declares its own `reqwest = { version = "0.11", features = ["blocking"] }` pinned to **0.11** while the workspace defines `reqwest = "0.12"`. This causes a dependency conflict that prevents the workspace from resolving.

**Fix:** Change `phantom-voice/Cargo.toml` to use `reqwest.workspace = true` (or add blocking feature to workspace dep).

---

#### [BUG-003] `phantom-core/capture/audio.rs`: Non-f32 audio formats crash
**File:** `crates/phantom-core/src/capture/audio.rs:186`
**Status:** 🔴 OPEN — Runtime panic on most Windows hardware

`PlatformAudio::start()` bails with an error for any sample format other than f32. WASAPI on Windows defaults to i16 or i32 on the vast majority of hardware. The entire audio capture pipeline silently fails on non-f32 devices.

**Fix:** Add i16→f32 and i32→f32 conversion branches.

---

### 🟠 HIGH — Silent Failures / Stubbed Core Functionality

#### [BUG-004] `phantom-core/encoder.rs`: Encoder is fully stubbed — recordings are empty
**File:** `crates/phantom-core/src/encoder.rs`
**Status:** 🟠 OPEN — Silent data loss

All `ffmpeg-next` calls in `start()`, `push_video_frame()`, `push_audio_chunk()`, and `finish()` are commented out. The encoder accepts frames and returns `Ok(())` but writes **zero bytes** to the output file. Users believe they have a recording but the file is empty.

**Fix:** Uncomment and wire up the `ffmpeg-next` muxer. Use `std::process::Command::new("ffmpeg")` as a fallback if linking `ffmpeg-next` is not available in CI.

---

#### [BUG-005] `phantom-uplink/src/supabase_client.rs`: Hardcoded placeholder credentials
**File:** `crates/phantom-uplink/src/supabase_client.rs:8,44`
**Status:** 🟠 OPEN — All cloud features fail at runtime

`DEFAULT_SUPABASE_URL = "https://YOUR_PROJECT.supabase.co"` and `anon_key = "YOUR_ANON_KEY"` are baked into `with_defaults()`. Any call through `SupabaseClient::with_defaults()` will fail with a network error (the CLI `Upload` command uses this). The URL must be injected from environment variables or a config file.

**Fix:** Read `PHANTOM_SUPABASE_URL` and `PHANTOM_SUPABASE_ANON_KEY` from environment at startup. Fail fast at launch with a clear error if not set.

---

#### [BUG-006] `phantom-uplink/src/updater.rs`: Hardcoded placeholder manifest URL
**File:** `crates/phantom-uplink/src/updater.rs:27`
**Status:** 🟠 OPEN — Auto-updater never works

`MANIFEST_URL` still points to `"https://YOUR_PROJECT.supabase.co/..."`. The updater will get a 400/404 on every check.

**Fix:** Same env-var injection pattern as BUG-005.

---

#### [BUG-007] `phantom-uplink/src/share.rs`: Share metadata silently discarded
**File:** `crates/phantom-uplink/src/share.rs:80`
**Status:** 🟠 OPEN — Share links are in-memory only, not persisted

`let _ = (signed_url, pw_hash, recording_id);` — after creating the signed storage URL and optionally hashing the password, all three values are thrown away. The share token and metadata are **never written to the database**. The player URL returned to the user points to a token that no storage backend knows about.

**Fix:** Call `RecordingDb::set_share()` with the generated token, password hash, and expiry before returning `ShareLink`.

---

#### [BUG-008] `phantom-ai/src/nano_client.rs`: Nano inference is not real
**File:** `crates/phantom-ai/src/nano_client.rs`
**Status:** 🟠 OPEN — Free tier "AI" is fake

`probe_nano_availability()` unconditionally returns `true` on Windows without checking for the Windows AI Foundry runtime or NPU. `nano_summarise()` does not call any on-device model — it truncates the first 15 words of the transcript and wraps them in a string. Free users receive meaningless "summaries".

**Fix:** Implement real extractive summarisation as the true free-tier fallback (select top-3 most information-dense sentences using TF-IDF or similar heuristic). Mark `nano_summarise` as the proper fallback path with a `TODO` for real Nano bindings when the Windows AI SDK is available.

---

#### [BUG-009] `phantom-ui/src/editor.rs`: Export is async but never awaited from egui
**File:** `crates/phantom-ui/src/editor.rs:261-271`
**Status:** 🟠 OPEN — Export button does nothing

The `Export` button in `EditorApp::update()` opens a file dialog and logs "Export queued" but never calls `self.export(out).await`. Since `egui::App::update` is synchronous, the async export task cannot be driven from inside it. The editor needs a `tokio::spawn` or a `oneshot` channel to hand off the export path.

**Fix:** Store the pending export path in `EditorApp` state and spawn a detached Tokio task for the actual encode on each frame check.

---

### 🟡 MEDIUM — Design Gaps / Missing Integration

#### [BUG-010] `phantom-storage/src/db.rs`: `RecordingDb` is not `Send` — can't cross async tasks
**File:** `crates/phantom-storage/src/db.rs`
**Status:** 🟡 OPEN

`RecordingDb { conn: Connection }` is not wrapped in `Arc<Mutex<>>`. SQLite `Connection` is `!Send`, so `RecordingDb` cannot be shared across threads or `tokio::spawn` tasks. Any async code that tries to pass a `RecordingDb` to another task will fail to compile.

**Fix:** Wrap `conn` in `Arc<Mutex<Connection>>` (matching the pattern in `phantom-billing`).

---

#### [BUG-011] `phantom-billing/src/stripe_client.rs`: Placeholder Stripe price IDs
**File:** `crates/phantom-billing/src/stripe_client.rs:15-16`
**Status:** 🟡 OPEN — Checkout always creates invalid sessions

`PRO_MONTHLY = "price_PHANTOM_PRO_MONTHLY"` and `TEAM_MONTHLY = "price_PHANTOM_TEAM_MONTHLY"` are not real Stripe price IDs. Checkout API calls will return 400. Read from `PHANTOM_STRIPE_SECRET_KEY`, `PHANTOM_STRIPE_PRO_PRICE_ID`, `PHANTOM_STRIPE_TEAM_PRICE_ID` env vars.

---

#### [BUG-012] `phantom-cli/main.rs`: `Upload` ignores sign-in errors
**File:** `crates/phantom-cli/src/main.rs:165`
**Status:** 🟡 OPEN

`let _ = supabase.sign_in(&email, &password).await;` — the sign-in result is silently discarded. If auth fails (wrong password, network error), the subsequent `upload_file` call will use the anon key and will be rejected by Supabase storage. The user gets a confusing upload error rather than an auth error.

**Fix:** Propagate the sign-in result with `?`.

---

#### [BUG-013] `phantom-ai/src/features/summarizer.rs`: `key_moments` is always empty
**File:** `crates/phantom-ai/src/features/summarizer.rs:84`
**Status:** 🟡 OPEN

`key_moments: vec![]` — a TODO comment marks this but it is never populated. The `RecordingSummary` struct always returns an empty `key_moments` vec.

---

#### [BUG-014] `phantom-core/src/capture/audio.rs`: System loopback not captured on Windows
**File:** `crates/phantom-core/src/capture/audio.rs:159-162`
**Status:** 🟡 OPEN

`AudioConfig::default()` sets `capture_system: true` and `capture_mic: true` but `PlatformAudio::start()` only opens the **default input device** (microphone). System loopback on Windows requires opening WASAPI in exclusive/loopback mode — an entirely different code path. Recordings contain only microphone audio.

---

### 🟢 LOW — Polish / Non-Blocking

#### [BUG-015] `phantom-ai/src/nano_client.rs`: macOS always returns `false` for Nano availability
Even on Apple Silicon M3 (which supports Apple Intelligence), `probe_nano_availability()` returns `false` because the non-Windows branch is a blanket `return false`. macOS users always get the extractive fallback.

#### [BUG-016] `phantom-uplink/src/updater.rs`: Updater writes installer to `/tmp` without cleanup
`tmp_path = std::env::temp_dir().join("phantom_update_installer")` — fixed filename with no extension. On Windows, `msiexec /i` requires a `.msi` extension. On macOS, `open` needs `.dmg`. The install will fail silently.

#### [BUG-017] `phantom-core/src/capture/screen.rs`: WASM `PlatformCapture::next_frame` always returns `None`
Expected — WASM capture is handled by `MediaRecorder` in the browser. However the code should assert or log a warning rather than silently return `None` to avoid confusion during WASM debugging.

#### [BUG-018] Workspace does not declare `phantom-cli` in `[workspace.dependencies]`
`phantom-cli` is a `[[bin]]` member but its deps (`clap`, `keyring`) are not in the shared workspace table. This is fine but inconsistent — future crates that need `clap` will re-declare it locally.

---

## Fix Progress

| ID | Severity | Description | Status |
|----|----------|-------------|--------|
| BUG-001 | 🔴 | `phantom-cli`: Wrong import + missing `uuid` dep | ✅ Fixed |
| BUG-002 | 🔴 | `phantom-voice`: reqwest 0.11 vs 0.12 conflict | ✅ Fixed |
| BUG-003 | 🔴 | Audio: non-f32 formats crash WASAPI pipeline | ✅ Fixed |
| BUG-004 | 🟠 | Encoder: ffmpeg calls all commented out | ✅ Fixed |
| BUG-005 | 🟠 | Supabase: hardcoded placeholder credentials | ✅ Fixed |
| BUG-006 | 🟠 | Updater: hardcoded placeholder manifest URL | ✅ Fixed |
| BUG-007 | 🟠 | Share: metadata discarded, never persisted | ✅ Fixed |
| BUG-008 | 🟠 | Nano: fake free-tier inference | ✅ Fixed |
| BUG-009 | 🟠 | Editor: export async never awaited | ✅ Fixed |
| BUG-010 | 🟡 | `RecordingDb` not Send-safe | ✅ Fixed |
| BUG-011 | 🟡 | Stripe: placeholder price IDs | ✅ Fixed |
| BUG-012 | 🟡 | CLI Upload: sign-in error silently swallowed | ✅ Fixed |
| BUG-013 | 🟡 | Summariser: `key_moments` always empty | 🔲 Deferred |
| BUG-014 | 🟡 | Audio: system loopback not captured | 🔲 Deferred (platform-specific) |
| BUG-015 | 🟢 | macOS Nano detection always false | 🔲 Deferred |
| BUG-016 | 🟢 | Updater installer extension missing | ✅ Fixed |
| BUG-017 | 🟢 | WASM silent None on next_frame | 🔲 Deferred |
| BUG-018 | 🟢 | `clap`/`keyring` not in workspace deps | 🔲 Deferred |
