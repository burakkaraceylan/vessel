use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};
use tracing::{debug, info, warn};

use super::oauth::TokenResponse;

#[derive(Debug, Serialize, Deserialize)]
pub struct CachedToken {
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub expires_at: u64,
}

impl CachedToken {
    pub fn is_expired(&self) -> bool {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        // Consider expired 60s early to avoid edge cases
        now >= self.expires_at.saturating_sub(60)
    }
}

fn token_path() -> Result<PathBuf> {
    let dir = dirs::data_local_dir()
        .context("Could not determine local app data directory")?
        .join("vessel");
    Ok(dir.join("discord_token.json"))
}

pub fn save(token: &TokenResponse) -> Result<()> {
    let path = token_path()?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .context("Failed to create vessel data directory")?;
    }

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();

    let cached = CachedToken {
        access_token: token.access_token.clone(),
        refresh_token: token.refresh_token.clone(),
        expires_at: now + token.expires_in,
    };

    let json = serde_json::to_string_pretty(&cached)?;
    std::fs::write(&path, json).context("Failed to write token cache")?;
    info!("Token cached to {}", path.display());
    Ok(())
}

pub fn load() -> Result<Option<CachedToken>> {
    let path = token_path()?;
    if !path.exists() {
        debug!("No cached token at {}", path.display());
        return Ok(None);
    }

    let data = std::fs::read_to_string(&path).context("Failed to read token cache")?;
    match serde_json::from_str::<CachedToken>(&data) {
        Ok(cached) => {
            debug!("Loaded cached token (expires_at={})", cached.expires_at);
            Ok(Some(cached))
        }
        Err(e) => {
            warn!("Corrupt token cache, removing: {}", e);
            let _ = std::fs::remove_file(&path);
            Ok(None)
        }
    }
}

pub fn clear() -> Result<()> {
    let path = token_path()?;
    if path.exists() {
        std::fs::remove_file(&path).context("Failed to remove token cache")?;
        info!("Token cache cleared");
    }
    Ok(())
}
