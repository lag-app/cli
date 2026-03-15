// Copyright (c) 2026 Lag
// SPDX-License-Identifier: MIT

use crate::api::ApiClient;
use crate::auth;
use crate::config;
use crate::ws::{WsClient, WsServerMessage};
use anyhow::Result;
use crossterm::event::{self, Event, KeyCode, KeyModifiers};
use crossterm::terminal;
use lag_voice_core::{AudioEngine, AudioSettings, PushToTalkManager, VoiceEvent, VoiceRoom};
use parking_lot::Mutex;
use std::io::Write;
use std::sync::Arc;

pub async fn run(
    server: String,
    room: String,
    ptt_key: Option<String>,
    no_vad: bool,
    input_device: Option<String>,
    output_device: Option<String>,
    with_chat: bool,
) -> Result<()> {
    let creds = auth::require_auth()?;
    let mut api = ApiClient::new(creds)?;

    // Resolve server and room
    let (server_id, room_id, server_name, room_name) =
        resolve_voice_room(&mut api, &server, &room).await?;

    // Get voice token
    let token_resp: serde_json::Value = api
        .post(
            &format!("/voice/rooms/{}/token", room_id),
            &serde_json::json!({}),
        )
        .await?;

    let voice_url = token_resp["voiceUrl"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("No voice URL in response"))?;
    let voice_token = token_resp["participantToken"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("No voice token in response"))?;

    // Participants are nested under room.participants
    let participants_json = token_resp["room"]["participants"]
        .as_array()
        .cloned()
        .unwrap_or_default();

    let participants: Vec<lag_voice_core::ParticipantInfo> =
        serde_json::from_value(serde_json::Value::Array(participants_json))?;

    // Set up audio engine
    let settings_dir = config::config_dir();
    let audio_settings = AudioSettings::load(&settings_dir);
    let mut engine = AudioEngine::new();
    engine.set_input_volume(audio_settings.input_volume);
    engine.set_output_volume(audio_settings.output_volume);

    if let Some(ref dev) = input_device.or(audio_settings.input_device.clone()) {
        engine.set_input_device(dev)?;
    }
    if let Some(ref dev) = output_device.or(audio_settings.output_device.clone()) {
        engine.set_output_device(dev)?;
    }

    let engine = Arc::new(Mutex::new(engine));

    // Set up PTT
    let ptt = Arc::new(PushToTalkManager::new());
    if let Some(ref key) = ptt_key.or(audio_settings.ptt_key.clone()) {
        ptt.set_key_from_string(key);
        ptt.set_enabled(true);
    }
    ptt.clone().start_listener();

    let vad_threshold = if no_vad {
        1.0
    } else {
        audio_settings.vad_threshold
    };

    // Connect to voice
    let (event_tx, mut event_rx) = VoiceRoom::create_event_channel();

    let mut voice_room = VoiceRoom::connect(
        voice_url,
        voice_token,
        participants,
        engine.clone(),
        vad_threshold,
        event_tx,
    )
    .await?;

    println!("-- Voice: {} / {} --", server_name, room_name);
    println!("Connected. Press Ctrl+C to disconnect.\n");

    // Wire PTT mute callback
    {
        let engine_ref = engine.clone();
        ptt.set_callback(move |muted| {
            let mut eng = engine_ref.lock();
            if muted {
                let _ = eng.stop_capture();
            } else {
                let _ = eng.start_capture();
            }
        });
    }

    // Optionally connect WS for chat
    let mut ws = if with_chat {
        let ws = WsClient::connect(api.base_url(), api.access_token()).await?;
        ws.send(crate::ws::WsClientMessage::SubscribeServerRoom {
            server_id: server_id.clone(),
            room_id: Some(room_id.clone()),
        })?;
        Some(ws)
    } else {
        None
    };

    if with_chat {
        // Load recent messages
        let messages: Vec<serde_json::Value> = api
            .get(&format!(
                "/servers/{}/rooms/{}/messages?limit=20",
                server_id, room_id
            ))
            .await?;
        for msg in messages.iter().rev() {
            let name = msg["displayName"]
                .as_str()
                .or_else(|| msg["display_name"].as_str())
                .filter(|s| !s.is_empty())
                .or_else(|| msg["username"].as_str())
                .unwrap_or("?");
            let content = msg["content"].as_str().unwrap_or("");
            println!("[chat] {}: {}", name, content);
        }
    }

    let mut input_buf = String::new();
    if with_chat {
        terminal::enable_raw_mode()?;
        print!("> ");
        std::io::stdout().flush()?;
    }

    let result: Result<()> = async {
        loop {
            tokio::select! {
                voice_event = event_rx.recv() => {
                    match voice_event {
                        Some(VoiceEvent::Connected { participants }) => {
                            let names: Vec<&str> = participants.iter()
                                .map(|p| p.username.as_str())
                                .collect();
                            println!("Participants: {}", names.join(", "));
                        }
                        Some(VoiceEvent::ParticipantJoined { participant }) => {
                            println!("+ {} joined", participant.username);
                        }
                        Some(VoiceEvent::ParticipantLeft { user_id }) => {
                            println!("- {} left", user_id);
                        }
                        Some(VoiceEvent::Speaking { user_id, speaking }) => {
                            if speaking {
                                print!("\r{} speaking...\x1b[K", user_id);
                                std::io::stdout().flush()?;
                            }
                        }
                        Some(VoiceEvent::Disconnected { reason }) => {
                            println!("Disconnected: {}", reason);
                            break;
                        }
                        Some(VoiceEvent::Reconnecting) => {
                            println!("Reconnecting...");
                        }
                        Some(VoiceEvent::Reconnected { .. }) => {
                            println!("Reconnected.");
                        }
                        None => break,
                        _ => {}
                    }
                }
                ws_msg = async {
                    if let Some(ref mut ws) = ws {
                        ws.recv().await
                    } else {
                        std::future::pending().await
                    }
                } => {
                    if let Some(WsServerMessage::RoomMessage(val)) = ws_msg {
                        let username = val["username"].as_str().unwrap_or("?");
                        let content = val["content"].as_str().unwrap_or("");
                        println!("\r[chat] {}: {}\x1b[K", username, content);
                        if with_chat {
                            print!("> {}", input_buf);
                            std::io::stdout().flush()?;
                        }
                    }
                }
                _ = async {
                    if with_chat {
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
                                            let _ = api.post::<_, serde_json::Value>(
                                                &format!("/servers/{}/rooms/{}/messages", server_id, room_id),
                                                &serde_json::json!({ "content": msg }),
                                            ).await;
                                            print!("\r> \x1b[K");
                                            std::io::stdout().flush().ok();
                                        }
                                    }
                                    (KeyCode::Backspace, _) => {
                                        input_buf.pop();
                                        print!("\r> {}\x1b[K", input_buf);
                                        std::io::stdout().flush().ok();
                                    }
                                    (KeyCode::Char(c), _) => {
                                        input_buf.push(c);
                                        print!("\r> {}\x1b[K", input_buf);
                                        std::io::stdout().flush().ok();
                                    }
                                    _ => {}
                                }
                            }
                        }
                        false
                    } else {
                        // No chat mode - just wait for Ctrl+C via signal
                        tokio::signal::ctrl_c().await.ok();
                        true
                    }
                } => {}
            }
        }
        Ok(())
    }
    .await;

    if with_chat {
        let _ = terminal::disable_raw_mode();
    }

    // Graceful disconnect
    println!("\nDisconnecting...");
    voice_room.disconnect().await?;
    api.delete_no_body(&format!("/voice/rooms/{}/leave", room_id))
        .await?;
    println!("Disconnected.");

    result
}

async fn resolve_voice_room(
    api: &mut ApiClient,
    server_query: &str,
    room_query: &str,
) -> Result<(String, String, String, String)> {
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
    let server_name = server["name"].as_str().unwrap_or("?").to_string();
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
    let room_name = room["name"].as_str().unwrap_or("?").to_string();

    Ok((server_id, room_id, server_name, room_name))
}
