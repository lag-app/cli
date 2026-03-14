// Copyright (c) 2026 Lag
// SPDX-License-Identifier: MIT

use anyhow::Result;
use crate::api::ApiClient;
use crate::auth;
use crate::cli::ChatAction;
use crate::ws::{WsClient, WsClientMessage, WsServerMessage};
use crossterm::event::{self, Event, KeyCode, KeyModifiers};
use crossterm::terminal;
use std::io::Write;

pub async fn run(action: Option<ChatAction>) -> Result<()> {
    let creds = auth::require_auth()?;
    let mut api = ApiClient::new(creds)?;

    match action {
        None => {
            println!("Usage: lag chat open <server> <room>");
            println!("       lag chat send <server> <room> <message>");
            Ok(())
        }
        Some(ChatAction::Open { server, room }) => open_chat(&mut api, &server, &room).await,
        Some(ChatAction::Send {
            server,
            room,
            message,
        }) => send_message(&mut api, &server, &room, &message).await,
    }
}

async fn send_message(
    api: &mut ApiClient,
    server_query: &str,
    room_query: &str,
    message: &str,
) -> Result<()> {
    let (server_id, room_id) = resolve_server_room(api, server_query, room_query).await?;

    let _: serde_json::Value = api
        .post(
            &format!("/servers/{}/rooms/{}/messages", server_id, room_id),
            &serde_json::json!({ "content": message }),
        )
        .await?;

    println!("Message sent.");
    Ok(())
}

async fn open_chat(api: &mut ApiClient, server_query: &str, room_query: &str) -> Result<()> {
    let (server_id, room_id) = resolve_server_room(api, server_query, room_query).await?;

    let messages: Vec<serde_json::Value> = api
        .get(&format!(
            "/servers/{}/rooms/{}/messages?limit=50",
            server_id, room_id
        ))
        .await?;

    println!("-- {} / {} --", server_query, room_query);
    println!("(Press Ctrl+C or Esc to exit)\n");

    for msg in messages.iter().rev() {
        print_message(msg);
    }

    let mut ws = WsClient::connect(api.base_url(), api.access_token()).await?;
    ws.send(WsClientMessage::SubscribeServerRoom {
        server_id: server_id.clone(),
        room_id: Some(room_id.clone()),
    })?;

    terminal::enable_raw_mode()?;
    let _cleanup = RawModeGuard;

    let mut input_buf = String::new();
    print_prompt(&input_buf);

    loop {
        tokio::select! {
            ws_msg = ws.recv() => {
                match ws_msg {
                    Some(WsServerMessage::RoomMessage(val)) => {
                        let msg_room_id = val["roomId"].as_str().unwrap_or("");
                        if msg_room_id == room_id {
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

                                    let _ = api.post::<_, serde_json::Value>(
                                        &format!("/servers/{}/rooms/{}/messages", server_id, room_id),
                                        &serde_json::json!({ "content": msg }),
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
            } => {}
        }
    }

    Ok(())
}

async fn resolve_server_room(
    api: &mut ApiClient,
    server_query: &str,
    room_query: &str,
) -> Result<(String, String)> {
    let servers: Vec<serde_json::Value> = api.get("/servers/me").await?;

    let server = servers
        .iter()
        .find(|s| {
            let name = s["name"].as_str().unwrap_or("");
            let id = s["id"].as_str().unwrap_or("");
            name.eq_ignore_ascii_case(server_query) || id.starts_with(server_query)
        })
        .ok_or_else(|| anyhow::anyhow!("Server '{}' not found", server_query))?;

    let server_id = server["id"].as_str().unwrap().to_string();
    let details: serde_json::Value = api.get(&format!("/servers/{}", server_id)).await?;

    let rooms = details["rooms"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("No rooms in server"))?;

    let room = rooms
        .iter()
        .find(|r| {
            let name = r["name"].as_str().unwrap_or("");
            let id = r["id"].as_str().unwrap_or("");
            name.eq_ignore_ascii_case(room_query) || id.starts_with(room_query)
        })
        .ok_or_else(|| anyhow::anyhow!("Room '{}' not found", room_query))?;

    let room_id = room["id"].as_str().unwrap().to_string();
    Ok((server_id, room_id))
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
    let time = chrono::DateTime::parse_from_rfc3339(created_at)
        .map(|dt| dt.format("%H:%M").to_string())
        .unwrap_or_else(|_| "??:??".to_string());
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

struct RawModeGuard;
impl Drop for RawModeGuard {
    fn drop(&mut self) {
        let _ = terminal::disable_raw_mode();
        println!();
    }
}