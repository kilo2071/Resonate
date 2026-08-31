use crate::plugins::{ParamKind, PluginParam, ResonatePlugin};

/// Waveshaper distortion: `tanh(drive · x)` normalised so peaks stay near 1.0,
/// with dry/wet mix and an output level.
pub struct DistortionPlugin {
    enabled: bool,
    drive: f32, // 1.0–50.0
    mix: f32,   // 0.0–1.0 dry/wet
    level: f32, // 0.0–2.0 output gain
}

impl DistortionPlugin {
    pub fn new(drive: f32, mix: f32, level: f32, enabled: bool) -> Self {
        Self {
            enabled,
            drive: drive.clamp(1.0, 50.0),
            mix: mix.clamp(0.0, 1.0),
            level: level.clamp(0.0, 2.0),
        }
    }
}

impl Default for DistortionPlugin {
    fn default() -> Self {
        Self::new(5.0, 1.0, 1.0, true)
    }
}

impl ResonatePlugin for DistortionPlugin {
    fn name(&self) -> &str { "Distortion" }
    fn id(&self) -> &str { "distortion" }
    fn is_enabled(&self) -> bool { self.enabled }
    fn set_enabled(&mut self, enabled: bool) { self.enabled = enabled; }

    fn process(&mut self, samples: &mut [f32], _sample_rate: u32) {
        if !self.enabled { return; }
        let norm = self.drive.tanh().max(1e-6);
        for s in samples.iter_mut() {
            let dry = *s;
            let wet = (dry * self.drive).tanh() / norm;
            *s = ((dry + (wet - dry) * self.mix) * self.level).clamp(-1.0, 1.0);
        }
    }

    fn params(&self) -> Vec<PluginParam> {
        vec![
            PluginParam {
                id: "drive".into(),
                label: "Drive".into(),
                min: 1.0,
                max: 50.0,
                default: 5.0,
                value: self.drive,
                kind: ParamKind::Continuous,
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
            PluginParam {
                id: "level".into(),
                label: "Output level".into(),
                min: 0.0,
                max: 2.0,
                default: 1.0,
                value: self.level,
                kind: ParamKind::Continuous,
            },
        ]
    }

    fn set_param(&mut self, id: &str, value: f32) {
        match id {
            "drive" => self.drive = value.clamp(1.0, 50.0),
            "mix" => self.mix = value.clamp(0.0, 1.0),
            "level" => self.level = value.clamp(0.0, 2.0),
            _ => {}
        }
    }
}
