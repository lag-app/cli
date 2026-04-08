// Copyright (c) 2026 Lag
// SPDX-License-Identifier: MIT

use crate::config::{self, Credentials};
use anyhow::{anyhow, Result};
use base64::Engine;
use rand::Rng;
use url::Url;

const SUPABASE_URL: &str = "https://base.trylag.com";
const SUPABASE_ANON_KEY: &str = "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJpc3MiOiJzdXBhYmFzZSIsInJlZiI6InVmeXlsZGp5c3pjamtsYW5ucnRzIiwicm9sZSI6ImFub24iLCJpYXQiOjE3NzEzNjQxNjYsImV4cCI6MjA4Njk0MDE2Nn0.WntE5XNUuzNs5j-OnK0ZMG2sxrfPTSCGi8dgdfWlCrw";
const WEB_URL: &str = "https://trylag.com";

pub fn is_token_expired(token: &str) -> bool {
    let parts: Vec<&str> = token.split('.').collect();
    if parts.len() != 3 {
        return true;
    }
    let payload = match base64::engine::general_purpose::URL_SAFE_NO_PAD.decode(parts[1]) {
        Ok(bytes) => bytes,
        Err(_) => return true,
    };
    let json: serde_json::Value = match serde_json::from_slice(&payload) {
        Ok(v) => v,
        Err(_) => return true,
    };
    let exp = match json.get("exp").and_then(|v| v.as_i64()) {
        Some(e) => e,
        None => return true,
    };
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;
    now >= exp
}

pub async fn ensure_auth() -> Result<Credentials> {
    if let Some(creds) = config::load_credentials() {
        // PAT-based auth - no expiry to check
        if creds.pat.is_some() {
            return Ok(creds);
        }
        // Legacy JWT-based auth
        if !is_token_expired(&creds.access_token) {
            return Ok(creds);
        }
        // Access token expired - try refreshing then exchanging for PAT
        match refresh_token(&creds.refresh_token).await {
            Ok(refreshed) => {
                // Try to upgrade to PAT
                match exchange_for_pat(&refreshed.access_token).await {
                    Ok(pat_creds) => return Ok(pat_creds),
                    Err(_) => return Ok(refreshed),
                }
            }
            Err(_) => {
                let _ = config::clear_credentials();
            }
        }
    }
    login_flow().await
}

pub async fn login_flow() -> Result<Credentials> {
    let state = generate_state();
    let (port, server) = start_callback_server()?;

    let auth_url = format!("{}/auth/cli?port={}&state={}", WEB_URL, port, state,);

    println!("Opening browser for authentication...");
    println!("If the browser doesn't open, visit:\n{}\n", auth_url);

    if open::that(&auth_url).is_err() {
        eprintln!("Could not open browser automatically.");
    }

    println!("Waiting for authentication...");
    let supabase_creds = wait_for_callback(server, &state)?;

    // Exchange Supabase tokens for a long-lived PAT
    let creds = match exchange_for_pat(&supabase_creds.access_token).await {
        Ok(pat_creds) => pat_creds,
        Err(e) => {
            eprintln!(
                "Warning: Could not create long-lived token ({}). Using session token.",
                e
            );
            supabase_creds
        }
    };

    config::save_credentials(&creds)?;
    println!("Logged in successfully.");
    Ok(creds)
}

/// Exchange a short-lived Supabase JWT for a long-lived Personal Access Token.
async fn exchange_for_pat(access_token: &str) -> Result<Credentials> {
    let cfg = config::load_config();
    let api_url = cfg.effective_api_url();
    let client = reqwest::Client::new();

    let resp = client
        .post(format!("{}/tokens/cli", api_url))
        .header("Authorization", format!("Bearer {}", access_token))
        .send()
        .await?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(anyhow!("PAT creation failed ({}): {}", status, body));
    }

    let body: serde_json::Value = resp.json().await?;
    let pat = body["token"]
        .as_str()
        .ok_or_else(|| anyhow!("No token in response"))?
        .to_string();

    Ok(Credentials {
        access_token: String::new(),
        refresh_token: String::new(),
        pat: Some(pat),
    })
}

fn generate_state() -> String {
    let mut rng = rand::thread_rng();
    (0..32)
        .map(|_| format!("{:02x}", rng.gen::<u8>()))
        .collect()
}

fn start_callback_server() -> Result<(u16, tiny_http::Server)> {
    let server = tiny_http::Server::http("127.0.0.1:0")
        .map_err(|e| anyhow!("Failed to start callback server: {}", e))?;
    let port = server.server_addr().to_ip().unwrap().port();
    Ok((port, server))
}

fn wait_for_callback(server: tiny_http::Server, expected_state: &str) -> Result<Credentials> {
    let timeout = std::time::Duration::from_secs(300);
    let start = std::time::Instant::now();

    loop {
        if start.elapsed() > timeout {
            return Err(anyhow!("Authentication timed out"));
        }

        let request = match server.recv_timeout(std::time::Duration::from_secs(1)) {
            Ok(Some(req)) => req,
            Ok(None) => continue,
            Err(_) => continue,
        };

        let url_str = format!("http://localhost{}", request.url());
        let url = Url::parse(&url_str)?;

        if url.path() != "/callback" {
            let response = tiny_http::Response::from_string("Not found").with_status_code(404);
            let _ = request.respond(response);
            continue;
        }

        let params: std::collections::HashMap<String, String> = url
            .query_pairs()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();

        if let (Some(access_token), Some(refresh_token)) =
            (params.get("access_token"), params.get("refresh_token"))
        {
            // Verify state to prevent CSRF
            if let Some(state) = params.get("state") {
                if state != expected_state {
                    let response =
                        tiny_http::Response::from_string("Invalid state").with_status_code(400);
                    let _ = request.respond(response);
                    continue;
                }
            }

            // Redirect browser to the website's success page
            let success_url = format!("{}/auth/cli/success", WEB_URL);
            let response = tiny_http::Response::empty(302).with_header(
                format!("Location: {}", success_url)
                    .parse::<tiny_http::Header>()
                    .unwrap(),
            );
            let _ = request.respond(response);

            return Ok(Credentials {
                access_token: access_token.clone(),
                refresh_token: refresh_token.clone(),
                pat: None,
            });
        }

        // No tokens in request - return 400
        let response = tiny_http::Response::from_string("Missing tokens").with_status_code(400);
        let _ = request.respond(response);
    }
}

pub async fn refresh_token(refresh_token: &str) -> Result<Credentials> {
    let client = reqwest::Client::new();
    let resp = client
        .post(format!(
            "{}/auth/v1/token?grant_type=refresh_token",
            SUPABASE_URL
        ))
        .header("apikey", SUPABASE_ANON_KEY)
        .json(&serde_json::json!({ "refresh_token": refresh_token }))
        .send()
        .await?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();

        // Only clear credentials on definitive auth failures (400/401/403)
        // Not on transient errors (500, network, etc.)
        if status.as_u16() == 400 || status.as_u16() == 401 || status.as_u16() == 403 {
            let _ = config::clear_credentials();
            return Err(anyhow!(
                "Session expired. Run `lag login` to sign in again."
            ));
        }

        return Err(anyhow!("Token refresh failed ({}): {}", status, body));
    }

    let body: serde_json::Value = resp.json().await?;
    let access_token = body["access_token"]
        .as_str()
        .ok_or_else(|| anyhow!("No access_token in refresh response"))?
        .to_string();
    let new_refresh = body["refresh_token"]
        .as_str()
        .unwrap_or(refresh_token)
        .to_string();

    let creds = Credentials {
        access_token,
        refresh_token: new_refresh,
        pat: None,
    };
    config::save_credentials(&creds)?;
    Ok(creds)
}
