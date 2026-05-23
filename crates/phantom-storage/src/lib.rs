//! phantom-storage — SQLite recording index + file management.

pub mod db;
pub mod file_manager;

pub use db::RecordingDb;
pub use file_manager::FileManager;
