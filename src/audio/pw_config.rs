//! PipeWire-native virtual-microphone routing setup.
//!
//! Instead of hand-mixing samples in the app, Resonate builds the virtual mic out
//! of two `module-loopback` instances inside the PipeWire graph:
//!
//!   1. "bridge" loopback — capture side is an `Audio/Sink` ("Resonate"),
//!      playback side is an `Audio/Source` ("Resonate Microphone").
//!      Anything played into the sink comes out the source.
//!   2. "mic" loopback — captures the physical microphone and plays it into the
//!      Resonate sink, so the user's voice is always part of the virtual mic,
//!      even when Resonate itself is not running.
//!
//! These are created two ways:
//!   * For the **current session**: `pw-cli load-module` loads them into the running
//!     PipeWire daemon. Because they live in the daemon (not in our process), they
//!     persist after Resonate exits, until the daemon restarts.
//!   * For **future logins / daemon restarts**: a drop-in at
//!     `~/.config/pipewire/pipewire.conf.d/resonate.conf` recreates them on boot.
//!
//! The app then only runs a single playback stream that feeds soundboard PCM into
//! the Resonate sink (see `virtual_device.rs`); PipeWire does the mixing.

use std::path::PathBuf;
use std::process::Command;

use super::virtual_device::{enumerate_nodes, AudioNode, SINK_NAME, SOURCE_NAME};

const MIC_IN_NAME: &str = "resonate_mic_in";
const MIC_OUT_NAME: &str = "resonate_mic_out";

/// Path to the generated drop-in config.
pub fn dropin_path() -> PathBuf {
    gtk::glib::user_config_dir()
        .join("pipewire")
        .join("pipewire.conf.d")
        .join("resonate.conf")
}

/// What the in-process PipeWire thread should set up, decided by [`claim_routing`].
pub struct RoutingPlan {
    /// Physical mic node to capture from (the in-process capture target), or
    /// `None` to let PipeWire pick the default source.
    pub mic_target: Option<String>,
    /// Whether the sink/source bridge must be created in our context (it isn't
    /// already in the graph, e.g. from the drop-in applied at login).
    pub create_bridge: bool,
}

/// Claim the mic for Resonate's in-process effects path.
///
/// Persists the drop-in (so the raw mic pass-through works at the next login when
/// Resonate is *not* running), tears down any live raw mic loopback so it doesn't
/// double the in-process effected mic, and reports whether the bridge needs
/// creating in our own context.
///
/// Note: `pw-cli load-module` does *not* persist (the module dies with the client),
/// so the session bridge is created in-process (see `virtual_device.rs`) and the
/// offline pass-through is provided solely by the drop-in at login.
///
/// `source_desc`    — display name for the virtual mic ("Resonate Microphone").
/// `input_dev_desc` — description of the physical mic chosen in settings; empty = auto.
pub fn claim_routing(source_desc: &str, input_dev_desc: &str) -> RoutingPlan {
    let nodes = enumerate_nodes();
    let mic = detect_physical_mic(input_dev_desc, &nodes);
    if mic.is_none() {
        log::warn!("No physical microphone detected; virtual mic will carry soundboard only");
    }

    // Persist for future logins / daemon restarts (offline mic pass-through).
    let content = generate_dropin(source_desc, mic.as_deref());
    write_dropin(&content);

    let have_bridge =
        nodes.iter().any(|n| n.name == SINK_NAME) && nodes.iter().any(|n| n.name == SOURCE_NAME);

    // Hand the mic over to the in-process path: remove the raw loopback (from a
    // previous session or the boot drop-in) so the mic isn't fed to the sink twice.
    teardown_mic_loopback();

    RoutingPlan {
        mic_target: mic,
        create_bridge: !have_bridge,
    }
}

/// SPA-JSON args for the `libpipewire-module-loopback` bridge: an `Audio/Sink`
/// ("Resonate") whose playback side is an `Audio/Source` (the virtual mic).
/// Used to load the module into our own context via `pw_context_load_module`.
pub fn bridge_module_args(source_desc: &str) -> String {
    format!(
        "{{ \
            node.description = \"Resonate\" \
            capture.props = {{ \
                media.class = Audio/Sink \
                node.name = \"{sink}\" \
                node.description = \"Resonate\" \
                audio.position = [ FL FR ] \
            }} \
            playback.props = {{ \
                media.class = Audio/Source \
                node.name = \"{source}\" \
                node.description = \"{desc}\" \
                audio.position = [ FL FR ] \
            }} \
        }}",
        sink = SINK_NAME,
        source = SOURCE_NAME,
        desc = source_desc,
    )
}

/// Destroy the raw mic loopback nodes (`resonate_mic_in` / `resonate_mic_out`) if
/// present, so they stop feeding the sink. Best-effort.
fn teardown_mic_loopback() {
    let ids: Vec<u32> = enumerate_nodes()
        .into_iter()
        .filter(|n| n.name == MIC_IN_NAME || n.name == MIC_OUT_NAME)
        .map(|n| n.id)
        .collect();
    for id in ids {
        match Command::new("pw-cli").arg("destroy").arg(id.to_string()).status() {
            Ok(s) if s.success() => log::info!("Removed raw mic loopback node {id}"),
            Ok(s) => log::warn!("pw-cli destroy {id} exited with {s}"),
            Err(e) => log::warn!("pw-cli destroy failed: {e}"),
        }
    }
}

// ── Physical mic detection ──────────────────────────────────────────────────────

/// Resolve which physical source to feed into the Resonate sink.
/// Never returns one of our own virtual nodes (which would cause a feedback loop).
fn detect_physical_mic(configured_desc: &str, nodes: &[AudioNode]) -> Option<String> {
    let is_ours = |name: &str| name.starts_with("resonate");

    // 1. Explicitly chosen device (matched by description).
    if !configured_desc.is_empty() {
        if let Some(n) = nodes
            .iter()
            .find(|n| n.description == configured_desc && !is_ours(&n.name))
        {
            return Some(n.name.clone());
        }
    }

    // 2. The system default source — unless it's already our virtual mic.
    if let Ok(out) = Command::new("pactl").arg("get-default-source").output() {
        let name = String::from_utf8_lossy(&out.stdout).trim().to_string();
        if !name.is_empty() && !is_ours(&name) {
            return Some(name);
        }
    }

    // 3. First real capture device.
    nodes
        .iter()
        .find(|n| {
            n.media_class.contains("Source") && !is_ours(&n.name) && n.name.starts_with("alsa_input")
        })
        .map(|n| n.name.clone())
}

// ── Drop-in generation (persistence) ──────────────────────────────────────────

fn generate_dropin(source_desc: &str, mic_node: Option<&str>) -> String {
    let mut s = String::new();
    s.push_str("# Resonate virtual microphone routing.\n");
    s.push_str("# Auto-generated by Resonate — manual edits are overwritten.\n");
    s.push_str("context.modules = [\n");

    // Bridge: Resonate sink -> Resonate Microphone source.
    s.push_str("    { name = libpipewire-module-loopback\n");
    s.push_str("      args = {\n");
    s.push_str("          node.description = \"Resonate\"\n");
    s.push_str("          capture.props = {\n");
    s.push_str("              media.class = Audio/Sink\n");
    s.push_str(&format!("              node.name = \"{SINK_NAME}\"\n"));
    s.push_str("              node.description = \"Resonate\"\n");
    s.push_str("              audio.position = [ FL FR ]\n");
    s.push_str("          }\n");
    s.push_str("          playback.props = {\n");
    s.push_str("              media.class = Audio/Source\n");
    s.push_str(&format!("              node.name = \"{SOURCE_NAME}\"\n"));
    s.push_str(&format!("              node.description = \"{source_desc}\"\n"));
    s.push_str("              audio.position = [ FL FR ]\n");
    s.push_str("          }\n");
    s.push_str("      }\n");
    s.push_str("    }\n");

    // Mic pass-through (only if we have a physical mic to pin to).
    if let Some(mic) = mic_node {
        s.push_str("    { name = libpipewire-module-loopback\n");
        s.push_str("      args = {\n");
        s.push_str("          node.description = \"Resonate Mic Pass-through\"\n");
        s.push_str("          capture.props = {\n");
        s.push_str(&format!("              node.name = \"{MIC_IN_NAME}\"\n"));
        s.push_str(&format!("              node.target = \"{mic}\"\n"));
        s.push_str("          }\n");
        s.push_str("          playback.props = {\n");
        s.push_str("              media.class = Stream/Output/Audio\n");
        s.push_str(&format!("              node.name = \"{MIC_OUT_NAME}\"\n"));
        s.push_str(&format!("              node.target = \"{SINK_NAME}\"\n"));
        s.push_str("              audio.position = [ FL FR ]\n");
        s.push_str("          }\n");
        s.push_str("      }\n");
        s.push_str("    }\n");
    }

    s.push_str("]\n");
    s
}

/// Write the drop-in if its contents changed. Returns true if it was (re)written.
fn write_dropin(content: &str) -> bool {
    let path = dropin_path();
    if let Ok(existing) = std::fs::read_to_string(&path) {
        if existing == content {
            return false;
        }
    }
    if let Some(parent) = path.parent() {
        if let Err(e) = std::fs::create_dir_all(parent) {
            log::warn!("Could not create {}: {e}", parent.display());
            return false;
        }
    }
    match std::fs::write(&path, content) {
        Ok(_) => {
            log::info!("Wrote PipeWire drop-in {}", path.display());
            true
        }
        Err(e) => {
            log::warn!("Could not write {}: {e}", path.display());
            false
        }
    }
}
