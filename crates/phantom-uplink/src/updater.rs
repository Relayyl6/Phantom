//! Self-hosted auto-updater — checks Supabase Storage for new releases.
//!
//! Manifest format (`latest.json` in the `phantom-releases` bucket):
//! {
//!   "version": "0.2.0",
//!   "platforms": {
//!     "windows-x86_64": {
//!       "url": "https://…/phantom-0.2.0-windows-x86_64.msi",
//!       "sha256": "abc123…",
//!       "size_bytes": 45000000
//!     },
//!     "macos-aarch64": { … },
//!     "macos-x86_64":  { … }
//!   },
//!   "changelog": "- Added Live AI Assistant\n- Fixed audio sync"
//! }

use anyhow::Result;
use reqwest::Client;
use semver::Version;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::collections::HashMap;


/// Current app version — set by Cargo at compile time.
pub const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Debug, Deserialize)]
pub struct ReleaseManifest {
    pub version: String,
    pub platforms: HashMap<String, PlatformRelease>,
    pub changelog: String,
}

#[derive(Debug, Deserialize)]
pub struct PlatformRelease {
    pub url: String,
    pub sha256: String,
    pub size_bytes: u64,
}

/// Result of an update check.
#[derive(Debug)]
pub enum UpdateCheckResult {
    /// Already on the latest version.
    UpToDate,
    /// A new version is available.
    UpdateAvailable {
        new_version: String,
        changelog: String,
        release: PlatformRelease,
    },
}

pub struct Updater {
    http: Client,
    manifest_url: String,
}

impl Updater {
    pub fn new() -> Self {
        let base_url = std::env::var("SUPABASE_URL")
            .unwrap_or_else(|_| "https://YOUR_PROJECT.supabase.co".to_owned());
        let manifest_url = format!("{}/storage/v1/object/public/phantom-releases/latest.json", base_url);
        
        Self { 
            http: Client::new(),
            manifest_url,
        }
    }

    /// Detect current platform string (matches manifest keys).
    pub fn platform_key() -> &'static str {
        #[cfg(all(target_os = "windows", target_arch = "x86_64"))]
        return "windows-x86_64";
        #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
        return "macos-aarch64";
        #[cfg(all(target_os = "macos", target_arch = "x86_64"))]
        return "macos-x86_64";
        #[cfg(not(any(target_os = "windows", target_os = "macos")))]
        return "unknown";
    }

    /// Fetch the manifest and compare against the running version.
    pub async fn check(&self) -> Result<UpdateCheckResult> {
        let manifest: ReleaseManifest = self.http
            .get(&self.manifest_url)
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;

        let current = Version::parse(CURRENT_VERSION)?;
        let latest  = Version::parse(&manifest.version)?;

        if latest <= current {
            return Ok(UpdateCheckResult::UpToDate);
        }

        let platform = Self::platform_key();
        let release = manifest.platforms
            .into_iter()
            .find(|(k, _)| k == platform)
            .map(|(_, r)| r)
            .ok_or_else(|| anyhow::anyhow!("No release for platform: {platform}"))?;

        Ok(UpdateCheckResult::UpdateAvailable {
            new_version: manifest.version,
            changelog: manifest.changelog,
            release,
        })
    }

    /// Download the installer, verify SHA256, and run it.
    pub async fn download_and_install(
        &self,
        release: &PlatformRelease,
        progress_cb: impl Fn(f32),
    ) -> Result<()> {
        use tokio::io::AsyncWriteExt;

        tracing::info!(url = release.url, "Downloading update");
        let mut resp = self.http.get(&release.url).send().await?.error_for_status()?;
        let total = release.size_bytes as f64;
        let mut downloaded: u64 = 0;

        let tmp_path = std::env::temp_dir().join("phantom_update_installer");
        let mut file = tokio::fs::File::create(&tmp_path).await?;
        let mut hasher = Sha256::new();

        while let Some(chunk) = resp.chunk().await? {
            hasher.update(&chunk);
            file.write_all(&chunk).await?;
            downloaded += chunk.len() as u64;
            progress_cb((downloaded as f64 / total) as f32);
        }
        file.flush().await?;

        let digest = hex::encode(hasher.finalize());
        if digest != release.sha256 {
            anyhow::bail!("SHA256 mismatch — download may be corrupted. Expected {}, got {digest}", release.sha256);
        }
        tracing::info!("SHA256 verified — launching installer");

        // Launch the installer. On Windows this is an MSI; on macOS a DMG.
        #[cfg(target_os = "windows")]
        std::process::Command::new("msiexec")
            .args(["/i", tmp_path.to_str().unwrap(), "/passive"])
            .spawn()?;

        #[cfg(target_os = "macos")]
        std::process::Command::new("open")
            .arg(&tmp_path)
            .spawn()?;

        Ok(())
    }
}

impl Default for Updater {
    fn default() -> Self { Self::new() }
}
