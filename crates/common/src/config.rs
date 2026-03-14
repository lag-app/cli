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
