# Resonate

There wasn't a nice native soundboard app for GNOME, so I vibecoded one over a couple of days with [Claude](https://www.anthropic.com/claude-code). It turned into a GTK 4 + Libadwaita app in Rust with polyphonic playback, a sequential queue, a real-time LCD panel with oscilloscope and scrubbing, per-sound editing (start/trim/fades), global numpad hotkeys, **a PipeWire virtual microphone, and a real-time mic effects chain (built-in + LV2 plugins, with presets)**.

> ⚠️ **Use at your own risk.** This is a hobby project that is largely AI-generated and lightly tested. It pokes at your PipeWire graph and sets your default input device, so expect rough edges — back up nothing important and don't run it in the middle of an important call. No warranty, etc. etc.

![Platform](https://img.shields.io/badge/platform-Fedora%2044%20%2F%20GNOME%2050-blue)
![Language](https://img.shields.io/badge/language-Rust-orange)

## Features

### Soundboard
- **Compact tile grid** — load any number of audio files; play/cue on the tile, volume + editor + rename/remove in the tile's ⋮ menu; **search** box to filter; **drag a tile onto another to reorder** (order is saved)
- **Per-sound settings, persisted** — volume, start marker, end trim and fade in/out are saved per sound (`config.json`) and survive renames; volume changes apply live to already-playing sounds
- **Sound editor** — per-sound dialog with the decoded waveform: drag the orange start marker / red end-trim marker, set fades, and preview from the marker (monitor-only, with playhead)
- **Selection + one-shot start** — click a tile to select it, then drag the LCD progress bar to set a *one-shot* start point that only the next play uses; the transport preview (headphones) and play buttons act on the selection
- **Scrub while playing** — drag the progress bar during playback to seek
- **Loudness normalization** — newly imported sounds are analyzed and their volume pre-set so peaks land near −1 dBFS (loud files are only ever turned down)
- **Global hotkeys** — Ctrl+Alt + a tile number typed on the numpad plays that tile (e.g. `5` `6` → tile 56); Ctrl+Alt+Numpad Enter stops everything (XDG GlobalShortcuts portal; GNOME asks for approval once)
- **Polyphonic & sequential modes** — play (and stack) sounds simultaneously, or queue them up; toggled in Settings
- **Cue button** — always appends to the queue, regardless of mode
- **Skip** — stops all currently playing sounds and immediately starts the next item in the queue
- **LCD playback panel** — scrollable track list with per-track countdown, a live **oscilloscope** of the playing mix, aggregate time totals, and the interactive progress bar
- **Rename** — rename sound tiles with an in-place dialog; the file on disk, saved settings and all playback references follow
- **Remove with undo** — moves the file out of the Sounds Folder (cross-filesystem safe), removes the tile, and shows a 6-second undo toast; permanent deletion happens on dismiss
- **Sounds Folder** — configure a folder in Settings; all audio files found there are loaded automatically on launch

### Settings
- **Sounds Folder** + **Move Added Files** — where sounds live, and whether new files are moved there
- **Input Device** — pick which physical microphone feeds the effects chain (default = auto-detect)
- **Monitor Output** — hear the soundboard yourself on the system default output (toggle); monitor level is the slider on the soundboard bar
- **Playback** — Play Multiple Sounds (polyphonic) and Default Volume for new tiles
- **Virtual Device** — set the virtual mic's display name and whether it is created on launch
- **Startup** — Start on Login (run hidden in the background and keep mic effects active)

### Virtual microphone
- **PipeWire virtual mic** — Resonate exposes a "Resonate Microphone" source that other apps (Discord, OBS, browsers…) can select as an input
- **Sounds + mic mixed together** — soundboard playback and your real microphone are summed into the virtual mic, so others hear both
- **Set as default input on launch** — the virtual mic is made the system default input automatically
- **Survives across logins** — a PipeWire drop-in (`~/.config/pipewire/pipewire.conf.d/resonate.conf`) keeps a raw mic pass-through alive even when Resonate isn't running

### Mic effects
- **Real-time effects chain** applied to the microphone, grouped into **Voice & Cleanup** and **Character & Fun**:
  - *Voice & Cleanup* — built-in **Noise Gate** and **Gain**, plus curated LV2 (hosted via [`livi`](https://github.com/wmedrano/livi)): **Noise Suppression (RNNoise)**, **Auto Gain**, **Compressor**, **De-esser**, **Limiter**, **Graphic/Parametric EQ**, **Exciter**, **Bass Enhancer** — the same plugins Easy Effects wraps
  - *Character & Fun* — built-in **Distortion**, **Bitcrusher** and **Telephone**, plus **Pitch Shifter** (chipmunk/demon), **Auto-Tune**, **Ring Modulator**, **Vocoder**, **Reverb**, **Vintage/Reverse Delay**, **Rotary Speaker**, **Multi Chorus**, **Flanger**, **Phaser**, **Pulsator**, **Saturator**, **Crusher** and **Tape Simulator**
- **Add / remove / reorder** effects from the Effects page; controls are generated automatically per plugin and rendered by type — sliders, switches (toggles) and dropdowns (enumerated choices)
- **Chain presets** — save the current chain under a name and switch between chains in one click, from the app or the tray menu (which marks the active one, or "Custom" once you tweak a knob)
- **Per-effect presets** — a *Preset* dropdown above each effect's knobs with ready-made starting points ("Podcast", "Radio DJ", "Chipmunk", "Stadium announcer"), so a 70-knob compressor is one pick instead of an afternoon. Presets shipped by the plugin itself (Calf's, say) show up in the same list
- **Level meter** — live post-effects mic level, so you can set the gate threshold by eye
- Chain and presets are persisted to `config.json` and re-applied on launch. (The Add sheet only lists the curated plugins, but a chain referencing any other installed LV2 id still loads.)

### Background mode (Easy Effects-style)
- **Run in the background** — closing the window hides Resonate instead of quitting, so the mic effects keep processing
- **Tray indicator** — a StatusNotifierItem with *Show Resonate* / *Quit* (on GNOME this needs the [AppIndicator and KStatusNotifierItem Support](https://extensions.gnome.org/extension/615/appindicator-support/) shell extension)
- **Start on Login** — a Settings switch installs an autostart entry that launches Resonate hidden, so your processed mic is available right after login

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
sudo dnf install lilv-devel lv2-devel serd-devel sord-devel sratom-devel dbus-devel
```

## Build & Run

```bash
# Debug build
cargo build
cargo run

# With verbose logging
RUST_LOG=resonate=debug cargo run

# Launch hidden in the background (used by the autostart entry)
cargo run -- --hidden
```

## Build an RPM (Fedora)

A spec for a local, installable package lives in `packaging/resonate.spec`:

```bash
sudo dnf install rpm-build rpmdevtools
rpmdev-setuptree

VERSION=$(awk -F'"' '/^version/{print $2; exit}' Cargo.toml)
git archive --format=tar.gz --prefix=resonate-$VERSION/ \
  -o ~/rpmbuild/SOURCES/resonate-$VERSION.tar.gz HEAD
rpmbuild -bb packaging/resonate.spec

sudo dnf install ~/rpmbuild/RPMS/*/resonate-$VERSION-*.rpm
```

The package installs the binary to `/usr/bin/resonate` and the desktop entry +
icon under `/usr/share`, and softly recommends `lsp-plugins-lv2` for a ready-made
set of LV2 effects.

## Installing LV2 effect plugins

Resonate hosts any LV2 plugin found on the standard search path (`~/.lv2`,
`/usr/lib64/lv2`, …). Useful packages on Fedora:

```bash
# Voice: a large, high-quality suite (gate, compressor, autogain, EQ, …)
sudo dnf install lsp-plugins-lv2

# Character & fun: reverb, delays, ring modulator, vocoder, rotary speaker,
# chorus/flanger/phaser, saturator, crusher, tape
sudo dnf install lv2-calf-plugins

# Pitch shifting (chipmunk/demon) and auto-tune
sudo dnf install lv2-rubberband-plugins lv2-x42-plugins

# Noise cancellation (via the audinux COPR)
sudo dnf copr enable ycollet/audinux
sudo dnf install noise-repellent
```

The Add Effect sheet shows the built-ins plus a curated set of installed LV2
plugins, grouped by category (discovery runs once when the Effects page first
opens); curated effects whose plugin isn't installed are simply not listed. A
handy mic chain, in order: **Noise Gate** → **Noise Suppression (RNNoise)** →
**Auto Gain** → **Gain**. For a silly one: **Pitch Shifter** ("Demon") →
**Ring Modulator** ("Robot") → **Vintage Delay** ("Stadium announcer").

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
  application.rs        — AdwApplication subclass (registers --hidden)
  window.rs             — AdwApplicationWindow; audio/effects/hotkey wiring, rename/remove logic
  config.rs             — Config, EffectEntry, per-sound SoundSettings (serde_json)
  hotkeys.rs            — global hotkeys via the GlobalShortcuts portal (blocking dbus)
  tray.rs               — StatusNotifierItem tray (ksni), incl. preset submenu
  audio/
    engine.rs           — rodio polyphonic engine, queue, tick loop, seek, envelope, scope feed
    virtual_device.rs   — in-process PipeWire streams: bridge, soundboard + mic capture/playback
    pw_config.rs        — routing plan, mic detection, loopback teardown, drop-in persistence
    wave.rs             — waveform peak data shared by the editor and the LCD
    sampler.rs          — (legacy stub)
  plugins/
    mod.rs              — ResonatePlugin trait, PluginParam, BUILTINS + CURATED_LV2 registries
    host.rs             — PluginChain
    lv2.rs              — LV2 host (livi): discovery + Lv2Plugin
    builtin/
      gain.rs           — Gain (0.0–4.0× linear)
      gate.rs           — Noise Gate (RMS, attack/release)
      distortion.rs     — tanh waveshaper (drive/mix/level)
      bitcrush.rs       — bit depth + sample-and-hold downsampling
      telephone.rs      — band-limit filter (old phone/radio)
  ui/
    soundboard_page.rs  — sound grid, selection, LCD panel (oscilloscope, progress scrub)
    sound_tile.rs       — individual tile widget (menu: volume, edit, rename, remove)
    sound_editor.rs     — waveform editor dialog (markers, fades, preview)
    effects_page.rs     — mic effects chain editor (built-in + curated LV2, presets, meter)
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

## License

[GPL-3.0-or-later](LICENSE).
