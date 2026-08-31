pub mod builtin;
pub mod host;
pub mod lv2;

use crate::config::EffectEntry;

/// Build a boxed plugin from a saved chain entry. Built-ins by id; anything with
/// the `lv2:` prefix is hosted via [`lv2::Lv2Plugin`]. Returns `None` if the
/// effect id is unknown or an LV2 plugin failed to instantiate.
pub fn plugin_from_entry(entry: &EffectEntry) -> Option<Box<dyn ResonatePlugin>> {
    let p: Box<dyn ResonatePlugin> = match entry.id.as_str() {
        "gain" => Box::new(builtin::GainPlugin::new(
            entry.params.get("gain").copied().unwrap_or(1.0),
            entry.enabled,
        )),
        "gate" => Box::new(builtin::NoiseGatePlugin::new(
            entry.params.get("threshold").copied().unwrap_or(0.02),
            entry.params.get("attack_ms").copied().unwrap_or(10.0),
            entry.params.get("release_ms").copied().unwrap_or(100.0),
            entry.enabled,
        )),
        "distortion" => Box::new(builtin::DistortionPlugin::new(
            entry.params.get("drive").copied().unwrap_or(5.0),
            entry.params.get("mix").copied().unwrap_or(1.0),
            entry.params.get("level").copied().unwrap_or(1.0),
            entry.enabled,
        )),
        "bitcrush" => Box::new(builtin::BitcrushPlugin::new(
            entry.params.get("bits").copied().unwrap_or(8.0),
            entry.params.get("downsample").copied().unwrap_or(8.0),
            entry.params.get("mix").copied().unwrap_or(1.0),
            entry.enabled,
        )),
        "telephone" => Box::new(builtin::TelephonePlugin::new(
            entry.params.get("low_cut").copied().unwrap_or(300.0),
            entry.params.get("high_cut").copied().unwrap_or(3400.0),
            entry.enabled,
        )),
        other => {
            let uri = lv2::uri_from_id(other)?;
            let mut p = lv2::Lv2Plugin::instantiate(uri, &entry.params)?;
            p.set_enabled(entry.enabled);
            return Some(Box::new(p));
        }
    };
    Some(p)
}

/// `(id, name, description)` of every built-in effect, for the add sheet and
/// chain display names.
pub const BUILTINS: &[(&str, &str, &str)] = &[
    ("gate", "Noise Gate", "Silence audio below a threshold"),
    ("gain", "Gain", "Boost or cut the microphone volume"),
    ("distortion", "Distortion", "Waveshaper drive for a gritty, overdriven voice"),
    ("bitcrush", "Bitcrusher", "Lo-fi bit depth and sample rate reduction"),
    ("telephone", "Telephone", "Band-limit the voice like an old phone or radio"),
];

/// Curated LV2 plugins (Easy Effects-style): friendly names for the same
/// plugins Easy Effects wraps, shown above the raw LV2 listing when installed.
pub const CURATED_LV2: &[(&str, &str, &str)] = &[
    (
        "https://github.com/werman/noise-suppression-for-voice#stereo",
        "Noise Suppression (RNNoise)",
        "AI voice noise removal — the engine Easy Effects uses",
    ),
    (
        "http://lsp-plug.in/plugins/lv2/autogain_stereo",
        "Auto Gain",
        "Keeps your voice at a steady loudness (LSP)",
    ),
    (
        "http://lsp-plug.in/plugins/lv2/compressor_stereo",
        "Compressor",
        "Even out your voice level (LSP)",
    ),
    (
        "http://lsp-plug.in/plugins/lv2/limiter_stereo",
        "Limiter",
        "Catch peaks before they clip (LSP)",
    ),
    (
        "http://lsp-plug.in/plugins/lv2/graph_equalizer_x16_stereo",
        "Graphic Equalizer",
        "16-band tone shaping (LSP)",
    ),
    (
        "http://lsp-plug.in/plugins/lv2/para_equalizer_x16_stereo",
        "Parametric Equalizer",
        "16 fully adjustable bands (LSP)",
    ),
];

/// Friendly display name for a chain id, if it is a built-in or curated plugin.
pub fn friendly_name(id: &str) -> Option<&'static str> {
    if let Some((_, name, _)) = BUILTINS.iter().find(|(bid, _, _)| *bid == id) {
        return Some(name);
    }
    let uri = lv2::uri_from_id(id)?;
    CURATED_LV2
        .iter()
        .find(|(curi, _, _)| *curi == uri)
        .map(|(_, name, _)| *name)
}

/// Core trait every Resonate effect plugin must implement.
pub trait ResonatePlugin: Send {
    fn name(&self) -> &str;
    fn id(&self) -> &str;
    fn is_enabled(&self) -> bool;
    fn set_enabled(&mut self, enabled: bool);
    fn process(&mut self, samples: &mut [f32], sample_rate: u32);
    fn params(&self) -> Vec<PluginParam>;
    fn set_param(&mut self, id: &str, value: f32);
}

/// How a parameter should be presented and edited in the effects UI.
#[derive(Debug, Clone)]
pub enum ParamKind {
    /// Continuous float — a slider.
    Continuous,
    /// Integer-valued — a slider with whole-number steps.
    Integer,
    /// Boolean — a switch (0.0 = off, 1.0 = on).
    Toggle,
    /// Enumerated choices — a dropdown of `(label, value)` options.
    Enum(Vec<(String, f32)>),
}

#[derive(Debug, Clone)]
pub struct PluginParam {
    pub id: String,
    pub label: String,
    pub min: f32,
    pub max: f32,
    pub default: f32,
    pub value: f32,
    pub kind: ParamKind,
}
