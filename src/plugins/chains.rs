//! Chain presets that ship with Resonate.
//!
//! `Config::effect_presets` holds the chains *you* save; these are the ones that
//! are simply there, so the Effects popover and the tray submenu have something
//! in them on a fresh install. They are built from the per-effect presets in
//! [`crate::plugins::presets`] rather than repeating their numbers, so a chain
//! called "Podcast" really is the compressor's "Podcast" preset.
//!
//! A factory chain is only offered when every effect in it is available: the
//! silly ones need Calf or Rubber Band installed, while "Old Radio" is built-ins
//! only and always works. They cannot be deleted (there is nothing to delete —
//! they are code), but saving a chain under the same name shadows one.

use std::collections::HashMap;

use crate::config::EffectEntry;
use crate::plugins::{lv2, presets, BUILTINS};

/// One effect in a factory chain.
pub struct ChainStep {
    /// Chain id, as in `EffectEntry::id`.
    pub id: &'static str,
    /// Per-effect preset to start from, if any.
    pub preset: Option<&'static str>,
    /// Parameters set after the preset (or on their own).
    pub params: &'static [(&'static str, f32)],
}

/// Shorthand for a step that is just a named preset.
const fn step(id: &'static str, preset: &'static str) -> ChainStep {
    ChainStep {
        id,
        preset: Some(preset),
        params: &[],
    }
}

/// Shorthand for a step at its default settings.
const fn plain(id: &'static str) -> ChainStep {
    ChainStep {
        id,
        preset: None,
        params: &[],
    }
}

const RNNOISE: &str = "lv2:https://github.com/werman/noise-suppression-for-voice#stereo";
const COMPRESSOR: &str = "lv2:http://lsp-plug.in/plugins/lv2/compressor_stereo";
const LIMITER: &str = "lv2:http://lsp-plug.in/plugins/lv2/limiter_stereo";
const PITCH: &str = "lv2:http://breakfastquay.com/rdf/lv2-rubberband#livestereo";
const RINGMOD: &str = "lv2:http://calf.sourceforge.net/plugins/RingModulator";
const VOCODER: &str = "lv2:http://calf.sourceforge.net/plugins/Vocoder";
const DELAY: &str = "lv2:http://calf.sourceforge.net/plugins/VintageDelay";
const REVERB: &str = "lv2:http://calf.sourceforge.net/plugins/Reverb";

const FACTORY: &[(&str, &[ChainStep])] = &[
    // ── Voices you might actually use ───────────────────────────────────────
    (
        "Podcast",
        &[
            step("gate", "Quiet room"),
            plain(RNNOISE),
            step(COMPRESSOR, "Podcast"),
            plain("gain"),
        ],
    ),
    (
        "Broadcast",
        &[
            plain(RNNOISE),
            step(COMPRESSOR, "Radio DJ"),
            plain(LIMITER),
            plain("gain"),
        ],
    ),
    (
        "Noisy Room",
        &[
            step("gate", "Noisy room"),
            plain(RNNOISE),
            step(COMPRESSOR, "Gentle voice"),
        ],
    ),
    // ── The silly end ───────────────────────────────────────────────────────
    (
        // Built-ins only, so this one works with nothing installed.
        "Old Radio",
        &[
            step("telephone", "AM radio"),
            step("bitcrush", "Subtle lo-fi"),
            step("distortion", "Warm grit"),
        ],
    ),
    ("Robot", &[step(RINGMOD, "Robot"), plain("gain")]),
    (
        "Vocoder Robot",
        &[step(VOCODER, "Robot (noise carrier)"), plain("gain")],
    ),
    (
        "Demon",
        &[
            step(PITCH, "Demon (octave down)"),
            step("distortion", "Warm grit"),
        ],
    ),
    ("Chipmunk", &[step(PITCH, "Chipmunk")]),
    (
        "Stadium Announcer",
        &[
            step(COMPRESSOR, "Radio DJ"),
            step(DELAY, "Stadium announcer"),
            ChainStep {
                id: REVERB,
                preset: None,
                params: &[("decay_time", 3.0), ("room_size", 3.0), ("amount", 0.35)],
            },
        ],
    ),
    (
        "Cathedral",
        &[ChainStep {
            id: REVERB,
            preset: None,
            params: &[
                ("decay_time", 8.0),
                ("room_size", 4.0),
                ("amount", 0.6),
                ("predelay", 40.0),
                ("hf_damp", 4000.0),
            ],
        }],
    ),
];

/// Is this effect usable on this machine? Built-ins always are; a curated LV2
/// entry only when its plugin is installed.
fn available(id: &str) -> bool {
    match lv2::uri_from_id(id) {
        Some(uri) => lv2::name_for_uri(uri).is_some(),
        None => BUILTINS.iter().any(|(bid, ..)| *bid == id),
    }
}

fn build_entry(s: &ChainStep) -> EffectEntry {
    let mut params: HashMap<String, f32> = HashMap::new();
    if let Some(want) = s.preset {
        if let Some(preset) = presets::presets_for(s.id)
            .into_iter()
            .find(|p| p.name == want)
        {
            params.extend(preset.values);
        } else {
            // Checked by `factory_chains_are_buildable`; at runtime the chain is
            // still fine, the effect just starts at its own defaults.
            log::warn!("factory chain: '{}' has no preset '{want}'", s.id);
        }
    }
    params.extend(s.params.iter().map(|(k, v)| ((*k).to_string(), *v)));
    EffectEntry {
        id: s.id.to_string(),
        enabled: true,
        params,
    }
}

/// The factory chains this machine can actually run, in catalogue order.
/// Computed once: it inspects the plugin world, and bundles do not change under
/// a running process.
pub fn factory_chains() -> &'static [(String, Vec<EffectEntry>)] {
    static CHAINS: std::sync::OnceLock<Vec<(String, Vec<EffectEntry>)>> = std::sync::OnceLock::new();
    CHAINS.get_or_init(|| {
        FACTORY
            .iter()
            .filter(|(_, steps)| steps.iter().all(|s| available(s.id)))
            .map(|(name, steps)| ((*name).to_string(), steps.iter().map(build_entry).collect()))
            .collect()
    })
}

/// The factory chain with this name, if it is offered here.
pub fn factory_chain(name: &str) -> Option<&'static [EffectEntry]> {
    factory_chains()
        .iter()
        .find(|(n, _)| n == name)
        .map(|(_, chain)| chain.as_slice())
}

/// The factory chain `chain` currently matches, if any — the same derived
/// marking the saved chains use.
pub fn matching_factory(chain: &[EffectEntry]) -> Option<String> {
    factory_chains()
        .iter()
        .find(|(_, c)| crate::config::chains_match(c, chain))
        .map(|(name, _)| name.clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every step must name an effect in the catalogue, and every named preset
    /// must exist for it. Steps whose plugin is missing here are skipped, as the
    /// chain itself would be.
    #[test]
    fn factory_chains_are_buildable() {
        lv2::tests::init_test_world();

        for (name, steps) in FACTORY {
            assert!(!steps.is_empty(), "factory chain '{name}' is empty");
            for s in *steps {
                let known = BUILTINS.iter().any(|(bid, ..)| *bid == s.id)
                    || lv2::uri_from_id(s.id).is_some_and(|uri| {
                        crate::plugins::CURATED_LV2.iter().any(|(c, ..)| *c == uri)
                    });
                assert!(known, "chain '{name}': '{}' is not a curated effect", s.id);

                if !available(s.id) {
                    eprintln!("chain '{name}': '{}' not installed — skipping", s.id);
                    continue;
                }
                if let Some(want) = s.preset {
                    let found = presets::presets_for(s.id).into_iter().any(|p| p.name == want);
                    assert!(found, "chain '{name}': '{}' has no preset '{want}'", s.id);
                }
            }
        }
    }

    /// A built chain must round-trip: applying it and asking which factory chain
    /// is active must name it again.
    #[test]
    fn built_chains_match_themselves() {
        lv2::tests::init_test_world();

        let chains = factory_chains();
        assert!(
            chains.iter().any(|(n, _)| n == "Old Radio"),
            "the built-ins-only chain must always be offered"
        );
        for (name, chain) in chains {
            assert!(!chain.is_empty());
            assert_eq!(matching_factory(chain).as_deref(), Some(name.as_str()));
        }
    }
}
