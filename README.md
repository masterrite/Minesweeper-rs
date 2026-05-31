# Minesweeper — Rust + Slint

<img width="306" height="432" alt="Snipaste_2026-05-30_21-44-09" src="https://github.com/user-attachments/assets/dcd0c33a-1bd4-49c8-8320-2cd2c7b568c3" />

For fun. Built by Claude Opus 4.8.

## Features

- Set your own background
    + Background directory: ~\AppData\Roaming\minesweeper 
- Resizing the window also scales the UI
- ONLY 3.4 MB!!

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

## Build

```bash
# Install Rust once
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Linux only: X11 / font libs
sudo apt install libxcb1-dev libxkbcommon-dev libfontconfig1-dev

cargo run --release
```
