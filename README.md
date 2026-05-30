# Minesweeper — Rust + Slint

For fun. Built by Claude Opus 4.8.

## Build

```bash
# Install Rust once
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Linux only: X11 / font libs
sudo apt install libxcb1-dev libxkbcommon-dev libfontconfig1-dev

cargo run --release
```

## Controls

| Action | How |
|--------|-----|
| Reveal cell | Left click |
| Flag / unflag | Right click |
| **Chord** (reveal neighbours when flags match number) | **Left + Right click simultaneously on a revealed number** |
| New game (same difficulty) | Click 🙂 |
| Change difficulty | Easy / Medium / Hard buttons |
| Toggle light/dark | ☀ Light / 🌙 Dark button |
| Custom background | Click a colour swatch |

## Extra feature

- Set your own background
- Resizing the window also scales the UI
