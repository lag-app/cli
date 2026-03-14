// Copyright (c) 2026 Lag
// SPDX-License-Identifier: MIT

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "lag", about = "Lag - terminal voice chat", version)]
pub struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Authenticate with Lag via browser OAuth
    Login,
    /// Clear stored credentials
    Logout,
    /// Show current authenticated user
    Whoami,

    /// List your servers or show server details
    Servers {
        /// Server name or ID to inspect
        name_or_id: Option<String>,
    },

    /// Manage friends
    Friends {
        #[command(subcommand)]
        action: Option<FriendsAction>,
    },

    /// Direct messages
    Dms {
        #[command(subcommand)]
        action: Option<DmsAction>,
    },

    /// Text chat in a server room
    Chat {
        #[command(subcommand)]
        action: Option<ChatAction>,
    },

    /// Join a voice room (headless - stays connected until Ctrl+C)
    Join {
        /// Server name or ID
        server: String,
        /// Room name or ID
        room: String,
        /// Push-to-talk key
        #[arg(long)]
        ptt: Option<String>,
        /// Disable voice activity detection
        #[arg(long)]
        no_vad: bool,
        /// Select microphone by name
        #[arg(long)]
        input_device: Option<String>,
        /// Select speakers by name
        #[arg(long)]
        output_device: Option<String>,
        /// Show room text chat alongside voice
        #[arg(long)]
        with_chat: bool,
    },

    /// Audio device configuration
    Audio {
        #[command(subcommand)]
        action: Option<AudioAction>,
    },

    /// Set your online status
    Status {
        /// Status to set
        status: Option<String>,
    },

    /// Manage configuration
    Config {
        #[command(subcommand)]
        action: Option<ConfigAction>,
    },

    /// Interactive audio setup wizard
    Setup,

    /// Launch the full TUI
    Ui {
        /// Auto-navigate to a server
        #[arg(long)]
        server: Option<String>,
    },
}

#[derive(Subcommand)]
pub enum FriendsAction {
    /// Send a friend request
    Add { username: String },
    /// Remove a friend
    Remove { username: String },
    /// Show pending friend requests
    Requests,
    /// Accept a friend request
    Accept { username: String },
    /// Decline a friend request
    Decline { username: String },
}

#[derive(Subcommand)]
pub enum DmsAction {
    /// Open an interactive DM session
    Open { username: String },
    /// Send a one-off DM
    Send { username: String, message: String },
}

#[derive(Subcommand)]
pub enum ChatAction {
    /// Open interactive chat in a server room
    Open { server: String, room: String },
    /// Send a one-off message to a server room
    Send {
        server: String,
        room: String,
        message: String,
    },
}

#[derive(Subcommand)]
pub enum AudioAction {
    /// List all input/output devices
    Devices,
    /// Set default microphone
    SetInput { name: String },
    /// Set default speakers/headphones
    SetOutput { name: String },
    /// Set volume (input or output, 0-100)
    Volume {
        /// "input" or "output"
        target: String,
        /// Volume level 0-100
        level: u32,
    },
    /// Record 3s from mic and play back
    Test,
}

#[derive(Subcommand)]
pub enum ConfigAction {
    /// Set a config value
    Set { key: String, value: String },
    /// Reset to defaults
    Reset,
}

impl Cli {
    pub async fn run(self) -> anyhow::Result<()> {
        match self.command {
            Some(Commands::Login) => crate::commands::login::run().await,
            Some(Commands::Logout) => crate::commands::login::logout().await,
            Some(Commands::Whoami) => crate::commands::login::whoami().await,
            Some(Commands::Servers { name_or_id }) => {
                crate::commands::servers::run(name_or_id).await
            }
            Some(Commands::Friends { action }) => crate::commands::friends::run(action).await,
            Some(Commands::Dms { action }) => crate::commands::dms::run(action).await,
            Some(Commands::Chat { action }) => crate::commands::chat::run(action).await,
            Some(Commands::Join {
                server,
                room,
                ptt,
                no_vad,
                input_device,
                output_device,
                with_chat,
            }) => {
                crate::commands::join::run(
                    server,
                    room,
                    ptt,
                    no_vad,
                    input_device,
                    output_device,
                    with_chat,
                )
                .await
            }
            Some(Commands::Audio { action }) => crate::commands::audio::run(action).await,
            Some(Commands::Status { status }) => crate::commands::status::run(status).await,
            Some(Commands::Config { action }) => crate::commands::config_cmd::run(action).await,
            Some(Commands::Setup) => crate::commands::setup::run().await,
            Some(Commands::Ui { server }) => crate::tui::run(server).await,
            None => {
                // No subcommand - show help
                use clap::CommandFactory;
                Cli::command().print_help()?;
                println!();
                Ok(())
            }
        }
    }
}