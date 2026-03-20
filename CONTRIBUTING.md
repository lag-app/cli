# Contributing to Lag CLI

Thanks for your interest in contributing! We're a small team so PR reviews may take some time, but we appreciate every contribution.

## Getting Started

1. Fork the repository
2. Clone your fork:
   ```bash
   git clone https://github.com/<your-username>/cli.git
   cd cli
   ```
3. Create a branch for your changes:
   ```bash
   git checkout -b my-feature
   ```

## Development Setup

You'll need:
- [Rust](https://rustup.rs/) (stable toolchain)
- A C compiler (for native dependencies)
- CMake (for audio library builds)

Build and run:
```bash
cargo build -p lag-cli
cargo run -p lag-cli -- --help
```

Run tests:
```bash
cargo test --workspace
```

Enable debug logging:
```bash
RUST_LOG=debug cargo run -p lag-cli -- <command>
```

Point to a local API server for development:
```bash
lag config set api-url http://localhost:3001
```

## Project Structure

```
crates/
  cli/         # Main CLI application
  common/      # Shared types and utilities
  voice-core/  # Voice chat engine
```

## Making Changes

- Keep PRs focused — one feature or fix per PR
- Follow existing code style and conventions
- Make sure `cargo build --workspace` compiles without warnings
- Make sure `cargo test --workspace` passes
- Test your changes on your platform (macOS or Linux)

## Commit Messages

Write clear, concise commit messages. Use the imperative mood:
- `fix: handle expired token on reconnect`
- `feat: add push-to-talk toggle command`
- `docs: update install instructions for arm64`

## Submitting a Pull Request

1. Push your branch to your fork
2. Open a PR against `main`
3. Fill out the PR template
4. Wait for review — we'll get to it as soon as we can

## Reporting Bugs

Use the [bug report template](https://github.com/lag-app/cli/issues/new?template=bug_report.md). Include your OS, architecture, CLI version (`lag --version`), and steps to reproduce.

## Requesting Features

Use the [feature request template](https://github.com/lag-app/cli/issues/new?template=feature_request.md).

## License

By contributing, you agree that your contributions will be licensed under the [MIT License](LICENSE).
