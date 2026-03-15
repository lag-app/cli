// Copyright (c) 2026 Lag
// SPDX-License-Identifier: MIT

use crate::audio::engine::AudioEngine;
use crate::audio::vad::VoiceActivityDetector;
use anyhow::{anyhow, Result};
use futures_util::StreamExt;
use livekit::options::TrackPublishOptions;
use livekit::participant::Participant;
use livekit::track::{LocalAudioTrack, LocalTrack, RemoteTrack, TrackSource};
use livekit::webrtc::audio_frame::AudioFrame;
use livekit::webrtc::audio_source::native::NativeAudioSource;
use livekit::webrtc::audio_source::RtcAudioSource;
use livekit::webrtc::audio_stream::native::NativeAudioStream;
use livekit::webrtc::prelude::AudioSourceOptions;
use livekit::{Room, RoomEvent, RoomOptions};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender};
use tokio::sync::oneshot;
use tokio::task::JoinHandle;
use tracing::{debug, error, info, warn};

const SAMPLE_RATE: u32 = 48000;
const NUM_CHANNELS: u32 = 1;
const SAMPLES_PER_CHANNEL_10MS: u32 = SAMPLE_RATE / 100; // 480
const FRAME_SIZE_SAMPLES: usize = SAMPLES_PER_CHANNEL_10MS as usize;
const AUDIO_SOURCE_QUEUE_SIZE: u32 = 10;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParticipantInfo {
    #[serde(rename = "userId")]
    pub user_id: String,
    pub username: String,
    #[serde(rename = "displayName")]
    pub display_name: Option<String>,
    #[serde(rename = "avatarUrl")]
    pub avatar_url: Option<String>,
}

#[derive(Debug, Clone)]
pub enum VoiceEvent {
    Connected { participants: Vec<ParticipantInfo> },
    Disconnected { reason: String },
    ParticipantJoined { participant: ParticipantInfo },
    ParticipantLeft { user_id: String },
    Speaking { user_id: String, speaking: bool },
    TrackMuted { user_id: String, muted: bool },
    Reconnecting,
    Reconnected { participants: Vec<ParticipantInfo> },
    MicLevel { level: f32 },
    OutputLevel { level: f32 },
}

pub struct VoiceRoom {
    room: Arc<Room>,
    _audio_source: NativeAudioSource,
    audio_track: LocalAudioTrack,
    audio_engine: Arc<Mutex<AudioEngine>>,
    capture_stop_tx: Option<oneshot::Sender<()>>,
    capture_task: Option<JoinHandle<()>>,
    event_task: Option<JoinHandle<()>>,
    playback_tasks: Arc<Mutex<HashMap<String, JoinHandle<()>>>>,
    muted: Arc<AtomicBool>,
    deafened: Arc<AtomicBool>,
    local_identity: String,
    _participant_lookup: Arc<Mutex<HashMap<String, ParticipantInfo>>>,
}

impl VoiceRoom {
    pub async fn connect(
        voice_url: &str,
        token: &str,
        participants: Vec<ParticipantInfo>,
        audio_engine: Arc<Mutex<AudioEngine>>,
        vad_threshold: f32,
        event_tx: UnboundedSender<VoiceEvent>,
    ) -> Result<Self> {
        info!(%voice_url, "Connecting to voice room");

        let audio_source = NativeAudioSource::new(
            AudioSourceOptions {
                echo_cancellation: false,
                noise_suppression: false,
                auto_gain_control: false,
            },
            SAMPLE_RATE,
            NUM_CHANNELS,
            AUDIO_SOURCE_QUEUE_SIZE,
        );

        let audio_track = LocalAudioTrack::create_audio_track(
            "microphone",
            RtcAudioSource::Native(audio_source.clone()),
        );

        let mut room_options = RoomOptions::default();
        room_options.auto_subscribe = true;
        room_options.dynacast = false;

        let (room, events_rx) = Room::connect(voice_url, token, room_options)
            .await
            .map_err(|e| {
                error!("Voice room connection failed: {}", e);
                anyhow!("Failed to connect to voice server")
            })?;

        let room = Arc::new(room);
        let local_identity = room.local_participant().identity().to_string();

        info!(
            room_name = %room.name(),
            identity = %local_identity,
            "Connected to voice room"
        );

        room.local_participant()
            .publish_track(
                LocalTrack::Audio(audio_track.clone()),
                TrackPublishOptions {
                    source: TrackSource::Microphone,
                    ..Default::default()
                },
            )
            .await
            .map_err(|e| {
                error!("Audio track publish failed: {}", e);
                anyhow!("Failed to publish audio track")
            })?;

        info!("Audio track published");

        let participant_lookup = Arc::new(Mutex::new(HashMap::new()));
        for p in &participants {
            participant_lookup
                .lock()
                .insert(p.user_id.clone(), p.clone());
        }

        let muted = Arc::new(AtomicBool::new(false));
        let deafened = Arc::new(AtomicBool::new(false));
        let playback_tasks: Arc<Mutex<HashMap<String, JoinHandle<()>>>> =
            Arc::new(Mutex::new(HashMap::new()));

        let (capture_stop_tx, capture_stop_rx) = oneshot::channel();
        let capture_task = {
            let source = audio_source.clone();
            let engine = audio_engine.clone();
            let muted_flag = muted.clone();
            let tx = event_tx.clone();
            let identity = local_identity.clone();
            tokio::spawn(Self::capture_loop(
                source,
                engine,
                muted_flag,
                capture_stop_rx,
                vad_threshold,
                tx,
                identity,
            ))
        };

        let event_task = {
            let room_clone = room.clone();
            let lookup = participant_lookup.clone();
            let tx = event_tx.clone();
            let engine_clone = audio_engine.clone();
            let deafened_clone = deafened.clone();
            let tasks_clone = playback_tasks.clone();
            tokio::spawn(Self::event_loop(
                events_rx,
                room_clone,
                lookup,
                tx,
                engine_clone,
                deafened_clone,
                tasks_clone,
            ))
        };

        {
            let mut engine = audio_engine.lock();
            if let Err(e) = engine.start_capture() {
                warn!(
                    "Microphone unavailable, joining in listen-only mode: {}. \
                     On macOS, grant microphone access to your terminal app in \
                     System Settings > Privacy & Security > Microphone.",
                    e
                );
                muted.store(true, Ordering::Relaxed);
            }
            engine.start_playback().map_err(|e| {
                error!("Audio playback start failed: {}", e);
                anyhow!("Failed to start audio playback: {}", e)
            })?;
        }

        // Emit connected with merged participant list
        {
            let mut all_participants = participant_lookup.lock().clone();
            for rp in room.remote_participants().values() {
                let id = rp.identity().to_string();
                if id.ends_with("-viewer") {
                    continue;
                }
                all_participants.entry(id.clone()).or_insert_with(|| {
                    let name = rp.name().to_string();
                    ParticipantInfo {
                        user_id: id,
                        username: name.clone(),
                        display_name: if name.is_empty() { None } else { Some(name) },
                        avatar_url: None,
                    }
                });
            }
            let all: Vec<ParticipantInfo> = all_participants.values().cloned().collect();
            let _ = event_tx.send(VoiceEvent::Connected { participants: all });
        }

        Ok(Self {
            room,
            _audio_source: audio_source,
            audio_track,
            audio_engine,
            capture_stop_tx: Some(capture_stop_tx),
            capture_task: Some(capture_task),
            event_task: Some(event_task),
            playback_tasks,
            muted,
            deafened,
            local_identity,
            _participant_lookup: participant_lookup,
        })
    }

    pub async fn disconnect(&mut self) -> Result<()> {
        info!("Disconnecting from voice room");

        if let Some(tx) = self.capture_stop_tx.take() {
            let _ = tx.send(());
        }
        if let Some(task) = self.capture_task.take() {
            task.abort();
        }

        if let Some(task) = self.event_task.take() {
            task.abort();
        }

        {
            let mut tasks = self.playback_tasks.lock();
            for (_, task) in tasks.drain() {
                task.abort();
            }
        }

        {
            let mut engine = self.audio_engine.lock();
            let _ = engine.stop_capture();
            let _ = engine.stop_playback();
        }

        self.room.close().await.map_err(|e| {
            error!("Voice room close failed: {}", e);
            anyhow!("Failed to disconnect from voice")
        })?;

        info!("Disconnected from voice room");
        Ok(())
    }

    pub fn set_muted(&self, muted: bool) {
        self.muted.store(muted, Ordering::Relaxed);
        if muted {
            self.audio_track.mute();
            let mut engine = self.audio_engine.lock();
            let _ = engine.stop_capture();
        } else {
            self.audio_track.unmute();
            let mut engine = self.audio_engine.lock();
            let _ = engine.start_capture();
        }
        info!(muted, "Mute state changed");
    }

    pub fn set_deafened(&self, deafened: bool) {
        self.deafened.store(deafened, Ordering::Relaxed);
        if deafened {
            self.muted.store(true, Ordering::Relaxed);
            self.audio_track.mute();
            let mut engine = self.audio_engine.lock();
            let _ = engine.stop_capture();
            let _ = engine.stop_playback();
        } else {
            let mut engine = self.audio_engine.lock();
            let _ = engine.start_playback();
            let _ = engine.start_capture();
            self.muted.store(false, Ordering::Relaxed);
            self.audio_track.unmute();
        }
        info!(deafened, "Deafen state changed");
    }

    pub fn is_muted(&self) -> bool {
        self.muted.load(Ordering::Relaxed)
    }

    pub fn is_deafened(&self) -> bool {
        self.deafened.load(Ordering::Relaxed)
    }

    pub fn room(&self) -> Arc<Room> {
        self.room.clone()
    }

    pub fn local_identity(&self) -> &str {
        &self.local_identity
    }

    /// Create an event channel pair for receiving VoiceEvents.
    pub fn create_event_channel() -> (UnboundedSender<VoiceEvent>, UnboundedReceiver<VoiceEvent>) {
        unbounded_channel()
    }

    async fn capture_loop(
        source: NativeAudioSource,
        engine: Arc<Mutex<AudioEngine>>,
        muted: Arc<AtomicBool>,
        mut stop_rx: oneshot::Receiver<()>,
        vad_threshold: f32,
        event_tx: UnboundedSender<VoiceEvent>,
        local_identity: String,
    ) {
        info!("Audio capture loop started");
        let mut vad = VoiceActivityDetector::new().with_threshold(vad_threshold);
        let mut accumulator = vec![0.0f32; 0];
        let mut read_buf = vec![0.0f32; FRAME_SIZE_SAMPLES * 2];
        let mut i16_buf = vec![0i16; FRAME_SIZE_SAMPLES];
        let silent_frame = AudioFrame {
            data: vec![0i16; FRAME_SIZE_SAMPLES].into(),
            sample_rate: SAMPLE_RATE,
            num_channels: NUM_CHANNELS,
            samples_per_channel: SAMPLES_PER_CHANNEL_10MS,
        };
        let mut prev_speaking = false;
        let mut level_counter: u32 = 0;
        let mut interval = tokio::time::interval(Duration::from_millis(5));
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

        loop {
            tokio::select! {
                biased;
                _ = &mut stop_rx => {
                    info!("Capture loop stop signal received");
                    break;
                }
                _ = interval.tick() => {
                    if muted.load(Ordering::Relaxed) {
                        let _ = source.capture_frame(&silent_frame).await;
                        accumulator.clear();
                        level_counter += 1;
                        if level_counter.is_multiple_of(20) {
                            let _ = event_tx.send(VoiceEvent::MicLevel { level: 0.0 });
                        }
                        continue;
                    }

                    let count = engine.lock().read_capture_buffer(&mut read_buf);
                    if count > 0 {
                        accumulator.extend_from_slice(&read_buf[..count]);
                    }

                    while accumulator.len() >= FRAME_SIZE_SAMPLES {
                        let frame_data: Vec<f32> = accumulator.drain(..FRAME_SIZE_SAMPLES).collect();

                        // Compute RMS for VU meter (~10Hz)
                        level_counter += 1;
                        if level_counter.is_multiple_of(5) {
                            let rms = (frame_data.iter().map(|s| s * s).sum::<f32>() / frame_data.len() as f32).sqrt();
                            let level = (rms * 5.0).min(1.0); // scale up for visibility
                            let _ = event_tx.send(VoiceEvent::MicLevel { level });
                        }

                        let speaking = vad.process_frame(&frame_data);
                        if speaking != prev_speaking {
                            let _ = event_tx.send(VoiceEvent::Speaking {
                                user_id: local_identity.clone(),
                                speaking,
                            });
                            prev_speaking = speaking;
                        }

                        for (i, &s) in frame_data.iter().enumerate() {
                            i16_buf[i] = (s.clamp(-1.0, 1.0) * 32767.0) as i16;
                        }

                        let frame = AudioFrame {
                            data: i16_buf.clone().into(),
                            sample_rate: SAMPLE_RATE,
                            num_channels: NUM_CHANNELS,
                            samples_per_channel: SAMPLES_PER_CHANNEL_10MS,
                        };

                        if let Err(e) = source.capture_frame(&frame).await {
                            debug!("Failed to capture audio frame: {}", e);
                        }
                    }
                }
            }
        }
        info!("Audio capture loop ended");
    }

    async fn event_loop(
        mut events_rx: UnboundedReceiver<RoomEvent>,
        _room: Arc<Room>,
        participant_lookup: Arc<Mutex<HashMap<String, ParticipantInfo>>>,
        event_tx: UnboundedSender<VoiceEvent>,
        audio_engine: Arc<Mutex<AudioEngine>>,
        deafened: Arc<AtomicBool>,
        playback_tasks: Arc<Mutex<HashMap<String, JoinHandle<()>>>>,
    ) {
        info!("Room event loop started");
        let mut prev_speakers: std::collections::HashSet<String> = std::collections::HashSet::new();

        while let Some(event) = events_rx.recv().await {
            match event {
                RoomEvent::ParticipantConnected(participant) => {
                    let identity = participant.identity().to_string();
                    if identity.ends_with("-viewer") {
                        continue;
                    }
                    let name = participant.name().to_string();
                    info!(%identity, %name, "Participant connected");

                    let info = {
                        let mut lookup = participant_lookup.lock();
                        if let Some(existing) = lookup.get(&identity) {
                            existing.clone()
                        } else {
                            let new_info = ParticipantInfo {
                                user_id: identity.clone(),
                                username: name.clone(),
                                display_name: if name.is_empty() { None } else { Some(name) },
                                avatar_url: None,
                            };
                            lookup.insert(identity.clone(), new_info.clone());
                            new_info
                        }
                    };

                    let _ = event_tx.send(VoiceEvent::ParticipantJoined { participant: info });
                }

                RoomEvent::ParticipantDisconnected(participant) => {
                    let identity = participant.identity().to_string();
                    if identity.ends_with("-viewer") {
                        continue;
                    }
                    info!(%identity, "Participant disconnected");

                    participant_lookup.lock().remove(&identity);

                    if let Some(task) = playback_tasks.lock().remove(&identity) {
                        task.abort();
                    }

                    let _ = event_tx.send(VoiceEvent::ParticipantLeft { user_id: identity });
                }

                RoomEvent::TrackSubscribed {
                    track, participant, ..
                } => {
                    let identity = participant.identity().to_string();
                    match track {
                        RemoteTrack::Audio(audio_track) => {
                            info!(%identity, "Subscribed to remote audio track");

                            let audio_stream = NativeAudioStream::new(
                                audio_track.rtc_track(),
                                SAMPLE_RATE as i32,
                                NUM_CHANNELS as i32,
                            );

                            let engine = audio_engine.clone();
                            let deafened_flag = deafened.clone();
                            let level_tx = event_tx.clone();
                            let task = tokio::spawn(Self::playback_loop(
                                audio_stream,
                                engine,
                                deafened_flag,
                                identity.clone(),
                                level_tx,
                            ));

                            playback_tasks.lock().insert(identity, task);
                        }
                        RemoteTrack::Video(_) => {
                            debug!(%identity, "Ignoring remote video track in voice-core");
                        }
                    }
                }

                RoomEvent::TrackUnsubscribed {
                    track, participant, ..
                } => {
                    let identity = participant.identity().to_string();
                    match &track {
                        RemoteTrack::Audio(_) => {
                            info!(%identity, "Unsubscribed from remote audio track");
                            if let Some(task) = playback_tasks.lock().remove(&identity) {
                                task.abort();
                            }
                        }
                        RemoteTrack::Video(_) => {}
                    }
                }

                RoomEvent::TrackMuted { participant, .. } => {
                    let identity = match &participant {
                        Participant::Local(p) => p.identity().to_string(),
                        Participant::Remote(p) => p.identity().to_string(),
                    };
                    let _ = event_tx.send(VoiceEvent::TrackMuted {
                        user_id: identity,
                        muted: true,
                    });
                }

                RoomEvent::TrackUnmuted { participant, .. } => {
                    let identity = match &participant {
                        Participant::Local(p) => p.identity().to_string(),
                        Participant::Remote(p) => p.identity().to_string(),
                    };
                    let _ = event_tx.send(VoiceEvent::TrackMuted {
                        user_id: identity,
                        muted: false,
                    });
                }

                RoomEvent::ActiveSpeakersChanged { speakers } => {
                    let current: std::collections::HashSet<String> = speakers
                        .iter()
                        .map(|s| match s {
                            Participant::Local(p) => p.identity().to_string(),
                            Participant::Remote(p) => p.identity().to_string(),
                        })
                        .collect();

                    // Mark new speakers
                    for id in &current {
                        if !prev_speakers.contains(id) {
                            let _ = event_tx.send(VoiceEvent::Speaking {
                                user_id: id.clone(),
                                speaking: true,
                            });
                        }
                    }

                    // Clear people who stopped speaking
                    for id in &prev_speakers {
                        if !current.contains(id) {
                            let _ = event_tx.send(VoiceEvent::Speaking {
                                user_id: id.clone(),
                                speaking: false,
                            });
                        }
                    }

                    prev_speakers = current;
                }

                RoomEvent::Reconnecting => {
                    warn!("Room reconnecting");
                    let _ = event_tx.send(VoiceEvent::Reconnecting);
                }

                RoomEvent::Reconnected => {
                    info!("Room reconnected");
                    let participants: Vec<ParticipantInfo> =
                        participant_lookup.lock().values().cloned().collect();
                    let _ = event_tx.send(VoiceEvent::Reconnected { participants });
                }

                RoomEvent::Disconnected { reason } => {
                    let reason_str = format!("{:?}", reason);
                    warn!(%reason_str, "Room disconnected");
                    let _ = event_tx.send(VoiceEvent::Disconnected { reason: reason_str });
                }

                _ => {
                    debug!("Unhandled room event");
                }
            }
        }

        info!("Room event loop ended");
    }

    async fn playback_loop(
        mut audio_stream: NativeAudioStream,
        engine: Arc<Mutex<AudioEngine>>,
        deafened: Arc<AtomicBool>,
        participant_id: String,
        event_tx: UnboundedSender<VoiceEvent>,
    ) {
        info!(%participant_id, "Playback loop started for remote participant");
        let mut level_counter: u32 = 0;

        while let Some(frame) = audio_stream.next().await {
            if deafened.load(Ordering::Relaxed) {
                continue;
            }

            let f32_data: Vec<f32> = frame.data.iter().map(|&s| s as f32 / 32768.0).collect();

            level_counter += 1;
            if level_counter.is_multiple_of(5) {
                let rms = (f32_data.iter().map(|s| s * s).sum::<f32>()
                    / f32_data.len().max(1) as f32)
                    .sqrt();
                let level = (rms * 5.0).min(1.0);
                let _ = event_tx.send(VoiceEvent::OutputLevel { level });
            }

            engine.lock().write_playback_buffer(&f32_data);
        }

        info!(%participant_id, "Playback loop ended for remote participant");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn participant_info_serde_roundtrip() {
        let info = ParticipantInfo {
            user_id: "u123".to_string(),
            username: "alice".to_string(),
            display_name: Some("Alice".to_string()),
            avatar_url: Some("https://example.com/avatar.png".to_string()),
        };
        let json = serde_json::to_string(&info).unwrap();
        let restored: ParticipantInfo = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.user_id, "u123");
        assert_eq!(restored.username, "alice");
        assert_eq!(restored.display_name.as_deref(), Some("Alice"));
        assert_eq!(
            restored.avatar_url.as_deref(),
            Some("https://example.com/avatar.png")
        );
    }

    #[test]
    fn voice_event_debug_format() {
        let events: Vec<VoiceEvent> = vec![
            VoiceEvent::Connected {
                participants: vec![],
            },
            VoiceEvent::Disconnected {
                reason: "test".into(),
            },
            VoiceEvent::ParticipantJoined {
                participant: ParticipantInfo {
                    user_id: "u1".into(),
                    username: "bob".into(),
                    display_name: None,
                    avatar_url: None,
                },
            },
            VoiceEvent::ParticipantLeft {
                user_id: "u1".into(),
            },
            VoiceEvent::Speaking {
                user_id: "u1".into(),
                speaking: true,
            },
            VoiceEvent::TrackMuted {
                user_id: "u1".into(),
                muted: true,
            },
            VoiceEvent::Reconnecting,
            VoiceEvent::Reconnected {
                participants: vec![],
            },
            VoiceEvent::MicLevel { level: 0.5 },
            VoiceEvent::OutputLevel { level: 0.3 },
        ];
        for event in &events {
            let debug = format!("{:?}", event);
            assert!(!debug.is_empty());
        }
    }
}
