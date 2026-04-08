// Copyright (c) 2026 Lag
// SPDX-License-Identifier: MIT

use crate::auth;
use crate::config::{self, Credentials};
use anyhow::{anyhow, Result};
use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION};
use serde::de::DeserializeOwned;
use std::time::{Duration, Instant};

// Legacy JWT refresh constants (used only when PAT is not available)
const REFRESH_BUFFER: Duration = Duration::from_secs(300);
const TOKEN_LIFETIME: Duration = Duration::from_secs(3600); // Supabase default: 1 hour

pub struct ApiClient {
    client: reqwest::Client,
    base_url: String,
    creds: Credentials,
    token_acquired_at: Option<Instant>,
}

impl ApiClient {
    pub fn new(creds: Credentials) -> Result<Self> {
        let cfg = config::load_config();
        let client = reqwest::Client::new();
        Ok(Self {
            client,
            base_url: cfg.effective_api_url(),
            creds,
            token_acquired_at: None,
        })
    }

    fn auth_headers(&self) -> HeaderMap {
        let mut headers = HeaderMap::new();
        let token_str = if let Some(pat) = &self.creds.pat {
            format!("Bearer {}", pat)
        } else {
            format!("Bearer {}", self.creds.access_token)
        };
        if let Ok(val) = HeaderValue::from_str(&token_str) {
            headers.insert(AUTHORIZATION, val);
        }
        headers
    }

    fn uses_pat(&self) -> bool {
        self.creds.pat.is_some()
    }

    /// Proactively refresh if using legacy JWT and the token is known to be near expiry.
    async fn ensure_fresh_token(&mut self) -> Result<()> {
        if self.uses_pat() {
            return Ok(());
        }
        if let Some(acquired) = self.token_acquired_at {
            if acquired.elapsed() + REFRESH_BUFFER >= TOKEN_LIFETIME {
                self.do_refresh().await?;
            }
        }
        Ok(())
    }

    async fn do_refresh(&mut self) -> Result<()> {
        let refreshed = auth::refresh_token(&self.creds.refresh_token).await?;
        self.creds = refreshed;
        self.token_acquired_at = Some(Instant::now());
        Ok(())
    }

    async fn handle_unauthorized(&mut self) -> Result<()> {
        if self.uses_pat() {
            let _ = config::clear_credentials();
            return Err(anyhow!(
                "Token revoked or invalid. Run `lag login` to sign in again."
            ));
        }
        self.do_refresh().await
    }

    pub async fn get<T: DeserializeOwned>(&mut self, path: &str) -> Result<T> {
        self.ensure_fresh_token().await?;
        let url = format!("{}{}", self.base_url, path);
        let resp = self
            .client
            .get(&url)
            .headers(self.auth_headers())
            .send()
            .await?;

        if resp.status() == reqwest::StatusCode::UNAUTHORIZED {
            self.handle_unauthorized().await?;
            let resp = self
                .client
                .get(&url)
                .headers(self.auth_headers())
                .send()
                .await?;
            return parse_response(resp).await;
        }

        if self.token_acquired_at.is_none() && !self.uses_pat() {
            self.token_acquired_at = Some(Instant::now());
        }

        parse_response(resp).await
    }

    pub async fn post<B: serde::Serialize, T: DeserializeOwned>(
        &mut self,
        path: &str,
        body: &B,
    ) -> Result<T> {
        self.ensure_fresh_token().await?;
        let url = format!("{}{}", self.base_url, path);
        let resp = self
            .client
            .post(&url)
            .headers(self.auth_headers())
            .json(body)
            .send()
            .await?;

        if resp.status() == reqwest::StatusCode::UNAUTHORIZED {
            self.handle_unauthorized().await?;
            let resp = self
                .client
                .post(&url)
                .headers(self.auth_headers())
                .json(body)
                .send()
                .await?;
            return parse_response(resp).await;
        }

        parse_response(resp).await
    }

    pub async fn delete_no_body(&mut self, path: &str) -> Result<()> {
        self.ensure_fresh_token().await?;
        let url = format!("{}{}", self.base_url, path);
        let resp = self
            .client
            .delete(&url)
            .headers(self.auth_headers())
            .send()
            .await?;

        if resp.status() == reqwest::StatusCode::UNAUTHORIZED {
            self.handle_unauthorized().await?;
            let resp = self
                .client
                .delete(&url)
                .headers(self.auth_headers())
                .send()
                .await?;
            if !resp.status().is_success() {
                let status = resp.status();
                let text = resp.text().await.unwrap_or_default();
                return Err(anyhow!("Request failed ({}): {}", status, text));
            }
            return Ok(());
        }

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(anyhow!("Request failed ({}): {}", status, text));
        }
        Ok(())
    }

    pub fn access_token(&self) -> &str {
        if let Some(pat) = &self.creds.pat {
            pat
        } else {
            &self.creds.access_token
        }
    }

    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    /// Fetch a messages endpoint that returns `{"messages": [...]}` or a bare array.
    pub async fn get_messages(&mut self, path: &str) -> Result<Vec<serde_json::Value>> {
        let val: serde_json::Value = self.get(path).await?;
        Ok(extract_messages(val))
    }
}

/// Extract messages from either `{"messages": [...]}` or a bare `[...]`.
pub fn extract_messages(val: serde_json::Value) -> Vec<serde_json::Value> {
    if let Some(arr) = val.as_array() {
        arr.clone()
    } else if let Some(arr) = val["messages"].as_array() {
        arr.clone()
    } else {
        Vec::new()
    }
}

async fn parse_response<T: DeserializeOwned>(resp: reqwest::Response) -> Result<T> {
    let status = resp.status();
    if !status.is_success() {
        let text = resp.text().await.unwrap_or_default();
        if let Ok(err) = serde_json::from_str::<serde_json::Value>(&text) {
            if let Some(msg) = err.get("message").and_then(|m| m.as_str()) {
                return Err(anyhow!("{}", msg));
            }
        }
        return Err(anyhow!("Request failed ({}): {}", status, text));
    }
    let body = resp.json::<T>().await?;
    Ok(body)
}
