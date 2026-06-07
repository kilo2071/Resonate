//! LV2 plugin hosting via the `livi` crate (which links `lilv`).
//!
//! Resonate hosts installed LV2 audio-effect plugins on the microphone chain.
//! A single process-wide [`Lv2Host`] owns one `livi::World` (the plugin database)
//! plus a shared `Features` set; it must outlive every instance, because lilv's
//! activated instances do *not* keep the world alive on their own.
//!
//! Plugins are exposed through the same [`ResonatePlugin`] trait as the built-ins,
//! so the chain, config and effects UI treat LV2 effects uniformly. Control-input
//! ports become [`PluginParam`]s (keyed by port symbol); audio is converted between
//! our interleaved-stereo buffers and the plugin's per-channel layout.

use std::collections::HashMap;
use std::sync::OnceLock;

use livi::event::LV2AtomSequence;
use livi::{EmptyPortConnections, Instance, PortIndex, PortType};

use crate::audio::virtual_device::{CHANNELS, SAMPLE_RATE};
use crate::plugins::{ParamKind, PluginParam, ResonatePlugin};

// LV2 control-port property URIs used to pick the right UI control.
const LV2_TOGGLED: &str = "http://lv2plug.in/ns/lv2core#toggled";
const LV2_INTEGER: &str = "http://lv2plug.in/ns/lv2core#integer";
const LV2_ENUMERATION: &str = "http://lv2plug.in/ns/lv2core#enumeration";

/// `EffectEntry.id` prefix that marks an LV2 plugin (the rest is the LV2 URI).
pub const LV2_ID_PREFIX: &str = "lv2:";

/// Largest block we hand to a plugin in one `run` call; longer buffers are chunked.
const MAX_BLOCK: usize = 4096;
/// Atom-sequence scratch capacity (bytes). We never feed events, but plugins with
/// atom-input ports still need a valid (empty) sequence connected.
const ATOM_CAPACITY: usize = 4096;

/// Build an `EffectEntry.id` from an LV2 URI.
pub fn id_for_uri(uri: &str) -> String {
    format!("{LV2_ID_PREFIX}{uri}")
}

/// Extract the LV2 URI from an `EffectEntry.id`, if it is an LV2 entry.
pub fn uri_from_id(id: &str) -> Option<&str> {
    id.strip_prefix(LV2_ID_PREFIX)
}

// ── Global host ─────────────────────────────────────────────────────────────────

struct Lv2Host {
    world: livi::World,
    features: std::sync::Arc<livi::Features>,
}

// Safe: `livi::World` is backed by lilv's `Arc<Life>` (Send + Sync) and `Features`
// is declared Send + Sync by livi. We only ever share `&Lv2Host`.
unsafe impl Send for Lv2Host {}
unsafe impl Sync for Lv2Host {}

static HOST: OnceLock<Lv2Host> = OnceLock::new();

fn host() -> &'static Lv2Host {
    HOST.get_or_init(|| {
        let world = livi::World::new();
        let features = world.build_features(livi::FeaturesBuilder {
            min_block_length: 1,
            max_block_length: MAX_BLOCK,
        });
        Lv2Host { world, features }
    })
}

// ── Discovery ───────────────────────────────────────────────────────────────────

/// Metadata for an installed LV2 plugin suitable for hosting as a mic effect.
#[derive(Clone, Debug)]
pub struct Lv2Info {
    pub uri: String,
    pub name: String,
}

/// List installed LV2 plugins that can act as audio effects (≥1 audio in/out, no
/// CV ports, not instruments), sorted by display name.
pub fn discover() -> Vec<Lv2Info> {
    let h = host();
    let mut out: Vec<Lv2Info> = h
        .world
        .iter_plugins()
        .filter(|p| {
            let c = p.port_counts();
            c.audio_inputs >= 1
                && c.audio_outputs >= 1
                && c.cv_inputs == 0
                && c.cv_outputs == 0
                && !p.is_instrument()
        })
        .map(|p| Lv2Info {
            uri: p.uri(),
            name: p.name(),
        })
        .collect();
    out.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    out
}

/// Human-readable name for an LV2 URI, if installed.
pub fn name_for_uri(uri: &str) -> Option<String> {
    host().world.plugin_by_uri(uri).map(|p| p.name())
}

/// Inspect a control-input port via lilv to choose the right UI control:
/// toggle (boolean), dropdown (enumerated scale points), integer, or slider.
fn classify_port(world: &livi::World, plugin: &livi::Plugin, index: usize) -> ParamKind {
    let lworld = world.raw();
    let Some(lport) = plugin.raw().port_by_index(index) else {
        return ParamKind::Continuous;
    };

    if lport.has_property(&lworld.new_uri(LV2_TOGGLED)) {
        return ParamKind::Toggle;
    }

    if lport.has_property(&lworld.new_uri(LV2_ENUMERATION)) {
        let mut points: Vec<(String, f32)> = lport
            .scale_points()
            .into_iter()
            .filter_map(|sp| {
                let label = sp.label().as_str().map(str::to_string)?;
                // Scale-point values are often integer literals (e.g. `rdf:value 0`),
                // for which `as_float()` returns None — fall back to `as_int()`.
                let vn = sp.value();
                let value = vn.as_float().or_else(|| vn.as_int().map(|i| i as f32))?;
                Some((label, value))
            })
            .collect();
        if !points.is_empty() {
            points.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));
            return ParamKind::Enum(points);
        }
    }

    if lport.has_property(&lworld.new_uri(LV2_INTEGER)) {
        return ParamKind::Integer;
    }

    ParamKind::Continuous
}

// ── Hosted plugin ───────────────────────────────────────────────────────────────

pub struct Lv2Plugin {
    id: String,
    name: String,
    enabled: bool,

    instance: Instance,
    params: Vec<PluginParam>,
    /// port symbol → control-input port index
    control_ports: HashMap<String, PortIndex>,

    audio_in: usize,
    audio_out: usize,

    in_bufs: Vec<Vec<f32>>,
    out_bufs: Vec<Vec<f32>>,
    atom_in: Vec<LV2AtomSequence>,
    atom_out: Vec<LV2AtomSequence>,
}

impl Lv2Plugin {
    /// Instantiate the plugin identified by `uri`. `params` overrides initial
    /// control values (keyed by port symbol); missing controls keep their default.
    pub fn instantiate(uri: &str, overrides: &HashMap<String, f32>) -> Option<Self> {
        let h = host();
        let plugin = h.world.plugin_by_uri(uri)?;
        let counts = *plugin.port_counts();

        let mut params = Vec::new();
        let mut control_ports = HashMap::new();
        for port in plugin.ports() {
            if matches!(port.port_type, PortType::ControlInput) {
                let min = port.min_value.unwrap_or(0.0);
                let max = port.max_value.unwrap_or(1.0);
                let value = overrides
                    .get(&port.symbol)
                    .copied()
                    .unwrap_or(port.default_value)
                    .clamp(min.min(max), min.max(max));
                let kind = classify_port(&h.world, &plugin, port.index.0);
                params.push(PluginParam {
                    id: port.symbol.clone(),
                    label: port.name.clone(),
                    min,
                    max,
                    default: port.default_value,
                    value,
                    kind,
                });
                control_ports.insert(port.symbol.clone(), port.index);
            }
        }

        let mut instance = match unsafe { plugin.instantiate(h.features.clone(), SAMPLE_RATE as f64) }
        {
            Ok(i) => i,
            Err(e) => {
                log::warn!("LV2 instantiate '{uri}' failed: {e:?}");
                return None;
            }
        };

        // Apply initial control values.
        for p in &params {
            if let Some(idx) = control_ports.get(&p.id) {
                instance.set_control_input(*idx, p.value);
            }
        }

        let alloc = |n: usize| vec![vec![0.0f32; MAX_BLOCK]; n];
        let atoms = |n: usize| {
            (0..n)
                .map(|_| LV2AtomSequence::new(&h.features, ATOM_CAPACITY))
                .collect()
        };

        Some(Self {
            id: id_for_uri(uri),
            name: plugin.name(),
            enabled: true,
            instance,
            params,
            control_ports,
            audio_in: counts.audio_inputs,
            audio_out: counts.audio_outputs,
            in_bufs: alloc(counts.audio_inputs),
            out_bufs: alloc(counts.audio_outputs),
            atom_in: atoms(counts.atom_sequence_inputs),
            atom_out: atoms(counts.atom_sequence_outputs),
        })
    }
}

impl ResonatePlugin for Lv2Plugin {
    fn name(&self) -> &str {
        &self.name
    }

    fn id(&self) -> &str {
        &self.id
    }

    fn is_enabled(&self) -> bool {
        self.enabled
    }

    fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }

    fn process(&mut self, samples: &mut [f32], _sample_rate: u32) {
        if !self.enabled || self.audio_in == 0 || self.audio_out == 0 {
            return;
        }
        let frames = samples.len() / CHANNELS;
        let in_cnt = self.audio_in;
        let out_cnt = self.audio_out;

        let mut offset = 0;
        while offset < frames {
            let n = (frames - offset).min(MAX_BLOCK);

            // Interleaved stereo → per-channel plugin inputs.
            for f in 0..n {
                let base = (offset + f) * CHANNELS;
                let l = samples[base];
                let r = samples[base + 1];
                if in_cnt == 1 {
                    self.in_bufs[0][f] = 0.5 * (l + r);
                } else {
                    for ch in 0..in_cnt {
                        self.in_bufs[ch][f] = match ch {
                            0 => l,
                            1 => r,
                            _ => 0.5 * (l + r),
                        };
                    }
                }
            }

            let ports = EmptyPortConnections::new()
                .with_audio_inputs(self.in_bufs[..in_cnt].iter().map(|v| &v[..n]))
                .with_audio_outputs(self.out_bufs[..out_cnt].iter_mut().map(|v| &mut v[..n]))
                .with_atom_sequence_inputs(self.atom_in.iter())
                .with_atom_sequence_outputs(self.atom_out.iter_mut());

            if let Err(e) = unsafe { self.instance.run(n, ports) } {
                log::warn!("LV2 '{}' run error: {e:?}", self.id);
                return;
            }

            // Per-channel plugin outputs → interleaved stereo.
            for f in 0..n {
                let base = (offset + f) * CHANNELS;
                let (l, r) = if out_cnt == 1 {
                    (self.out_bufs[0][f], self.out_bufs[0][f])
                } else {
                    (self.out_bufs[0][f], self.out_bufs[1][f])
                };
                samples[base] = l;
                samples[base + 1] = r;
            }

            offset += n;
        }
    }

    fn params(&self) -> Vec<PluginParam> {
        self.params.clone()
    }

    fn set_param(&mut self, id: &str, value: f32) {
        if let Some(p) = self.params.iter_mut().find(|p| p.id == id) {
            let v = value.clamp(p.min.min(p.max), p.min.max(p.max));
            p.value = v;
            if let Some(idx) = self.control_ports.get(id) {
                self.instance.set_control_input(*idx, v);
            }
        }
    }
}
