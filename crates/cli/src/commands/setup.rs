// Copyright (c) 2026 Lag
// SPDX-License-Identifier: MIT

use anyhow::Result;
use lag_voice_core::{AudioEngine, AudioSettings};
use crate::config;
use std::io::{self, Write};

fn read_line() -> String {
    let mut input = String::new();
    io::stdin().read_line(&mut input).unwrap_or_default();
    input.trim().to_string()
}

fn print_step(step: u32, total: u32, title: &str) {
    println!("\n--- Step {}/{}: {} ---\n", step, total, title);
}

pub async fn run() -> Result<()> {
    println!("Welcome to Lag audio setup!\n");
    println!("This will walk you through configuring your microphone and speakers.");

    let settings_dir = config::config_dir();
    let mut settings = AudioSettings::load(&settings_dir);
    let total_steps = 4;

    // Step 1: Check microphone permission
    print_step(1, total_steps, "Microphone access");

    let engine = AudioEngine::new();
    let inputs = engine.list_input_devices();

    if inputs.is_empty() {
        println!("  No input devices found.");
        println!();
        if cfg!(target_os = "macos") {
            println!("  On macOS, your terminal app needs microphone permission:");
            println!("  1. Open System Settings > Privacy & Security > Microphone");
            println!("  2. Enable your terminal app (Terminal, iTerm2, etc.)");
            println!("  3. Restart your terminal and run `lag setup` again");
        } else {
            println!("  Ensure a microphone is connected and accessible.");
        }
        println!();
        print!("Continue anyway? [y/N] ");
        io::stdout().flush()?;
        let answer = read_line();
        if !answer.eq_ignore_ascii_case("y") {
            println!("Setup cancelled.");
            return Ok(());
        }
    } else {
        // Try to actually open the device to verify permission
        let mut test_engine = AudioEngine::new();
        match test_engine.start_capture() {
            Ok(()) => {
                let _ = test_engine.stop_capture();
                println!("  Microphone access: OK");
            }
            Err(_) => {
                println!("  Microphone detected but access denied.");
                println!();
                if cfg!(target_os = "macos") {
                    println!("  Grant microphone permission to your terminal app:");
                    println!("  1. Open System Settings > Privacy & Security > Microphone");
                    println!("  2. Enable your terminal app (Terminal, iTerm2, etc.)");
                    println!("  3. Restart your terminal and run `lag setup` again");
                }
                println!();
                print!("Continue anyway? (voice will be listen-only) [y/N] ");
                io::stdout().flush()?;
                let answer = read_line();
                if !answer.eq_ignore_ascii_case("y") {
                    println!("Setup cancelled.");
                    return Ok(());
                }
            }
        }
    }

    // Step 2: Select input device
    print_step(2, total_steps, "Select microphone");

    let inputs = engine.list_input_devices();
    if inputs.is_empty() {
        println!("  No input devices available, skipping.");
    } else {
        for (i, dev) in inputs.iter().enumerate() {
            let marker = if dev.is_default { " (default)" } else { "" };
            println!("  [{}] {}{}", i + 1, dev.name, marker);
        }
        println!();
        print!("Choose microphone [enter for default]: ");
        io::stdout().flush()?;
        let choice = read_line();
        if let Ok(idx) = choice.parse::<usize>() {
            if idx >= 1 && idx <= inputs.len() {
                settings.input_device = Some(inputs[idx - 1].name.clone());
                println!("  Selected: {}", inputs[idx - 1].name);
            }
        } else if choice.is_empty() {
            settings.input_device = None;
            println!("  Using default microphone.");
        }
    }

    // Step 3: Select output device
    print_step(3, total_steps, "Select speakers");

    let outputs = engine.list_output_devices();
    if outputs.is_empty() {
        println!("  No output devices available, skipping.");
    } else {
        for (i, dev) in outputs.iter().enumerate() {
            let marker = if dev.is_default { " (default)" } else { "" };
            println!("  [{}] {}{}", i + 1, dev.name, marker);
        }
        println!();
        print!("Choose speakers [enter for default]: ");
        io::stdout().flush()?;
        let choice = read_line();
        if let Ok(idx) = choice.parse::<usize>() {
            if idx >= 1 && idx <= outputs.len() {
                settings.output_device = Some(outputs[idx - 1].name.clone());
                println!("  Selected: {}", outputs[idx - 1].name);
            }
        } else if choice.is_empty() {
            settings.output_device = None;
            println!("  Using default speakers.");
        }
    }

    // Step 4: Test audio
    print_step(4, total_steps, "Test audio");

    print!("Record a 3-second test? [Y/n] ");
    io::stdout().flush()?;
    let answer = read_line();

    if answer.is_empty() || answer.eq_ignore_ascii_case("y") {
        let mut test_engine = AudioEngine::new();
        test_engine.set_input_volume(settings.input_volume);
        test_engine.set_output_volume(settings.output_volume);
        if let Some(ref dev) = settings.input_device {
            let _ = test_engine.set_input_device(dev);
        }
        if let Some(ref dev) = settings.output_device {
            let _ = test_engine.set_output_device(dev);
        }

        match test_engine.start_capture() {
            Ok(()) => {
                println!("  Recording for 3 seconds... speak now!");
                tokio::time::sleep(std::time::Duration::from_secs(3)).await;

                let mut recorded = Vec::new();
                let mut buf = vec![0.0f32; 4800];
                loop {
                    let count = test_engine.read_capture_buffer(&mut buf);
                    if count == 0 { break; }
                    recorded.extend_from_slice(&buf[..count]);
                }
                let _ = test_engine.stop_capture();

                let duration_ms = (recorded.len() as f64 / 48000.0 * 1000.0) as u64;
                println!("  Playing back {}ms of audio...", duration_ms);

                if test_engine.start_playback().is_ok() {
                    test_engine.write_playback_buffer(&recorded);
                    tokio::time::sleep(std::time::Duration::from_millis(duration_ms + 500)).await;
                    let _ = test_engine.stop_playback();
                    println!("  Done.");
                } else {
                    println!("  Playback failed. Check your output device.");
                }
            }
            Err(_) => {
                println!("  Cannot access microphone. Skipping test.");
            }
        }
    } else {
        println!("  Skipped.");
    }

    // Save
    settings.save(&settings_dir)?;
    println!("\nSetup complete! Settings saved.");
    println!("Run `lag ui` to start chatting.");

    Ok(())
}
