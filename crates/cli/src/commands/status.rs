// Copyright (c) 2026 Lag
// SPDX-License-Identifier: MIT

use crate::api::ApiClient;
use crate::auth;
use crate::ws::{WsClient, WsClientMessage};
use anyhow::Result;

pub async fn run(status: Option<String>) -> Result<()> {
    let creds = auth::ensure_auth().await?;

    match status {
        Some(s) => {
            let valid = matches!(s.as_str(), "online" | "idle");
            if !valid {
                anyhow::bail!("Status must be 'online' or 'idle'.");
            }

            let api = ApiClient::new(creds.clone())?;
            let ws = WsClient::connect(api.base_url(), api.access_token()).await?;
            ws.send(WsClientMessage::SetStatus { status: s.clone() })?;

            // Give the WS message time to deliver
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
            println!("Status set to {}.", s);
        }
        None => {
            let mut api = ApiClient::new(creds)?;
            let user: serde_json::Value = api.get("/users/me").await?;
            let current = user["status"].as_str().unwrap_or("offline");
            println!("Current status: {}", current);
        }
    }

    Ok(())
}
