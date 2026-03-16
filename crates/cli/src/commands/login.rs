// Copyright (c) 2026 Lag
// SPDX-License-Identifier: MIT

use crate::api::ApiClient;
use crate::auth;
use crate::config;
use anyhow::Result;

pub async fn run() -> Result<()> {
    if let Some(creds) = config::load_credentials() {
        if !auth::is_token_expired(&creds.access_token) {
            println!("Already logged in. Use `lag logout` first to switch accounts.");
            return Ok(());
        }
        // Access token expired — try refreshing
        match auth::refresh_token(&creds.refresh_token).await {
            Ok(_) => {
                println!("Session refreshed. You are logged in.");
                return Ok(());
            }
            Err(_) => {
                let _ = config::clear_credentials();
                println!("Session expired. Logging in again...");
            }
        }
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
    let creds = auth::ensure_auth().await?;
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
