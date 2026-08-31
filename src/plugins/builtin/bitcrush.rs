use crate::plugins::{ParamKind, PluginParam, ResonatePlugin};

/// Bitcrusher: bit-depth quantisation + sample-and-hold rate reduction.
/// Operates on interleaved stereo frames (hold state per channel).
pub struct BitcrushPlugin {
    enabled: bool,
    bits: f32,       // 1–16 (quantisation depth)
    downsample: f32, // 1–50 (hold each sample for N frames)
    mix: f32,        // 0.0–1.0 dry/wet

    hold: [f32; 2],
    counter: u32,
}

impl BitcrushPlugin {
    pub fn new(bits: f32, downsample: f32, mix: f32, enabled: bool) -> Self {
        Self {
            enabled,
            bits: bits.clamp(1.0, 16.0),
            downsample: downsample.clamp(1.0, 50.0),
            mix: mix.clamp(0.0, 1.0),
            hold: [0.0; 2],
            counter: 0,
        }
    }
}

impl Default for BitcrushPlugin {
    fn default() -> Self {
        Self::new(8.0, 8.0, 1.0, true)
    }
}

impl ResonatePlugin for BitcrushPlugin {
    fn name(&self) -> &str { "Bitcrusher" }
    fn id(&self) -> &str { "bitcrush" }
    fn is_enabled(&self) -> bool { self.enabled }
    fn set_enabled(&mut self, enabled: bool) { self.enabled = enabled; }

    fn process(&mut self, samples: &mut [f32], _sample_rate: u32) {
        if !self.enabled { return; }
        let steps = (2.0f32).powf(self.bits - 1.0).max(1.0);
        let factor = self.downsample.round().max(1.0) as u32;

        for frame in samples.chunks_mut(2) {
            if self.counter == 0 {
                for (ch, s) in frame.iter().enumerate() {
                    self.hold[ch] = (s * steps).round() / steps;
                }
            }
            self.counter = (self.counter + 1) % factor;
            for (ch, s) in frame.iter_mut().enumerate() {
                let dry = *s;
                *s = (dry + (self.hold[ch] - dry) * self.mix).clamp(-1.0, 1.0);
            }
        }
    }

    fn params(&self) -> Vec<PluginParam> {
        vec![
            PluginParam {
                id: "bits".into(),
                label: "Bit depth".into(),
                min: 1.0,
                max: 16.0,
                default: 8.0,
                value: self.bits,
                kind: ParamKind::Integer,
            },
            PluginParam {
                id: "downsample".into(),
                label: "Downsample".into(),
                min: 1.0,
                max: 50.0,
                default: 8.0,
                value: self.downsample,
                kind: ParamKind::Integer,
            },
            PluginParam {
                id: "mix".into(),
                label: "Mix".into(),
                min: 0.0,
                max: 1.0,
                default: 1.0,
                value: self.mix,
                kind: ParamKind::Continuous,
            },
        ]
    }

    fn set_param(&mut self, id: &str, value: f32) {
        match id {
            "bits" => self.bits = value.clamp(1.0, 16.0),
            "downsample" => self.downsample = value.clamp(1.0, 50.0),
            "mix" => self.mix = value.clamp(0.0, 1.0),
            _ => {}
        }
    }
}
