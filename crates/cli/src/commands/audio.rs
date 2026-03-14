// Copyright (c) 2026 Lag
// SPDX-License-Identifier: MIT

use anyhow::Result;
use lag_voice_core::{AudioEngine, AudioSettings};
use crate::cli::AudioAction;
use crate::config;


pub async fn run(action: Option<AudioAction>) -> Result<()> {
    match action {
        None => show_config(),
        Some(AudioAction::Devices) => list_devices(),
        Some(AudioAction::SetInput { name }) => set_input(&name),
        Some(AudioAction::SetOutput { name }) => set_output(&name),
        Some(AudioAction::Volume { target, level }) => set_volume(&target, level),
        Some(AudioAction::Test) => test_audio().await,
    }
}

fn show_config() -> Result<()> {
    let settings_dir = config::config_dir();
    let settings = AudioSettings::load(&settings_dir);

    println!("Audio configuration:\n");
    println!(
        "  Input device:  {}",
        settings.input_device.as_deref().unwrap_or("(default)")
    );
    println!(
        "  Output device: {}",
        settings.output_device.as_deref().unwrap_or("(default)")
    );
    println!("  Input volume:  {}%", (settings.input_volume * 100.0) as u32);
    println!(
        "  Output volume: {}%",
        (settings.output_volume * 100.0) as u32
    );
    println!("  PTT enabled:   {}", settings.ptt_enabled);
    println!(
        "  PTT key:       {}",
        settings.ptt_key.as_deref().unwrap_or("(none)")
    );
    println!(
        "  VAD threshold: {}%",
        (settings.vad_threshold * 100.0) as u32
    );

    Ok(())
}

fn list_devices() -> Result<()> {
    let engine = AudioEngine::new();

    let inputs = engine.list_input_devices();
    println!("Input devices:");
    for dev in &inputs {
        let marker = if dev.is_default { " (default)" } else { "" };
        println!("  {}{}", dev.name, marker);
    }

    println!();
    let outputs = engine.list_output_devices();
    println!("Output devices:");
    for dev in &outputs {
        let marker = if dev.is_default { " (default)" } else { "" };
        println!("  {}{}", dev.name, marker);
    }

    Ok(())
}

fn set_input(name: &str) -> Result<()> {
    let settings_dir = config::config_dir();
    let mut settings = AudioSettings::load(&settings_dir);

    // Verify device exists
    let engine = AudioEngine::new();
    let inputs = engine.list_input_devices();
    if !inputs.iter().any(|d| d.name == name) {
        anyhow::bail!("Input device '{}' not found. Run `lag audio devices` to see available devices.", name);
    }

    settings.input_device = Some(name.to_string());
    settings.save(&settings_dir)?;
    println!("Input device set to: {}", name);
    Ok(())
}

fn set_output(name: &str) -> Result<()> {
    let settings_dir = config::config_dir();
    let mut settings = AudioSettings::load(&settings_dir);

    let engine = AudioEngine::new();
    let outputs = engine.list_output_devices();
    if !outputs.iter().any(|d| d.name == name) {
        anyhow::bail!("Output device '{}' not found. Run `lag audio devices` to see available devices.", name);
    }

    settings.output_device = Some(name.to_string());
    settings.save(&settings_dir)?;
    println!("Output device set to: {}", name);
    Ok(())
}

fn set_volume(target: &str, level: u32) -> Result<()> {
    let settings_dir = config::config_dir();
    let mut settings = AudioSettings::load(&settings_dir);
    let vol = (level.min(100) as f32) / 100.0;

    match target {
        "input" => {
            settings.input_volume = vol;
            println!("Input volume set to {}%", level.min(100));
        }
        "output" => {
            settings.output_volume = vol;
            println!("Output volume set to {}%", level.min(100));
        }
        _ => anyhow::bail!("Unknown target '{}'. Use 'input' or 'output'.", target),
    }

    settings.save(&settings_dir)?;
    Ok(())
}

async fn test_audio() -> Result<()> {
    let settings_dir = config::config_dir();
    let audio_settings = AudioSettings::load(&settings_dir);

    let mut engine = AudioEngine::new();
    engine.set_input_volume(audio_settings.input_volume);
    engine.set_output_volume(audio_settings.output_volume);

    if let Some(ref dev) = audio_settings.input_device {
        engine.set_input_device(dev)?;
    }
    if let Some(ref dev) = audio_settings.output_device {
        engine.set_output_device(dev)?;
    }

    println!("Recording for 3 seconds...");
    engine.start_capture()?;

    tokio::time::sleep(std::time::Duration::from_secs(3)).await;

    // Read all captured audio
    let mut recorded = Vec::new();
    let mut buf = vec![0.0f32; 4800]; // 100ms chunks
    loop {
        let count = engine.read_capture_buffer(&mut buf);
        if count == 0 {
            break;
        }
        recorded.extend_from_slice(&buf[..count]);
    }
    engine.stop_capture()?;

    let duration_ms = (recorded.len() as f64 / 48000.0 * 1000.0) as u64;
    println!("Recorded {}ms of audio. Playing back...", duration_ms);

    engine.start_playback()?;
    engine.write_playback_buffer(&recorded);

    // Wait for playback to finish
    tokio::time::sleep(std::time::Duration::from_millis(duration_ms + 500)).await;
    engine.stop_playback()?;

    println!("Done.");
    Ok(())
}