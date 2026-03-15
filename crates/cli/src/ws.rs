// Copyright (c) 2026 Lag
// SPDX-License-Identifier: MIT

use anyhow::{anyhow, Result};
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::Message;
use tracing::{debug, info};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum WsClientMessage {
    #[serde(rename = "ping")]
    Ping,
    #[serde(rename = "set_status")]
    SetStatus { status: String },
    #[serde(rename = "subscribe_presence")]
    SubscribePresence { #[serde(rename = "userIds")] user_ids: Vec<String> },
    #[serde(rename = "unsubscribe_presence")]
    UnsubscribePresence { #[serde(rename = "userIds")] user_ids: Vec<String> },
    #[serde(rename = "subscribe_server_room")]
    SubscribeServerRoom {
        #[serde(rename = "serverId")] server_id: String,
        #[serde(rename = "roomId")] room_id: Option<String>,
    },
    #[serde(rename = "unsubscribe_server_room")]
    UnsubscribeServerRoom {
        #[serde(rename = "serverId")] server_id: String,
        #[serde(rename = "roomId")] room_id: Option<String>,
    },
    #[serde(rename = "typing_start")]
    TypingStart { #[serde(rename = "conversationId")] conversation_id: String },
    #[serde(rename = "typing_stop")]
    TypingStop { #[serde(rename = "conversationId")] conversation_id: String },
    #[serde(rename = "typing_room_start")]
    TypingRoomStart {
        #[serde(rename = "serverId")] server_id: String,
        #[serde(rename = "roomId")] room_id: String,
    },
    #[serde(rename = "typing_room_stop")]
    TypingRoomStop {
        #[serde(rename = "serverId")] server_id: String,
        #[serde(rename = "roomId")] room_id: String,
    },
}

/// Raw WS message — manually parsed since serde internally-tagged enums
/// don't support newtype variants with flattened JSON payloads.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub enum WsServerMessage {
    Pong,
    Error { message: String },
    FriendRequestReceived(serde_json::Value),
    FriendRequestAccepted(serde_json::Value),
    FriendOnline(serde_json::Value),
    FriendOffline(serde_json::Value),
    FriendStatusChanged(serde_json::Value),
    DmMessage(serde_json::Value),
    DmTyping(serde_json::Value),
    RoomMessage(serde_json::Value),
    VoiceRoomUserJoined(serde_json::Value),
    VoiceRoomUserLeft(serde_json::Value),
    ServerMemberJoined(serde_json::Value),
    ServerMemberLeft(serde_json::Value),
    ServerEvent { event: String, payload: serde_json::Value },
    Unknown,
}

impl WsServerMessage {
    pub fn parse(text: &str) -> Option<Self> {
        let val: serde_json::Value = serde_json::from_str(text).ok()?;
        let msg_type = val["type"].as_str()?;
        Some(match msg_type {
            "pong" => Self::Pong,
            "error" => Self::Error {
                message: val["message"].as_str().unwrap_or("").to_string(),
            },
            "friend_request_received" => Self::FriendRequestReceived(val),
            "friend_request_accepted" => Self::FriendRequestAccepted(val),
            "friend_online" => Self::FriendOnline(val),
            "friend_offline" => Self::FriendOffline(val),
            "friend_status_changed" => Self::FriendStatusChanged(val),
            "dm_message" => Self::DmMessage(val),
            "dm_typing" => Self::DmTyping(val),
            "room_message" => Self::RoomMessage(val),
            "voice_room_user_joined" => Self::VoiceRoomUserJoined(val),
            "voice_room_user_left" => Self::VoiceRoomUserLeft(val),
            "server_member_joined" => Self::ServerMemberJoined(val),
            "server_member_left" => Self::ServerMemberLeft(val),
            "server_event" => Self::ServerEvent {
                event: val["event"].as_str().unwrap_or("").to_string(),
                payload: val["payload"].clone(),
            },
            _ => Self::Unknown,
        })
    }
}

pub struct WsClient {
    send_tx: mpsc::UnboundedSender<WsClientMessage>,
    recv_rx: mpsc::UnboundedReceiver<WsServerMessage>,
}

impl WsClient {
    pub async fn connect(api_base_url: &str, token: &str) -> Result<Self> {
        let ws_url = api_base_url
            .replace("https://", "wss://")
            .replace("http://", "ws://");
        let full_url = format!("{}/ws?token={}", ws_url, token);

        let (send_tx, recv_rx, _alive) = Self::spawn_connection(&full_url).await?;

        Ok(Self { send_tx, recv_rx })
    }

    async fn spawn_connection(
        url: &str,
    ) -> Result<(
        mpsc::UnboundedSender<WsClientMessage>,
        mpsc::UnboundedReceiver<WsServerMessage>,
        Arc<AtomicBool>,
    )> {
        let (ws_stream, _) = tokio_tungstenite::connect_async(url)
            .await
            .map_err(|e| anyhow!("WebSocket connection failed: {}", e))?;

        info!("WebSocket connected");

        let (write, read) = ws_stream.split();
        let (send_tx, mut send_rx) = mpsc::unbounded_channel::<WsClientMessage>();
        let (recv_tx, recv_rx) = mpsc::unbounded_channel::<WsServerMessage>();
        let alive = Arc::new(AtomicBool::new(true));

        // Writer task
        let alive_w = alive.clone();
        let mut write = write;
        tokio::spawn(async move {
            while let Some(msg) = send_rx.recv().await {
                if let Ok(json) = serde_json::to_string(&msg) {
                    if write.send(Message::Text(json.into())).await.is_err() {
                        alive_w.store(false, Ordering::Relaxed);
                        break;
                    }
                }
            }
        });

        // Reader task
        let alive_r = alive.clone();
        let mut read = read;
        tokio::spawn(async move {
            while let Some(result) = read.next().await {
                match result {
                    Ok(Message::Text(text)) => {
                        if let Some(parsed) = WsServerMessage::parse(&text) {
                            let _ = recv_tx.send(parsed);
                        } else {
                            debug!("Unknown WS message: {}", text);
                        }
                    }
                    Ok(Message::Close(_)) => {
                        info!("WebSocket closed by server");
                        break;
                    }
                    Err(e) => {
                        debug!("WebSocket error: {}", e);
                        break;
                    }
                    _ => {}
                }
            }
            alive_r.store(false, Ordering::Relaxed);
        });

        // Ping task
        let ping_tx = send_tx.clone();
        let alive_p = alive.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(30));
            loop {
                interval.tick().await;
                if !alive_p.load(Ordering::Relaxed) {
                    break;
                }
                if ping_tx.send(WsClientMessage::Ping).is_err() {
                    break;
                }
            }
        });

        Ok((send_tx, recv_rx, alive))
    }

    /// Connect with automatic reconnection. Retries with exponential backoff.
    pub async fn connect_persistent(api_base_url: &str, token: &str) -> Result<Self> {
        let ws_url = api_base_url
            .replace("https://", "wss://")
            .replace("http://", "ws://");
        let full_url = format!("{}/ws?token={}", ws_url, token);

        let (user_send_tx, mut user_send_rx) = mpsc::unbounded_channel::<WsClientMessage>();
        let (user_recv_tx, user_recv_rx) = mpsc::unbounded_channel::<WsServerMessage>();
        let alive_mgr = Arc::new(AtomicBool::new(true));
        tokio::spawn(async move {
            let mut backoff = Duration::from_secs(1);
            let max_backoff = Duration::from_secs(30);

            loop {
                let conn = tokio_tungstenite::connect_async(&full_url).await;
                match conn {
                    Ok((ws_stream, _)) => {
                        info!("WebSocket connected");
                        backoff = Duration::from_secs(1);
                        alive_mgr.store(true, Ordering::Relaxed);

                        let (mut write, mut read) = ws_stream.split();
                        let inner_alive = Arc::new(AtomicBool::new(true));

                        // Ping task for this connection
                        let ping_alive = inner_alive.clone();
                        let _ping_tx = user_recv_tx.clone();
                        let (inner_send_tx, mut inner_send_rx) = mpsc::unbounded_channel::<WsClientMessage>();

                        // Forward user sends to this connection's writer
                        let _fwd_alive = inner_alive.clone();
                        let inner_send_tx2 = inner_send_tx.clone();
                        tokio::spawn(async move {
                            let mut interval = tokio::time::interval(Duration::from_secs(30));
                            loop {
                                interval.tick().await;
                                if !ping_alive.load(Ordering::Relaxed) { break; }
                                if inner_send_tx2.send(WsClientMessage::Ping).is_err() { break; }
                            }
                        });

                        // Writer
                        let write_alive = inner_alive.clone();
                        tokio::spawn(async move {
                            while let Some(msg) = inner_send_rx.recv().await {
                                if let Ok(json) = serde_json::to_string(&msg) {
                                    if write.send(Message::Text(json.into())).await.is_err() {
                                        write_alive.store(false, Ordering::Relaxed);
                                        break;
                                    }
                                }
                            }
                        });

                        // Read loop - runs until disconnect
                        loop {
                            tokio::select! {
                                msg = read.next() => {
                                    match msg {
                                        Some(Ok(Message::Text(text))) => {
                                            if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open("/tmp/lag-ws-raw.log") {
                                                use std::io::Write;
                                                let _ = writeln!(f, "{}", text);
                                            }
                                            if let Some(parsed) = WsServerMessage::parse(&text) {
                                                let _ = user_recv_tx.send(parsed);
                                            }
                                        }
                                        Some(Ok(Message::Close(_))) | None => {
                                            info!("WebSocket disconnected, will reconnect");
                                            break;
                                        }
                                        Some(Err(e)) => {
                                            debug!("WebSocket error: {}, will reconnect", e);
                                            break;
                                        }
                                        _ => {}
                                    }
                                }
                                user_msg = user_send_rx.recv() => {
                                    match user_msg {
                                        Some(msg) => { let _ = inner_send_tx.send(msg); }
                                        None => return, // caller dropped send channel, shut down
                                    }
                                }
                            }
                        }

                        inner_alive.store(false, Ordering::Relaxed);
                    }
                    Err(e) => {
                        debug!("WebSocket connection failed: {}, retrying in {:?}", e, backoff);
                    }
                }

                alive_mgr.store(false, Ordering::Relaxed);
                tokio::time::sleep(backoff).await;
                backoff = (backoff * 2).min(max_backoff);
            }
        });

        Ok(Self {
            send_tx: user_send_tx,
            recv_rx: user_recv_rx,
        })
    }

    pub fn send(&self, msg: WsClientMessage) -> Result<()> {
        self.send_tx
            .send(msg)
            .map_err(|_| anyhow!("WebSocket send channel closed"))
    }

    pub async fn recv(&mut self) -> Option<WsServerMessage> {
        self.recv_rx.recv().await
    }

    pub fn try_recv(&mut self) -> Option<WsServerMessage> {
        self.recv_rx.try_recv().ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_pong() {
        let msg = WsServerMessage::parse(r#"{"type":"pong"}"#).unwrap();
        assert!(matches!(msg, WsServerMessage::Pong));
    }

    #[test]
    fn parse_error() {
        let msg = WsServerMessage::parse(r#"{"type":"error","message":"bad token"}"#).unwrap();
        match msg {
            WsServerMessage::Error { message } => assert_eq!(message, "bad token"),
            _ => panic!("expected Error"),
        }
    }

    #[test]
    fn parse_dm_message() {
        let json = r#"{"type":"dm_message","content":"hello","from":"alice"}"#;
        let msg = WsServerMessage::parse(json).unwrap();
        match msg {
            WsServerMessage::DmMessage(val) => {
                assert_eq!(val["content"], "hello");
                assert_eq!(val["from"], "alice");
            }
            _ => panic!("expected DmMessage"),
        }
    }

    #[test]
    fn parse_dm_typing() {
        let msg = WsServerMessage::parse(r#"{"type":"dm_typing","userId":"u1"}"#).unwrap();
        assert!(matches!(msg, WsServerMessage::DmTyping(_)));
    }

    #[test]
    fn parse_room_message() {
        let msg = WsServerMessage::parse(r#"{"type":"room_message","text":"hi"}"#).unwrap();
        assert!(matches!(msg, WsServerMessage::RoomMessage(_)));
    }

    #[test]
    fn parse_friend_online() {
        let msg = WsServerMessage::parse(r#"{"type":"friend_online","userId":"u1"}"#).unwrap();
        assert!(matches!(msg, WsServerMessage::FriendOnline(_)));
    }

    #[test]
    fn parse_friend_offline() {
        let msg = WsServerMessage::parse(r#"{"type":"friend_offline","userId":"u1"}"#).unwrap();
        assert!(matches!(msg, WsServerMessage::FriendOffline(_)));
    }

    #[test]
    fn parse_friend_status_changed() {
        let msg = WsServerMessage::parse(r#"{"type":"friend_status_changed","status":"away"}"#).unwrap();
        assert!(matches!(msg, WsServerMessage::FriendStatusChanged(_)));
    }

    #[test]
    fn parse_server_event() {
        let json = r#"{"type":"server_event","event":"member_update","payload":{"id":"123"}}"#;
        let msg = WsServerMessage::parse(json).unwrap();
        match msg {
            WsServerMessage::ServerEvent { event, payload } => {
                assert_eq!(event, "member_update");
                assert_eq!(payload["id"], "123");
            }
            _ => panic!("expected ServerEvent"),
        }
    }

    #[test]
    fn parse_unknown_type() {
        let msg = WsServerMessage::parse(r#"{"type":"future_feature"}"#).unwrap();
        assert!(matches!(msg, WsServerMessage::Unknown));
    }

    #[test]
    fn parse_invalid_json() {
        assert!(WsServerMessage::parse("not json {{{").is_none());
    }

    #[test]
    fn parse_missing_type() {
        assert!(WsServerMessage::parse(r#"{"data":"no type field"}"#).is_none());
    }

    #[test]
    fn client_message_ping_serializes() {
        let json = serde_json::to_string(&WsClientMessage::Ping).unwrap();
        assert!(json.contains(r#""type":"ping""#));
    }
}