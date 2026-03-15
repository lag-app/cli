// Copyright (c) 2026 Lag
// SPDX-License-Identifier: MIT

use anyhow::Result;
use serde::de::DeserializeOwned;
use serde::Serialize;
use std::path::{Path, PathBuf};

const CONFIG_DIR_NAME: &str = "lag";

/// Returns the platform config directory for Lag (`~/.config/lag` on Linux, etc.).
pub fn config_dir() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(CONFIG_DIR_NAME)
}

/// Load a JSON file into `T`, returning `T::default()` if the file is missing or invalid.
pub fn load_json<T: DeserializeOwned + Default>(path: &Path) -> T {
    match std::fs::read_to_string(path) {
        Ok(contents) => serde_json::from_str(&contents).unwrap_or_default(),
        Err(_) => T::default(),
    }
}

/// Serialize `value` as pretty JSON and write it to `path`, creating parent directories.
pub fn save_json<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_string_pretty(value)?;
    std::fs::write(path, json)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;

    #[test]
    fn config_dir_ends_with_lag() {
        let dir = config_dir();
        assert_eq!(dir.file_name().unwrap(), "lag");
    }

    #[test]
    fn load_json_missing_file_returns_default() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nonexistent.json");
        let val: serde_json::Value = load_json(&path);
        assert_eq!(val, serde_json::Value::Null);
    }

    #[test]
    fn load_json_valid_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.json");
        std::fs::write(&path, r#"{"key":"value"}"#).unwrap();
        let val: serde_json::Value = load_json(&path);
        assert_eq!(val["key"], "value");
    }

    #[test]
    fn load_json_invalid_json_returns_default() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("bad.json");
        std::fs::write(&path, "not json {{{").unwrap();
        let val: serde_json::Value = load_json(&path);
        assert_eq!(val, serde_json::Value::Null);
    }

    #[test]
    fn save_json_creates_parent_dirs() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a").join("b").join("c").join("file.json");
        save_json(&path, &serde_json::json!({"nested": true})).unwrap();
        assert!(path.exists());
    }

    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
    struct TestConfig {
        name: String,
        count: u32,
    }

    #[test]
    fn save_json_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("roundtrip.json");
        let original = TestConfig {
            name: "test".into(),
            count: 42,
        };
        save_json(&path, &original).unwrap();
        let loaded: TestConfig = load_json(&path);
        assert_eq!(original, loaded);
    }

    #[test]
    fn save_json_overwrites_existing() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("overwrite.json");
        save_json(&path, &serde_json::json!({"v": 1})).unwrap();
        save_json(&path, &serde_json::json!({"v": 2})).unwrap();
        let val: serde_json::Value = load_json(&path);
        assert_eq!(val["v"], 2);
    }

    #[test]
    fn save_json_pretty_format() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("pretty.json");
        save_json(&path, &serde_json::json!({"a": 1, "b": 2})).unwrap();
        let contents = std::fs::read_to_string(&path).unwrap();
        assert!(contents.contains('\n'));
    }
}
