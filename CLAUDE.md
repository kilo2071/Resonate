# Resonate

A Libadwaita soundboard application for GNOME with virtual audio output and microphone effect plugins.

## Language & Rationale

**Rust** — chosen for this project because:
- `gtk4-rs` and `libadwaita` crates are mature and actively maintained by the GNOME Rust team
- `pipewire-rs` provides safe bindings to PipeWire for virtual audio device management
- Memory safety is critical for real-time audio processing (no use-after-free in audio callbacks)
- LV2 plugin loading can be done via the `lv2` crate or direct `libloading`
- GNOME itself ships multiple first-party Rust apps (Loupe, Snapshot, etc.)

## Target Platform

- **OS**: Fedora 44
- **Desktop**: GNOME 50
- **Audio**: PipeWire (default on Fedora)
- **UI toolkit**: GTK 4 + Libadwaita

## Features

### Core
- Soundboard: load and play audio samples through a PipeWire virtual sink
- Virtual audio device: create a PipeWire loopback/virtual source so sounds appear as a microphone input
- Plugin system: apply real-time effects to microphone input (gate, EQ, reverb, etc.)

### Plugin Architecture
- Plugins are dynamically loaded shared libraries (`.so`) implementing a common `ResonatePlugin` trait
- Support for LV2 plugins (standard Linux audio plugin format) via the `lv2` crate
- Plugin chain: microphone → [plugin 1] → [plugin 2] → virtual output

## Key Dependencies

```toml
[dependencies]
gtk = { version = "0.9", package = "gtk4", features = ["v4_14"] }
adw = { version = "0.7", package = "libadwaita", features = ["v1_6"] }
pipewire = "0.8"          # pipewire-rs
libloading = "0.8"        # dynamic plugin loading
serde = { version = "1", features = ["derive"] }
serde_json = "1"          # config/preset serialization
anyhow = "1"
tokio = { version = "1", features = ["rt", "macros"] }  # async runtime
```

## Project Structure

```
resonate/
├── src/
│   ├── main.rs               # app entry, GApplication setup
│   ├── application.rs        # AdwApplication subclass
│   ├── window.rs             # AdwApplicationWindow subclass
│   ├── audio/
│   │   ├── mod.rs
│   │   ├── engine.rs         # PipeWire main loop, graph management
│   │   ├── virtual_device.rs # virtual sink/source node creation
│   │   └── sampler.rs        # sound file playback
│   ├── plugins/
│   │   ├── mod.rs
│   │   ├── host.rs           # plugin loader and chain runner
│   │   ├── lv2.rs            # LV2 plugin adapter
│   │   └── builtin/          # built-in effects (gain, gate, etc.)
│   └── ui/
│       ├── soundboard.rs     # soundboard grid view
│       ├── mixer.rs          # volume/routing controls
│       └── plugin_rack.rs    # plugin chain editor
├── data/
│   ├── io.github.resonate.gschema.xml
│   ├── io.github.resonate.desktop
│   └── io.github.resonate.metainfo.xml
├── resources/
│   └── resources.gresource.xml
├── Cargo.toml
└── build.rs                  # gresource compilation
```

## App ID

`io.github.resonate`

## Build & Run

```bash
cargo build
cargo run

# With debug logging
RUST_LOG=resonate=debug cargo run
```

## GObject Subclassing Convention

Follow the `gtk4-rs` book pattern: imp module inside each widget, using `#[derive(CompositeTemplate)]` for UI templates defined in XML.

## Audio Architecture

```
[Microphone] → PipeWire → [Plugin Chain] → [Virtual Sink]
                                                  ↓
[Soundboard samples] → PipeWire ──────────→ [Virtual Sink]
                                                  ↓
                                        [Apps see "Resonate" mic]
```

PipeWire node management happens on a dedicated thread; all UI interaction crosses the thread boundary via channels (GLib main loop ↔ PipeWire thread).

## Plugin Trait (planned)

```rust
pub trait ResonatePlugin: Send {
    fn name(&self) -> &str;
    fn process(&mut self, input: &[f32], output: &mut [f32], sample_rate: u32);
    fn params(&self) -> Vec<PluginParam>;
    fn set_param(&mut self, id: &str, value: f32);
}
```

## Coding Conventions

- All GObject subclasses live in an `imp` submodule
- UI templates in `resources/ui/*.ui` (Blueprint or XML)
- Errors propagate with `anyhow::Result`; only panic on programmer errors
- No `unwrap()` in audio callback paths — log and recover instead
- PipeWire callbacks are `unsafe`; isolate unsafe blocks as tightly as possible
