//! Entitlement store — caches the user's billing tier locally in SQLite.
//!
//! On launch and every 24 hours, the store re-verifies with Supabase.
//! If Supabase is unreachable, the last-known tier is honoured for up
//! to 7 days (offline grace period), after which the app downgrades to Free.

use anyhow::Result;
use chrono::{DateTime, Duration, Utc};
use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Mutex;

/// Billing tier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Tier {
    Free,
    Pro,
    Team,
}

impl Tier {
    pub fn from_str(s: &str) -> Self {
        match s {
            "pro"  => Tier::Pro,
            "team" => Tier::Team,
            _      => Tier::Free,
        }
    }
    pub fn as_str(&self) -> &'static str {
        match self {
            Tier::Free => "free",
            Tier::Pro  => "pro",
            Tier::Team => "team",
        }
    }
}

/// Daily AI usage limits per tier.
impl Tier {
    pub fn daily_ai_limit(&self) -> u32 {
        match self {
            Tier::Free => 3,
            Tier::Pro | Tier::Team => u32::MAX,
        }
    }
}

struct EntitlementRow {
    tier: String,
    verified_at: DateTime<Utc>,
    daily_ai_count: u32,
    daily_ai_reset_at: DateTime<Utc>,
}

/// Thread-safe entitlement store.
#[derive(Clone)]
pub struct EntitlementStore {
    inner: Arc<Mutex<EntitlementInner>>,
}

struct EntitlementInner {
    conn: Connection,
}

impl EntitlementStore {
    /// Open (or create) the entitlement database at `db_path`.
    pub fn open(db_path: PathBuf) -> Result<Self> {
        let conn = Connection::open(&db_path)?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS entitlement (
                id               INTEGER PRIMARY KEY,
                tier             TEXT NOT NULL DEFAULT 'free',
                verified_at      TEXT NOT NULL,
                daily_ai_count   INTEGER NOT NULL DEFAULT 0,
                daily_ai_reset_at TEXT NOT NULL
            );
            INSERT OR IGNORE INTO entitlement (id, tier, verified_at, daily_ai_reset_at)
            VALUES (1, 'free', datetime('now'), datetime('now'));",
        )?;
        Ok(Self { inner: Arc::new(Mutex::new(EntitlementInner { conn })) })
    }

    /// Return the current effective tier, applying offline grace rules.
    pub async fn current_tier(&self) -> Result<Tier> {
        let inner = self.inner.lock().await;
        let row = inner.fetch_row()?;
        let tier = Tier::from_str(&row.tier);

        // If Pro/Team but last verified > 7 days ago, downgrade gracefully.
        if tier != Tier::Free {
            let age = Utc::now() - row.verified_at;
            if age > Duration::days(7) {
                tracing::warn!("Entitlement offline grace expired — downgrading to Free");
                return Ok(Tier::Free);
            }
        }
        Ok(tier)
    }

    /// Set the tier after a successful Supabase verification.
    pub async fn set_tier(&self, tier: Tier) -> Result<()> {
        let inner = self.inner.lock().await;
        inner.conn.execute(
            "UPDATE entitlement SET tier = ?1, verified_at = ?2 WHERE id = 1",
            params![tier.as_str(), Utc::now().to_rfc3339()],
        )?;
        tracing::info!(tier = tier.as_str(), "Entitlement updated");
        Ok(())
    }

    /// Check the daily AI usage cap and increment the counter.
    /// Returns an error with a user-friendly message if the cap is reached.
    pub async fn check_and_increment_daily_ai(&self) -> Result<()> {
        let inner = self.inner.lock().await;
        let row = inner.fetch_row()?;
        let tier = Tier::from_str(&row.tier);
        let limit = tier.daily_ai_limit();

        // Reset counter if it's a new day.
        let now = Utc::now();
        let (count, reset_at) = if now > row.daily_ai_reset_at {
            let tomorrow = now + Duration::days(1);
            inner.conn.execute(
                "UPDATE entitlement SET daily_ai_count = 0, daily_ai_reset_at = ?1 WHERE id = 1",
                params![tomorrow.to_rfc3339()],
            )?;
            (0u32, tomorrow)
        } else {
            (row.daily_ai_count, row.daily_ai_reset_at)
        };

        if count >= limit {
            let resets_in = reset_at - now;
            anyhow::bail!(
                "You've used all {} AI summaries for today (resets in {}h {}m). \
                 Upgrade to Phantom Pro for unlimited AI.",
                limit,
                resets_in.num_hours(),
                resets_in.num_minutes() % 60,
            );
        }

        inner.conn.execute(
            "UPDATE entitlement SET daily_ai_count = daily_ai_count + 1 WHERE id = 1",
            [],
        )?;
        tracing::debug!(used = count + 1, limit, "Daily AI usage incremented");
        Ok(())
    }
}

impl EntitlementInner {
    fn fetch_row(&self) -> Result<EntitlementRow> {
        self.conn.query_row(
            "SELECT tier, verified_at, daily_ai_count, daily_ai_reset_at FROM entitlement WHERE id = 1",
            [],
            |row| {
                Ok(EntitlementRow {
                    tier: row.get(0)?,
                    verified_at: row.get::<_, String>(1)?
                        .parse::<DateTime<Utc>>()
                        .map_err(|e| rusqlite::Error::InvalidColumnType(1, e.to_string(), rusqlite::types::Type::Text))?,
                    daily_ai_count: row.get(2)?,
                    daily_ai_reset_at: row.get::<_, String>(3)?
                        .parse::<DateTime<Utc>>()
                        .map_err(|e| rusqlite::Error::InvalidColumnType(3, e.to_string(), rusqlite::types::Type::Text))?,
                })
            },
        ).map_err(Into::into)
    }
}
