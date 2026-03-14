// Copyright (c) 2026 Lag
// SPDX-License-Identifier: MIT

use anyhow::Result;
use lag_voice_core::{AudioEngine, AudioSettings, VoiceRoom, VoiceEvent};
use parking_lot::Mutex;
use std::sync::Arc;
use crate::api::ApiClient;
use crate::config::{self, Credentials};
use crate::ws::{WsClient, WsServerMessage};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SidebarSection {
    Servers,
    Friends,
    Dms,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SidebarView {
    ServerList,
    RoomList,
}

#[derive(Debug)]
pub enum AppEvent {
    Quit,
    NavigateUp,
    NavigateDown,
    Select,
    Back,
    CyclePanel,
    ToggleMute,
    ToggleDeafen,
    EnterTyping,
    ExitTyping,
    TypeChar(char),
    DeleteChar,
    SubmitMessage,
    OpenAudioSettings,
    SwitchSection(SidebarSection),
    JoinVoice,
    LeaveVoice,
    ActionMenuSelect,
    FriendPopupNav(i32),
    FriendPopupSelect,
    AddFriendStart,
    AddFriendChar(char),
    AddFriendDelete,
    AddFriendSubmit,
    AddFriendCancel,
    WsMessage(WsServerMessage),
    VoiceEvent(VoiceEvent),
}

pub struct ServerDetail {
    pub id: String,
    pub name: String,
    pub rooms: Vec<serde_json::Value>,
}

pub struct ActionMenu {
    pub items: Vec<String>,
    pub selected: usize,
}

#[derive(Clone)]
pub enum FriendEntryKind {
    Friend,
    IncomingRequest,
    OutgoingRequest,
}

#[derive(Clone)]
pub struct FriendEntry {
    pub kind: FriendEntryKind,
    pub username: String,
    pub display_name: Option<String>,
    pub status: String,
    pub friendship_id: String,
    pub user_id: String,
    pub since: Option<String>,
}

impl FriendEntry {
    pub fn label(&self) -> String {
        let name = self.display_name.as_deref().unwrap_or(&self.username);
        match self.kind {
            FriendEntryKind::Friend => name.to_string(),
            FriendEntryKind::IncomingRequest => format!("(REQUEST) {}", name),
            FriendEntryKind::OutgoingRequest => format!("(PENDING) {}", name),
        }
    }
}

pub enum ContentLoadResult {
    ServerDetail {
        id: String,
        name: String,
        rooms: Vec<serde_json::Value>,
    },
    RoomMessages {
        server_id: String,
        room_id: String,
        room_name: String,
        messages: Vec<serde_json::Value>,
    },
    DmMessages {
        dm_id: String,
        dm_name: String,
        messages: Vec<serde_json::Value>,
    },
    Error(String),
}

pub enum InitUpdate {
    Status(String),
    Done {
        username: String,
        servers: Vec<serde_json::Value>,
        friends: Vec<serde_json::Value>,
        friend_requests: serde_json::Value,
        dms: Vec<serde_json::Value>,
    },
    Error(String),
}

pub struct FriendPopup {
    pub entry: FriendEntry,
    pub actions: Vec<String>,
    pub selected: usize,
    pub confirming: Option<String>,
}

pub struct App {
    pub api: ApiClient,
    pub ws: Option<WsClient>,
    pub sidebar_section: SidebarSection,
    pub sidebar_view: SidebarView,
    pub typing: bool,
    pub input_buf: String,
    pub servers: Vec<serde_json::Value>,
    pub friends: Vec<serde_json::Value>,
    pub friend_entries: Vec<FriendEntry>,
    pub friend_popup: Option<FriendPopup>,
    pub adding_friend: bool,
    pub add_friend_input: String,
    pub dms: Vec<serde_json::Value>,
    pub selected_index: usize,
    pub messages: Vec<serde_json::Value>,
    pub muted: bool,
    pub deafened: bool,
    pub connected_room: Option<String>,
    pub username: String,
    pub show_audio_settings: bool,
    // Server drill-down state
    pub selected_server: Option<ServerDetail>,
    pub selected_room_id: Option<String>,
    pub selected_room_name: Option<String>,
    // DM state
    pub selected_dm_id: Option<String>,
    pub selected_dm_name: Option<String>,
    pub dm_unread: std::collections::HashMap<String, u32>,
    // Action menu (shown after selecting a room)
    pub action_menu: Option<ActionMenu>,
    // Voice state
    pub voice_room: Option<VoiceRoom>,
    pub voice_event_rx: Option<tokio::sync::mpsc::UnboundedReceiver<VoiceEvent>>,
    pub voice_connecting: Option<String>,
    pub voice_token_rx: Option<tokio::sync::oneshot::Receiver<Result<serde_json::Value>>>,
    pub voice_connect_room_id: Option<String>,
    pub pending_voice_token: Option<Result<serde_json::Value>>,
    pub voice_participants: Vec<(String, String)>,  // (user_id, display_name)
    pub voice_speaking: std::collections::HashSet<String>,
    pub mic_level: f32,
    pub output_level: f32,
    pub sending_message: bool,
    pub send_result_rx: Option<tokio::sync::oneshot::Receiver<Result<()>>>,
    pub content_loading: Option<String>,
    pub content_load_rx: Option<tokio::sync::oneshot::Receiver<ContentLoadResult>>,
    pub pending_friend_reload: bool,
    pub init_started: bool,
    pub init_rx: Option<tokio::sync::mpsc::UnboundedReceiver<InitUpdate>>,
    pub loading: Option<String>,
    pub error_message: Option<String>,
}

impl App {
    pub fn new(creds: Credentials) -> Result<Self> {
        let api = ApiClient::new(creds)?;

        Ok(Self {
            api,
            ws: None,
            sidebar_section: SidebarSection::Servers,
            sidebar_view: SidebarView::ServerList,
            typing: false,
            input_buf: String::new(),
            servers: Vec::new(),
            friends: Vec::new(),
            friend_entries: Vec::new(),
            friend_popup: None,
            adding_friend: false,
            add_friend_input: String::new(),
            dms: Vec::new(),
            selected_index: 0,
            messages: Vec::new(),
            muted: false,
            deafened: false,
            connected_room: None,
            username: String::new(),
            show_audio_settings: false,
            selected_server: None,
            selected_room_id: None,
            selected_room_name: None,
            selected_dm_id: None,
            selected_dm_name: None,
            dm_unread: std::collections::HashMap::new(),
            action_menu: None,
            voice_connecting: None,
            voice_token_rx: None,
            voice_connect_room_id: None,
            pending_voice_token: None,
            voice_room: None,
            voice_event_rx: None,
            voice_participants: Vec::new(),
            voice_speaking: std::collections::HashSet::new(),
            mic_level: 0.0,
            output_level: 0.0,
            sending_message: false,
            send_result_rx: None,
            content_loading: None,
            content_load_rx: None,
            pending_friend_reload: false,
            init_started: false,
            init_rx: None,
            loading: Some("Connecting to Lag...".into()),
            error_message: None,
        })
    }

    pub fn start_init(&mut self) {
        self.init_started = true;
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        self.init_rx = Some(rx);

        let base_url = self.api.base_url().to_string();
        let token = self.api.access_token().to_string();

        tokio::spawn(async move {
            let client = reqwest::Client::new();
            let auth = format!("Bearer {}", token);

            let _ = tx.send(InitUpdate::Status("Authenticating...".into()));
            let user_resp = client.get(format!("{}/users/me", base_url))
                .header("Authorization", &auth)
                .send().await;
            let username = match user_resp {
                Ok(r) if r.status().is_success() => {
                    let body: serde_json::Value = r.json().await.unwrap_or_default();
                    body["username"].as_str().unwrap_or("you").to_string()
                }
                Ok(r) => {
                    let status = r.status();
                    let text = r.text().await.unwrap_or_default();
                    let _ = tx.send(InitUpdate::Error(format!("Auth failed ({}): {}", status, text)));
                    return;
                }
                Err(e) => {
                    let _ = tx.send(InitUpdate::Error(format!("Connection failed: {}", e)));
                    return;
                }
            };

            async fn fetch_json<T: serde::de::DeserializeOwned + Default>(
                client: &reqwest::Client, url: String, auth: &str
            ) -> T {
                match client.get(&url).header("Authorization", auth).send().await {
                    Ok(r) if r.status().is_success() => r.json::<T>().await.unwrap_or_default(),
                    _ => T::default(),
                }
            }

            let _ = tx.send(InitUpdate::Status("Loading servers...".into()));
            let servers: Vec<serde_json::Value> = fetch_json(&client, format!("{}/servers/me", base_url), &auth).await;

            let _ = tx.send(InitUpdate::Status("Loading friends...".into()));
            let friends: Vec<serde_json::Value> = fetch_json(&client, format!("{}/friends", base_url), &auth).await;
            let friend_requests: serde_json::Value = fetch_json(&client, format!("{}/friends/requests", base_url), &auth).await;

            let _ = tx.send(InitUpdate::Status("Loading conversations...".into()));
            let dms: Vec<serde_json::Value> = fetch_json(&client, format!("{}/dms", base_url), &auth).await;

            let _ = tx.send(InitUpdate::Done {
                username,
                servers,
                friends,
                friend_requests,
                dms,
            });
        });
    }

    pub async fn check_init_complete(&mut self) -> Result<()> {
        if let Some(ref mut rx) = self.init_rx {
            while let Ok(update) = rx.try_recv() {
                match update {
                    InitUpdate::Status(msg) => {
                        self.loading = Some(msg);
                    }
                    InitUpdate::Done { username, servers, friends, friend_requests, dms } => {
                        self.username = username;
                        self.servers = servers;
                        self.friends = friends;
                        self.build_friend_entries(&friend_requests);
                        self.dms = dms;

                        self.loading = Some("Connecting websocket...".into());
                        self.ws = WsClient::connect_persistent(self.api.base_url(), self.api.access_token())
                            .await
                            .ok();

                        self.loading = None;
                        self.init_rx = None;
                        return Ok(());
                    }
                    InitUpdate::Error(msg) => {
                        self.loading = None;
                        self.init_rx = None;
                        return Err(anyhow::anyhow!("{}", msg));
                    }
                }
            }
        }
        Ok(())
    }

    fn relative_time_short(iso: &str) -> String {
        let Ok(dt) = chrono::DateTime::parse_from_rfc3339(iso) else {
            return String::new();
        };
        let diff = chrono::Utc::now().signed_duration_since(dt);
        if diff.num_seconds() < 60 { "now".into() }
        else if diff.num_minutes() < 60 { format!("{}m", diff.num_minutes()) }
        else if diff.num_hours() < 24 { format!("{}h", diff.num_hours()) }
        else { format!("{}d", diff.num_days()) }
    }

    fn build_friend_entries(&mut self, requests: &serde_json::Value) {
        let mut entries = Vec::new();

        // Accepted friends
        for f in &self.friends {
            entries.push(FriendEntry {
                kind: FriendEntryKind::Friend,
                username: f["user"]["username"].as_str().unwrap_or("?").to_string(),
                display_name: f["user"]["displayName"].as_str().map(String::from),
                status: f["user"]["status"].as_str().unwrap_or("offline").to_string(),
                friendship_id: f["friendshipId"].as_str()
                    .or_else(|| f["id"].as_str())
                    .unwrap_or("").to_string(),
                user_id: f["user"]["id"].as_str()
                    .or_else(|| f["id"].as_str())
                    .unwrap_or("").to_string(),
                since: f["since"].as_str().map(String::from),
            });
        }

        // Incoming requests
        if let Some(incoming) = requests["incoming"].as_array() {
            for r in incoming {
                entries.push(FriendEntry {
                    kind: FriendEntryKind::IncomingRequest,
                    username: r["from"]["username"].as_str().unwrap_or("?").to_string(),
                    display_name: r["from"]["displayName"].as_str().map(String::from),
                    status: r["from"]["status"].as_str().unwrap_or("offline").to_string(),
                    friendship_id: r["id"].as_str().unwrap_or("").to_string(),
                    user_id: r["from"]["id"].as_str()
                        .or_else(|| r["requesterId"].as_str())
                        .unwrap_or("").to_string(),
                    since: None,
                });
            }
        }

        // Outgoing requests
        if let Some(outgoing) = requests["outgoing"].as_array() {
            for r in outgoing {
                entries.push(FriendEntry {
                    kind: FriendEntryKind::OutgoingRequest,
                    username: r["to"]["username"].as_str().unwrap_or("?").to_string(),
                    display_name: r["to"]["displayName"].as_str().map(String::from),
                    status: r["to"]["status"].as_str().unwrap_or("offline").to_string(),
                    friendship_id: r["id"].as_str().unwrap_or("").to_string(),
                    user_id: r["to"]["id"].as_str()
                        .or_else(|| r["addresseeId"].as_str())
                        .unwrap_or("").to_string(),
                    since: None,
                });
            }
        }

        self.friend_entries = entries;
    }

    pub fn is_typing(&self) -> bool {
        self.typing
    }

    pub fn sidebar_items(&self) -> Vec<String> {
        match self.sidebar_section {
            SidebarSection::Servers => {
                match self.sidebar_view {
                    SidebarView::ServerList => {
                        self.servers.iter().map(|s| {
                            let name = s["name"].as_str().unwrap_or("?");
                            name.to_string()
                        }).collect()
                    }
                    SidebarView::RoomList => {
                        if let Some(ref detail) = self.selected_server {
                            let mut items = vec![format!("< {}", detail.name)];
                            for room in &detail.rooms {
                                let name = room["name"].as_str().unwrap_or("?");
                                let count = room["participants"]
                                    .as_array()
                                    .map(|p| p.len())
                                    .unwrap_or(0);
                                if count > 0 {
                                    items.push(format!("  {} ({})", name, count));
                                } else {
                                    items.push(format!("  {}", name));
                                }
                            }
                            items
                        } else {
                            vec![]
                        }
                    }
                }
            }
            SidebarSection::Friends => {
                self.friend_entries.iter().map(|e| e.label()).collect()
            }
            SidebarSection::Dms => {
                self.dms.iter().map(|dm| {
                    let conv_id = dm["id"].as_str().unwrap_or("");
                    let name = dm["otherUser"]["displayName"].as_str()
                        .or_else(|| dm["otherUser"]["username"].as_str())
                        .unwrap_or("?");
                    let unread = self.dm_unread.get(conv_id).copied().unwrap_or(0);
                    let preview = dm["lastMessage"]["content"].as_str()
                        .map(|c| {
                            let truncated: String = c.chars().take(20).collect();
                            if c.len() > 20 { format!("{}...", truncated) } else { truncated }
                        });
                    let time = dm["lastMessage"]["createdAt"].as_str()
                        .map(|t| {
                            let rel = Self::relative_time_short(t);
                            if rel.is_empty() { String::new() } else { format!(" {}", rel) }
                        })
                        .unwrap_or_default();

                    let unread_prefix = if unread > 0 {
                        format!("({}) ", unread)
                    } else {
                        String::new()
                    };

                    if let Some(preview) = preview {
                        format!("{}{}{} - {}", unread_prefix, name, time, preview)
                    } else {
                        format!("{}{}{}", unread_prefix, name, time)
                    }
                }).collect()
            }
        }
    }

    pub fn sidebar_item_count(&self) -> usize {
        self.sidebar_items().len()
    }

    pub fn content_title(&self) -> String {
        if let Some(ref dm_name) = self.selected_dm_name {
            return format!("DM with {}", dm_name);
        }
        if let Some(ref room_name) = self.selected_room_name {
            if let Some(ref detail) = self.selected_server {
                return format!("{} / {}", detail.name, room_name);
            }
        }
        if let Some(ref detail) = self.selected_server {
            return detail.name.clone();
        }
        "Select a server".to_string()
    }

    pub async fn handle_event(&mut self, event: AppEvent) -> Result<()> {
        match event {
            AppEvent::NavigateUp => {
                if let Some(ref mut menu) = self.action_menu {
                    if menu.selected > 0 { menu.selected -= 1; }
                } else if self.selected_index > 0 {
                    self.selected_index -= 1;
                }
            }
            AppEvent::NavigateDown => {
                if let Some(ref mut menu) = self.action_menu {
                    if menu.selected < menu.items.len() - 1 { menu.selected += 1; }
                } else {
                    let max = self.sidebar_item_count().saturating_sub(1);
                    if self.selected_index < max {
                        self.selected_index += 1;
                    }
                }
            }
            AppEvent::Select => {
                self.on_select().await?;
            }
            AppEvent::Back => {
                if self.error_message.is_some() {
                    self.error_message = None;
                } else if self.friend_popup.as_ref().map_or(false, |p| p.confirming.is_some()) {
                    if let Some(ref mut popup) = self.friend_popup {
                        popup.confirming = None;
                    }
                } else if self.friend_popup.is_some() {
                    self.friend_popup = None;
                } else if self.action_menu.is_some() {
                    self.action_menu = None;
                } else {
                    self.on_back().await;
                }
            }
            AppEvent::FriendPopupNav(dir) => {
                if let Some(ref mut popup) = self.friend_popup {
                    let len = popup.actions.len();
                    if dir < 0 && popup.selected > 0 {
                        popup.selected -= 1;
                    } else if dir > 0 && popup.selected < len.saturating_sub(1) {
                        popup.selected += 1;
                    }
                }
            }
            AppEvent::FriendPopupSelect => {
                self.on_friend_popup_select().await?;
            }
            AppEvent::CyclePanel => {
                self.sidebar_section = match self.sidebar_section {
                    SidebarSection::Servers => SidebarSection::Friends,
                    SidebarSection::Friends => SidebarSection::Dms,
                    SidebarSection::Dms => SidebarSection::Servers,
                };
                self.sidebar_view = SidebarView::ServerList;
                self.selected_index = 0;
                self.selected_server = None;
                self.selected_room_id = None;
                self.selected_room_name = None;
                self.selected_dm_id = None;
                self.selected_dm_name = None;
                self.messages.clear();
            }
            AppEvent::SwitchSection(section) => {
                self.sidebar_section = section;
                self.sidebar_view = SidebarView::ServerList;
                self.selected_index = 0;
                self.selected_server = None;
                self.selected_room_id = None;
                self.selected_room_name = None;
                self.selected_dm_id = None;
                self.selected_dm_name = None;
                self.messages.clear();
            }
            AppEvent::ToggleMute => {
                self.muted = !self.muted;
                if let Some(ref room) = self.voice_room {
                    room.set_muted(self.muted);
                }
            }
            AppEvent::ToggleDeafen => {
                self.deafened = !self.deafened;
                if self.deafened { self.muted = true; }
                if let Some(ref room) = self.voice_room {
                    room.set_deafened(self.deafened);
                }
            }
            AppEvent::EnterTyping => {
                self.typing = true;
            }
            AppEvent::ExitTyping => {
                self.typing = false;
            }
            AppEvent::TypeChar(c) => {
                self.input_buf.push(c);
            }
            AppEvent::DeleteChar => {
                self.input_buf.pop();
            }
            AppEvent::SubmitMessage => {
                self.send_message();
            }
            AppEvent::AddFriendStart => {
                self.adding_friend = true;
                self.add_friend_input.clear();
            }
            AppEvent::AddFriendChar(c) => {
                self.add_friend_input.push(c);
            }
            AppEvent::AddFriendDelete => {
                self.add_friend_input.pop();
            }
            AppEvent::AddFriendCancel => {
                self.adding_friend = false;
                self.add_friend_input.clear();
            }
            AppEvent::AddFriendSubmit => {
                let username = self.add_friend_input.trim().to_string();
                self.adding_friend = false;
                self.add_friend_input.clear();
                if !username.is_empty() {
                    match self.api.post::<_, serde_json::Value>(
                        "/friends/request",
                        &serde_json::json!({ "username": username }),
                    ).await {
                        Ok(_) => {
                            self.error_message = Some(format!("Friend request sent to {}", username));
                            self.reload_friends().await;
                        }
                        Err(e) => {
                            self.error_message = Some(format!("{}", e));
                        }
                    }
                }
            }
            AppEvent::OpenAudioSettings => {
                self.show_audio_settings = !self.show_audio_settings;
            }
            AppEvent::JoinVoice => {
                self.start_join_voice();
                self.action_menu = None;
            }
            AppEvent::LeaveVoice => {
                if let Err(e) = self.leave_voice().await {
                    self.error_message = Some(format!("{}", e));
                }
                self.action_menu = None;
            }
            AppEvent::ActionMenuSelect => {
                self.on_action_menu_select().await?;
            }
            AppEvent::WsMessage(msg) => {
                self.handle_ws_message(msg);
            }
            AppEvent::VoiceEvent(evt) => {
                self.handle_voice_event(evt);
            }
            AppEvent::Quit => {}
        }
        Ok(())
    }

    pub async fn poll_async_events(&mut self) -> Option<AppEvent> {
        tokio::select! {
            ws_msg = async {
                if let Some(ref mut ws) = self.ws {
                    ws.recv().await
                } else {
                    std::future::pending().await
                }
            } => {
                ws_msg.map(AppEvent::WsMessage)
            }
            voice_evt = async {
                if let Some(ref mut rx) = self.voice_event_rx {
                    rx.recv().await
                } else {
                    std::future::pending().await
                }
            } => {
                voice_evt.map(AppEvent::VoiceEvent)
            }
            _ = tokio::time::sleep(std::time::Duration::from_secs(1)) => {
                None
            }
        }
    }

    /// Drain all pending async events without blocking.
    pub fn drain_async_events(&mut self) {
        // Collect all pending events first to avoid borrow conflicts
        let voice_events: Vec<_> = self.voice_event_rx.as_mut()
            .map(|rx| {
                let mut events = Vec::new();
                while let Ok(evt) = rx.try_recv() {
                    events.push(evt);
                }
                events
            })
            .unwrap_or_default();

        let ws_events: Vec<_> = self.ws.as_mut()
            .map(|ws| {
                let mut events = Vec::new();
                while let Some(msg) = ws.try_recv() {
                    events.push(msg);
                }
                events
            })
            .unwrap_or_default();

        for evt in voice_events {
            self.handle_voice_event(evt);
        }
        for msg in ws_events {
            self.handle_ws_message(msg);
        }

        // Check if a background send completed
        if let Some(ref mut rx) = self.send_result_rx {
            if let Ok(result) = rx.try_recv() {
                self.sending_message = false;
                if let Err(e) = result {
                    self.error_message = Some(format!("Failed to send: {}", e));
                }
                self.send_result_rx = None;
            }
        }

        // Check if voice token fetch completed
        let voice_result = if let Some(ref mut rx) = self.voice_token_rx {
            rx.try_recv().ok()
        } else {
            None
        };
        if let Some(result) = voice_result {
            self.voice_token_rx = None;
            self.pending_voice_token = Some(result);
        }

        // Check if content load completed
        let content_result = if let Some(ref mut rx) = self.content_load_rx {
            rx.try_recv().ok()
        } else {
            None
        };
        if let Some(result) = content_result {
            self.content_load_rx = None;
            self.content_loading = None;
            match result {
                ContentLoadResult::ServerDetail { id, name, rooms } => {
                    let has_rooms = !rooms.is_empty();
                    self.selected_server = Some(ServerDetail { id, name, rooms });
                    self.sidebar_view = SidebarView::RoomList;
                    self.selected_index = if has_rooms { 1 } else { 0 };
                    self.messages.clear();
                    self.selected_room_id = None;
                    self.selected_room_name = None;
                }
                ContentLoadResult::RoomMessages { server_id, room_id, room_name, messages } => {
                    self.messages = messages;
                    self.selected_room_id = Some(room_id.clone());
                    self.selected_room_name = Some(room_name);
                    if let Some(ref ws) = self.ws {
                        let _ = ws.send(crate::ws::WsClientMessage::SubscribeServerRoom {
                            server_id,
                            room_id: Some(room_id),
                        });
                    }
                }
                ContentLoadResult::DmMessages { dm_id, dm_name, messages } => {
                    self.messages = messages;
                    self.dm_unread.remove(&dm_id);
                    self.selected_dm_id = Some(dm_id);
                    self.selected_dm_name = Some(dm_name);
                    self.selected_server = None;
                    self.selected_room_id = None;
                    self.selected_room_name = None;
                }
                ContentLoadResult::Error(msg) => {
                    self.error_message = Some(msg);
                }
            }
        }
    }

    /// Returns a pending voice token result if one is ready.
    pub fn take_pending_voice_token(&mut self) -> Option<Result<serde_json::Value>> {
        self.pending_voice_token.take()
    }

    pub async fn cleanup(&mut self) -> Result<()> {
        self.leave_voice().await?;
        Ok(())
    }

    fn start_join_voice(&mut self) {
        let (room_id, connect_msg) = match (&self.selected_server, &self.selected_room_id, &self.selected_room_name) {
            (Some(detail), Some(room_id), Some(room_name)) => (
                room_id.clone(),
                format!("Connecting to {} → {}", detail.name, room_name),
            ),
            (Some(detail), Some(room_id), None) => (
                room_id.clone(),
                format!("Connecting to {}...", detail.name),
            ),
            _ => return,
        };

        if self.voice_room.is_some() || self.voice_connecting.is_some() {
            return;
        }

        self.voice_connecting = Some(connect_msg);
        self.voice_connect_room_id = Some(room_id.clone());

        // Spawn token fetch in background
        let base_url = self.api.base_url().to_string();
        let token = self.api.access_token().to_string();
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.voice_token_rx = Some(rx);

        tokio::spawn(async move {
            let client = reqwest::Client::new();
            let result = client
                .post(format!("{}/voice/rooms/{}/token", base_url, room_id))
                .header("Authorization", format!("Bearer {}", token))
                .json(&serde_json::json!({}))
                .send()
                .await;

            let outcome = match result {
                Ok(resp) if resp.status().is_success() => {
                    resp.json::<serde_json::Value>().await
                        .map_err(|e| anyhow::anyhow!("{}", e))
                }
                Ok(resp) => {
                    let status = resp.status();
                    let text = resp.text().await.unwrap_or_default();
                    Err(anyhow::anyhow!("Voice token failed ({}): {}", status, text))
                }
                Err(e) => Err(anyhow::anyhow!("{}", e)),
            };
            let _ = tx.send(outcome);
        });
    }

    pub async fn finish_join_voice(&mut self, token_resp: serde_json::Value) -> Result<()> {
        let voice_url = token_resp["voiceUrl"].as_str()
            .ok_or_else(|| anyhow::anyhow!("No voice URL"))?;
        let voice_token = token_resp["participantToken"].as_str()
            .ok_or_else(|| anyhow::anyhow!("No voice token"))?;

        let participants_json = token_resp["room"]["participants"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        let participants: Vec<lag_voice_core::ParticipantInfo> =
            serde_json::from_value(serde_json::Value::Array(participants_json))?;

        let settings_dir = config::config_dir();
        let audio_settings = AudioSettings::load(&settings_dir);
        let mut engine = AudioEngine::new();
        engine.set_input_volume(audio_settings.input_volume);
        engine.set_output_volume(audio_settings.output_volume);
        if let Some(ref dev) = audio_settings.input_device {
            let _ = engine.set_input_device(dev);
        }
        if let Some(ref dev) = audio_settings.output_device {
            let _ = engine.set_output_device(dev);
        }
        let engine = Arc::new(Mutex::new(engine));

        let (event_tx, event_rx) = VoiceRoom::create_event_channel();

        self.voice_connecting = Some("Joining voice room...".into());
        let room = VoiceRoom::connect(
            voice_url,
            voice_token,
            participants,
            engine,
            audio_settings.vad_threshold,
            event_tx,
        )
        .await?;

        self.muted = room.is_muted();
        self.deafened = room.is_deafened();
        self.voice_room = Some(room);
        self.voice_event_rx = Some(event_rx);
        self.connected_room = self.selected_room_name.clone();
        self.voice_connecting = None;

        Ok(())
    }

    async fn leave_voice(&mut self) -> Result<()> {
        if let Some(mut room) = self.voice_room.take() {
            room.disconnect().await?;
        }
        self.voice_event_rx = None;
        self.voice_participants.clear();
        self.voice_speaking.clear();
        self.mic_level = 0.0;
        self.output_level = 0.0;

        if let (Some(ref _detail), Some(ref room_id)) = (&self.selected_server, &self.connected_room.clone().and_then(|_| self.selected_room_id.clone())) {
            let _ = self.api.delete_no_body(&format!("/voice/rooms/{}/leave", room_id)).await;
        }

        self.connected_room = None;
        Ok(())
    }

    fn handle_voice_event(&mut self, evt: VoiceEvent) {
        match evt {
            VoiceEvent::Connected { participants } => {
                self.voice_participants = participants.iter()
                    .map(|p| (p.user_id.clone(), p.display_name.clone().unwrap_or_else(|| p.username.clone())))
                    .collect();
            }
            VoiceEvent::ParticipantJoined { participant } => {
                let id = participant.user_id.clone();
                let name = participant.display_name.unwrap_or(participant.username);
                if !self.voice_participants.iter().any(|(uid, _)| uid == &id) {
                    self.voice_participants.push((id, name));
                }
            }
            VoiceEvent::ParticipantLeft { user_id } => {
                self.voice_participants.retain(|(uid, _)| uid != &user_id);
                self.voice_speaking.remove(&user_id);
            }
            VoiceEvent::Speaking { user_id, speaking } => {
                if speaking {
                    self.voice_speaking.insert(user_id);
                } else {
                    self.voice_speaking.remove(&user_id);
                }
            }
            VoiceEvent::Disconnected { .. } => {
                self.voice_room = None;
                self.voice_event_rx = None;
                self.voice_participants.clear();
                self.voice_speaking.clear();
                self.connected_room = None;
            }
            VoiceEvent::TrackMuted { user_id: _, muted: _ } => {}
            VoiceEvent::Reconnecting => {}
            VoiceEvent::Reconnected { participants } => {
                self.voice_participants = participants.iter()
                    .map(|p| (p.user_id.clone(), p.display_name.clone().unwrap_or_else(|| p.username.clone())))
                    .collect();
            }
            VoiceEvent::MicLevel { level } => {
                self.mic_level = level;
            }
            VoiceEvent::OutputLevel { level } => {
                self.output_level = level;
            }
        }
    }

    async fn on_friend_popup_select(&mut self) -> Result<()> {
        let (action, friendship_id, confirming) = match self.friend_popup.as_ref() {
            Some(popup) => (
                popup.actions[popup.selected].clone(),
                popup.entry.friendship_id.clone(),
                popup.confirming.clone(),
            ),
            None => return Ok(()),
        };

        // Double-confirm for destructive actions
        let needs_confirm = matches!(action.as_str(), "Remove friend" | "Block user" | "Cancel request");
        if needs_confirm && confirming.as_deref() != Some(&action) {
            if let Some(ref mut popup) = self.friend_popup {
                popup.confirming = Some(action);
            }
            return Ok(());
        }

        match action.as_str() {
            "Send DM" => {
                let user_id = self.friend_popup.as_ref().map(|p| p.entry.user_id.clone()).unwrap_or_default();
                let dm_name = self.friend_popup.as_ref().map(|p| {
                    p.entry.display_name.clone().unwrap_or_else(|| p.entry.username.clone())
                }).unwrap_or_default();
                self.friend_popup = None;

                // Create or get DM conversation
                self.loading = Some(format!("Opening DM with {}...", dm_name));
                let result = self.api.post::<_, serde_json::Value>(
                    "/dms",
                    &serde_json::json!({ "userId": user_id }),
                ).await;
                match result {
                    Ok(conv) => {
                        let conv_id = conv["id"].as_str().unwrap_or("").to_string();
                        let msgs: Vec<serde_json::Value> = self.api
                            .get(&format!("/dms/{}/messages", conv_id))
                            .await
                            .unwrap_or_default();
                        self.loading = None;
                        self.messages = msgs;
                        self.dm_unread.remove(&conv_id);
                        self.selected_dm_id = Some(conv_id);
                        self.selected_dm_name = Some(dm_name);
                        self.selected_server = None;
                        self.selected_room_id = None;
                        self.selected_room_name = None;
                        self.sidebar_section = SidebarSection::Dms;

                        // Refresh DM list to include this conversation
                        self.dms = self.api.get("/dms").await.unwrap_or_default();
                    }
                    Err(e) => {
                        self.loading = None;
                        self.error_message = Some(format!("{}", e));
                    }
                }
            }
            "Accept" => {
                let result = self.api.post::<_, serde_json::Value>(
                    "/friends/accept",
                    &serde_json::json!({ "requestId": friendship_id }),
                ).await;
                self.friend_popup = None;
                match result {
                    Ok(_) => self.reload_friends().await,
                    Err(e) => self.error_message = Some(format!("{}", e)),
                }
            }
            "Decline" => {
                let result = self.api.post::<_, serde_json::Value>(
                    "/friends/decline",
                    &serde_json::json!({ "requestId": friendship_id }),
                ).await;
                self.friend_popup = None;
                match result {
                    Ok(_) => self.reload_friends().await,
                    Err(e) => self.error_message = Some(format!("{}", e)),
                }
            }
            "Remove friend" | "Cancel request" => {
                let result = self.api.delete_no_body(&format!("/friends/{}", friendship_id)).await;
                self.friend_popup = None;
                match result {
                    Ok(_) => self.reload_friends().await,
                    Err(e) => self.error_message = Some(format!("{}", e)),
                }
            }
            "Block user" => {
                let user_id = self.friend_popup.as_ref().map(|p| p.entry.user_id.clone()).unwrap_or_default();
                let result = self.api.post::<_, serde_json::Value>(
                    "/friends/block",
                    &serde_json::json!({ "userId": user_id }),
                ).await;
                self.friend_popup = None;
                match result {
                    Ok(_) => self.reload_friends().await,
                    Err(e) => self.error_message = Some(format!("{}", e)),
                }
            }
            "Cancel" => {
                self.friend_popup = None;
            }
            _ => {
                self.friend_popup = None;
            }
        }
        Ok(())
    }

    pub async fn reload_friends(&mut self) {
        self.friends = self.api.get("/friends").await.unwrap_or_default();
        let requests: serde_json::Value = self.api.get("/friends/requests").await.unwrap_or_default();
        self.build_friend_entries(&requests);
        if self.selected_index >= self.friend_entries.len() {
            self.selected_index = self.friend_entries.len().saturating_sub(1);
        }
    }

    async fn on_action_menu_select(&mut self) -> Result<()> {
        let (label, _) = match self.action_menu.as_ref() {
            Some(menu) => (menu.items[menu.selected].clone(), menu.selected),
            None => return Ok(()),
        };

        match label.as_str() {
            "Join voice" => {
                self.action_menu = None;
                self.start_join_voice();
            }
            "Leave voice" => {
                self.action_menu = None;
                self.leave_voice().await?;
            }
            "Chat only" => {
                self.action_menu = None;
            }
            _ => {
                self.action_menu = None;
            }
        }
        Ok(())
    }

    async fn on_select(&mut self) -> Result<()> {
        if self.content_loading.is_some() { return Ok(()); }

        match self.sidebar_section {
            SidebarSection::Servers => {
                match self.sidebar_view {
                    SidebarView::ServerList => {
                        if let Some(server) = self.servers.get(self.selected_index) {
                            let id = server["id"].as_str().unwrap_or("").to_string();
                            let name = server["name"].as_str().unwrap_or("?").to_string();

                            self.content_loading = Some(format!("Loading {}...", name));
                            let base_url = self.api.base_url().to_string();
                            let token = self.api.access_token().to_string();
                            let server_id = id.clone();
                            let server_name = name.clone();
                            let (tx, rx) = tokio::sync::oneshot::channel();
                            self.content_load_rx = Some(rx);

                            tokio::spawn(async move {
                                let client = reqwest::Client::new();
                                match client.get(format!("{}/servers/{}", base_url, server_id))
                                    .header("Authorization", format!("Bearer {}", token))
                                    .send().await
                                {
                                    Ok(resp) if resp.status().is_success() => {
                                        let details: serde_json::Value = resp.json().await.unwrap_or_default();
                                        let rooms = details["rooms"].as_array().cloned().unwrap_or_default();
                                        let _ = tx.send(ContentLoadResult::ServerDetail {
                                            id: server_id,
                                            name: server_name,
                                            rooms,
                                        });
                                    }
                                    Ok(resp) => {
                                        let text = resp.text().await.unwrap_or_default();
                                        let _ = tx.send(ContentLoadResult::Error(text));
                                    }
                                    Err(e) => {
                                        let _ = tx.send(ContentLoadResult::Error(format!("{}", e)));
                                    }
                                }
                            });
                        }
                    }
                    SidebarView::RoomList => {
                        if self.selected_index == 0 {
                            self.on_back().await;
                            return Ok(());
                        }
                        if let Some(ref detail) = self.selected_server {
                            let room_idx = self.selected_index - 1;
                            if let Some(room) = detail.rooms.get(room_idx) {
                                let room_id = room["id"].as_str().unwrap_or("").to_string();
                                let room_name = room["name"].as_str().unwrap_or("?").to_string();
                                let server_id = detail.id.clone();

                                self.content_loading = Some(format!("Loading #{}...", room_name));
                                let base_url = self.api.base_url().to_string();
                                let token = self.api.access_token().to_string();
                                let sid = server_id.clone();
                                let rid = room_id.clone();
                                let rname = room_name.clone();
                                let (tx, rx) = tokio::sync::oneshot::channel();
                                self.content_load_rx = Some(rx);

                                tokio::spawn(async move {
                                    let client = reqwest::Client::new();
                                    let msgs: Vec<serde_json::Value> = match client
                                        .get(format!("{}/servers/{}/rooms/{}/messages?limit=50", base_url, sid, rid))
                                        .header("Authorization", format!("Bearer {}", token))
                                        .send().await
                                    {
                                        Ok(resp) if resp.status().is_success() => resp.json().await.unwrap_or_default(),
                                        _ => Vec::new(),
                                    };
                                    let _ = tx.send(ContentLoadResult::RoomMessages {
                                        server_id: sid,
                                        room_id: rid,
                                        room_name: rname,
                                        messages: msgs,
                                    });
                                });
                            }
                        }
                    }
                }
            }
            SidebarSection::Friends => {
                if let Some(entry) = self.friend_entries.get(self.selected_index).cloned() {
                    let actions = match entry.kind {
                        FriendEntryKind::Friend => vec![
                            "Send DM".to_string(),
                            "Remove friend".to_string(),
                            "Block user".to_string(),
                            "Cancel".to_string(),
                        ],
                        FriendEntryKind::IncomingRequest => vec![
                            "Accept".to_string(),
                            "Decline".to_string(),
                            "Block user".to_string(),
                            "Cancel".to_string(),
                        ],
                        FriendEntryKind::OutgoingRequest => vec![
                            "Cancel request".to_string(),
                            "Cancel".to_string(),
                        ],
                    };
                    self.friend_popup = Some(FriendPopup {
                        entry,
                        actions,
                        selected: 0,
                        confirming: None,
                    });
                }
            }
            SidebarSection::Dms => {
                if let Some(conv) = self.dms.get(self.selected_index).cloned() {
                    let id = conv["id"].as_str().unwrap_or("").to_string();
                    let name = conv["otherUser"]["displayName"].as_str()
                        .or_else(|| conv["otherUser"]["username"].as_str())
                        .unwrap_or("?")
                        .to_string();

                    self.content_loading = Some(format!("Loading DM with {}...", name));
                    let base_url = self.api.base_url().to_string();
                    let token = self.api.access_token().to_string();
                    let dm_id = id.clone();
                    let dm_name = name.clone();
                    let (tx, rx) = tokio::sync::oneshot::channel();
                    self.content_load_rx = Some(rx);

                    tokio::spawn(async move {
                        let client = reqwest::Client::new();
                        let msgs: Vec<serde_json::Value> = match client
                            .get(format!("{}/dms/{}/messages", base_url, dm_id))
                            .header("Authorization", format!("Bearer {}", token))
                            .send().await
                        {
                            Ok(resp) if resp.status().is_success() => resp.json().await.unwrap_or_default(),
                            _ => Vec::new(),
                        };
                        let _ = tx.send(ContentLoadResult::DmMessages {
                            dm_id,
                            dm_name,
                            messages: msgs,
                        });
                    });
                }
            }
        }
        Ok(())
    }

    async fn on_back(&mut self) {
        match self.sidebar_view {
            SidebarView::RoomList => {
                self.sidebar_view = SidebarView::ServerList;
                self.selected_server = None;
                self.selected_room_id = None;
                self.selected_room_name = None;
                self.selected_index = 0;
                self.messages.clear();
            }
            SidebarView::ServerList => {
                self.messages.clear();
            }
        }
    }

    fn send_message(&mut self) {
        let content = self.input_buf.trim().to_string();
        if content.is_empty() {
            self.typing = false;
            return;
        }

        // Build the URL for the POST
        let url = if let (Some(ref detail), Some(ref room_id)) = (&self.selected_server, &self.selected_room_id) {
            format!("{}/servers/{}/rooms/{}/messages", self.api.base_url(), detail.id, room_id)
        } else if let Some(ref dm_id) = self.selected_dm_id {
            format!("{}/dms/{}/messages", self.api.base_url(), dm_id)
        } else {
            return;
        };

        self.input_buf.clear();
        self.typing = false;
        self.sending_message = true;

        // Spawn the HTTP request in the background
        let token = self.api.access_token().to_string();
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.send_result_rx = Some(rx);

        tokio::spawn(async move {
            let client = reqwest::Client::new();
            let result = client
                .post(&url)
                .header("Authorization", format!("Bearer {}", token))
                .json(&serde_json::json!({ "content": content }))
                .send()
                .await;

            let outcome = match result {
                Ok(resp) if resp.status().is_success() => Ok(()),
                Ok(resp) => {
                    let status = resp.status();
                    let text = resp.text().await.unwrap_or_default();
                    Err(anyhow::anyhow!("Failed ({}): {}", status, text))
                }
                Err(e) => Err(anyhow::anyhow!("{}", e)),
            };
            let _ = tx.send(outcome);
        });
    }

    fn handle_ws_message(&mut self, msg: WsServerMessage) {
        match msg {
            WsServerMessage::DmMessage(val) => {
                let dm_msg = if val.get("message").is_some() {
                    val["message"].clone()
                } else {
                    val
                };
                let msg_conv_id = dm_msg["conversationId"].as_str()
                    .or_else(|| dm_msg["conversation_id"].as_str())
                    .unwrap_or("")
                    .to_string();

                let is_active = self.selected_dm_id.as_deref() == Some(&msg_conv_id);
                let is_from_me = dm_msg["username"].as_str() == Some(self.username.as_str());

                // Append to active conversation
                if is_active {
                    self.messages.push(dm_msg.clone());
                }

                // Track unread for inactive conversations (only for messages from others)
                if !is_active && !is_from_me {
                    *self.dm_unread.entry(msg_conv_id.clone()).or_insert(0) += 1;
                }

                // Update lastMessage in the DM list for sidebar preview
                for conv in &mut self.dms {
                    if conv["id"].as_str() == Some(&msg_conv_id) {
                        conv["lastMessage"] = dm_msg;
                        break;
                    }
                }
            }
            WsServerMessage::RoomMessage(val) => {
                if let Some(ref room_id) = self.selected_room_id {
                    let msg_room_id = val["roomId"].as_str()
                        .or_else(|| val["room_id"].as_str())
                        .unwrap_or("");
                    if msg_room_id == room_id {
                        self.messages.push(val);
                    }
                }
            }
            WsServerMessage::ServerEvent { event, payload } => {
                match event.as_str() {
                    "room_message" => {
                        if let Some(ref room_id) = self.selected_room_id {
                            let msg_room_id = payload["room_id"].as_str()
                                .or_else(|| payload["roomId"].as_str())
                                .unwrap_or("");
                            if msg_room_id == room_id {
                                self.messages.push(payload);
                            }
                        }
                    }
                    _ => {}
                }
            }
            WsServerMessage::FriendRequestReceived(_) |
            WsServerMessage::FriendRequestAccepted(_) => {
                self.pending_friend_reload = true;
            }
            WsServerMessage::FriendOnline(val) => {
                let uid = val["userId"].as_str().unwrap_or("");
                for entry in &mut self.friend_entries {
                    if entry.user_id == uid {
                        entry.status = "online".to_string();
                    }
                }
            }
            WsServerMessage::FriendOffline(val) => {
                let uid = val["userId"].as_str().unwrap_or("");
                for entry in &mut self.friend_entries {
                    if entry.user_id == uid {
                        entry.status = "offline".to_string();
                    }
                }
            }
            WsServerMessage::FriendStatusChanged(val) => {
                let uid = val["userId"].as_str().unwrap_or("");
                let new_status = val["status"].as_str().unwrap_or("offline");
                for entry in &mut self.friend_entries {
                    if entry.user_id == uid {
                        entry.status = new_status.to_string();
                    }
                }
            }
            _ => {}
        }
    }
}