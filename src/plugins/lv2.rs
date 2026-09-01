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

/// Serialises every use of the plugin world. lilv is not thread-safe: the world
/// is a mutable RDF model, `load_resource` writes to it, and instantiating reads
/// it — doing two of those at once segfaults (reproduced by running the plugin
/// tests in parallel). In the app all of this happens on the GTK thread anyway;
/// the lock makes the `unsafe impl Sync` above honest. The audio thread never
/// touches the world, only instances it already holds, so it never waits here.
fn world_lock() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    LOCK.lock().unwrap_or_else(|e| e.into_inner())
}

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
    let _guard = world_lock();
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
    let _guard = world_lock();
    host().world.plugin_by_uri(uri).map(|p| p.name())
}

// ── Factory presets ─────────────────────────────────────────────────────────────

/// LV2 class every `pset:Preset` is an instance of.
const LV2_PRESET_CLASS: &str = "http://lv2plug.in/ns/ext/presets#Preset";

/// Minimal URID map. lilv needs one to turn a preset's RDF port values into
/// typed atoms; livi keeps its own map private, and it does not matter that ours
/// hands out different numbers — the ids we compare against come from the very
/// same map, and the atoms never reach a plugin.
struct UridMap {
    uris: std::sync::Mutex<Vec<std::ffi::CString>>,
}

impl UridMap {
    fn map(&self, uri: &std::ffi::CStr) -> u32 {
        // Poisoning can only mean a panic while appending a URI; the vector is
        // still consistent, so recover rather than take down the UI thread.
        let mut uris = self.uris.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(pos) = uris.iter().position(|u| u.as_c_str() == uri) {
            return pos as u32 + 1;
        }
        uris.push(uri.to_owned());
        uris.len() as u32
    }
}

extern "C" fn urid_map_cb(
    handle: lv2_raw::LV2UridMapHandle,
    uri: *const std::os::raw::c_char,
) -> u32 {
    if handle.is_null() || uri.is_null() {
        return 0;
    }
    // Safe: `handle` is the &'static UridMap we install below, and lilv only
    // passes us NUL-terminated URI strings it owns for the duration of the call.
    let map = unsafe { &*(handle as *const UridMap) };
    map.map(unsafe { std::ffi::CStr::from_ptr(uri) })
}

/// Atom type ids (in our own map) for the value types presets actually use.
struct Urids {
    float: u32,
    double: u32,
    int: u32,
    long: u32,
    bool_: u32,
}

struct PresetMapper {
    map: &'static UridMap,
    feature: lv2_raw::LV2UridMap,
    urids: Urids,
}

// Safe: the only pointer inside is the handle to our own `'static` UridMap,
// whose interior is a Mutex; the map callback is re-entrant across threads.
unsafe impl Send for PresetMapper {}
unsafe impl Sync for PresetMapper {}

fn preset_mapper() -> &'static PresetMapper {
    static MAPPER: OnceLock<PresetMapper> = OnceLock::new();
    MAPPER.get_or_init(|| {
        let map: &'static UridMap = Box::leak(Box::new(UridMap {
            uris: std::sync::Mutex::new(Vec::new()),
        }));
        let urid = |uri: &str| {
            let c = std::ffi::CString::new(uri).unwrap_or_default();
            map.map(&c)
        };
        let urids = Urids {
            float: urid("http://lv2plug.in/ns/ext/atom#Float"),
            double: urid("http://lv2plug.in/ns/ext/atom#Double"),
            int: urid("http://lv2plug.in/ns/ext/atom#Int"),
            long: urid("http://lv2plug.in/ns/ext/atom#Long"),
            bool_: urid("http://lv2plug.in/ns/ext/atom#Bool"),
        };
        PresetMapper {
            map,
            feature: lv2_raw::LV2UridMap {
                handle: map as *const UridMap as *mut std::os::raw::c_void,
                map: urid_map_cb,
            },
            urids,
        }
    })
}

/// Collector handed to lilv as `user_data` while walking one preset.
struct PortValues<'a> {
    urids: &'a Urids,
    values: Vec<(String, f32)>,
}

/// lilv calls this once per port value stored in a preset.
unsafe extern "C" fn collect_port_value(
    symbol: *const std::os::raw::c_char,
    user_data: *mut std::os::raw::c_void,
    value: *const std::os::raw::c_void,
    size: u32,
    type_: u32,
) {
    if symbol.is_null() || user_data.is_null() || value.is_null() {
        return;
    }
    let out = unsafe { &mut *(user_data as *mut PortValues) };
    let Ok(symbol) = (unsafe { std::ffi::CStr::from_ptr(symbol) }).to_str() else {
        return;
    };
    let u = out.urids;
    // Presets store plain scalars; anything else (a file path, a vector) is not
    // something our slider UI can represent, so skip it.
    let v = if type_ == u.float && size as usize == std::mem::size_of::<f32>() {
        unsafe { *(value as *const f32) }
    } else if type_ == u.double && size as usize == std::mem::size_of::<f64>() {
        unsafe { *(value as *const f64) as f32 }
    } else if (type_ == u.int || type_ == u.bool_) && size as usize == std::mem::size_of::<i32>() {
        unsafe { *(value as *const i32) as f32 }
    } else if type_ == u.long && size as usize == std::mem::size_of::<i64>() {
        unsafe { *(value as *const i64) as f32 }
    } else {
        return;
    };
    out.values.push((symbol.to_string(), v));
}

/// Presets shipped with an installed LV2 plugin (Calf ships a set for its
/// Reverb, Flanger, Filter and Mono Compressor, and users can add their own).
/// Returns `(preset name, [(port symbol, value)])`, sorted by name.
pub fn factory_presets(uri: &str) -> Vec<(String, Vec<(String, f32)>)> {
    // Reading presets means parsing each preset file, and the effects page asks
    // every time a row is selected. Bundles do not change under a running
    // process, so read once per URI and keep it.
    static CACHE: OnceLock<std::sync::Mutex<HashMap<String, Vec<(String, Vec<(String, f32)>)>>>> =
        OnceLock::new();
    let cache = CACHE.get_or_init(|| std::sync::Mutex::new(HashMap::new()));
    if let Ok(c) = cache.lock() {
        if let Some(hit) = c.get(uri) {
            return hit.clone();
        }
    }

    let presets = read_factory_presets(uri);
    if let Ok(mut c) = cache.lock() {
        c.insert(uri.to_string(), presets.clone());
    }
    presets
}

fn read_factory_presets(uri: &str) -> Vec<(String, Vec<(String, f32)>)> {
    let _guard = world_lock();
    let h = host();
    let Some(plugin) = h.world.plugin_by_uri(uri) else {
        return Vec::new();
    };
    let lworld = h.world.raw();
    let preset_class = lworld.new_uri(LV2_PRESET_CLASS);
    let Some(related) = plugin.raw().related(Some(&preset_class)) else {
        return Vec::new();
    };
    let mapper = preset_mapper();

    let mut out: Vec<(String, Vec<(String, f32)>)> = Vec::new();
    for node in related.iter() {
        // A preset is only a URI until its bundle is pulled into the world.
        let _ = lworld.load_resource(&node);

        // Safe: the world and node outlive this block, the mapper is 'static,
        // and every state pointer we get is freed before we leave the loop.
        let state = unsafe {
            lilv_sys::lilv_state_new_from_world(
                lworld.as_ptr(),
                &mapper.feature as *const lv2_raw::LV2UridMap as *mut lv2_raw::LV2UridMap,
                node.as_ptr(),
            )
        };
        if state.is_null() {
            continue;
        }

        let label_ptr = unsafe { lilv_sys::lilv_state_get_label(state) };
        let label = if label_ptr.is_null() {
            node.as_uri().unwrap_or("Preset").to_string()
        } else {
            unsafe { std::ffi::CStr::from_ptr(label_ptr) }
                .to_string_lossy()
                .into_owned()
        };

        let mut collector = PortValues {
            urids: &mapper.urids,
            values: Vec::new(),
        };
        unsafe {
            lilv_sys::lilv_state_emit_port_values(
                state,
                Some(collect_port_value),
                &mut collector as *mut PortValues as *mut std::os::raw::c_void,
            );
            lilv_sys::lilv_state_free(state);
        }

        if !collector.values.is_empty() {
            out.push((label, collector.values));
        }
    }
    let _ = mapper.map; // keep the map alive for as long as the feature exists
    out.sort_by(|a, b| a.0.to_lowercase().cmp(&b.0.to_lowercase()));
    out
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
        let _guard = world_lock();
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

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::io::Write;

    const RNNOISE: &str = "https://github.com/werman/noise-suppression-for-voice#stereo";

    /// Prepare the plugin world for tests, exactly once per process: stage a
    /// bundle with a known preset and extend `LV2_PATH` before anything builds
    /// the world (`host()` initialises it once and keeps it forever). Every test
    /// that touches the world — here or in `presets` — must call this first, so
    /// that whichever runs first does the staging while the others wait.
    pub fn init_test_world() {
        static INIT: std::sync::Once = std::sync::Once::new();
        INIT.call_once(|| {
            stage_preset_bundle(RNNOISE, "vad_threshold", 42.0);
        });
    }

    /// Write a throwaway LV2 bundle holding one preset for `plugin_uri`, and put
    /// it on `LV2_PATH` alongside the system bundles.
    fn stage_preset_bundle(plugin_uri: &str, symbol: &str, value: f32) -> std::path::PathBuf {
        // LV2_PATH entries are directories *containing* bundles, so the bundle
        // gets its own parent rather than being listed directly.
        let root = std::env::temp_dir().join("resonate-lv2-test");
        let dir = root.join("resonate-preset-test.lv2");
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&dir).expect("create test bundle");

        let manifest = format!(
            "@prefix lv2: <http://lv2plug.in/ns/lv2core#> .\n\
             @prefix pset: <http://lv2plug.in/ns/ext/presets#> .\n\
             @prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .\n\
             <http://resonate.test/preset/one>\n\
             \ta pset:Preset ;\n\
             \tlv2:appliesTo <{plugin_uri}> ;\n\
             \trdfs:seeAlso <preset.ttl> .\n"
        );
        let preset = format!(
            "@prefix lv2: <http://lv2plug.in/ns/lv2core#> .\n\
             @prefix pset: <http://lv2plug.in/ns/ext/presets#> .\n\
             @prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .\n\
             <http://resonate.test/preset/one>\n\
             \ta pset:Preset ;\n\
             \tlv2:appliesTo <{plugin_uri}> ;\n\
             \trdfs:label \"Resonate Test Preset\" ;\n\
             \tlv2:port [ lv2:symbol \"{symbol}\" ; pset:value {value:.1} ] .\n"
        );
        for (name, body) in [("manifest.ttl", manifest), ("preset.ttl", preset)] {
            let mut f = std::fs::File::create(dir.join(name)).expect("write bundle file");
            f.write_all(body.as_bytes()).expect("write bundle file");
        }

        let existing = std::env::var("LV2_PATH").unwrap_or_else(|_| "/usr/lib64/lv2".to_string());
        // Safe: single-threaded here — this runs before the world (and any
        // other thread that could read the environment) exists.
        unsafe { std::env::set_var("LV2_PATH", format!("{existing}:{}", root.display())) };
        root
    }

    /// Exercises the lilv state FFI end to end: a staged bundle's preset must
    /// come back with its label and its port value. Skipped when the plugin it
    /// applies to isn't installed, so the suite still passes elsewhere.
    #[test]
    fn factory_presets_read_port_values() {
        init_test_world();

        if host().world.plugin_by_uri(RNNOISE).is_none() {
            eprintln!("RNNoise not installed — skipping");
            return;
        }

        let presets = factory_presets(RNNOISE);
        let staged = presets
            .iter()
            .find(|(name, _)| name == "Resonate Test Preset")
            .expect("staged preset is found");
        assert_eq!(staged.1, vec![("vad_threshold".to_string(), 42.0)]);

        // Every preset we do return must be well formed…
        for (name, values) in &presets {
            assert!(!name.is_empty());
            for (symbol, value) in values {
                assert!(!symbol.is_empty());
                assert!(value.is_finite(), "'{symbol}' = {value}");
            }
        }
        // …and an unknown URI must come back empty rather than panic.
        assert!(factory_presets("http://example.org/not-a-plugin").is_empty());
    }
}
