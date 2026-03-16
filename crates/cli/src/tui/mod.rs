// Copyright (c) 2026 Lag
// SPDX-License-Identifier: MIT

mod app;
mod keybindings;
mod theme;
mod ui;
pub mod widgets;

use crate::auth;
use anyhow::Result;
use app::{App, AppEvent};
use crossterm::event::{self, Event};
use crossterm::execute;
use crossterm::terminal::{self, EnterAlternateScreen, LeaveAlternateScreen};
use ratatui::prelude::*;
use std::io;

pub async fn run(_server: Option<String>) -> Result<()> {
    let creds = auth::ensure_auth().await?;

    // Suggest setup on first run
    let settings_path = crate::config::config_dir().join("audio-settings.json");
    if !settings_path.exists() {
        eprintln!("Tip: Run `lag setup` to configure your microphone and speakers.\n");
    }

    terminal::enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut app = App::new(creds)?;

    let result = run_loop(&mut terminal, &mut app).await;

    terminal::disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    result
}

async fn run_loop(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut App,
) -> Result<()> {
    let mut tick = tokio::time::interval(std::time::Duration::from_millis(33));
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    loop {
        terminal.draw(|f| ui::draw(f, app))?;

        // During loading, just tick for animation — don't block on async events
        if app.loading.is_some() && !app.init_started {
            app.start_init();
        }

        if app.loading.is_some() {
            // Fast tick for animation, check if init finished
            tokio::select! {
                biased;
                _ = tick.tick() => {}
            }
            app.check_init_complete().await?;

            // Still allow quit during loading
            while event::poll(std::time::Duration::from_millis(0))? {
                if let Event::Key(key) = event::read()? {
                    if matches!(key.code, crossterm::event::KeyCode::Char('q'))
                        || matches!(
                            (key.code, key.modifiers),
                            (
                                crossterm::event::KeyCode::Char('c'),
                                crossterm::event::KeyModifiers::CONTROL
                            )
                        )
                    {
                        return Ok(());
                    }
                }
            }
            continue;
        }

        // Wait for either a tick or an async event
        tokio::select! {
            biased;

            _ = tick.tick() => {}

            event = app.poll_async_events() => {
                if let Some(event) = event {
                    app.handle_event(event).await?;
                }
            }
        }

        // Drain all pending keyboard events (always responsive)
        while event::poll(std::time::Duration::from_millis(0))? {
            if let Event::Key(key) = event::read()? {
                let action = keybindings::handle_key(key, app);
                if let Some(AppEvent::Quit) = action {
                    app.cleanup().await?;
                    return Ok(());
                }
                if let Some(evt) = action {
                    app.handle_event(evt).await?;
                }
            }
        }

        // Drain all pending async events in one batch (no redraw between them)
        app.drain_async_events();

        // Reload friends list if a WS event flagged it
        if app.pending_friend_reload {
            app.pending_friend_reload = false;
            app.reload_friends().await;
        }

        // Complete voice connection if token arrived
        if let Some(result) = app.take_pending_voice_token() {
            match result {
                Ok(token_resp) => {
                    if let Err(e) = app.finish_join_voice(token_resp).await {
                        app.voice_connecting = None;
                        app.error_message = Some(format!("{}", e));
                    }
                }
                Err(e) => {
                    app.voice_connecting = None;
                    app.error_message = Some(format!("{}", e));
                }
            }
        }
    }
}
