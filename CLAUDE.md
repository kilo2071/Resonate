# Resonate

A Libadwaita soundboard application for GNOME with virtual audio output and microphone effect plugins.

## Language & Rationale

**Rust** — chosen for this project because:
- `gtk4-rs` and `libadwaita` crates are mature and actively maintained by the GNOME Rust team
- `pipewire-rs` provides safe bindings to PipeWire for virtual audio device management
- Memory safety is critical for real-time audio processing (no use-after-free in audio callbacks)
- GNOME itself ships multiple first-party Rust apps (Loupe, Snapshot, etc.)

## Target Platform

- **OS**: Fedora 44
- **Desktop**: GNOME 50
- **Audio**: PipeWire (default on Fedora)
- **UI toolkit**: GTK 4 + Libadwaita

## App ID

`io.github.kilo2071.Resonate`

Config stored at: `~/.config/io.github.kilo2071.Resonate/config.json`

## Features

### Core
- Soundboard: load and play audio samples (per-tile volume, polyphonic stacking + queue), monitored through the system default output
- Virtual audio device: PipeWire virtual source so sounds and mic appear as one mic input to other apps
- Plugin system: apply real-time effects to microphone input (built-in Noise Gate, Gain + any installed LV2 plugin)
- Background mode: close-to-tray keeps effects running; SNI tray indicator + optional start-on-login (see "Lifecycle / background mode")

### Plugin Architecture
- Built-in plugins implement `ResonatePlugin` trait in `src/plugins/builtin/`
- Installed LV2 plugins are hosted via the `livi` crate (links `lilv`) in `src/plugins/lv2.rs`, wrapped behind the same `ResonatePlugin` trait. A single process-global `Lv2Host` owns the `livi::World` for the program lifetime (lilv instances don't keep the world alive).
- The chain runs in-process on the PipeWire **mic capture** callback (`src/audio/virtual_device.rs`): physical mic → `PluginChain::process` → Resonate sink. The chain is `Arc<Mutex<PluginChain>>`, shared with the UI thread (`try_lock` in the RT callback).
- Chain is serialised to `config.json` as `effects_chain: Vec<EffectEntry>`. Built-in entries use ids `gain`/`gate`; LV2 entries use id `lv2:<uri>` with control values keyed by port symbol.
- The effects page builds parameter sliders generically from `ResonatePlugin::params()`, so LV2 plugins get controls automatically; the "Add effect" sheet lists built-ins + `lv2::discover()`.

## Key Dependencies

```toml
[dependencies]
gtk = { version = "0.11", package = "gtk4", features = ["v4_14"] }
adw = { version = "0.9", package = "libadwaita", features = ["v1_6"] }
pipewire = "0.10"
livi = "0.7"          # LV2 host (links system lilv/lv2 dev libs)
libloading = "0.8"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
anyhow = "1"
log = "0.4"
env_logger = "0.11"
rodio = { version = "0.20", default-features = false, features = ["symphonia-all"] }
ksni = "0.2"          # StatusNotifierItem tray (0.2 = libdbus backend, no tokio)

[build-dependencies]
glib-build-tools = "0.20"
```

No `tokio`, no `ringbuf`. `ksni` is pinned to **0.2** on purpose: 0.3+ pulls in
`zbus` + `tokio`. 0.2 uses the blocking `dbus` crate (links system `libdbus`),
which keeps the no-async-runtime constraint.

## Project Structure

```
resonate/
├── src/
│   ├── main.rs                     # app entry, GApplication setup; APP_ID, START_HIDDEN, --hidden
│   ├── application.rs              # AdwApplication subclass; single-instance activate, --hidden
│   ├── window.rs                   # AdwApplicationWindow + wiring; close-to-background, tray poll, autostart
│   ├── tray.rs                     # StatusNotifierItem tray (ksni); TrayCmd channel → GTK
│   ├── config.rs                   # Config, EffectEntry (serde_json)
│   ├── audio/
│   │   ├── mod.rs
│   │   ├── engine.rs               # rodio Sink mgmt, tick loop, PCM decode, shared FX chain
│   │   ├── virtual_device.rs       # in-process PipeWire streams: bridge load, soundboard + mic capture/playback
│   │   ├── pw_config.rs            # routing plan, mic detection, loopback teardown, drop-in persistence
│   │   └── sampler.rs              # (legacy helper, mostly superseded by engine)
│   ├── plugins/
│   │   ├── mod.rs                  # ResonatePlugin trait, PluginParam, plugin_from_entry factory
│   │   ├── host.rs                 # PluginChain (Vec<Box<dyn ResonatePlugin>>)
│   │   ├── lv2.rs                  # LV2 host (livi): Lv2Host, discover(), Lv2Plugin
│   │   └── builtin/
│   │       ├── mod.rs
│   │       ├── gain.rs             # GainPlugin (0.0–4.0x linear)
│   │       └── gate.rs             # NoiseGatePlugin (RMS, attack/release counters)
│   └── ui/
│       ├── mod.rs
│       ├── soundboard_page.rs      # soundboard grid
│       ├── sound_tile.rs           # individual tile widget
│       ├── settings_page.rs        # virtual device / input settings
│       └── effects_page.rs         # mic effects chain editor
├── resources/
│   ├── resources.gresource.xml
│   └── ui/
│       ├── window.ui
│       ├── soundboard_page.ui
│       ├── settings_page.ui
│       └── effects_page.ui
├── data/
│   ├── io.github.kilo2071.Resonate.desktop   # desktop entry (Exec=resonate)
│   └── icons/io.github.kilo2071.Resonate.svg # app icon (also bundled in gresource)
├── packaging/
│   └── resonate.spec               # local RPM build
├── Cargo.toml
└── build.rs                        # gresource compilation
```

GResource paths use prefix `/io/github/kilo2071/Resonate/`.

## Lifecycle / background mode

- `APP_ID` (`io.github.kilo2071.Resonate`) is the GApplication id **and** the
  basename of the installed `.desktop` file **and** the icon name — GNOME matches
  a running window to its desktop entry by this id, so all three must agree or the
  Overview/Alt-Tab show a generic name/icon.
- **Close = hide to background.** The window's `close-request` handler hides the
  window (returns `Propagation::Stop`) instead of destroying it, so the PipeWire
  thread and effects chain keep running. Real teardown happens only via the tray
  **Quit** (`do_quit` sets `force_quit`, drops `virtual_device`, `app.quit()`),
  which lets the next `close-request` proceed.
- **Tray** (`tray.rs`): `ksni::TrayService` runs on its own thread; clicks are
  sent as `TrayCmd` over an `mpsc` channel and drained by a 150 ms glib timeout on
  the main thread (no async runtime). Needs the GNOME "AppIndicator/KStatusNotifier"
  extension to be visible.
- **Autostart**: a Settings switch writes/removes
  `~/.config/autostart/<APP_ID>.desktop` with `Exec=<exe> --hidden`. `--hidden`
  sets `START_HIDDEN`, so `activate()` creates the window (effects start) but does
  not present it. `activate()` is single-instance: re-launching raises the
  existing window instead of opening a second one.

## Build & Run

```bash
cargo build
cargo run

# With debug logging
RUST_LOG=resonate=debug cargo run
```

## GObject Subclassing Convention

Follow the `gtk4-rs` book pattern: `imp` module inside each widget, `#[derive(gtk::CompositeTemplate)]` for UI templates in XML.

## Audio Architecture

The virtual mic is a PipeWire `module-loopback` **bridge**: an `Audio/Sink`
(`resonate_sink`) whose playback side is an `Audio/Source` (`resonate_source`,
shown to other apps as "Resonate Microphone"). Two in-process PipeWire output
streams feed the sink; PipeWire sums them there.

```
[Microphone] → resonate-mic-capture (Input) → [PluginChain] → mic bridge (Rc<RefCell<VecDeque>>)
                                                                     ↓
                                                  resonate-mic (Output) ─┐
[Soundboard] → rodio + PCM decode → elapsed-time tick() →                ├──▶ resonate_sink
               audio_queue (Mutex<VecDeque>) → resonate-soundboard (Output) ┘        │
                                                                                  (bridge)
                                                                                     ▼
                                                                            resonate_source
                                                                     [Apps see "Resonate Microphone"]
```

### Bridge creation gotcha

`pw-cli load-module` does **not** persist — the module dies when the pw-cli
client exits, so it can't be used to create the bridge. The session bridge is
created **in-process** via `pw_sys::pw_context_load_module` on our own
long-lived context (`virtual_device.rs`); it lives until the context drops. The
**offline** mic pass-through (mic → sink while Resonate isn't running) is provided
by a drop-in at `~/.config/pipewire/pipewire.conf.d/resonate.conf`, applied at
login. On startup, `pw_config::claim_routing` tears down the raw loopback nodes
(`resonate_mic_in/out`) so the mic isn't fed to the sink twice.

### Thread model

- **GLib main loop** — UI, tick timer (every ~40ms), config saves, FX add/remove/param
- **PipeWire thread** — soundboard playback + mic capture/playback RT callbacks (no blocking)
- **Background thread** — PCM decode for soundboard files

### Cross-thread audio transport

- GLib tick → soundboard stream: `audio_queue` (`Arc<Mutex<VecDeque<f32>>>`); UI thread `lock()`, RT callback `try_lock()`
- mic capture → mic playback: a per-PW-thread `Rc<RefCell<VecDeque<f32>>>` bridge (both callbacks on the same PW thread, no locking)
- mic effects chain: `Arc<Mutex<PluginChain>>` shared UI↔PW; mutated on UI thread, `try_lock()` in the capture callback
- capture is channel-aware (mono mics are upmixed to stereo); it pins to the physical mic with `target.object`/`node.target`/`node.dont-reconnect` and never autoconnects to the default (which would be our own source → feedback)

### Elapsed-time mixing

`AudioEngine::tick()` calculates `elapsed_secs` since `last_tick`, converts to sample count `n`, and pushes `n` frames to the virtual device. This avoids relying on `sink.get_pos()` which returns near-zero on PipeWire/cpal.

If PCM is still decoding, `pcm_pos` is advanced speculatively so playback starts mid-stream correctly once the decode completes.

## Plugin Trait

```rust
pub trait ResonatePlugin: Send {
    fn name(&self) -> &str;
    fn id(&self) -> &str;
    fn is_enabled(&self) -> bool;
    fn set_enabled(&mut self, enabled: bool);
    fn process(&mut self, samples: &mut [f32], sample_rate: u32); // in-place
    fn params(&self) -> Vec<PluginParam>;
    fn set_param(&mut self, id: &str, value: f32);
}
```

Processing is in-place (`&mut [f32]`). Chain is applied sequentially in `PluginChain::process()`.

## Config Schema

```rust
pub struct Config {
    pub sounds_folder: PathBuf,
    pub move_files_to_folder: bool,
    pub polyphonic: bool,
    pub default_volume: u32,               // 0–100 start volume for new tiles
    pub virtual_device_name: String,
    pub virtual_device_enabled: bool,
    pub monitor_enabled: bool,             // play soundboard on the default output
    pub monitor_volume: f32,
    pub input_device_name: String,
    pub mic_volume: f32,
    pub effects_chain: Vec<EffectEntry>,  // default: [gate(disabled), gain(enabled)]
}

pub struct EffectEntry {
    pub id: String,                        // "gain" | "gate" | "lv2:<uri>"
    pub enabled: bool,
    pub params: HashMap<String, f32>,      // e.g. {"gain": 1.0}, {"threshold": 0.02, ...}
}
```

Soundboard playback settings are applied in `AudioEngine`: `polyphonic` gates
`play()` (off = queue instead of overlap; the same sound can be stacked while
polyphonic is on); per-tile volume is a 0–1 factor passed into `play()`/`cue()`
(read from each tile's slider at click time) that scales both the rodio monitor
sink and the virtual-mic PCM mix. The monitor always uses the system default
output (there is no monitor-device picker).

## Coding Conventions

- All GObject subclasses live in an `imp` submodule
- UI templates in `resources/ui/*.ui` (GTK XML, not Blueprint)
- Errors propagate with `anyhow::Result`; only panic on programmer errors
- No `unwrap()` in audio callback paths — log and recover instead
- PipeWire callbacks are `unsafe`; isolate unsafe blocks as tightly as possible
- `try_lock()` (non-blocking) in all PipeWire RT callbacks; `lock()` only on UI thread
- GTK4 switch state-set callbacks must return `glib::Propagation::Proceed` (not `bool`)
- Use `widget.downgrade()` / `upgrade()` pattern for weak refs in closures that outlive the widget
