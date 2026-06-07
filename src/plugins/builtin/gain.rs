use crate::plugins::{ParamKind, PluginParam, ResonatePlugin};

pub struct GainPlugin {
    enabled: bool,
    gain: f32,
}

impl GainPlugin {
    pub fn new(gain: f32, enabled: bool) -> Self {
        Self { enabled, gain: gain.clamp(0.0, 4.0) }
    }
}

impl Default for GainPlugin {
    fn default() -> Self {
        Self::new(1.0, true)
    }
}

impl ResonatePlugin for GainPlugin {
    fn name(&self) -> &str { "Gain" }
    fn id(&self) -> &str { "gain" }
    fn is_enabled(&self) -> bool { self.enabled }
    fn set_enabled(&mut self, enabled: bool) { self.enabled = enabled; }

    fn process(&mut self, samples: &mut [f32], _sample_rate: u32) {
        if !self.enabled || (self.gain - 1.0).abs() < 0.001 { return; }
        for s in samples.iter_mut() {
            *s = (*s * self.gain).clamp(-1.0, 1.0);
        }
    }

    fn params(&self) -> Vec<PluginParam> {
        vec![PluginParam {
            id: "gain".into(),
            label: "Gain".into(),
            min: 0.0,
            max: 4.0,
            default: 1.0,
            value: self.gain,
            kind: ParamKind::Continuous,
        }]
    }

    fn set_param(&mut self, id: &str, value: f32) {
        if id == "gain" {
            self.gain = value.clamp(0.0, 4.0);
        }
    }
}
