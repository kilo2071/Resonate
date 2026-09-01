pub mod builtin;
pub mod host;
pub mod layout;
pub mod lv2;
pub mod presets;

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

/// Where an effect sits in the add sheet: workhorses that make a voice sound
/// *better*, and character effects that make it sound *different*.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Category {
    Voice,
    Fun,
}

impl Category {
    pub fn label(self) -> &'static str {
        match self {
            Category::Voice => "Voice & Cleanup",
            Category::Fun => "Character & Fun",
        }
    }
}

use Category::{Fun, Voice};

/// `(id, name, description, category)` of every built-in effect, for the add
/// sheet and chain display names.
pub const BUILTINS: &[(&str, &str, &str, Category)] = &[
    ("gate", "Noise Gate", "Silence audio below a threshold", Voice),
    ("gain", "Gain", "Boost or cut the microphone volume", Voice),
    ("distortion", "Distortion", "Waveshaper drive for a gritty, overdriven voice", Fun),
    ("bitcrush", "Bitcrusher", "Lo-fi bit depth and sample rate reduction", Fun),
    ("telephone", "Telephone", "Band-limit the voice like an old phone or radio", Fun),
];

/// Curated LV2 plugins: friendly names for plugins worth putting on a mic, so
/// the picker reads like an effects menu instead of a 200-entry lilv dump.
/// Entries whose plugin is not installed are simply not listed (see
/// `effects_page::populate_available_effects`), so shipping more here than the
/// user has installed is harmless. The packages behind them are `Recommends:`
/// in the RPM spec: lsp-plugins-lv2, noise-suppression-for-voice,
/// lv2-calf-plugins, lv2-rubberband-plugins, lv2-x42-plugins.
pub const CURATED_LV2: &[(&str, &str, &str, Category)] = &[
    // ── Voice & cleanup ─────────────────────────────────────────────────────
    (
        "https://github.com/werman/noise-suppression-for-voice#stereo",
        "Noise Suppression (RNNoise)",
        "AI voice noise removal — the engine Easy Effects uses",
        Voice,
    ),
    (
        "http://lsp-plug.in/plugins/lv2/autogain_stereo",
        "Auto Gain",
        "Keeps your voice at a steady loudness (LSP)",
        Voice,
    ),
    (
        "http://lsp-plug.in/plugins/lv2/compressor_stereo",
        "Compressor",
        "Even out your voice level (LSP)",
        Voice,
    ),
    (
        "http://calf.sourceforge.net/plugins/Compressor",
        "Compressor (Calf)",
        "Simpler compressor with ready-made vocal presets",
        Voice,
    ),
    (
        "http://calf.sourceforge.net/plugins/Deesser",
        "De-esser",
        "Tames harsh S sounds (Calf)",
        Voice,
    ),
    (
        "http://lsp-plug.in/plugins/lv2/limiter_stereo",
        "Limiter",
        "Catch peaks before they clip (LSP)",
        Voice,
    ),
    (
        "http://lsp-plug.in/plugins/lv2/graph_equalizer_x16_stereo",
        "Graphic Equalizer",
        "16-band tone shaping (LSP)",
        Voice,
    ),
    (
        "http://lsp-plug.in/plugins/lv2/para_equalizer_x16_stereo",
        "Parametric Equalizer",
        "16 fully adjustable bands (LSP)",
        Voice,
    ),
    (
        "http://calf.sourceforge.net/plugins/Exciter",
        "Exciter",
        "Adds air and presence to a dull mic (Calf)",
        Voice,
    ),
    (
        "http://calf.sourceforge.net/plugins/BassEnhancer",
        "Bass Enhancer",
        "Adds weight to a thin voice (Calf)",
        Voice,
    ),
    // ── Character & fun ─────────────────────────────────────────────────────
    (
        "http://breakfastquay.com/rdf/lv2-rubberband#livestereo",
        "Pitch Shifter",
        "Chipmunk or demon voice — low-latency Rubber Band",
        Fun,
    ),
    (
        "http://gareus.org/oss/lv2/fat1",
        "Auto-Tune",
        "Snaps your pitch to a scale — hard-tune singing (x42)",
        Fun,
    ),
    (
        "http://calf.sourceforge.net/plugins/RingModulator",
        "Ring Modulator",
        "Metallic robot / Dalek voice (Calf)",
        Fun,
    ),
    (
        "http://calf.sourceforge.net/plugins/Vocoder",
        "Vocoder",
        "Classic talk-box robot; best with a carrier signal (Calf)",
        Fun,
    ),
    (
        "http://calf.sourceforge.net/plugins/Reverb",
        "Reverb",
        "Room, hall or cathedral space (Calf)",
        Fun,
    ),
    (
        "http://calf.sourceforge.net/plugins/VintageDelay",
        "Vintage Delay",
        "Echo with tape character — stadium announcer (Calf)",
        Fun,
    ),
    (
        "http://calf.sourceforge.net/plugins/ReverseDelay",
        "Reverse Delay",
        "Echoes that play backwards (Calf)",
        Fun,
    ),
    (
        "http://calf.sourceforge.net/plugins/RotarySpeaker",
        "Rotary Speaker",
        "Swirling Leslie cabinet (Calf)",
        Fun,
    ),
    (
        "http://calf.sourceforge.net/plugins/MultiChorus",
        "Multi Chorus",
        "Many detuned copies — a crowd of you (Calf)",
        Fun,
    ),
    (
        "http://calf.sourceforge.net/plugins/Flanger",
        "Flanger",
        "Metallic comb-filtered sweep (Calf)",
        Fun,
    ),
    (
        "http://calf.sourceforge.net/plugins/Phaser",
        "Phaser",
        "Jet-plane whoosh (Calf)",
        Fun,
    ),
    (
        "http://calf.sourceforge.net/plugins/Pulsator",
        "Pulsator",
        "Rhythmic tremolo / chopper (Calf)",
        Fun,
    ),
    (
        "http://calf.sourceforge.net/plugins/Saturator",
        "Saturator",
        "Tube-style warmth and grit (Calf)",
        Fun,
    ),
    (
        "http://calf.sourceforge.net/plugins/Crusher",
        "Crusher",
        "Bit and sample-rate destruction (Calf)",
        Fun,
    ),
    (
        "http://calf.sourceforge.net/plugins/TapeSimulator",
        "Tape Simulator",
        "Warm, slightly wobbly tape (Calf)",
        Fun,
    ),
    // Calf's Vinyl is deliberately absent: it segfaults in the plugin's own
    // cleanup when the instance is dropped (reproduced by
    // `curated_lv2_entries_load_and_process`), which would take the whole
    // process — mic effects and all — down when the effect is removed.
];

/// Friendly display name for a chain id, if it is a built-in or curated plugin.
pub fn friendly_name(id: &str) -> Option<&'static str> {
    if let Some((_, name, _, _)) = BUILTINS.iter().find(|(bid, _, _, _)| *bid == id) {
        return Some(name);
    }
    let uri = lv2::uri_from_id(id)?;
    CURATED_LV2
        .iter()
        .find(|(curi, _, _, _)| *curi == uri)
        .map(|(_, name, _, _)| *name)
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::EffectEntry;

    /// Every curated entry must name a real plugin that instantiates and passes
    /// audio — a typo in one of these URIs would just make the entry vanish from
    /// the add sheet, with nothing to notice. Entries whose plugin is not
    /// installed are skipped (that is exactly how the picker treats them), so
    /// this checks as much as the machine can offer.
    #[test]
    #[ignore = "reporting aid, run with --ignored"]
    fn report_param_counts() {
        lv2::tests::init_test_world();
        for (id, name, ..) in BUILTINS {
            let e = EffectEntry { id: (*id).to_string(), enabled: true, params: Default::default() };
            if let Some(p) = plugin_from_entry(&e) {
                eprintln!("{:>4}  {name}", p.params().len());
            }
        }
        for (uri, name, ..) in CURATED_LV2 {
            let e = EffectEntry { id: lv2::id_for_uri(uri), enabled: true, params: Default::default() };
            if let Some(p) = plugin_from_entry(&e) {
                let names: Vec<String> = p.params().iter().map(|x| x.id.clone()).collect();
                eprintln!("{:>4}  {name}  [{}]", names.len(), names.join(" "));
            } else {
                eprintln!("   -  {name} (not installed)");
            }
        }
    }

    #[test]
    fn curated_lv2_entries_load_and_process() {
        lv2::tests::init_test_world();

        let mut checked = 0;
        for (uri, name, ..) in CURATED_LV2 {
            if lv2::name_for_uri(uri).is_none() {
                eprintln!("{name} not installed — skipping");
                continue;
            }
            let entry = EffectEntry {
                id: lv2::id_for_uri(uri),
                enabled: true,
                params: std::collections::HashMap::new(),
            };
            let mut plugin =
                plugin_from_entry(&entry).unwrap_or_else(|| panic!("{name} failed to instantiate"));

            // A second of quiet noise through the plugin: no panic, no NaNs.
            let mut samples: Vec<f32> = (0..1024)
                .map(|i| ((i as f32) * 0.01).sin() * 0.1)
                .collect();
            plugin.process(&mut samples, crate::audio::virtual_device::SAMPLE_RATE);
            assert!(
                samples.iter().all(|s| s.is_finite()),
                "{name} produced non-finite output"
            );
            // Dropping matters as much as running: Calf's Vinyl segfaults here,
            // which is why it is not in the list above.
            drop(plugin);
            checked += 1;
        }
        eprintln!("checked {checked} curated LV2 plugins");
    }
}
