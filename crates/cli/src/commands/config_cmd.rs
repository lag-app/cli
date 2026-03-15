// Copyright (c) 2026 Lag
// SPDX-License-Identifier: MIT

use crate::cli::ConfigAction;
use crate::config::{self, CliConfig};
use anyhow::Result;

pub async fn run(action: Option<ConfigAction>) -> Result<()> {
    match action {
        None => show_config(),
        Some(ConfigAction::Set { key, value }) => set_config(&key, &value),
        Some(ConfigAction::Reset) => reset_config(),
    }
}

fn show_config() -> Result<()> {
    let cfg = config::load_config();
    let mode = if cfg!(debug_assertions) {
        "dev"
    } else {
        "release"
    };
    println!("Configuration ({} mode):\n", mode);
    println!(
        "  api-url:       {} {}",
        cfg.effective_api_url(),
        if cfg.api_url.is_none() {
            "(default)"
        } else {
            "(custom)"
        }
    );
    println!(
        "  ptt-key:       {}",
        cfg.ptt_key.as_deref().unwrap_or("(none)")
    );
    println!("  vad-threshold: {}", (cfg.vad_threshold * 100.0) as u32);
    println!(
        "\nConfig file: {}",
        config::config_dir().join("config.json").display()
    );
    Ok(())
}

fn set_config(key: &str, value: &str) -> Result<()> {
    let mut cfg = config::load_config();

    match key {
        "api-url" => {
            cfg.api_url = Some(value.to_string());
            println!("api-url set to {}", value);
        }
        "ptt-key" => {
            cfg.ptt_key = Some(value.to_string());
            println!("ptt-key set to {}", value);
        }
        "vad-threshold" => {
            let val: f32 = value
                .parse()
                .map_err(|_| anyhow::anyhow!("vad-threshold must be a number"))?;
            cfg.vad_threshold = val.clamp(0.0, 1.0);
            println!("vad-threshold set to {}", cfg.vad_threshold);
        }
        _ => {
            anyhow::bail!(
                "Unknown config key '{}'. Valid keys: api-url, ptt-key, vad-threshold",
                key
            );
        }
    }

    config::save_config(&cfg)?;
    Ok(())
}

fn reset_config() -> Result<()> {
    let cfg = CliConfig::default();
    config::save_config(&cfg)?;
    println!("Configuration reset to defaults.");
    Ok(())
}
