//! phantom-ui — egui overlay, dashboard, editor, and share panel.

pub mod app_recorder;
pub mod app_state;
pub mod panels;
pub mod dashboard;
pub mod editor;
pub mod overlay;
pub mod share_panel;

#[cfg(target_arch = "wasm32")]
pub mod app_web;

pub use overlay::OverlayApp;
pub use dashboard::DashboardApp;
pub use editor::EditorApp;

#[cfg(target_arch = "wasm32")]
pub use app_web::start_web;
