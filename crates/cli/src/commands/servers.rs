// Copyright (c) 2026 Lag
// SPDX-License-Identifier: MIT

use anyhow::Result;
use crate::api::ApiClient;
use crate::auth;
use crossterm::event::{self, Event, KeyCode, KeyModifiers};
use crossterm::terminal;

use std::io::{self, Write};

pub async fn run(name_or_id: Option<String>) -> Result<()> {
    let creds = auth::require_auth()?;
    let mut api = ApiClient::new(creds)?;

    match name_or_id {
        Some(ref query) => show_server(&mut api, query).await,
        None => interactive_server_picker(&mut api).await,
    }
}

async fn interactive_server_picker(api: &mut ApiClient) -> Result<()> {
    let servers: Vec<serde_json::Value> = api.get("/servers/me").await?;

    if servers.is_empty() {
        println!("No servers. Join one with an invite link or create one on the web.");
        return Ok(());
    }

    let selected = arrow_select(
        "Select a server",
        &servers.iter().map(|s| {
            let name = s["name"].as_str().unwrap_or("?");
            let members = s["memberCount"].as_u64().unwrap_or(0);
            let emoji = s["iconEmoji"].as_str().unwrap_or("");
            if !emoji.is_empty() {
                format!("{} {} ({} members)", emoji, name, members)
            } else {
                format!("{} ({} members)", name, members)
            }
        }).collect::<Vec<_>>(),
    )?;

    let server = &servers[selected];
    let server_id = server["id"].as_str().unwrap().to_string();
    let server_name = server["name"].as_str().unwrap_or("?").to_string();

    interactive_room_picker(api, &server_id, &server_name).await
}

fn interactive_room_picker<'a>(api: &'a mut ApiClient, server_id: &'a str, server_name: &'a str) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<()>> + 'a>> {
    Box::pin(interactive_room_picker_inner(api, server_id, server_name))
}

async fn interactive_room_picker_inner(api: &mut ApiClient, server_id: &str, server_name: &str) -> Result<()> {
    let details: serde_json::Value = api.get(&format!("/servers/{}", server_id)).await?;

    let rooms = details["rooms"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("No rooms in server"))?;

    if rooms.is_empty() {
        println!("No rooms in {}.", server_name);
        return Ok(());
    }

    let labels: Vec<String> = rooms.iter().map(|r| {
        let name = r["name"].as_str().unwrap_or("?");
        let participants = r["participants"]
            .as_array()
            .map(|p| p.len())
            .unwrap_or(0);
        if participants > 0 {
            let names: Vec<&str> = r["participants"]
                .as_array()
                .unwrap()
                .iter()
                .filter_map(|p| p["username"].as_str())
                .collect();
            format!("{} ({}: {})", name, participants, names.join(", "))
        } else {
            name.to_string()
        }
    }).collect();

    let selected = arrow_select(
        &format!("{} - select a room", server_name),
        &labels,
    )?;

    let room = &rooms[selected];
    let _room_id = room["id"].as_str().unwrap().to_string();
    let room_name = room["name"].as_str().unwrap_or("?").to_string();

    // Ask what to do
    let action = arrow_select(
        &format!("{} / {}", server_name, room_name),
        &[
            "Join voice".to_string(),
            "Join voice + chat".to_string(),
            "Open chat only".to_string(),
            "Back".to_string(),
        ],
    )?;

    match action {
        0 => {
            crate::commands::join::run(
                server_name.to_string(), room_name.to_string(),
                None, false, None, None, false,
            ).await
        }
        1 => {
            crate::commands::join::run(
                server_name.to_string(), room_name.to_string(),
                None, false, None, None, true,
            ).await
        }
        2 => {
            crate::commands::chat::run(Some(crate::cli::ChatAction::Open {
                server: server_name.to_string(),
                room: room_name.to_string(),
            })).await
        }
        3 => {
            interactive_room_picker(api, server_id, server_name).await
        }
        _ => Ok(()),
    }
}

async fn show_server(api: &mut ApiClient, query: &str) -> Result<()> {
    let servers: Vec<serde_json::Value> = api.get("/servers/me").await?;

    let server = servers
        .iter()
        .find(|s| {
            let name = s["name"].as_str().unwrap_or("");
            let id = s["id"].as_str().unwrap_or("");
            name.eq_ignore_ascii_case(query) || id.starts_with(query)
        })
        .ok_or_else(|| anyhow::anyhow!("Server '{}' not found", query))?;

    let server_id = server["id"].as_str().unwrap().to_string();
    let server_name = server["name"].as_str().unwrap_or("?").to_string();

    interactive_room_picker(api, &server_id, &server_name).await
}

fn arrow_select(title: &str, items: &[String]) -> Result<usize> {
    if items.is_empty() {
        anyhow::bail!("Nothing to select");
    }

    terminal::enable_raw_mode()?;
    let mut stdout = io::stdout();

    let mut selected: usize = 0;
    let total_lines = items.len() + 3; // title + hint + blank + items

    let result = loop {
        write!(stdout, "\r\x1b[J")?;
        write!(stdout, "\x1b[1m{}\x1b[0m\r\n", title)?;
        write!(stdout, "\x1b[90m(arrows to move, enter to select, esc to quit)\x1b[0m\r\n\r\n")?;

        for (i, item) in items.iter().enumerate() {
            if i == selected {
                write!(stdout, "  \x1b[36m> {}\x1b[0m\r\n", item)?;
            } else {
                write!(stdout, "    \x1b[90m{}\x1b[0m\r\n", item)?;
            }
        }

        stdout.flush()?;

        if let Event::Key(key) = event::read()? {
            match (key.code, key.modifiers) {
                (KeyCode::Up | KeyCode::Char('k'), _) => {
                    selected = selected.saturating_sub(1);
                }
                (KeyCode::Down | KeyCode::Char('j'), _) => {
                    if selected < items.len() - 1 { selected += 1; }
                }
                (KeyCode::Enter, _) => break Some(selected),
                (KeyCode::Esc, _) | (KeyCode::Char('c'), KeyModifiers::CONTROL) => break None,
                _ => {}
            }
        }
    };

    // Clear the menu
    for _ in 0..total_lines {
        write!(stdout, "\x1b[A\r\x1b[K")?;
    }
    stdout.flush()?;
    terminal::disable_raw_mode()?;

    match result {
        Some(idx) => Ok(idx),
        None => std::process::exit(0),
    }
}