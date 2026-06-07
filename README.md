# Resonate

A native GNOME soundboard with polyphonic playback, a sequential queue, a real-time LCD panel, **a PipeWire virtual microphone, and a real-time mic effects chain (built-in + LV2 plugins)** — built with GTK 4 + Libadwaita in Rust.

![Platform](https://img.shields.io/badge/platform-Fedora%2044%20%2F%20GNOME%2050-blue)
![Language](https://img.shields.io/badge/language-Rust-orange)

## Features

### Soundboard
- **Soundboard grid** — load any number of audio files; each tile has its own volume slider and play/cue controls
- **Per-sound volume** — each tile's slider scales that sound in both the local monitor and the virtual mic; new tiles start at the configurable **Default Volume**
- **Polyphonic & sequential modes** — play (and stack) sounds simultaneously, or queue them up; toggled in Settings
- **Cue button** — always appends to the queue, regardless of mode
- **Skip** — stops all currently playing sounds and immediately starts the next item in the queue
- **LCD playback panel** — fixed-height scrollable list of all active tracks with per-track countdown; large aggregate total-time counter on the right; "time until next" appears below it when a queue is active
- **Rename** — rename sound tiles with an in-place dialog; the file on disk and all playback references are updated atomically
- **Remove with undo** — moves the file out of the Sounds Folder (cross-filesystem safe), removes the tile, and shows a 6-second undo toast; permanent deletion happens on dismiss
- **Sounds Folder** — configure a folder in Settings; all audio files found there are loaded automatically on launch

### Settings
- **Sounds Folder** + **Move Added Files** — where sounds live, and whether new files are moved there
- **Input Device** — pick which physical microphone feeds the effects chain (default = auto-detect)
- **Monitor Output** — hear the soundboard yourself on the system default output (toggle); monitor level is the slider on the soundboard bar
- **Playback** — Play Multiple Sounds (polyphonic) and Default Volume for new tiles
- **Virtual Device** — set the virtual mic's display name and whether it is created on launch

### Virtual microphone
- **PipeWire virtual mic** — Resonate exposes a "Resonate Microphone" source that other apps (Discord, OBS, browsers…) can select as an input
- **Sounds + mic mixed together** — soundboard playback and your real microphone are summed into the virtual mic, so others hear both
- **Set as default input on launch** — the virtual mic is made the system default input automatically
- **Survives across logins** — a PipeWire drop-in (`~/.config/pipewire/pipewire.conf.d/resonate.conf`) keeps a raw mic pass-through alive even when Resonate isn't running

### Mic effects
- **Real-time effects chain** applied to the microphone: built-in **Noise Gate** and **Gain**, plus **any installed LV2 plugin** (hosted via [`livi`](https://github.com/wmedrano/livi))
- **Add / remove / reorder-by-list** effects from the Effects page; parameter sliders are generated automatically from each plugin's controls
- Chain is persisted to `config.json` and re-applied on launch

## Requirements

| Dependency | Version / Notes |
|---|---|
| Rust | stable (edition 2024) |
| GTK 4 | ≥ 4.14 |
| Libadwaita | ≥ 1.6 |
| PipeWire | system default on Fedora |
| LV2 / lilv (dev) | needed to build the LV2 host |

Audio decoding is handled entirely in Rust via [rodio](https://github.com/RustAudio/rodio) + [Symphonia](https://github.com/pdeljanov/Symphonia) — no GStreamer headers required.

Supported audio formats: WAV, MP3, FLAC, OGG, AAC, M4A, Opus, WMA.

### Build dependencies (Fedora)

```bash
sudo dnf install lilv-devel lv2-devel serd-devel sord-devel sratom-devel
```

## Build & Run

```bash
# Debug build
cargo build
cargo run

# With verbose logging
RUST_LOG=resonate=debug cargo run
```

## Installing LV2 effect plugins

Resonate hosts any LV2 plugin found on the standard search path (`~/.lv2`,
`/usr/lib64/lv2`, …). Useful packages on Fedora:

```bash
# A large, high-quality plugin suite (gate, compressor, autogain, EQ, …)
sudo dnf install lsp-plugins-lv2 zam-plugins calf

# Noise cancellation (via the audinux COPR)
sudo dnf copr enable ycollet/audinux
sudo dnf install noise-repellent
```

Newly installed plugins are picked up the next time Resonate starts (discovery
runs once when the Effects page first opens). A handy mic chain, in order:
**Noise Gate** → **noise-repellent / RNNoise** → **LSP Autogain** → **Gain**.

## GNOME integration (icon & app name)

GNOME matches a running window to its `.desktop` file by **app ID**
(`io.github.kilo2071.Resonate`), so the desktop entry's basename, its `Icon=`,
and the installed icon file must all use that exact ID — otherwise the Overview
and Alt+Tab show a generic name/icon.

For a development build, install the committed desktop entry and icon once
(pointing `Exec` at your built binary):

```bash
mkdir -p ~/.local/share/icons/hicolor/scalable/apps ~/.local/share/applications
cp data/icons/io.github.kilo2071.Resonate.svg ~/.local/share/icons/hicolor/scalable/apps/

sed "s|^Exec=resonate|Exec=$PWD/target/debug/resonate|" \
  data/io.github.kilo2071.Resonate.desktop \
  > ~/.local/share/applications/io.github.kilo2071.Resonate.desktop

gtk-update-icon-cache -f -t ~/.local/share/icons/hicolor/
update-desktop-database ~/.local/share/applications/
```

(An RPM install puts the same files under `/usr/share` with `Exec=resonate`.)

## Project layout

```
src/
  main.rs               — GTK/Adwaita init, GResources, CSS, app entry
  application.rs        — AdwApplication subclass
  window.rs             — AdwApplicationWindow; audio/effects wiring, rename/remove logic
  config.rs             — Config + EffectEntry (serde_json)
  audio/
    engine.rs           — rodio polyphonic engine, queue, tick loop, shared FX chain
    virtual_device.rs   — in-process PipeWire streams: bridge, soundboard + mic capture/playback
    pw_config.rs        — routing plan, mic detection, loopback teardown, drop-in persistence
    sampler.rs          — (legacy stub)
  plugins/
    mod.rs              — ResonatePlugin trait, PluginParam, plugin_from_entry factory
    host.rs             — PluginChain
    lv2.rs              — LV2 host (livi): discovery + Lv2Plugin
    builtin/
      gain.rs           — Gain (0.0–4.0× linear)
      gate.rs           — Noise Gate (RMS, attack/release)
  ui/
    soundboard_page.rs  — sound grid, LCD panel, time totals
    sound_tile.rs       — individual tile widget
    effects_page.rs     — mic effects chain editor (built-in + LV2)
    settings_page.rs    — folder picker, virtual device / input settings
resources/
  ui/                   — GTK XML UI templates
  style.css             — LCD panel and overlay styles
data/
  icons/                — app icon SVG
```

See `CLAUDE.md` for the detailed audio architecture (the virtual-mic bridge, the
in-process mic FX path, and PipeWire gotchas).

## App ID

`io.github.kilo2071.Resonate`

Config is stored at `~/.config/io.github.kilo2071.Resonate/config.json`.
