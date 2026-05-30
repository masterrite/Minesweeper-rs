# Minesweeper-rs
A minesweeper game written in rust and slint using Claude and Gemini

## Features
- Resizeable window with scaling game tiles
- Customize background
- Toggle between light/dark mode

## Building

### Prerequisites

- **Rust stable** — https://rustup.rs
- **Linux extras**: `sudo apt install libxcb-shape0-dev libxkbcommon-dev libfontconfig1-dev`
- macOS / Windows: no extras needed

```bash
cd collector
cargo run              # development
cargo build --release  # → target/release/collector
```

## Packaging

```bash
# Linux AppImage / .deb
cargo install cargo-bundle && cargo bundle --release

# macOS .app
cargo bundle --release   # → target/release/bundle/osx/Collector.app

# Windows: the .exe is already standalone
```
