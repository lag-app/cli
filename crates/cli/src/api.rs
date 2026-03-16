// Copyright (c) 2026 Lag
// SPDX-License-Identifier: MIT

use crate::auth;
use crate::config::{self, Credentials};
use anyhow::{anyhow, Result};
use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION};
use serde::de::DeserializeOwned;
use std::time::{Duration, Instant};

// Refresh 5 minutes before expiry to avoid hitting 401
const REFRESH_BUFFER: Duration = Duration::from_secs(300);
// Supabase default token lifetime
const TOKEN_LIFETIME: Duration = Duration::from_secs(28800);

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
            token_acquired_at: None, // unknown age — will refresh on first API call
        })
    }

    fn auth_headers(&self) -> HeaderMap {
        let mut headers = HeaderMap::new();
        if let Ok(val) = HeaderValue::from_str(&format!("Bearer {}", self.creds.access_token)) {
            headers.insert(AUTHORIZATION, val);
        }
        headers
    }

    /// Proactively refresh if the token is known to be near expiry.
    /// For unknown-age tokens (loaded from disk), skip proactive refresh
    /// and let the 401 retry handle it — avoids wiping fresh tokens.
    async fn ensure_fresh_token(&mut self) -> Result<()> {
        if let Some(acquired) = self.token_acquired_at {
            if acquired.elapsed() + REFRESH_BUFFER >= TOKEN_LIFETIME {
                self.do_refresh().await?;
            }
        }
        // If token_acquired_at is None, we don't know the age.
        // Try the token as-is; the 401 handler in get/post will refresh if needed.
        Ok(())
    }

    async fn do_refresh(&mut self) -> Result<()> {
        let refreshed = auth::refresh_token(&self.creds.refresh_token).await?;
        self.creds = refreshed;
        self.token_acquired_at = Some(Instant::now());
        Ok(())
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
            self.do_refresh().await?;
            let resp = self
                .client
                .get(&url)
                .headers(self.auth_headers())
                .send()
                .await?;
            return parse_response(resp).await;
        }

        // Token works — mark acquisition time if unknown
        if self.token_acquired_at.is_none() {
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
            self.do_refresh().await?;
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
            self.do_refresh().await?;
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
        &self.creds.access_token
    }

    pub fn base_url(&self) -> &str {
        &self.base_url
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
