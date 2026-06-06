# Resonate

A native GNOME soundboard with polyphonic playback, a sequential queue, and a real-time LCD panel — built with GTK 4 + Libadwaita in Rust.

![Platform](https://img.shields.io/badge/platform-Fedora%2044%20%2F%20GNOME%2050-blue)
![Language](https://img.shields.io/badge/language-Rust-orange)

## Features

- **Soundboard grid** — load any number of audio files; each tile shows name, volume and play/cue controls
- **Polyphonic & sequential modes** — play sounds simultaneously or queue them up; toggled in Settings
- **Cue button** — always appends to the queue, regardless of mode
- **Skip** — stops all currently playing sounds and immediately starts the next item in the queue
- **LCD playback panel** — fixed-height scrollable list of all active tracks with per-track countdown; large aggregate total-time counter on the right; "time until next" appears below it when a queue is active
- **Rename** — rename sound tiles with an in-place dialog; the file on disk and all playback references are updated atomically
- **Remove with undo** — moves the file out of the Sounds Folder (cross-filesystem safe), removes the tile, and shows a 6-second undo toast; permanent deletion happens on dismiss
- **Sounds Folder** — configure a folder in Settings; all audio files found there are loaded automatically on launch
- **Coming Soon** overlay on the Effects tab (plugin chain, LV2, gate, EQ, reverb — planned)

## Requirements

| Dependency | Version |
|---|---|
| Rust | stable (edition 2024) |
| GTK 4 | ≥ 4.14 |
| Libadwaita | ≥ 1.6 |
| ALSA / PipeWire ALSA layer | system default on Fedora |

Audio decoding is handled entirely in Rust via [rodio](https://github.com/RustAudio/rodio) + [Symphonia](https://github.com/pdeljanov/Symphonia) — no GStreamer headers required.

Supported audio formats: WAV, MP3, FLAC, OGG, AAC, M4A, Opus, WMA.

## Build & Run

```bash
# Debug build
cargo build
cargo run

# With verbose logging
RUST_LOG=resonate=debug cargo run
```

## GNOME integration (icon & app name)

To make the icon and "Resonate" name appear in GNOME Overview and Alt+Tab, install the development desktop entry once:

```bash
mkdir -p ~/.local/share/icons/hicolor/scalable/apps
cp data/icons/io.github.resonate.svg ~/.local/share/icons/hicolor/scalable/apps/

cat > ~/.local/share/applications/io.github.resonate.desktop << 'EOF'
[Desktop Entry]
Name=Resonate
Exec=/path/to/target/debug/resonate
Icon=io.github.resonate
Type=Application
Categories=AudioVideo;Audio;
EOF

gtk-update-icon-cache -f -t ~/.local/share/icons/hicolor/
update-desktop-database ~/.local/share/applications/
```

## Project layout

```
src/
  main.rs               — GTK/Adwaita init, GResources, CSS, app entry
  application.rs        — AdwApplication subclass
  window.rs             — AdwApplicationWindow; audio wiring, rename/remove logic
  audio/
    engine.rs           — rodio-based polyphonic engine with queue
    sampler.rs          — (stub)
    virtual_device.rs   — (stub, PipeWire virtual sink — planned)
  plugins/
    mod.rs              — ResonatePlugin trait (planned)
  ui/
    soundboard_page.rs  — sound grid, LCD panel, time totals
    sound_tile.rs       — individual tile widget
    effects_page.rs     — Coming Soon placeholder
    settings_page.rs    — folder picker, polyphonic toggle
resources/
  ui/                   — Blueprint/XML UI templates
  style.css             — LCD panel, Coming Soon overlay styles
data/
  icons/                — app icon SVG
```

## App ID

`io.github.resonate`
