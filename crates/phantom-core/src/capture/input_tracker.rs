//! Input event tracking — keyboard and mouse events during a recording.
//!
//! Events are stored as a lightweight timeline so that:
//!   • The UI can render a "keystroke visualiser" overlay on playback.
//!   • Gemini AI can understand *what* the user was doing at each moment.
//!
//! PRIVACY: raw text typed is NEVER stored. Only key codes + timestamps.
//! Mouse absolute positions are only stored if `capture_mouse_position` is true.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};

/// A single input event in the recording timeline.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InputEvent {
    /// Milliseconds since recording start.
    pub timestamp_ms: u64,
    /// The event itself.
    pub kind: InputEventKind,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum InputEventKind {
    KeyPress {
        /// Human-readable key name ("Ctrl", "Enter", "A", …). NOT raw text.
        key_name: String,
        /// Is this a modifier-combo (e.g. Ctrl+S)?
        is_combo: bool,
    },
    MouseClick {
        button: MouseButton,
        /// Optional position (x, y) in screen pixels.
        position: Option<(i32, i32)>,
    },
    MouseScroll {
        /// Positive = scroll down, negative = scroll up.
        delta_y: f32,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MouseButton {
    Left,
    Right,
    Middle,
}

/// Configuration for input tracking.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InputTrackerConfig {
    pub enabled: bool,
    /// Store absolute mouse coordinates. Disable for privacy.
    pub capture_mouse_position: bool,
}

impl Default for InputTrackerConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            capture_mouse_position: true,
        }
    }
}

/// Collects input events during a recording session.
pub struct InputTracker {
    config: InputTrackerConfig,
    /// Shared event buffer filled by the rdev background thread.
    events: Arc<Mutex<Vec<InputEvent>>>,
    session_start: Option<DateTime<Utc>>,
    /// Shutdown flag — set to true to stop the rdev listener.
    stop_flag: Arc<std::sync::atomic::AtomicBool>,
}

impl InputTracker {
    pub fn new(config: InputTrackerConfig) -> Self {
        Self {
            config,
            events: Arc::new(Mutex::new(Vec::new())),
            session_start: None,
            stop_flag: Arc::new(std::sync::atomic::AtomicBool::new(false)),
        }
    }

    /// Start the rdev global input listener in a background thread.
    pub fn start(&mut self) {
        if !self.config.enabled {
            return;
        }
        self.session_start = Some(Utc::now());
        self.stop_flag.store(false, std::sync::atomic::Ordering::Relaxed);
        let events_arc = self.events.clone();
        let stop_flag = self.stop_flag.clone();
        let capture_mouse = self.config.capture_mouse_position;
        let session_start = std::time::Instant::now();

        std::thread::spawn(move || {
            // rdev::listen blocks this thread; we filter events and push into the shared buffer.
            let _ = rdev::listen(move |event: rdev::Event| {
                if stop_flag.load(std::sync::atomic::Ordering::Relaxed) {
                    // rdev doesn't support cancellation, but we can just drop events.
                    return;
                }
                let timestamp_ms = session_start.elapsed().as_millis() as u64;

                let kind = match event.event_type {
                    rdev::EventType::KeyPress(key) => {
                        let key_name = format!("{key:?}");
                        // Detect modifier combos by checking for modifier key names
                        let is_combo = matches!(key,
                            rdev::Key::ControlLeft | rdev::Key::ControlRight |
                            rdev::Key::Alt | rdev::Key::AltGr |
                            rdev::Key::MetaLeft | rdev::Key::MetaRight |
                            rdev::Key::ShiftLeft | rdev::Key::ShiftRight
                        );
                        Some(InputEventKind::KeyPress { key_name, is_combo })
                    }
                    rdev::EventType::ButtonPress(btn) => {
                        let button = match btn {
                            rdev::Button::Left   => MouseButton::Left,
                            rdev::Button::Right  => MouseButton::Right,
                            rdev::Button::Middle => MouseButton::Middle,
                            _                    => return,
                        };
                        let position = if capture_mouse {
                            event.name.as_deref().map(|_| (0i32, 0i32)) // position in mouse events is not in name
                        } else {
                            None
                        };
                        Some(InputEventKind::MouseClick { button, position })
                    }
                    rdev::EventType::Wheel { delta_x: _, delta_y } => {
                        Some(InputEventKind::MouseScroll { delta_y: delta_y as f32 })
                    }
                    _ => None,
                };

                if let Some(k) = kind {
                    if let Ok(mut ev) = events_arc.lock() {
                        ev.push(InputEvent { timestamp_ms, kind: k });
                    }
                }
            });
        });

        tracing::debug!("Input tracker armed with rdev");
    }

    /// Stop the listener and return all collected events.
    pub fn stop(&mut self) -> Vec<InputEvent> {
        self.stop_flag.store(true, std::sync::atomic::Ordering::Relaxed);
        let events = self.events.lock()
            .map(|mut g| std::mem::take(&mut *g))
            .unwrap_or_default();
        tracing::debug!("Input tracker disarmed, {} events", events.len());
        events
    }

    /// Called externally to inject an event (for testing or UI).
    pub fn push(&mut self, event: InputEvent) {
        if self.config.enabled {
            if let Ok(mut ev) = self.events.lock() {
                ev.push(event);
            }
        }
    }
}
