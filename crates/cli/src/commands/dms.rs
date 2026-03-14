// Copyright (c) 2026 Lag
// SPDX-License-Identifier: MIT

use anyhow::Result;
use crate::api::ApiClient;
use crate::auth;
use crate::cli::DmsAction;
use crate::ws::{WsClient, WsServerMessage};
use crossterm::event::{self, Event, KeyCode, KeyModifiers};
use crossterm::terminal;
use std::io::Write;

pub async fn run(action: Option<DmsAction>) -> Result<()> {
    let creds = auth::require_auth()?;
    let mut api = ApiClient::new(creds)?;

    match action {
        None => list_conversations(&mut api).await,
        Some(DmsAction::Open { username }) => open_dm(&mut api, &username).await,
        Some(DmsAction::Send { username, message }) => {
            send_dm(&mut api, &username, &message).await
        }
    }
}

async fn list_conversations(api: &mut ApiClient) -> Result<()> {
    let conversations: Vec<serde_json::Value> = api.get("/dms").await?;

    if conversations.is_empty() {
        println!("No DM conversations.");
        return Ok(());
    }

    println!("DM conversations:\n");
    for conv in &conversations {
        let username = conv["otherUser"]["username"].as_str().unwrap_or("?");
        let unread = conv["unreadCount"].as_u64().unwrap_or(0);
        let last_msg = conv["lastMessage"]["content"].as_str().unwrap_or("");

        if unread > 0 {
            println!("  {} ({} new) - {}", username, unread, truncate(last_msg, 50));
        } else {
            println!("  {} - {}", username, truncate(last_msg, 50));
        }
    }

    Ok(())
}

async fn send_dm(api: &mut ApiClient, username: &str, message: &str) -> Result<()> {
    let conv = find_or_create_conversation(api, username).await?;
    let conv_id = conv["id"].as_str().unwrap();

    let _: serde_json::Value = api
        .post(
            &format!("/dms/{}/messages", conv_id),
            &serde_json::json!({ "content": message }),
        )
        .await?;

    println!("Message sent to {}.", username);
    Ok(())
}

async fn open_dm(api: &mut ApiClient, username: &str) -> Result<()> {
    let conv = find_or_create_conversation(api, username).await?;
    let conv_id = conv["id"].as_str().unwrap().to_string();

    // Load recent messages
    let messages: Vec<serde_json::Value> = api
        .get(&format!("/dms/{}/messages?limit=50", conv_id))
        .await?;

    println!("-- DM with {} --", username);
    println!("(Press Ctrl+C or Esc to exit)\n");

    // Print messages oldest-first
    for msg in messages.iter().rev() {
        print_message(msg);
    }

    // Connect WebSocket for real-time messages
    let mut ws = WsClient::connect(api.base_url(), api.access_token()).await?;

    terminal::enable_raw_mode()?;
    let _cleanup = RawModeGuard;

    let mut input_buf = String::new();
    print_prompt(&input_buf);

    loop {
        tokio::select! {
            ws_msg = ws.recv() => {
                match ws_msg {
                    Some(WsServerMessage::DmMessage(val)) => {
                        let msg_conv_id = val["conversationId"].as_str().unwrap_or("");
                        if msg_conv_id == conv_id {
                            clear_line();
                            print_message(&val);
                            print_prompt(&input_buf);
                        }
                    }
                    None => break,
                    _ => {}
                }
            }
            _ = async {
                if event::poll(std::time::Duration::from_millis(50)).unwrap_or(false) {
                    if let Ok(Event::Key(key)) = event::read() {
                        match (key.code, key.modifiers) {
                            (KeyCode::Char('c'), KeyModifiers::CONTROL) | (KeyCode::Esc, _) => {
                                return true;
                            }
                            (KeyCode::Enter, _) => {
                                if !input_buf.trim().is_empty() {
                                    let msg = input_buf.trim().to_string();
                                    input_buf.clear();
                                    clear_line();

                                    let body = serde_json::json!({ "content": msg });
                                    // Send in background - don't block input
                                    let _ = api.post::<_, serde_json::Value>(
                                        &format!("/dms/{}/messages", conv_id), &body
                                    ).await;

                                    print_prompt(&input_buf);
                                }
                            }
                            (KeyCode::Backspace, _) => {
                                input_buf.pop();
                                print_prompt(&input_buf);
                            }
                            (KeyCode::Char(c), _) => {
                                input_buf.push(c);
                                print_prompt(&input_buf);
                            }
                            _ => {}
                        }
                    }
                }
                false
            } => {
                // If the async block returned true, break
            }
        }
    }

    Ok(())
}

async fn find_or_create_conversation(
    api: &mut ApiClient,
    username: &str,
) -> Result<serde_json::Value> {
    // Search for user first
    let users: Vec<serde_json::Value> = api
        .get(&format!("/users/search?q={}", username))
        .await?;

    let user = users
        .iter()
        .find(|u| u["username"].as_str() == Some(username))
        .ok_or_else(|| anyhow::anyhow!("User '{}' not found", username))?;

    let user_id = user["id"].as_str().unwrap();
    let conv: serde_json::Value = api
        .post("/dms", &serde_json::json!({ "userId": user_id }))
        .await?;

    Ok(conv)
}

fn print_message(msg: &serde_json::Value) {
    let name = msg["displayName"].as_str()
        .or_else(|| msg["display_name"].as_str())
        .filter(|s| !s.is_empty())
        .or_else(|| msg["username"].as_str())
        .unwrap_or("?");
    let content = msg["content"].as_str().unwrap_or("");
    let created_at = msg["createdAt"].as_str()
        .or_else(|| msg["created_at"].as_str())
        .unwrap_or("");
    let time = format_time(created_at);

    println!("\r[{}] {}: {}", time, name, content);
}

fn print_prompt(input: &str) {
    print!("\r> {}\x1b[K", input);
    std::io::stdout().flush().unwrap_or_default();
}

fn clear_line() {
    print!("\r\x1b[K");
    std::io::stdout().flush().unwrap_or_default();
}

fn format_time(iso: &str) -> String {
    chrono::DateTime::parse_from_rfc3339(iso)
        .map(|dt| dt.format("%H:%M").to_string())
        .unwrap_or_else(|_| "??:??".to_string())
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() > max {
        format!("{}...", &s[..max])
    } else {
        s.to_string()
    }
}

struct RawModeGuard;
impl Drop for RawModeGuard {
    fn drop(&mut self) {
        let _ = terminal::disable_raw_mode();
        println!();
    }
}