// Copyright (c) 2026 Lag
// SPDX-License-Identifier: MIT

use crate::api::ApiClient;
use crate::auth;
use crate::config;
use anyhow::Result;

pub async fn run() -> Result<()> {
    if config::load_credentials().is_some() {
        println!("Already logged in. Use `lag logout` first to switch accounts.");
        return Ok(());
    }

    auth::login_flow().await?;
    Ok(())
}

pub async fn logout() -> Result<()> {
    config::clear_credentials()?;
    println!("Logged out.");
    Ok(())
}

pub async fn whoami() -> Result<()> {
    let creds = auth::require_auth()?;
    let mut api = ApiClient::new(creds)?;

    let user: serde_json::Value = api.get("/users/me").await?;

    let username = user["username"].as_str().unwrap_or("unknown");
    let display_name = user["displayName"].as_str();
    let status = user["status"].as_str().unwrap_or("offline");

    if let Some(name) = display_name {
        println!("{} ({}) - {}", name, username, status);
    } else {
        println!("{} - {}", username, status);
    }

    Ok(())
}
