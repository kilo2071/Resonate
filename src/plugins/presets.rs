//! Per-effect presets — ready-made starting points for one effect.
//!
//! Distinct from the *chain* presets in `Config::effect_presets`: those save the
//! whole mic chain, these fill in the knobs of a single effect. They exist
//! because the useful plugins are also the fiddly ones — the LSP compressor has
//! 70 ports, and "which four of those make me sound like a podcast" is not
//! something a slider wall answers.
//!
//! Two sources, concatenated:
//!
//! * the curated table below — hand-written, verified against each plugin's
//!   `.ttl` (port symbols and value ranges), and always available;
//! * presets shipped by the plugin itself, read through lilv
//!   ([`lv2::factory_presets`]). LSP ships none; Calf ships a set for its
//!   Reverb, Flanger, Filter and Mono Compressor, and anything the user saves
//!   from another host shows up here too.
//!
//! A preset only names the parameters it cares about; everything else keeps its
//! current value, so switching between two presets that touch different knobs
//! does not silently reset the rest.

use crate::plugins::lv2;

/// One ready-made setting for a single effect.
#[derive(Clone, Debug)]
pub struct EffectPreset {
    pub name: String,
    /// `(param id / port symbol, value)` — a subset of the effect's parameters.
    pub values: Vec<(String, f32)>,
}

type Values = &'static [(&'static str, f32)];

/// Hand-written presets, keyed by chain id (`EffectEntry::id`).
///
/// LV2 gain-style ports take *linear* values, so the dB figures in the comments
/// are pre-converted: -18 dB = 0.126, -24 dB = 0.063, -30 dB = 0.032,
/// +3 dB = 1.41, +6 dB = 2.00, +9 dB = 2.82, +12 dB = 3.98.
const CURATED: &[(&str, &[(&str, Values)])] = &[
    (
        "gate",
        &[
            ("Quiet room", &[("threshold", 0.01), ("attack_ms", 5.0), ("release_ms", 120.0)]),
            ("Noisy room", &[("threshold", 0.05), ("attack_ms", 5.0), ("release_ms", 150.0)]),
            ("Keyboard basher", &[("threshold", 0.10), ("attack_ms", 2.0), ("release_ms", 80.0)]),
        ],
    ),
    (
        "distortion",
        &[
            ("Warm grit", &[("drive", 2.0), ("mix", 0.6), ("level", 1.0)]),
            ("Overdriven", &[("drive", 5.0), ("mix", 1.0), ("level", 0.9)]),
            ("Demon", &[("drive", 12.0), ("mix", 1.0), ("level", 0.7)]),
        ],
    ),
    (
        "bitcrush",
        &[
            ("Subtle lo-fi", &[("bits", 10.0), ("downsample", 2.0), ("mix", 0.7)]),
            ("Retro game", &[("bits", 6.0), ("downsample", 6.0), ("mix", 1.0)]),
            ("Broken robot", &[("bits", 3.0), ("downsample", 16.0), ("mix", 1.0)]),
        ],
    ),
    (
        "telephone",
        &[
            ("Old phone", &[("low_cut", 300.0), ("high_cut", 3400.0)]),
            ("Walkie-talkie", &[("low_cut", 500.0), ("high_cut", 2500.0)]),
            ("AM radio", &[("low_cut", 200.0), ("high_cut", 4500.0)]),
        ],
    ),
    (
        // LSP ships no LV2 presets at all, and this is the plugin people ask
        // about: al = attack threshold (linear), cr = ratio, mk = makeup.
        "lv2:http://lsp-plug.in/plugins/lv2/compressor_stereo",
        &[
            (
                "Gentle voice",
                &[
                    ("al", 0.126),
                    ("at", 15.0),
                    ("rt", 150.0),
                    ("cr", 2.5),
                    ("kn", 0.501),
                    ("mk", 1.41),
                ],
            ),
            (
                "Podcast",
                &[
                    ("al", 0.063),
                    ("at", 10.0),
                    ("rt", 120.0),
                    ("cr", 4.0),
                    ("kn", 0.501),
                    ("mk", 2.0),
                ],
            ),
            (
                "Radio DJ",
                &[
                    ("al", 0.032),
                    ("at", 5.0),
                    ("rt", 80.0),
                    ("cr", 8.0),
                    ("kn", 0.355),
                    ("mk", 2.82),
                ],
            ),
            (
                "Squashed (streaming)",
                &[
                    ("al", 0.032),
                    ("at", 1.0),
                    ("rt", 60.0),
                    ("cr", 20.0),
                    ("kn", 0.251),
                    ("mk", 3.98),
                ],
            ),
        ],
    ),
    (
        // Rubber Band: integer semitone/octave shifts. `formant` on keeps the
        // vocal tract size, which is what stops a shift sounding like a cartoon.
        "lv2:http://breakfastquay.com/rdf/lv2-rubberband#livestereo",
        &[
            ("Slightly higher", &[("semitones", 2.0), ("octaves", 0.0), ("cents", 0.0), ("formant", 1.0)]),
            ("Slightly deeper", &[("semitones", -2.0), ("octaves", 0.0), ("cents", 0.0), ("formant", 1.0)]),
            ("Chipmunk", &[("semitones", 7.0), ("octaves", 0.0), ("cents", 0.0), ("formant", 0.0)]),
            ("Squeaky (octave up)", &[("semitones", 0.0), ("octaves", 1.0), ("cents", 0.0), ("formant", 0.0)]),
            ("Demon (octave down)", &[("semitones", 0.0), ("octaves", -1.0), ("cents", 0.0), ("formant", 0.0)]),
            ("Anonymous witness", &[("semitones", -5.0), ("octaves", 0.0), ("cents", 0.0), ("formant", 0.0)]),
        ],
    ),
    (
        // Calf Ring Modulator: mod_mode 0 = sine carrier.
        "lv2:http://calf.sourceforge.net/plugins/RingModulator",
        &[
            ("Robot", &[("mod_mode", 0.0), ("mod_freq", 60.0), ("mod_amount", 1.0)]),
            ("Dalek", &[("mod_mode", 0.0), ("mod_freq", 30.0), ("mod_amount", 1.0)]),
            ("Alien", &[("mod_mode", 0.0), ("mod_freq", 440.0), ("mod_amount", 0.8)]),
            ("Wobble", &[("mod_mode", 0.0), ("mod_freq", 8.0), ("mod_amount", 0.6)]),
        ],
    ),
    (
        // Calf Vintage Delay: timing 1 = milliseconds (0 syncs to a host tempo
        // we do not provide), amount = wet level, feedback = repeats.
        "lv2:http://calf.sourceforge.net/plugins/VintageDelay",
        &[
            ("Slapback", &[("timing", 1.0), ("ms", 90.0), ("feedback", 0.15), ("amount", 0.35)]),
            ("Stadium announcer", &[("timing", 1.0), ("ms", 400.0), ("feedback", 0.45), ("amount", 0.5)]),
            ("Canyon", &[("timing", 1.0), ("ms", 700.0), ("feedback", 0.7), ("amount", 0.6)]),
        ],
    ),
    (
        "lv2:http://calf.sourceforge.net/plugins/Crusher",
        &[
            ("Gentle crunch", &[("bits", 10.0), ("samples", 2.0), ("morph", 0.5)]),
            ("Arcade", &[("bits", 5.0), ("samples", 8.0), ("morph", 0.5)]),
            ("Destroyed", &[("bits", 2.0), ("samples", 40.0), ("morph", 0.0)]),
        ],
    ),
];

/// Calf's vocoder id — it needs presets built rather than tabulated.
pub const CALF_VOCODER: &str = "lv2:http://calf.sourceforge.net/plugins/Vocoder";

/// The vocoder has no oscillator of its own: it filters a *carrier* with the
/// envelopes of a *modulator*, and Resonate feeds the microphone into both, so
/// out of the box it just hands your voice back slightly band-shaped. What
/// rescues it is `noise1`–`noise32`: a per-band noise generator, defaulted to
/// -96 dB (off). Bring those up and the noise becomes the carrier — the classic
/// whispered-robot vocoder, from one voice.
///
/// Built rather than written out because a table of 32 identical lines per
/// preset would be unreadable.
fn vocoder_presets() -> Vec<EffectPreset> {
    let with_noise = |name: &str, noise: f32, extra: &[(&str, f32)]| {
        let mut values: Vec<(String, f32)> = (1..=32)
            .map(|band| (format!("noise{band}"), noise))
            .collect();
        // Only the vocoded signal, no dry voice or raw carrier bleeding through.
        values.push(("processed".to_string(), 1.0));
        values.push(("modulator".to_string(), 0.0));
        values.push(("carrier".to_string(), 0.0));
        values.extend(extra.iter().map(|(k, v)| ((*k).to_string(), *v)));
        EffectPreset {
            name: name.to_string(),
            values,
        }
    };
    vec![
        // Snappy envelopes keep consonants; high isolation keeps it robotic.
        with_noise("Robot (noise carrier)", 1.0, &[("attack", 5.0), ("release", 50.0), ("order", 6.0)]),
        // Slower and softer: breathy rather than mechanical.
        with_noise("Whisper", 0.5, &[("attack", 20.0), ("release", 200.0), ("order", 4.0)]),
        // A little dry voice mixed back under the robot.
        with_noise("Robot + voice", 1.0, &[("attack", 5.0), ("release", 50.0), ("modulator", 0.4)]),
    ]
}

/// The presets Resonate itself provides for an effect: the table above, plus any
/// that have to be generated.
pub(crate) fn curated_presets(id: &str) -> Vec<EffectPreset> {
    if id == CALF_VOCODER {
        return vocoder_presets();
    }
    CURATED
        .iter()
        .find(|(key, _)| *key == id)
        .map(|(_, presets)| {
            presets
                .iter()
                .map(|(name, values)| EffectPreset {
                    name: (*name).to_string(),
                    values: values.iter().map(|(k, v)| ((*k).to_string(), *v)).collect(),
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Every preset available for a chain entry: the ones Resonate provides first,
/// then any the plugin ships itself.
pub fn presets_for(id: &str) -> Vec<EffectPreset> {
    let mut out = curated_presets(id);

    if let Some(uri) = lv2::uri_from_id(id) {
        for (name, values) in lv2::factory_presets(uri) {
            // A curated preset wins over a factory one of the same name.
            if out.iter().any(|p| p.name.eq_ignore_ascii_case(&name)) {
                continue;
            }
            out.push(EffectPreset { name, values });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every curated preset must name parameters the effect actually has, with
    /// values inside their range — a typo'd port symbol or an out-of-range value
    /// would silently do nothing at runtime. LV2 effects whose plugin is not
    /// installed here are skipped, so the suite still passes on a bare machine.
    #[test]
    fn curated_presets_name_real_params() {
        crate::plugins::lv2::tests::init_test_world();

        // Everything Resonate offers, table-driven or generated, for every
        // effect in the catalogue — a typo'd symbol or an out-of-range value
        // would silently do nothing at runtime.
        let ids: Vec<String> = crate::plugins::BUILTINS
            .iter()
            .map(|(id, ..)| (*id).to_string())
            .chain(
                crate::plugins::CURATED_LV2
                    .iter()
                    .map(|(uri, ..)| lv2::id_for_uri(uri)),
            )
            .collect();

        let mut checked = 0;
        for id in &ids {
            let presets = curated_presets(id);
            if presets.is_empty() {
                continue;
            }
            let entry = crate::config::EffectEntry {
                id: id.clone(),
                enabled: true,
                params: std::collections::HashMap::new(),
            };
            let Some(plugin) = crate::plugins::plugin_from_entry(&entry) else {
                eprintln!("'{id}' not installed — skipping");
                continue;
            };
            let params = plugin.params();
            for preset in &presets {
                for (param, value) in &preset.values {
                    let p = params
                        .iter()
                        .find(|p| p.id == *param)
                        .unwrap_or_else(|| {
                            panic!("'{id}' preset '{}': no param '{param}'", preset.name)
                        });
                    let (lo, hi) = (p.min.min(p.max), p.min.max(p.max));
                    assert!(
                        *value >= lo && *value <= hi,
                        "'{id}' preset '{}': {param} = {value} outside {lo}..={hi}",
                        preset.name
                    );
                    checked += 1;
                }
            }
        }
        assert!(checked > 0, "no curated preset could be checked at all");
        eprintln!("checked {checked} curated preset values");
    }

    /// The table keys must stay in step with the catalogue: an effect removed
    /// from `CURATED_LV2` leaves presets nothing to attach to.
    #[test]
    fn curated_keys_are_known_effects() {
        for (id, presets) in CURATED {
            let known = crate::plugins::BUILTINS.iter().any(|(bid, ..)| bid == id)
                || lv2::uri_from_id(id).is_some_and(|uri| {
                    crate::plugins::CURATED_LV2.iter().any(|(curi, ..)| *curi == uri)
                });
            assert!(known, "curated presets for unknown effect '{id}'");
            assert!(!presets.is_empty(), "'{id}' has an empty preset list");
            for (name, values) in *presets {
                assert!(!values.is_empty(), "'{id}' preset '{name}' sets nothing");
            }
        }
    }
}
