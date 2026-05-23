//! Share link generation — free (view-only) and Pro (download + password).
//!
//! Free tier:  signed streaming URL, no download button, 15-min TTL auto-refresh.
//! Pro tier:   download enabled, optional password hash, configurable expiry.

use anyhow::Result;
use chrono::{DateTime, Duration, Utc};
use phantom_billing::entitlement::{EntitlementStore, Tier};
use sha2::{Digest, Sha256};
use uuid::Uuid;
use crate::supabase_client::SupabaseClient;

/// A generated share link.
#[derive(Debug, Clone)]
pub struct ShareLink {
    pub token: String,
    pub url: String,
    pub download_enabled: bool,
    pub expires_at: Option<DateTime<Utc>>,
}

pub struct ShareManager {
    supabase: SupabaseClient,
    entitlement: EntitlementStore,
    /// Base URL of the Phantom web share player.
    player_base_url: String,
}

impl ShareManager {
    pub fn new(
        supabase: SupabaseClient,
        entitlement: EntitlementStore,
        player_base_url: String,
    ) -> Self {
        Self { supabase, entitlement, player_base_url }
    }

    /// Generate a share link for the given recording.
    ///
    /// `recording_id`     — UUID of the recording.
    /// `storage_path`     — path in Supabase Storage (e.g. `user_id/session_id/video.mp4`).
    /// `password`         — optional password (Pro only; ignored on Free tier).
    /// `expires_in_days`  — None = never expire; only honoured on Pro tier.
    /// `allow_download`   — only meaningful on Pro; Free is always view-only.
    pub async fn generate(
        &self,
        recording_id: Uuid,
        storage_path: &str,
        password: Option<&str>,
        expires_in_days: Option<u64>,
        allow_download: bool,
    ) -> Result<ShareLink> {
        let tier = self.entitlement.current_tier().await?;

        let token = Uuid::new_v4().to_string().replace('-', "");

        // Free tier: always view-only, no password, signed URL with 15-min TTL
        //            (the share player will re-sign on each page load).
        let (download_enabled, expires_at, pw_hash) = match tier {
            Tier::Free => {
                tracing::debug!("Generating free-tier view-only share link");
                (false, None, None)
            }
            Tier::Pro | Tier::Team => {
                let exp = expires_in_days
                    .map(|d| Utc::now() + Duration::days(d as i64));
                let ph = password.map(|p| {
                    let mut h = Sha256::new();
                    h.update(p.as_bytes());
                    hex::encode(h.finalize())
                });
                (allow_download, exp, ph)
            }
        };

        // Create a signed storage URL (15-min TTL for stream; player refreshes).
        let signed_url = self.supabase
            .create_signed_url("recordings", storage_path, 900)
            .await?;
        let _ = (signed_url, pw_hash, recording_id); // will be stored in DB

        let url = format!("{}/watch/{token}", self.player_base_url);

        Ok(ShareLink {
            token,
            url,
            download_enabled,
            expires_at,
        })
    }
}
