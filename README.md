# Minesweeper — Rust + Slint

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

## Binary size optimisations (Cargo.toml)

| Setting | Effect |
|---------|--------|
| `opt-level = "z"` | Optimise for size |
| `lto = true` | Dead-code elimination across all crates |
| `codegen-units = 1` | Maximum LTO effectiveness |
| `panic = "abort"` | No unwinding machinery |
| `strip = true` | No debug symbols |
| `slint` minimal features | Only `backend-winit` + `renderer-software` (no GL/Skia) |
| `rand` minimal features | `small_rng` + `getrandom` only |
