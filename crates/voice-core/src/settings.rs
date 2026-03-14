// Copyright (c) 2026 Lag
// SPDX-License-Identifier: MIT

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tracing::info;

const SETTINGS_FILENAME: &str = "audio-settings.json";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioSettings {
    pub input_device: Option<String>,
    pub output_device: Option<String>,
    pub input_volume: f32,
    pub output_volume: f32,
    pub ptt_enabled: bool,
    pub ptt_key: Option<String>,
    pub vad_threshold: f32,
}

impl Default for AudioSettings {
    fn default() -> Self {
        Self {
            input_device: None,
            output_device: None,
            input_volume: 1.0,
            output_volume: 1.0,
            ptt_enabled: false,
            ptt_key: None,
            vad_threshold: 0.01,
        }
    }
}

impl AudioSettings {
    pub fn clamp_volumes(&mut self) {
        self.input_volume = self.input_volume.clamp(0.0, 1.0);
        self.output_volume = self.output_volume.clamp(0.0, 1.0);
        self.vad_threshold = self.vad_threshold.clamp(0.0, 1.0);
    }

    fn settings_path(app_data_dir: &Path) -> PathBuf {
        app_data_dir.join(SETTINGS_FILENAME)
    }

    pub fn load(app_data_dir: &Path) -> Self {
        let path = Self::settings_path(app_data_dir);
        let mut settings: Self = lag_common::load_json(&path);
        settings.clamp_volumes();
        settings
    }

    pub fn save(&self, app_data_dir: &Path) -> Result<()> {
        let path = Self::settings_path(app_data_dir);
        lag_common::save_json(&path, self)?;
        info!(?path, "Audio settings saved");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn default_settings_are_valid() {
        let s = AudioSettings::default();
        assert_eq!(s.input_volume, 1.0);
        assert_eq!(s.output_volume, 1.0);
        assert!(!s.ptt_enabled);
        assert!(s.ptt_key.is_none());
        assert!(s.input_device.is_none());
        assert!(s.output_device.is_none());
    }

    #[test]
    fn serialize_deserialize_roundtrip() {
        let original = AudioSettings {
            input_device: Some("Mic 1".into()),
            output_device: Some("Speakers".into()),
            input_volume: 0.75,
            output_volume: 0.5,
            ptt_enabled: true,
            ptt_key: Some("KeyV".into()),
            vad_threshold: 0.02,
        };
        let json = serde_json::to_string(&original).unwrap();
        let restored: AudioSettings = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.input_device, original.input_device);
        assert_eq!(restored.output_device, original.output_device);
        assert_eq!(restored.input_volume, original.input_volume);
        assert_eq!(restored.output_volume, original.output_volume);
        assert_eq!(restored.ptt_enabled, original.ptt_enabled);
        assert_eq!(restored.ptt_key, original.ptt_key);
        assert_eq!(restored.vad_threshold, original.vad_threshold);
    }

    #[test]
    fn load_nonexistent_returns_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let settings = AudioSettings::load(dir.path());
        assert_eq!(settings.input_volume, 1.0);
        assert_eq!(settings.output_volume, 1.0);
    }

    #[test]
    fn save_and_load_preserves_values() {
        let dir = tempfile::tempdir().unwrap();
        let original = AudioSettings {
            input_device: Some("Test Mic".into()),
            output_device: None,
            input_volume: 0.3,
            output_volume: 0.8,
            ptt_enabled: true,
            ptt_key: Some("KeyG".into()),
            vad_threshold: 0.05,
        };
        original.save(dir.path()).unwrap();
        let loaded = AudioSettings::load(dir.path());
        assert_eq!(loaded.input_device, original.input_device);
        assert_eq!(loaded.input_volume, original.input_volume);
        assert_eq!(loaded.ptt_key, original.ptt_key);
    }

    #[test]
    fn invalid_json_returns_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(SETTINGS_FILENAME);
        fs::write(&path, "not valid json {{{").unwrap();
        let settings = AudioSettings::load(dir.path());
        assert_eq!(settings.input_volume, 1.0);
    }

    #[test]
    fn volumes_clamped() {
        let mut s = AudioSettings {
            input_volume: 5.0,
            output_volume: -1.0,
            vad_threshold: 2.0,
            ..Default::default()
        };
        s.clamp_volumes();
        assert_eq!(s.input_volume, 1.0);
        assert_eq!(s.output_volume, 0.0);
        assert_eq!(s.vad_threshold, 1.0);
    }
}