// Copyright (c) 2026 Lag
// SPDX-License-Identifier: MIT

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use super::app::{App, AppEvent, SidebarSection};

pub fn handle_key(key: KeyEvent, app: &App) -> Option<AppEvent> {
    match (key.code, key.modifiers) {
        (KeyCode::Char('c'), KeyModifiers::CONTROL) | (KeyCode::Char('q'), KeyModifiers::NONE) => {
            if !app.is_typing() && app.action_menu.is_none() {
                return Some(AppEvent::Quit);
            }
        }
        _ => {}
    }

    // Dismiss error on any key
    if app.error_message.is_some() {
        return Some(AppEvent::Back);
    }

    // Add friend input mode
    if app.adding_friend {
        return match key.code {
            KeyCode::Esc => Some(AppEvent::AddFriendCancel),
            KeyCode::Enter => Some(AppEvent::AddFriendSubmit),
            KeyCode::Backspace => Some(AppEvent::AddFriendDelete),
            KeyCode::Char(c) => Some(AppEvent::AddFriendChar(c)),
            _ => None,
        };
    }

    // Friend popup mode
    if app.friend_popup.is_some() {
        return match key.code {
            KeyCode::Up | KeyCode::Char('k') => Some(AppEvent::FriendPopupNav(-1)),
            KeyCode::Down | KeyCode::Char('j') => Some(AppEvent::FriendPopupNav(1)),
            KeyCode::Enter => Some(AppEvent::FriendPopupSelect),
            KeyCode::Esc => Some(AppEvent::Back),
            _ => None,
        };
    }

    // Action menu mode
    if let Some(ref _menu) = app.action_menu {
        return match key.code {
            KeyCode::Up | KeyCode::Char('k') => Some(AppEvent::NavigateUp),
            KeyCode::Down | KeyCode::Char('j') => Some(AppEvent::NavigateDown),
            KeyCode::Enter => Some(AppEvent::ActionMenuSelect),
            KeyCode::Esc => Some(AppEvent::Back),
            _ => None,
        };
    }

    // Typing mode
    if app.is_typing() {
        return match key.code {
            KeyCode::Esc => Some(AppEvent::ExitTyping),
            KeyCode::Enter => Some(AppEvent::SubmitMessage),
            KeyCode::Backspace => Some(AppEvent::DeleteChar),
            KeyCode::Char(c) => Some(AppEvent::TypeChar(c)),
            _ => None,
        };
    }

    // Navigation mode
    match key.code {
        KeyCode::Char('j') | KeyCode::Down => Some(AppEvent::NavigateDown),
        KeyCode::Char('k') | KeyCode::Up => Some(AppEvent::NavigateUp),
        KeyCode::Enter | KeyCode::Char('l') | KeyCode::Right => Some(AppEvent::Select),
        KeyCode::Esc | KeyCode::Char('h') | KeyCode::Left => Some(AppEvent::Back),
        KeyCode::Tab => Some(AppEvent::CyclePanel),
        KeyCode::Char('m') => Some(AppEvent::ToggleMute),
        KeyCode::Char('d') => Some(AppEvent::ToggleDeafen),
        KeyCode::Char('v') => {
            if app.voice_room.is_some() {
                Some(AppEvent::LeaveVoice)
            } else {
                Some(AppEvent::JoinVoice)
            }
        }
        KeyCode::Char('a') if app.sidebar_section == SidebarSection::Friends => {
            Some(AppEvent::AddFriendStart)
        }
        KeyCode::Char('i') => Some(AppEvent::EnterTyping),
        KeyCode::Char('s') => Some(AppEvent::OpenAudioSettings),
        KeyCode::Char('1') => Some(AppEvent::SwitchSection(SidebarSection::Servers)),
        KeyCode::Char('2') => Some(AppEvent::SwitchSection(SidebarSection::Friends)),
        KeyCode::Char('3') => Some(AppEvent::SwitchSection(SidebarSection::Dms)),
        _ => None,
    }
}