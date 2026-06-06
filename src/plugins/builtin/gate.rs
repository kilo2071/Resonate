use crate::plugins::{PluginParam, ResonatePlugin};

/// Simple RMS noise gate. Below threshold the gate closes; above (with hysteresis) it opens.
pub struct NoiseGatePlugin {
    enabled: bool,
    threshold: f32,     // linear RMS threshold (0.0 – 1.0)
    attack_ms: f32,     // gate open time in ms
    release_ms: f32,    // gate close time in ms

    gate_open: bool,
    attack_counter: usize,
    release_counter: usize,
}

impl NoiseGatePlugin {
    pub fn new(threshold: f32, attack_ms: f32, release_ms: f32, enabled: bool) -> Self {
        Self {
            enabled,
            threshold: threshold.clamp(0.0, 1.0),
            attack_ms,
            release_ms,
            gate_open: false,
            attack_counter: 0,
            release_counter: 0,
        }
    }

    fn rms(samples: &[f32]) -> f32 {
        if samples.is_empty() { return 0.0; }
        let sum_sq: f32 = samples.iter().map(|s| s * s).sum();
        (sum_sq / samples.len() as f32).sqrt()
    }
}

impl Default for NoiseGatePlugin {
    fn default() -> Self {
        Self::new(0.02, 10.0, 100.0, false)
    }
}

impl ResonatePlugin for NoiseGatePlugin {
    fn name(&self) -> &str { "Noise Gate" }
    fn id(&self) -> &str { "gate" }
    fn is_enabled(&self) -> bool { self.enabled }
    fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
        if !enabled {
            self.gate_open = true; // open gate when disabled so audio passes
        }
    }

    fn process(&mut self, samples: &mut [f32], sample_rate: u32) {
        if !self.enabled { return; }

        let attack_samples = ((self.attack_ms / 1000.0) * sample_rate as f32) as usize;
        let release_samples = ((self.release_ms / 1000.0) * sample_rate as f32) as usize;

        let level = Self::rms(samples);

        if level >= self.threshold {
            self.release_counter = 0;
            if !self.gate_open {
                self.attack_counter += samples.len();
                if self.attack_counter >= attack_samples.max(1) {
                    self.gate_open = true;
                    self.attack_counter = 0;
                }
            }
        } else {
            self.attack_counter = 0;
            if self.gate_open {
                self.release_counter += samples.len();
                if self.release_counter >= release_samples.max(1) {
                    self.gate_open = false;
                    self.release_counter = 0;
                }
            }
        }

        if !self.gate_open {
            for s in samples.iter_mut() {
                *s = 0.0;
            }
        }
    }

    fn params(&self) -> Vec<PluginParam> {
        vec![
            PluginParam {
                id: "threshold".into(),
                label: "Threshold".into(),
                min: 0.0,
                max: 1.0,
                default: 0.02,
                value: self.threshold,
            },
            PluginParam {
                id: "attack_ms".into(),
                label: "Attack (ms)".into(),
                min: 0.0,
                max: 100.0,
                default: 10.0,
                value: self.attack_ms,
            },
            PluginParam {
                id: "release_ms".into(),
                label: "Release (ms)".into(),
                min: 10.0,
                max: 1000.0,
                default: 100.0,
                value: self.release_ms,
            },
        ]
    }

    fn set_param(&mut self, id: &str, value: f32) {
        match id {
            "threshold" => self.threshold = value.clamp(0.0, 1.0),
            "attack_ms" => self.attack_ms = value.clamp(0.0, 100.0),
            "release_ms" => self.release_ms = value.clamp(10.0, 1000.0),
            _ => {}
        }
    }
}
