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
    #[serde(default = "default_true")]
    pub notifications_enabled: bool,
}

fn default_true() -> bool {
    true
}

impl Default for CliConfig {
    fn default() -> Self {
        Self {
            api_url: None,
            ptt_key: None,
            vad_threshold: 0.01,
            notifications_enabled: true,
        }
    }
}

impl CliConfig {
    pub fn effective_api_url(&self) -> String {
        self.api_url
            .clone()
            .unwrap_or_else(|| DEFAULT_API_URL.to_string())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Credentials {
    #[serde(default)]
    pub access_token: String,
    #[serde(default)]
    pub refresh_token: String,
    #[serde(default)]
    pub pat: Option<String>,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_values() {
        let config = CliConfig::default();
        assert!(config.api_url.is_none());
        assert!(config.ptt_key.is_none());
        assert!((config.vad_threshold - 0.01).abs() < f32::EPSILON);
    }

    #[test]
    fn effective_api_url_default() {
        let config = CliConfig::default();
        assert_eq!(config.effective_api_url(), "https://api.trylag.com");
    }

    #[test]
    fn effective_api_url_custom() {
        let config = CliConfig {
            api_url: Some("https://custom.example.com".to_string()),
            ..Default::default()
        };
        assert_eq!(config.effective_api_url(), "https://custom.example.com");
    }

    #[test]
    fn credentials_serde_roundtrip() {
        let creds = Credentials {
            access_token: "access_123".to_string(),
            refresh_token: "refresh_456".to_string(),
            pat: None,
        };
        let json = serde_json::to_string(&creds).unwrap();
        let restored: Credentials = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.access_token, "access_123");
        assert_eq!(restored.refresh_token, "refresh_456");
        assert!(restored.pat.is_none());
    }

    #[test]
    fn credentials_serde_with_pat() {
        let creds = Credentials {
            access_token: String::new(),
            refresh_token: String::new(),
            pat: Some("lag_pat_abcd1234_secret".to_string()),
        };
        let json = serde_json::to_string(&creds).unwrap();
        let restored: Credentials = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.pat.as_deref(), Some("lag_pat_abcd1234_secret"));
        assert!(restored.access_token.is_empty());
    }

    #[test]
    fn credentials_backward_compat_no_pat_field() {
        // Old credential files without the pat field should deserialize fine
        let json = r#"{"access_token":"abc","refresh_token":"def"}"#;
        let creds: Credentials = serde_json::from_str(json).unwrap();
        assert_eq!(creds.access_token, "abc");
        assert_eq!(creds.refresh_token, "def");
        assert!(creds.pat.is_none());
    }

    #[test]
    fn cli_config_serde_roundtrip() {
        let config = CliConfig {
            api_url: Some("https://example.com".to_string()),
            ptt_key: Some("KeyV".to_string()),
            vad_threshold: 0.05,
            notifications_enabled: true,
        };
        let json = serde_json::to_string(&config).unwrap();
        let restored: CliConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.api_url, config.api_url);
        assert_eq!(restored.ptt_key, config.ptt_key);
        assert!((restored.vad_threshold - 0.05).abs() < f32::EPSILON);
    }

    #[test]
    fn config_dir_returns_path() {
        let dir = config_dir();
        assert!(!dir.as_os_str().is_empty());
        assert_eq!(dir.file_name().unwrap(), "lag");
    }

    #[test]
    fn default_api_url_is_production() {
        assert_eq!(DEFAULT_API_URL, "https://api.trylag.com");
    }
}
