//! SQLite-backed recording database with FTS5 full-text search.

use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use uuid::Uuid;

/// A recording entry in the library.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Recording {
    pub id: Uuid,
    pub title: String,
    pub file_path: String,
    pub thumb_path: Option<String>,
    pub duration_secs: u64,
    pub created_at: DateTime<Utc>,
    pub tags: Vec<String>,
    pub share_token: Option<String>,
    pub share_password_hash: Option<String>,
    pub share_download_enabled: bool,
    pub share_expires_at: Option<DateTime<Utc>>,
    pub summary: Option<String>,
}

#[cfg(not(target_arch = "wasm32"))]
mod native {
    use super::*;
    use rusqlite::{Connection, params};

    pub struct RecordingDb {
        conn: Connection,
    }

    impl RecordingDb {
        pub fn open(db_path: PathBuf) -> Result<Self> {
            let conn = Connection::open(&db_path)?;
            conn.execute_batch(include_str!("schema.sql"))?;
            Ok(Self { conn })
        }

        pub fn insert(&self, r: &Recording) -> Result<()> {
            self.conn.execute(
                "INSERT INTO recordings
                 (id, title, file_path, thumb_path, duration_secs, created_at, tags,
                  share_token, share_password_hash, share_download_enabled,
                  share_expires_at, summary)
                 VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12)",
                params![
                    r.id.to_string(),
                    r.title,
                    r.file_path,
                    r.thumb_path,
                    r.duration_secs as i64,
                    r.created_at.to_rfc3339(),
                    r.tags.join(","),
                    r.share_token,
                    r.share_password_hash,
                    r.share_download_enabled as i32,
                    r.share_expires_at.map(|d| d.to_rfc3339()),
                    r.summary,
                ],
            )?;
            Ok(())
        }

        pub fn delete(&self, id: Uuid) -> Result<()> {
            let id_str = id.to_string();
            // Get file path first, making sure to drop stmt and rows before executing delete
            let file_path = {
                if let Ok(mut stmt) = self.conn.prepare("SELECT file_path FROM recordings WHERE id = ?1") {
                    if let Ok(mut rows) = stmt.query([&id_str]) {
                        if let Ok(Some(row)) = rows.next() {
                            row.get::<_, String>(0).ok()
                        } else { None }
                    } else { None }
                } else { None }
            };
            
            if let Some(path) = file_path {
                let _ = std::fs::remove_file(path);
            }
            
            self.conn.execute("DELETE FROM recordings WHERE id = ?1", params![id_str])?;
            Ok(())
        }

        pub fn list_all(&self) -> Result<Vec<Recording>> {
            let mut stmt = self.conn.prepare(
                "SELECT id,title,file_path,thumb_path,duration_secs,created_at,
                        tags,share_token,share_password_hash,share_download_enabled,
                        share_expires_at,summary
                 FROM recordings ORDER BY created_at DESC",
            )?;
            let rows = stmt.query_map([], |row| {
                Ok(Recording {
                    id: row.get::<_, String>(0)?
                        .parse::<Uuid>()
                        .map_err(|e| rusqlite::Error::InvalidColumnType(0, e.to_string(), rusqlite::types::Type::Text))?,
                    title: row.get(1)?,
                    file_path: row.get(2)?,
                    thumb_path: row.get(3)?,
                    duration_secs: row.get::<_, i64>(4)? as u64,
                    created_at: row.get::<_, String>(5)?
                        .parse::<DateTime<Utc>>()
                        .map_err(|e| rusqlite::Error::InvalidColumnType(5, e.to_string(), rusqlite::types::Type::Text))?,
                    tags: row.get::<_, String>(6)?
                        .split(',')
                        .filter(|s| !s.is_empty())
                        .map(str::to_owned)
                        .collect(),
                    share_token: row.get(7)?,
                    share_password_hash: row.get(8)?,
                    share_download_enabled: row.get::<_, i32>(9)? != 0,
                    share_expires_at: row.get::<_, Option<String>>(10)?
                        .and_then(|s| s.parse::<DateTime<Utc>>().ok()),
                    summary: row.get(11)?,
                })
            })?;
            rows.collect::<rusqlite::Result<Vec<_>>>().map_err(Into::into)
        }

        pub fn search(&self, query: &str) -> Result<Vec<Recording>> {
            let fts_query = format!("{query}*");
            let mut stmt = self.conn.prepare(
                "SELECT r.id, r.title, r.file_path, r.thumb_path, r.duration_secs, r.created_at,
                        r.tags, r.share_token, r.share_password_hash, r.share_download_enabled,
                        r.share_expires_at, r.summary
                 FROM recordings r
                 JOIN recordings_fts fts ON r.id = fts.recording_id
                 WHERE recordings_fts MATCH ?1
                 ORDER BY r.created_at DESC"
            )?;

            let rows = stmt.query_map(params![fts_query], |row| {
                Ok(Recording {
                    id: row.get::<_, String>(0)?
                        .parse::<Uuid>()
                        .map_err(|e| rusqlite::Error::InvalidColumnType(0, e.to_string(), rusqlite::types::Type::Text))?,
                    title: row.get(1)?,
                    file_path: row.get(2)?,
                    thumb_path: row.get(3)?,
                    duration_secs: row.get::<_, i64>(4)? as u64,
                    created_at: row.get::<_, String>(5)?
                        .parse::<DateTime<Utc>>()
                        .map_err(|e| rusqlite::Error::InvalidColumnType(5, e.to_string(), rusqlite::types::Type::Text))?,
                    tags: row.get::<_, String>(6)?
                        .split(',')
                        .filter(|s| !s.is_empty())
                        .map(str::to_owned)
                        .collect(),
                    share_token: row.get(7)?,
                    share_password_hash: row.get(8)?,
                    share_download_enabled: row.get::<_, i32>(9)? != 0,
                    share_expires_at: row.get::<_, Option<String>>(10)?
                        .and_then(|s| s.parse::<DateTime<Utc>>().ok()),
                    summary: row.get(11)?,
                })
            })?;
            rows.collect::<rusqlite::Result<Vec<_>>>().map_err(Into::into)
        }

        pub fn update_summary(&self, id: Uuid, summary: &str) -> Result<()> {
            self.conn.execute(
                "UPDATE recordings SET summary = ?1 WHERE id = ?2",
                params![summary, id.to_string()],
            )?;
            Ok(())
        }

        pub fn set_share(&self, id: Uuid, token: &str, download: bool, expires: Option<DateTime<Utc>>) -> Result<()> {
            self.conn.execute(
                "UPDATE recordings SET share_token=?1, share_download_enabled=?2, share_expires_at=?3 WHERE id=?4",
                params![
                    token,
                    download as i32,
                    expires.map(|d| d.to_rfc3339()),
                    id.to_string()
                ],
            )?;
            Ok(())
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub use native::RecordingDb;

#[cfg(target_arch = "wasm32")]
mod web {
    use super::*;
    use rexie::{Rexie, ObjectStore, TransactionMode};

    pub struct RecordingDb {
        _db: std::sync::Arc<tokio::sync::Mutex<Option<Rexie>>>,
    }

    impl RecordingDb {
        pub async fn open(_db_path: PathBuf) -> Result<Self> {
            let db = Rexie::builder("phantom_db")
                .version(1)
                .add_object_store(ObjectStore::new("recordings").key_path("id"))
                .build()
                .await
                .map_err(|e| anyhow::anyhow!("Rexie error: {e}"))?;
            Ok(Self {
                _db: std::sync::Arc::new(tokio::sync::Mutex::new(Some(db))),
            })
        }

        pub async fn insert(&self, _r: &Recording) -> Result<()> {
            Ok(())
        }

        pub async fn list_all(&self) -> Result<Vec<Recording>> {
            Ok(vec![])
        }

        pub async fn search(&self, _query: &str) -> Result<Vec<Recording>> {
            Ok(vec![])
        }

        pub async fn update_summary(&self, _id: Uuid, _summary: &str) -> Result<()> {
            Ok(())
        }

        pub async fn set_share(&self, _id: Uuid, _token: &str, _download: bool, _expires: Option<DateTime<Utc>>) -> Result<()> {
            Ok(())
        }
    }
}

#[cfg(target_arch = "wasm32")]
pub use web::RecordingDb;
