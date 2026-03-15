// Copyright (c) 2026 Lag
// SPDX-License-Identifier: MIT

use parking_lot::Mutex;
use rdev::{listen, Event, EventType, Key};
use std::sync::Arc;
use tracing::{debug, error, info};

#[derive(Debug, Clone)]
struct PttState {
    enabled: bool,
    key: Option<Key>,
    pressed: bool,
}

pub struct PushToTalkManager {
    state: Arc<Mutex<PttState>>,
    #[allow(clippy::type_complexity)]
    mute_callback: Arc<Mutex<Option<Box<dyn Fn(bool) + Send + Sync>>>>,
}

impl Default for PushToTalkManager {
    fn default() -> Self {
        Self::new()
    }
}

impl PushToTalkManager {
    pub fn new() -> Self {
        Self {
            state: Arc::new(Mutex::new(PttState {
                enabled: false,
                key: None,
                pressed: false,
            })),
            mute_callback: Arc::new(Mutex::new(None)),
        }
    }

    pub fn set_callback<F: Fn(bool) + Send + Sync + 'static>(&self, callback: F) {
        *self.mute_callback.lock() = Some(Box::new(callback));
    }

    pub fn set_enabled(&self, enabled: bool) {
        let mut state = self.state.lock();
        let was_enabled = state.enabled;
        state.enabled = enabled;

        if was_enabled && !enabled && state.pressed {
            state.pressed = false;
            drop(state);
            self.fire_mute(true);
        }

        info!(enabled, "PTT enabled state changed");
    }

    pub fn set_key(&self, key: Key) {
        let mut state = self.state.lock();
        state.key = Some(key);
        if state.pressed {
            state.pressed = false;
            drop(state);
            self.fire_mute(true);
        }
        debug!(?key, "PTT key changed");
    }

    pub fn set_key_from_string(&self, key_str: &str) -> bool {
        if let Some(key) = string_to_key(key_str) {
            self.set_key(key);
            true
        } else {
            false
        }
    }

    pub fn is_enabled(&self) -> bool {
        self.state.lock().enabled
    }

    pub fn is_pressed(&self) -> bool {
        self.state.lock().pressed
    }

    /// Process a raw key event. Returns true if the event was consumed by PTT.
    pub fn process_event(&self, event_type: &EventType) -> bool {
        let mut state = self.state.lock();
        if !state.enabled {
            return false;
        }

        let target_key = match state.key {
            Some(k) => k,
            None => return false,
        };

        match event_type {
            EventType::KeyPress(key) if *key == target_key => {
                if !state.pressed {
                    state.pressed = true;
                    drop(state);
                    self.fire_mute(false);
                }
                true
            }
            EventType::KeyRelease(key) if *key == target_key => {
                if state.pressed {
                    state.pressed = false;
                    drop(state);
                    self.fire_mute(true);
                }
                true
            }
            _ => false,
        }
    }

    fn fire_mute(&self, muted: bool) {
        if let Some(ref cb) = *self.mute_callback.lock() {
            cb(muted);
        }
    }

    /// Start the global key listener on a background thread.
    /// This blocks the thread, so call from a dedicated thread.
    pub fn start_listener(self: Arc<Self>) {
        let ptt = self;
        std::thread::spawn(move || {
            info!("PTT global key listener started");
            if let Err(e) = listen(move |event: Event| {
                ptt.process_event(&event.event_type);
            }) {
                error!(?e, "PTT listener error");
            }
        });
    }
}

pub fn string_to_key(s: &str) -> Option<Key> {
    match s {
        "Backquote" | "IntlBackslash" => Some(Key::BackQuote),
        "Tab" => Some(Key::Tab),
        "CapsLock" => Some(Key::CapsLock),
        "ShiftLeft" => Some(Key::ShiftLeft),
        "ShiftRight" => Some(Key::ShiftRight),
        "ControlLeft" => Some(Key::ControlLeft),
        "ControlRight" => Some(Key::ControlRight),
        "AltLeft" => Some(Key::Alt),
        "AltRight" => Some(Key::AltGr),
        "Space" => Some(Key::Space),
        "F1" => Some(Key::F1),
        "F2" => Some(Key::F2),
        "F3" => Some(Key::F3),
        "F4" => Some(Key::F4),
        "F5" => Some(Key::F5),
        "F6" => Some(Key::F6),
        "F7" => Some(Key::F7),
        "F8" => Some(Key::F8),
        "F9" => Some(Key::F9),
        "F10" => Some(Key::F10),
        "F11" => Some(Key::F11),
        "F12" => Some(Key::F12),
        "KeyA" => Some(Key::KeyA),
        "KeyB" => Some(Key::KeyB),
        "KeyC" => Some(Key::KeyC),
        "KeyD" => Some(Key::KeyD),
        "KeyE" => Some(Key::KeyE),
        "KeyF" => Some(Key::KeyF),
        "KeyG" => Some(Key::KeyG),
        "KeyH" => Some(Key::KeyH),
        "KeyI" => Some(Key::KeyI),
        "KeyJ" => Some(Key::KeyJ),
        "KeyK" => Some(Key::KeyK),
        "KeyL" => Some(Key::KeyL),
        "KeyM" => Some(Key::KeyM),
        "KeyN" => Some(Key::KeyN),
        "KeyO" => Some(Key::KeyO),
        "KeyP" => Some(Key::KeyP),
        "KeyQ" => Some(Key::KeyQ),
        "KeyR" => Some(Key::KeyR),
        "KeyS" => Some(Key::KeyS),
        "KeyT" => Some(Key::KeyT),
        "KeyU" => Some(Key::KeyU),
        "KeyV" => Some(Key::KeyV),
        "KeyW" => Some(Key::KeyW),
        "KeyX" => Some(Key::KeyX),
        "KeyY" => Some(Key::KeyY),
        "KeyZ" => Some(Key::KeyZ),
        _ => None,
    }
}

pub fn key_to_string(key: &Key) -> Option<&'static str> {
    match key {
        Key::BackQuote => Some("Backquote"),
        Key::Tab => Some("Tab"),
        Key::CapsLock => Some("CapsLock"),
        Key::ShiftLeft => Some("ShiftLeft"),
        Key::ShiftRight => Some("ShiftRight"),
        Key::ControlLeft => Some("ControlLeft"),
        Key::ControlRight => Some("ControlRight"),
        Key::Alt => Some("AltLeft"),
        Key::AltGr => Some("AltRight"),
        Key::Space => Some("Space"),
        Key::F1 => Some("F1"),
        Key::F2 => Some("F2"),
        Key::F3 => Some("F3"),
        Key::F4 => Some("F4"),
        Key::F5 => Some("F5"),
        Key::F6 => Some("F6"),
        Key::F7 => Some("F7"),
        Key::F8 => Some("F8"),
        Key::F9 => Some("F9"),
        Key::F10 => Some("F10"),
        Key::F11 => Some("F11"),
        Key::F12 => Some("F12"),
        Key::KeyA => Some("KeyA"),
        Key::KeyB => Some("KeyB"),
        Key::KeyC => Some("KeyC"),
        Key::KeyD => Some("KeyD"),
        Key::KeyE => Some("KeyE"),
        Key::KeyF => Some("KeyF"),
        Key::KeyG => Some("KeyG"),
        Key::KeyH => Some("KeyH"),
        Key::KeyI => Some("KeyI"),
        Key::KeyJ => Some("KeyJ"),
        Key::KeyK => Some("KeyK"),
        Key::KeyL => Some("KeyL"),
        Key::KeyM => Some("KeyM"),
        Key::KeyN => Some("KeyN"),
        Key::KeyO => Some("KeyO"),
        Key::KeyP => Some("KeyP"),
        Key::KeyQ => Some("KeyQ"),
        Key::KeyR => Some("KeyR"),
        Key::KeyS => Some("KeyS"),
        Key::KeyT => Some("KeyT"),
        Key::KeyU => Some("KeyU"),
        Key::KeyV => Some("KeyV"),
        Key::KeyW => Some("KeyW"),
        Key::KeyX => Some("KeyX"),
        Key::KeyY => Some("KeyY"),
        Key::KeyZ => Some("KeyZ"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_press_when_enabled_unmutes() {
        let ptt = PushToTalkManager::new();
        let muted = Arc::new(Mutex::new(true));
        let muted_clone = muted.clone();
        ptt.set_callback(move |m| *muted_clone.lock() = m);
        ptt.set_enabled(true);
        ptt.set_key(Key::KeyV);

        ptt.process_event(&EventType::KeyPress(Key::KeyV));
        assert!(!*muted.lock());
    }

    #[test]
    fn key_release_when_enabled_mutes() {
        let ptt = PushToTalkManager::new();
        let muted = Arc::new(Mutex::new(false));
        let muted_clone = muted.clone();
        ptt.set_callback(move |m| *muted_clone.lock() = m);
        ptt.set_enabled(true);
        ptt.set_key(Key::KeyV);

        ptt.process_event(&EventType::KeyPress(Key::KeyV));
        ptt.process_event(&EventType::KeyRelease(Key::KeyV));
        assert!(*muted.lock());
    }

    #[test]
    fn key_press_when_disabled_does_nothing() {
        let ptt = PushToTalkManager::new();
        let called = Arc::new(Mutex::new(false));
        let called_clone = called.clone();
        ptt.set_callback(move |_| *called_clone.lock() = true);
        ptt.set_key(Key::KeyV);

        let consumed = ptt.process_event(&EventType::KeyPress(Key::KeyV));
        assert!(!consumed);
        assert!(!*called.lock());
    }

    #[test]
    fn changing_key_updates_binding() {
        let ptt = PushToTalkManager::new();
        let muted = Arc::new(Mutex::new(true));
        let muted_clone = muted.clone();
        ptt.set_callback(move |m| *muted_clone.lock() = m);
        ptt.set_enabled(true);
        ptt.set_key(Key::KeyV);

        ptt.set_key(Key::KeyG);
        let consumed = ptt.process_event(&EventType::KeyPress(Key::KeyV));
        assert!(!consumed);

        ptt.process_event(&EventType::KeyPress(Key::KeyG));
        assert!(!*muted.lock());
    }

    #[test]
    fn disabling_while_pressed_mutes() {
        let ptt = PushToTalkManager::new();
        let muted = Arc::new(Mutex::new(true));
        let muted_clone = muted.clone();
        ptt.set_callback(move |m| *muted_clone.lock() = m);
        ptt.set_enabled(true);
        ptt.set_key(Key::KeyV);

        ptt.process_event(&EventType::KeyPress(Key::KeyV));
        assert!(!*muted.lock());

        ptt.set_enabled(false);
        assert!(*muted.lock());
    }

    #[test]
    fn unknown_key_ignored() {
        let ptt = PushToTalkManager::new();
        let called = Arc::new(Mutex::new(false));
        let called_clone = called.clone();
        ptt.set_callback(move |_| *called_clone.lock() = true);
        ptt.set_enabled(true);
        ptt.set_key(Key::KeyV);

        let consumed = ptt.process_event(&EventType::KeyPress(Key::KeyA));
        assert!(!consumed);
        assert!(!*called.lock());
    }

    #[test]
    fn string_to_key_roundtrip() {
        let key = Key::KeyV;
        let s = key_to_string(&key).unwrap();
        let restored = string_to_key(s).unwrap();
        assert_eq!(format!("{:?}", key), format!("{:?}", restored));
    }

    #[test]
    fn set_key_from_string_valid() {
        let ptt = PushToTalkManager::new();
        assert!(ptt.set_key_from_string("KeyV"));
    }

    #[test]
    fn set_key_from_string_invalid() {
        let ptt = PushToTalkManager::new();
        assert!(!ptt.set_key_from_string("NotARealKey"));
    }

    #[test]
    fn key_to_string_roundtrip_all() {
        let keys = vec![
            Key::Tab, Key::CapsLock, Key::ShiftLeft, Key::Space,
            Key::F1, Key::F12, Key::KeyA, Key::KeyZ,
        ];
        for key in keys {
            let s = key_to_string(&key).expect(&format!("key_to_string failed for {:?}", key));
            let restored = string_to_key(s).expect(&format!("string_to_key failed for {}", s));
            assert_eq!(format!("{:?}", key), format!("{:?}", restored));
        }
    }
}