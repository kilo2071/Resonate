//! Which parameters of an effect are worth showing first.
//!
//! Generic slider walls are fine for a five-knob plugin and hopeless for a
//! serious one: the LSP parametric equaliser exposes 181 control ports, the Calf
//! vocoder 210, the LSP compressor 48 — mostly per-band arrays and controls that
//! only exist to drive the plugin's own graph widget, which Resonate does not
//! draw. Listing them all buries the four knobs anyone actually turns.
//!
//! So each unwieldy effect gets a hand-written list of primary parameters, in
//! the order they should appear; the effects page shows those and folds the rest
//! into an "All parameters" expander. Nothing is hidden — an effect with no entry
//! here (or one whose parameters are all primary) is still shown flat.
//!
//! Symbols are taken from each plugin's `.ttl` and checked by
//! `tests::primary_params_exist`, so a typo cannot silently drop a control.

/// `(effect id, primary parameter ids in display order)`.
const PRIMARY: &[(&str, &[&str])] = &[
    (
        // LSP compressor: al = attack threshold, cr = ratio, kn = knee,
        // mk = makeup. The other 40 ports are sidechain, filters and the graph.
        "lv2:http://lsp-plug.in/plugins/lv2/compressor_stereo",
        &["al", "cr", "at", "rt", "kn", "mk", "g_in", "g_out"],
    ),
    (
        "lv2:http://lsp-plug.in/plugins/lv2/limiter_stereo",
        &["th", "at", "rt", "lk", "mode", "boost", "g_in", "g_out"],
    ),
    (
        "lv2:http://lsp-plug.in/plugins/lv2/autogain_stereo",
        &["level", "preamp", "max_amp", "silence", "lkahead", "weight"],
    ),
    (
        // Four fully adjustable bands up front (type/frequency/gain/Q); the other
        // twelve are one expander away.
        "lv2:http://lsp-plug.in/plugins/lv2/para_equalizer_x16_stereo",
        &[
            "g_in", "g_out", "ft_0", "f_0", "g_0", "q_0", "ft_1", "f_1", "g_1", "q_1", "ft_2",
            "f_2", "g_2", "q_2", "ft_3", "f_3", "g_3", "q_3",
        ],
    ),
    (
        // The 32 per-band volume/pan/noise/solo/Q sets are the whole tail here.
        "lv2:http://calf.sourceforge.net/plugins/Vocoder",
        &[
            "bands", "order", "attack", "release", "hiq", "carrier", "modulator", "processed",
            "lower", "upper", "tilt",
        ],
    ),
    (
        "lv2:http://calf.sourceforge.net/plugins/RingModulator",
        &[
            "mod_mode",
            "mod_freq",
            "mod_amount",
            "mod_phase",
            "mod_detune",
            "level_in",
            "level_out",
        ],
    ),
    (
        // m00–m11 switch individual notes of the scale on and off.
        "lv2:http://gareus.org/oss/lv2/fat1",
        &["mode", "tuning", "bias", "filter", "corr", "offset", "bendrange", "fastmode"],
    ),
    (
        // timing picks between host tempo (which we never supply) and ms/hz.
        "lv2:http://calf.sourceforge.net/plugins/VintageDelay",
        &["timing", "ms", "feedback", "amount", "dry", "mix_mode", "medium", "width", "level_out"],
    ),
    (
        "lv2:http://calf.sourceforge.net/plugins/ReverseDelay",
        &["window", "time_l", "time_r", "feedback", "amount", "width", "level_out"],
    ),
    (
        "lv2:http://calf.sourceforge.net/plugins/MultiChorus",
        &["voices", "mod_depth", "mod_rate", "min_delay", "amount", "dry", "stereo"],
    ),
    (
        "lv2:http://calf.sourceforge.net/plugins/Pulsator",
        &["mode", "amount", "ms", "pulsewidth", "mono", "offset_l", "offset_r"],
    ),
    (
        "lv2:http://calf.sourceforge.net/plugins/Saturator",
        &["drive", "blend", "mix", "level_in", "level_out"],
    ),
    (
        "lv2:http://calf.sourceforge.net/plugins/RotarySpeaker",
        &["vib_speed", "treble_speed", "bass_speed", "mod_depth", "mic_distance", "am_depth"],
    ),
];

/// The parameters to show above the fold for `id`, or `None` when the effect is
/// small enough to show whole.
pub fn primary_params(id: &str) -> Option<&'static [&'static str]> {
    PRIMARY
        .iter()
        .find(|(key, _)| *key == id)
        .map(|(_, params)| *params)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::EffectEntry;
    use crate::plugins::{lv2, plugin_from_entry};

    /// A primary list that names a port the plugin does not have would quietly
    /// hide that control instead of promoting it. Effects whose plugin is not
    /// installed here are skipped.
    #[test]
    fn primary_params_exist() {
        lv2::tests::init_test_world();

        let mut checked = 0;
        for (id, primary) in PRIMARY {
            let entry = EffectEntry {
                id: (*id).to_string(),
                enabled: true,
                params: std::collections::HashMap::new(),
            };
            let Some(plugin) = plugin_from_entry(&entry) else {
                eprintln!("'{id}' not installed — skipping");
                continue;
            };
            let params = plugin.params();
            for want in *primary {
                assert!(
                    params.iter().any(|p| p.id == *want),
                    "'{id}' has no parameter '{want}'"
                );
            }
            assert!(
                params.len() > primary.len(),
                "'{id}' lists every parameter as primary — drop the entry instead"
            );
            checked += 1;
        }
        eprintln!("checked {checked} primary parameter lists");
    }
}
