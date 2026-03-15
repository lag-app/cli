// Copyright (c) 2026 Lag
// SPDX-License-Identifier: MIT

use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, List, ListItem, Paragraph};
use super::app::{App, ActionMenu, FriendEntryKind, FriendPopup, SidebarSection, SidebarView};
use super::theme;

pub fn draw(f: &mut Frame, app: &App) {
    let size = f.area();

    let bg = Block::default().style(Style::default().bg(theme::BG));
    f.render_widget(bg, size);

    // Loading screen with warp animation
    if let Some(ref msg) = app.loading {
        draw_warp_screen(f, msg, size);
        return;
    }

    // Reserve bottom row for toolbar
    let outer = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(3),
            Constraint::Length(1),
            Constraint::Length(1),
        ])
        .split(size);

    let content_area = outer[0];
    let spacer_area = outer[1];
    let toolbar_area = outer[2];

    // Spacer background
    let spacer = Block::default().style(Style::default().bg(theme::BG));
    f.render_widget(spacer, spacer_area);

    // Main layout: sidebar | content | voice panel (when in voice)
    let in_voice = app.connected_room.is_some();
    let main_layout = if in_voice {
        Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Length(26),
                Constraint::Min(30),
                Constraint::Length(22),
            ])
            .split(content_area)
    } else {
        Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Length(26),
                Constraint::Min(30),
            ])
            .split(content_area)
    };

    draw_sidebar(f, app, main_layout[0]);
    draw_content(f, app, main_layout[1]);

    if in_voice {
        draw_voice_panel(f, app, main_layout[2]);
    }

    // Action menu overlay
    if let Some(ref menu) = app.action_menu {
        draw_action_menu(f, menu, main_layout[1]);
    }

    // Friend popup overlay
    if let Some(ref popup) = app.friend_popup {
        draw_friend_popup(f, popup, main_layout[1]);
    }

    // Add friend input overlay
    if app.adding_friend {
        draw_add_friend(f, &app.add_friend_input, main_layout[1]);
    }

    // Content loading overlay (server/room/DM)
    if let Some(ref msg) = app.content_loading {
        draw_connecting_popup(f, msg, main_layout[1]);
    }

    // Voice connecting overlay
    if let Some(ref msg) = app.voice_connecting {
        draw_connecting_popup(f, msg, main_layout[1]);
    }

    // Error overlay
    if let Some(ref msg) = app.error_message {
        draw_error(f, msg, main_layout[1]);
    }

    draw_toolbar(f, app, toolbar_area);
}

fn draw_sidebar(f: &mut Frame, app: &App, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(8),
            Constraint::Length(8),
            Constraint::Length(8),
        ])
        .split(area);

    let section_title = match (&app.sidebar_section, &app.sidebar_view) {
        (SidebarSection::Servers, SidebarView::ServerList) => " SERVERS ",
        (SidebarSection::Servers, SidebarView::RoomList) => " ROOMS ",
        (SidebarSection::Friends, _) => " FRIENDS ",
        (SidebarSection::Dms, _) => " DMS ",
    };

    let is_servers = app.sidebar_section == SidebarSection::Servers;
    let title_style = if is_servers {
        Style::default().fg(theme::ACCENT)
    } else {
        Style::default().fg(theme::TEXT_SECONDARY)
    };

    let sidebar_items = app.sidebar_items();
    let list_items: Vec<ListItem> = sidebar_items
        .iter()
        .enumerate()
        .map(|(i, label)| {
            let is_selected = i == app.selected_index;
            let is_active_room = if let SidebarView::RoomList = app.sidebar_view {
                if i > 0 {
                    if let Some(ref detail) = app.selected_server {
                        let room_idx = i - 1;
                        detail.rooms.get(room_idx)
                            .and_then(|r| r["id"].as_str())
                            .map(|id| app.selected_room_id.as_deref() == Some(id))
                            .unwrap_or(false)
                    } else { false }
                } else { false }
            } else { false };

            let style = if is_selected {
                Style::default().fg(theme::TEXT_PRIMARY).bg(theme::SURFACE_HOVER)
            } else if is_active_room {
                Style::default().fg(theme::ACCENT)
            } else if i == 0 && app.sidebar_view == SidebarView::RoomList {
                Style::default().fg(theme::TEXT_MUTED)
            } else {
                Style::default().fg(theme::TEXT_PRIMARY)
            };

            ListItem::new(format!(" {}", label)).style(style)
        })
        .collect();

    let main_list = List::new(list_items).block(
        Block::default()
            .title(section_title)
            .title_style(title_style)
            .borders(Borders::ALL)
            .border_style(Style::default().fg(theme::BORDER))
            .style(Style::default().bg(theme::SURFACE)),
    );
    f.render_widget(main_list, chunks[0]);

    // Friends section (compact)
    let friends_style = if app.sidebar_section == SidebarSection::Friends {
        Style::default().fg(theme::ACCENT)
    } else {
        Style::default().fg(theme::TEXT_SECONDARY)
    };

    let friend_items: Vec<ListItem> = app.friend_entries.iter().take(5).map(|entry| {
        let color = match entry.kind {
            FriendEntryKind::IncomingRequest => theme::WARNING,
            FriendEntryKind::OutgoingRequest => theme::TEXT_MUTED,
            FriendEntryKind::Friend => match entry.status.as_str() {
                "online" => theme::SUCCESS,
                "idle" => theme::WARNING,
                _ => theme::TEXT_MUTED,
            },
        };
        ListItem::new(format!(" {}", entry.label())).style(Style::default().fg(color))
    }).collect();

    let friends = List::new(friend_items).block(
        Block::default()
            .title(" FRIENDS ")
            .title_style(friends_style)
            .borders(Borders::ALL)
            .border_style(Style::default().fg(theme::BORDER))
            .style(Style::default().bg(theme::SURFACE)),
    );
    f.render_widget(friends, chunks[1]);

    // DMs section (compact)
    let total_dm_unread: u32 = app.dm_unread.values().sum();
    let dms_title = if total_dm_unread > 0 {
        format!(" DMS ({}) ", total_dm_unread)
    } else {
        " DMS ".to_string()
    };
    let dms_style = if total_dm_unread > 0 || app.sidebar_section == SidebarSection::Dms {
        Style::default().fg(theme::ACCENT)
    } else {
        Style::default().fg(theme::TEXT_SECONDARY)
    };

    let dm_items: Vec<ListItem> = app.dms.iter().take(5).map(|dm| {
        let conv_id = dm["id"].as_str().unwrap_or("");
        let name = dm["otherUser"]["displayName"].as_str()
            .or_else(|| dm["otherUser"]["username"].as_str())
            .unwrap_or("?");
        let unread = app.dm_unread.get(conv_id).copied().unwrap_or(0);
        let color = if unread > 0 {
            theme::ACCENT
        } else {
            let status = dm["otherUser"]["status"].as_str().unwrap_or("offline");
            match status {
                "online" => theme::SUCCESS,
                "idle" => theme::WARNING,
                _ => theme::TEXT_MUTED,
            }
        };
        let label = if unread > 0 {
            format!(" ({}) {}", unread, name)
        } else {
            format!(" {}", name)
        };
        ListItem::new(label).style(Style::default().fg(color))
    }).collect();

    let dms = List::new(dm_items).block(
        Block::default()
            .title(dms_title)
            .title_style(dms_style)
            .borders(Borders::ALL)
            .border_style(Style::default().fg(theme::BORDER))
            .style(Style::default().bg(theme::SURFACE)),
    );
    f.render_widget(dms, chunks[2]);
}

const QUOTES: &[&str] = &[
    "\"The best code is no code at all.\" - Jeff Atwood",
    "\"It works on my machine.\" - Every developer ever",
    "\"Have you tried turning it off and on again?\"",
    "\"There are only two hard things: cache invalidation and naming things.\"",
    "\"It's not a bug, it's a feature.\"",
    "\"sudo make me a sandwich\"",
    "\"I don't always test my code, but when I do, I do it in production.\"",
    "\"99 little bugs in the code, 99 little bugs. Take one down, patch it around... 127 little bugs in the code.\"",
    "\"Weeks of coding can save you hours of planning.\"",
    "\"The cloud is just someone else's computer.\"",
    "\"Real programmers count from 0.\"",
    "\"There's no place like 127.0.0.1\"",
    "\"Talk is cheap. Show me the code.\" - Linus Torvalds",
    "\"First, solve the problem. Then, write the code.\" - John Johnson",
    "\"Copy and paste is a design error.\" - David Parnas",
    "\"The Internet? Is that thing still around?\" - Homer Simpson",
    "\"In order to understand recursion, one must first understand recursion.\"",
    "\"A SQL query walks into a bar, walks up to two tables and asks... can I join you?\"",
    "\"!false - it's funny because it's true\"",
    "\"Algorithm: word used by programmers when they don't want to explain what they did.\"",
];

fn daily_quote() -> &'static str {
    let day = chrono::Utc::now().format("%j").to_string().parse::<usize>().unwrap_or(0);
    QUOTES[day % QUOTES.len()]
}

fn draw_warp_screen(f: &mut Frame, status_msg: &str, area: Rect) {
    let w = area.width as usize;
    let h = area.height as usize;
    if w == 0 || h == 0 { return; }

    let cx = w / 2;
    let cy = h / 2;

    // Time-based animation
    let t = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();

    // Build the star field as a 2D buffer
    let mut buf: Vec<Vec<(char, Color)>> = vec![vec![(' ', Color::Reset); w]; h];

    let num_stars = 150;
    for i in 0..num_stars {
        let seed = (i as u64).wrapping_mul(2654435761);
        let angle = (seed % 36000) as f64 / 36000.0 * std::f64::consts::TAU;
        let speed = 0.2 + (seed % 800) as f64 / 1000.0;
        let phase = (seed % 1000) as f64 / 1000.0;

        let max_dist = (w.max(h) as f64) * 0.8;
        let progress = (t as f64 / 1000.0 * speed + phase) % 1.0;
        let dist = progress * max_dist;

        let x = cx as f64 + angle.cos() * dist * 2.0;
        let y = cy as f64 + angle.sin() * dist;

        let ix = x as usize;
        let iy = y as usize;

        if ix < w && iy < h {
            let brightness = (40.0 + progress * 215.0) as u8;
            let ch = if progress > 0.6 { '*' }
                else if progress > 0.3 { '.' }
                else { '·' };
            buf[iy][ix] = (ch, Color::Rgb(brightness, brightness, brightness));
        }

        // Trails for stars past 40%
        if progress > 0.4 {
            for t_off in 1..=2 {
                let trail_dist = dist - (t_off as f64 * 1.2);
                if trail_dist < 0.0 { continue; }
                let trail_x = cx as f64 + angle.cos() * trail_dist * 2.0;
                let trail_y = cy as f64 + angle.sin() * trail_dist;
                let tix = trail_x as usize;
                let tiy = trail_y as usize;
                if tix < w && tiy < h && buf[tiy][tix].0 == ' ' {
                    let tb = (progress * 60.0 / t_off as f64) as u8;
                    buf[tiy][tix] = ('·', Color::Rgb(tb, tb, tb));
                }
            }
        }
    }

    // Overlay text directly into the buffer
    let quote = daily_quote();
    let title = "Connecting to Lag";
    let spinner = ['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];
    let spin_idx = (t / 80) as usize % spinner.len();
    let spin_line = format!("{} {}", spinner[spin_idx], status_msg);

    let text_rows: Vec<(&str, Color)> = vec![
        (title, theme::ACCENT),
        ("", theme::BG),
        (&spin_line, theme::TEXT_SECONDARY),
        ("", theme::BG),
        (quote, theme::TEXT_MUTED),
    ];

    let text_start_y = cy.saturating_sub(text_rows.len() / 2);
    for (row_offset, (text, color)) in text_rows.iter().enumerate() {
        let row_y = text_start_y + row_offset;
        if row_y >= h || text.is_empty() { continue; }
        let text_start_x = cx.saturating_sub(text.chars().count() / 2);
        for (col_offset, ch) in text.chars().enumerate() {
            let col_x = text_start_x + col_offset;
            if col_x < w {
                buf[row_y][col_x] = (ch, *color);
            }
        }
    }

    // Render the combined buffer as a single widget
    let lines: Vec<Line> = buf.iter().map(|row| {
        let spans: Vec<Span> = row.iter().map(|(ch, color)| {
            Span::styled(ch.to_string(), Style::default().fg(*color))
        }).collect();
        Line::from(spans)
    }).collect();

    let canvas = Paragraph::new(lines);
    f.render_widget(canvas, area);
}

fn relative_time(iso: &str) -> String {
    let Ok(dt) = chrono::DateTime::parse_from_rfc3339(iso) else {
        return String::new();
    };
    let now = chrono::Utc::now();
    let diff = now.signed_duration_since(dt);
    let secs = diff.num_seconds();

    if secs < 5 {
        "just now".to_string()
    } else if secs < 60 {
        format!("{}s ago", secs)
    } else if diff.num_minutes() < 60 {
        format!("{}m ago", diff.num_minutes())
    } else if diff.num_hours() < 24 {
        format!("{}h ago", diff.num_hours())
    } else {
        format!("{}d ago", diff.num_days())
    }
}

fn draw_content(f: &mut Frame, app: &App, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(5),
            Constraint::Length(3),
        ])
        .split(area);

    let title = app.content_title();

    // Each message takes 2 lines: content + timestamp
    let msg_lines: Vec<(Line, Line)> = app.messages.iter().map(|msg| {
        let username = msg["username"].as_str().unwrap_or("");
        let is_me = !app.username.is_empty() && username == app.username;

        let display_name = if is_me {
            "(me)".to_string()
        } else {
            msg["displayName"].as_str()
                .or_else(|| msg["display_name"].as_str())
                .filter(|s| !s.is_empty())
                .or_else(|| msg["username"].as_str())
                .unwrap_or("?")
                .to_string()
        };

        let content = msg["content"].as_str().unwrap_or("");

        let created_at = msg["createdAt"].as_str()
            .or_else(|| msg["created_at"].as_str())
            .unwrap_or("");
        let time_str = relative_time(created_at);

        let msg_line = Line::from(vec![
            Span::styled(
                format!("{}: ", display_name),
                Style::default().fg(if is_me { theme::TEXT_SECONDARY } else { theme::ACCENT }),
            ),
            Span::styled(content, Style::default().fg(theme::TEXT_PRIMARY)),
        ]);

        let time_line = Line::from(Span::styled(
            format!("  {}", time_str),
            Style::default().fg(theme::TEXT_MUTED),
        ));

        (msg_line, time_line)
    }).collect();

    // Build list items anchored to bottom: take only what fits
    let inner_height = chunks[0].height.saturating_sub(2) as usize; // minus borders
    let total_lines: usize = msg_lines.len() * 2;
    let skip = total_lines.saturating_sub(inner_height);

    let mut all_lines: Vec<Line> = Vec::with_capacity(total_lines);
    for (msg_line, time_line) in &msg_lines {
        all_lines.push(msg_line.clone());
        all_lines.push(time_line.clone());
    }

    // Skip from the top to anchor to bottom
    let visible: Vec<ListItem> = all_lines.into_iter()
        .skip(skip)
        .map(ListItem::new)
        .collect();

    let messages = List::new(visible).block(
        Block::default()
            .title(format!(" {} ", title))
            .title_style(Style::default().fg(theme::TEXT_SECONDARY))
            .borders(Borders::ALL)
            .border_style(Style::default().fg(theme::BORDER))
            .style(Style::default().bg(theme::SURFACE)),
    );
    f.render_widget(messages, chunks[0]);

    // Input
    let can_type = app.selected_room_id.is_some() || app.selected_dm_id.is_some();

    let (input_text, input_style) = if app.sending_message {
        let spinner = ['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];
        let idx = (std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() / 80) as usize % spinner.len();
        (
            format!("{} Sending...", spinner[idx]),
            Style::default().fg(theme::TEXT_MUTED).bg(theme::SURFACE),
        )
    } else if app.typing {
        (
            format!("> {}", app.input_buf),
            Style::default().fg(theme::TEXT_PRIMARY).bg(theme::SURFACE_ELEVATED),
        )
    } else if can_type {
        (
            "Press [i] to type...".to_string(),
            Style::default().fg(theme::TEXT_MUTED).bg(theme::SURFACE),
        )
    } else {
        (
            "Select a room to chat".to_string(),
            Style::default().fg(theme::TEXT_MUTED).bg(theme::SURFACE),
        )
    };

    let input = Paragraph::new(input_text).style(input_style).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(theme::BORDER)),
    );
    f.render_widget(input, chunks[1]);
}

fn draw_action_menu(f: &mut Frame, menu: &ActionMenu, content_area: Rect) {
    let menu_height = (menu.items.len() as u16) + 2;
    let menu_width = 24;
    let x = content_area.x + (content_area.width.saturating_sub(menu_width)) / 2;
    let y = content_area.y + (content_area.height.saturating_sub(menu_height)) / 2;
    let area = Rect::new(x, y, menu_width, menu_height);

    let clear = Block::default().style(Style::default().bg(theme::SURFACE_ELEVATED));
    f.render_widget(clear, area);

    let items: Vec<ListItem> = menu.items.iter().enumerate().map(|(i, label)| {
        let style = if i == menu.selected {
            Style::default().fg(theme::TEXT_PRIMARY).bg(theme::SURFACE_HOVER)
        } else {
            Style::default().fg(theme::TEXT_SECONDARY)
        };
        if i == menu.selected {
            ListItem::new(format!(" > {} ", label)).style(style)
        } else {
            ListItem::new(format!("   {} ", label)).style(style)
        }
    }).collect();

    let list = List::new(items).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(theme::ACCENT))
            .style(Style::default().bg(theme::SURFACE_ELEVATED)),
    );
    f.render_widget(list, area);
}

fn draw_friend_popup(f: &mut Frame, popup: &FriendPopup, content_area: Rect) {
    let entry = &popup.entry;
    let name = entry.display_name.as_deref().unwrap_or(&entry.username);

    let mut lines: Vec<Line> = Vec::new();

    // Header
    let kind_label = match entry.kind {
        FriendEntryKind::Friend => "Friend",
        FriendEntryKind::IncomingRequest => "Friend Request",
        FriendEntryKind::OutgoingRequest => "Outgoing Request",
    };
    lines.push(Line::from(Span::styled(kind_label, Style::default().fg(theme::TEXT_MUTED))));
    lines.push(Line::from(Span::styled(name, Style::default().fg(theme::ACCENT))));

    // Username (if display name differs)
    if entry.display_name.is_some() {
        lines.push(Line::from(Span::styled(
            format!("@{}", entry.username),
            Style::default().fg(theme::TEXT_SECONDARY),
        )));
    }

    // Status
    let status_color = match entry.status.as_str() {
        "online" => theme::SUCCESS,
        "idle" => theme::WARNING,
        _ => theme::TEXT_MUTED,
    };
    lines.push(Line::from(vec![
        Span::styled("● ", Style::default().fg(status_color)),
        Span::styled(&entry.status, Style::default().fg(theme::TEXT_SECONDARY)),
    ]));

    // Since
    if let Some(ref since) = entry.since {
        let rel = super::ui::relative_time(since);
        if !rel.is_empty() {
            lines.push(Line::from(Span::styled(
                format!("Friends {}", rel),
                Style::default().fg(theme::TEXT_MUTED),
            )));
        }
    }

    lines.push(Line::from(""));

    // Actions
    for (i, action) in popup.actions.iter().enumerate() {
        let is_selected = i == popup.selected;
        let is_confirming = popup.confirming.as_deref() == Some(action.as_str());

        let label = if is_confirming {
            format!("  !! {} - press enter to confirm", action)
        } else if is_selected {
            format!(" > {}", action)
        } else {
            format!("   {}", action)
        };

        let style = if is_confirming {
            Style::default().fg(theme::DANGER)
        } else if is_selected {
            Style::default().fg(theme::TEXT_PRIMARY).bg(theme::SURFACE_HOVER)
        } else {
            Style::default().fg(theme::TEXT_SECONDARY)
        };

        lines.push(Line::from(Span::styled(label, style)));
    }

    let height = (lines.len() as u16) + 2;
    let width = 38.min(content_area.width.saturating_sub(4));
    let x = content_area.x + (content_area.width.saturating_sub(width)) / 2;
    let y = content_area.y + (content_area.height.saturating_sub(height)) / 2;
    let area = Rect::new(x, y, width, height);

    let popup_widget = Paragraph::new(lines).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(theme::ACCENT))
            .style(Style::default().bg(theme::SURFACE_ELEVATED)),
    );
    f.render_widget(popup_widget, area);
}

fn draw_add_friend(f: &mut Frame, input: &str, content_area: Rect) {
    let lines = vec![
        Line::from(Span::styled("Add Friend", Style::default().fg(theme::ACCENT))),
        Line::from(""),
        Line::from(Span::styled("Enter username:", Style::default().fg(theme::TEXT_SECONDARY))),
        Line::from(Span::styled(
            format!("> {}_", input),
            Style::default().fg(theme::TEXT_PRIMARY),
        )),
        Line::from(""),
        Line::from(Span::styled("[enter] send  [esc] cancel", Style::default().fg(theme::TEXT_MUTED))),
    ];

    let height = (lines.len() as u16) + 2;
    let width = 34.min(content_area.width.saturating_sub(4));
    let x = content_area.x + (content_area.width.saturating_sub(width)) / 2;
    let y = content_area.y + (content_area.height.saturating_sub(height)) / 2;
    let area = Rect::new(x, y, width, height);

    let popup = Paragraph::new(lines).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(theme::ACCENT))
            .style(Style::default().bg(theme::SURFACE_ELEVATED)),
    );
    f.render_widget(popup, area);
}

fn draw_connecting_popup(f: &mut Frame, msg: &str, content_area: Rect) {
    let t = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    let spinner = ['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];
    let idx = (t / 80) as usize % spinner.len();

    let lines = vec![
        Line::from(Span::styled(
            format!("{} {}", spinner[idx], msg),
            Style::default().fg(theme::ACCENT),
        )),
    ];

    let height = 3_u16;
    let width = (msg.len() as u16 + 6).min(content_area.width.saturating_sub(4));
    let x = content_area.x + (content_area.width.saturating_sub(width)) / 2;
    let y = content_area.y + (content_area.height.saturating_sub(height)) / 2;
    let area = Rect::new(x, y, width, height);

    let popup = Paragraph::new(lines)
        .alignment(Alignment::Center)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(theme::ACCENT))
                .style(Style::default().bg(theme::SURFACE_ELEVATED)),
        );
    f.render_widget(popup, area);
}

fn draw_error(f: &mut Frame, msg: &str, content_area: Rect) {
    let lines = vec![
        Line::from(Span::styled("Error", Style::default().fg(theme::DANGER))),
        Line::from(""),
        Line::from(Span::styled(msg, Style::default().fg(theme::TEXT_PRIMARY))),
        Line::from(""),
        Line::from(Span::styled("Press any key to dismiss", Style::default().fg(theme::TEXT_MUTED))),
    ];

    let height = (lines.len() as u16) + 2;
    let width = 40.min(content_area.width.saturating_sub(4));
    let x = content_area.x + (content_area.width.saturating_sub(width)) / 2;
    let y = content_area.y + (content_area.height.saturating_sub(height)) / 2;
    let area = Rect::new(x, y, width, height);

    let popup = Paragraph::new(lines)
        .alignment(Alignment::Center)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(theme::DANGER))
                .style(Style::default().bg(theme::SURFACE_ELEVATED)),
        );
    f.render_widget(popup, area);
}

fn vu_bar(level: f32, width: u16) -> Line<'static> {
    let filled = ((level * width as f32) as u16).min(width);
    let empty = width.saturating_sub(filled);

    let color = if level > 0.7 {
        theme::DANGER
    } else if level > 0.3 {
        theme::SUCCESS
    } else {
        theme::TEXT_MUTED
    };

    Line::from(vec![
        Span::styled("▮".repeat(filled as usize), Style::default().fg(color)),
        Span::styled("▯".repeat(empty as usize), Style::default().fg(theme::BORDER)),
    ])
}

fn draw_voice_panel(f: &mut Frame, app: &App, area: Rect) {
    let inner_width = area.width.saturating_sub(2);
    let mut lines: Vec<Line> = Vec::new();

    // Connection status
    let status_text = if app.voice_room.is_some() { "Connected" } else { "Connecting..." };
    let status_color = if app.voice_room.is_some() { theme::SUCCESS } else { theme::WARNING };
    lines.push(Line::from(vec![
        Span::styled("● ", Style::default().fg(status_color)),
        Span::styled(status_text, Style::default().fg(theme::TEXT_PRIMARY)),
    ]));

    if let Some(ref room_name) = app.connected_room {
        lines.push(Line::from(Span::styled(
            room_name.as_str(),
            Style::default().fg(theme::ACCENT),
        )));
    }

    lines.push(Line::from(""));

    // VU meters
    lines.push(Line::from(Span::styled(
        if app.muted { "MIC (muted)" } else { "MIC" },
        Style::default().fg(if app.muted { theme::DANGER } else { theme::TEXT_SECONDARY }),
    )));
    lines.push(vu_bar(if app.muted { 0.0 } else { app.mic_level }, inner_width));

    lines.push(Line::from(""));

    lines.push(Line::from(Span::styled(
        if app.deafened { "OUT (deaf)" } else { "OUTPUT" },
        Style::default().fg(if app.deafened { theme::DANGER } else { theme::TEXT_SECONDARY }),
    )));
    lines.push(vu_bar(if app.deafened { 0.0 } else { app.output_level }, inner_width));

    lines.push(Line::from(""));

    // Participants
    lines.push(Line::from(Span::styled(
        "PARTICIPANTS",
        Style::default().fg(theme::TEXT_SECONDARY),
    )));

    if app.voice_participants.is_empty() {
        lines.push(Line::from(Span::styled(
            "  (empty)",
            Style::default().fg(theme::TEXT_MUTED),
        )));
    } else {
        for (user_id, display_name) in &app.voice_participants {
            let speaking = app.voice_speaking.contains(user_id);
            let dot_color = if speaking { theme::SUCCESS } else { theme::TEXT_MUTED };
            lines.push(Line::from(vec![
                Span::styled("● ", Style::default().fg(dot_color)),
                Span::styled(
                    display_name.as_str(),
                    Style::default().fg(if speaking { theme::TEXT_PRIMARY } else { theme::TEXT_SECONDARY }),
                ),
            ]));
        }
    }

    let panel = Paragraph::new(lines).block(
        Block::default()
            .title(" VOICE ")
            .title_style(Style::default().fg(theme::SUCCESS))
            .borders(Borders::ALL)
            .border_style(Style::default().fg(theme::BORDER))
            .style(Style::default().bg(theme::SURFACE)),
    );
    f.render_widget(panel, area);
}

fn draw_toolbar(f: &mut Frame, app: &App, area: Rect) {
    // Build left side: keybindings
    let mut left_spans = vec![
        Span::styled(" [q]", Style::default().fg(theme::ACCENT)),
        Span::styled("quit ", Style::default().fg(theme::TEXT_MUTED)),
        Span::styled("[j/k]", Style::default().fg(theme::ACCENT)),
        Span::styled("nav ", Style::default().fg(theme::TEXT_MUTED)),
        Span::styled("[enter]", Style::default().fg(theme::ACCENT)),
        Span::styled("sel ", Style::default().fg(theme::TEXT_MUTED)),
        Span::styled("[esc]", Style::default().fg(theme::ACCENT)),
        Span::styled("back ", Style::default().fg(theme::TEXT_MUTED)),
        Span::styled("[v]", Style::default().fg(theme::ACCENT)),
        Span::styled("voice ", Style::default().fg(theme::TEXT_MUTED)),
        Span::styled("[m]", Style::default().fg(theme::ACCENT)),
        Span::styled("mute ", Style::default().fg(theme::TEXT_MUTED)),
        Span::styled("[d]", Style::default().fg(theme::ACCENT)),
        Span::styled("deafen ", Style::default().fg(theme::TEXT_MUTED)),
        Span::styled("[i]", Style::default().fg(theme::ACCENT)),
        Span::styled("type ", Style::default().fg(theme::TEXT_MUTED)),
        Span::styled("[tab]", Style::default().fg(theme::ACCENT)),
        Span::styled("section ", Style::default().fg(theme::TEXT_MUTED)),
    ];

    if app.sidebar_section == SidebarSection::Friends {
        left_spans.push(Span::styled("[a]", Style::default().fg(theme::ACCENT)));
        left_spans.push(Span::styled("add", Style::default().fg(theme::TEXT_MUTED)));
    }

    // Build right side: connection status with colored circles
    let ws_connected = app.ws.is_some();
    let voice_connected = app.connected_room.is_some();

    let ws_color = if ws_connected { theme::SUCCESS } else { theme::DANGER };
    let voice_color = if voice_connected { theme::SUCCESS }
        else if app.selected_room_id.is_some() { theme::WARNING }
        else { theme::TEXT_MUTED };

    let mic_color = if app.muted { theme::DANGER } else { theme::SUCCESS };

    // Pad to push status to the right
    let left_len: usize = left_spans.iter().map(|s| s.content.len()).sum();
    let right_text = "  ● wss  ● voice  ● mic ";
    let padding = (area.width as usize).saturating_sub(left_len + right_text.len());
    left_spans.push(Span::styled(
        " ".repeat(padding),
        Style::default().bg(theme::SURFACE),
    ));

    left_spans.push(Span::styled("  ● ", Style::default().fg(ws_color)));
    left_spans.push(Span::styled("wss", Style::default().fg(theme::TEXT_MUTED)));
    left_spans.push(Span::styled("  ● ", Style::default().fg(voice_color)));
    left_spans.push(Span::styled("voice", Style::default().fg(theme::TEXT_MUTED)));
    left_spans.push(Span::styled("  ● ", Style::default().fg(mic_color)));
    left_spans.push(Span::styled("mic ", Style::default().fg(theme::TEXT_MUTED)));

    let bar = Paragraph::new(Line::from(left_spans))
        .style(Style::default().bg(theme::SURFACE));

    f.render_widget(bar, area);
}
