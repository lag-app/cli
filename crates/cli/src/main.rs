// Copyright (c) 2026 Lag
// SPDX-License-Identifier: MIT
#![allow(unexpected_cfgs)]

mod api;
mod auth;
mod cli;
mod commands;
mod config;
mod tui;
mod ws;

use clap::Parser;
use cli::Cli;

/// Initialize a minimal macOS application context so WebRTC's
/// VideoToolbox codec enumeration doesn't crash.
#[cfg(target_os = "macos")]
#[macro_use]
extern crate objc;

#[cfg(target_os = "macos")]
#[allow(deprecated, unexpected_cfgs)]
fn init_macos_app_context() {
    use cocoa::appkit::NSApp;
    unsafe {
        let app = NSApp();
        // Shared application must exist but we don't need to run the event loop.
        // Just creating it is enough for VideoToolbox to work.
        let _: () = msg_send![app, setActivationPolicy: 1i64]; // NSApplicationActivationPolicyAccessory
    }
}

#[cfg(not(target_os = "macos"))]
fn init_macos_app_context() {}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    init_macos_app_context();

    let cli = Cli::parse();

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("error")),
        )
        .with_writer(std::io::stderr)
        .init();

    cli.run().await
}
