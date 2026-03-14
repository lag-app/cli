<p align="left">
  <img src="https://trylag.com/lag_logo_trimmed_dark_mode.png" alt="Lag Logo" width="200">
</p>

# Lag CLI
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)

We have recently moved this repo into a public repo. PRs are welcome but we are a small team so reviewing them may take some time. We may employ AI in the near future to determine and sort these for us.

** PLEASE NOTE THIS IS A WORK IN PROGRESS. EVERYTHING IN ALPHA PLEASE EXPECT ISSUES ** 



Terminal client for [Lag](https://trylag.com) voice chat. Join voice rooms, send messages, and manage your servers — all from the terminal.

## Install

### macOS (Homebrew)

```bash
brew tap lagchat/tap
brew install lag
```

### Linux (Homebrew)

```bash
brew tap lagchat/tap
brew install lag
```

### Linux (Debian/Ubuntu)

```bash
# Download the latest .deb from the releases page
sudo dpkg -i lag_*.deb
```

### From source

```bash
cargo install --path crates/cli
```

### Pre-built binaries

Download the latest release for your platform from the [Releases](https://github.com/lagchat/cli/releases) page.

## Commands

```
lag login                          # Auth via browser
lag logout                         # Clear credentials
lag whoami                         # Show current user
lag setup                          # Audio setup wizard

lag servers                        # List servers
lag servers <name>                 # Show server details

lag friends                        # List friends
lag friends add <username>         # Send request
lag friends requests               # Pending requests
lag friends accept <username>      # Accept request

lag dms                            # List conversations
lag dms open <username>            # Interactive DM session
lag dms send <username> <msg>      # Send one-off DM

lag chat open <server> <room>      # Interactive room chat
lag chat send <server> <room> <msg>

lag join <server> <room>           # Headless voice
lag join <server> <room> --with-chat

lag audio                          # Show audio config
lag audio devices                  # List devices
lag audio set-input <name>         # Set mic
lag audio set-output <name>        # Set speakers
lag audio volume input <0-100>     # Mic volume
lag audio volume output <0-100>    # Speaker volume
lag audio test                     # 3s record + playback

lag status [online|idle]           # Set/show status
lag config                         # Show config
lag config set <key> <value>       # Set config
lag config reset                   # Reset defaults
lag ui                             # Full TUI
```

## Configuration

Override the API endpoint:

```bash
lag config set api-url https://your-api-url.com
```

## Requirements

macOS or Linux. Push-to-talk requires Input Monitoring permission on macOS (System Settings > Privacy & Security > Input Monitoring).

## Building from source

```bash
cargo build -p lag-cli --release
```

## Development

```bash
# Build debug binary
cargo build -p lag-cli

# Run directly
cargo run -p lag-cli -- --help

# Run tests
cargo test --workspace

# Point to a local API server
lag config set api-url http://localhost:3001
```

Enable debug logging with the `RUST_LOG` environment variable:

```bash
RUST_LOG=debug cargo run -p lag-cli -- login
```

## License

[MIT](LICENSE)
