// Copyright (c) 2026 Lag
// SPDX-License-Identifier: MIT

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

const CONFIG_FILENAME: &str = "config.json";
const CREDENTIALS_FILENAME: &str = "credentials.json";

const DEFAULT_API_URL: &str = "https://api.trylag.com";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CliConfig {
    pub api_url: Option<String>,
    pub ptt_key: Option<String>,
    pub vad_threshold: f32,
}

impl Default for CliConfig {
    fn default() -> Self {
        Self {
            api_url: None,
            ptt_key: None,
            vad_threshold: 0.01,
        }
    }
}

impl CliConfig {
    pub fn effective_api_url(&self) -> String {
        self.api_url.clone().unwrap_or_else(|| DEFAULT_API_URL.to_string())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Credentials {
    pub access_token: String,
    pub refresh_token: String,
}

pub fn config_dir() -> PathBuf {
    lag_common::config_dir()
}

pub fn load_config() -> CliConfig {
    let path = config_dir().join(CONFIG_FILENAME);
    lag_common::load_json(&path)
}

pub fn save_config(config: &CliConfig) -> Result<()> {
    let path = config_dir().join(CONFIG_FILENAME);
    lag_common::save_json(&path, config)
}

pub fn load_credentials() -> Option<Credentials> {
    let path = config_dir().join(CREDENTIALS_FILENAME);
    let contents = std::fs::read_to_string(&path).ok()?;
    serde_json::from_str(&contents).ok()
}

pub fn save_credentials(creds: &Credentials) -> Result<()> {
    let json = serde_json::to_string(creds)?;
    let dir = config_dir();
    std::fs::create_dir_all(&dir)?;
    let path = dir.join(CREDENTIALS_FILENAME);
    std::fs::write(&path, &json)?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;
    }

    Ok(())
}

pub fn clear_credentials() -> Result<()> {
    let path = config_dir().join(CREDENTIALS_FILENAME);
    if path.exists() {
        std::fs::remove_file(&path)?;
    }
    Ok(())
}
