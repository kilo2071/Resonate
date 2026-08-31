use crate::plugins::{ParamKind, PluginParam, ResonatePlugin};

/// One RBJ-cookbook biquad section with per-channel state (interleaved stereo).
#[derive(Default, Clone, Copy)]
struct Biquad {
    b0: f32,
    b1: f32,
    b2: f32,
    a1: f32,
    a2: f32,
    // Direct form 1 state per channel: x1, x2, y1, y2
    x1: [f32; 2],
    x2: [f32; 2],
    y1: [f32; 2],
    y2: [f32; 2],
}

impl Biquad {
    fn lowpass(freq: f32, sample_rate: f32) -> Self {
        Self::from_lp_hp(freq, sample_rate, false)
    }

    fn highpass(freq: f32, sample_rate: f32) -> Self {
        Self::from_lp_hp(freq, sample_rate, true)
    }

    fn from_lp_hp(freq: f32, sample_rate: f32, highpass: bool) -> Self {
        let q = std::f32::consts::FRAC_1_SQRT_2;
        let w0 = 2.0 * std::f32::consts::PI * (freq / sample_rate).clamp(0.0001, 0.49);
        let (sin_w0, cos_w0) = w0.sin_cos();
        let alpha = sin_w0 / (2.0 * q);
        let a0 = 1.0 + alpha;

        let (b0, b1) = if highpass {
            ((1.0 + cos_w0) / 2.0, -(1.0 + cos_w0))
        } else {
            ((1.0 - cos_w0) / 2.0, 1.0 - cos_w0)
        };
        Self {
            b0: b0 / a0,
            b1: b1 / a0,
            b2: b0 / a0,
            a1: -2.0 * cos_w0 / a0,
            a2: (1.0 - alpha) / a0,
            ..Default::default()
        }
    }

    fn process(&mut self, samples: &mut [f32]) {
        for frame in samples.chunks_mut(2) {
            for (ch, s) in frame.iter_mut().enumerate() {
                let x = *s;
                let y = self.b0 * x + self.b1 * self.x1[ch] + self.b2 * self.x2[ch]
                    - self.a1 * self.y1[ch]
                    - self.a2 * self.y2[ch];
                self.x2[ch] = self.x1[ch];
                self.x1[ch] = x;
                self.y2[ch] = self.y1[ch];
                self.y1[ch] = y;
                *s = y;
            }
        }
    }
}

/// Telephone / radio band-limit: high-pass + low-pass with adjustable cutoffs.
/// Defaults approximate a classic telephone line (300 Hz – 3.4 kHz).
pub struct TelephonePlugin {
    enabled: bool,
    low_cut: f32,  // high-pass cutoff, Hz
    high_cut: f32, // low-pass cutoff, Hz

    // Rebuilt whenever cutoffs or the sample rate change.
    hp: Biquad,
    lp: Biquad,
    built_for: (f32, f32, u32),
}

impl TelephonePlugin {
    pub fn new(low_cut: f32, high_cut: f32, enabled: bool) -> Self {
        Self {
            enabled,
            low_cut: low_cut.clamp(20.0, 2000.0),
            high_cut: high_cut.clamp(200.0, 20000.0),
            hp: Biquad::default(),
            lp: Biquad::default(),
            built_for: (0.0, 0.0, 0),
        }
    }
}

impl Default for TelephonePlugin {
    fn default() -> Self {
        Self::new(300.0, 3400.0, true)
    }
}

impl ResonatePlugin for TelephonePlugin {
    fn name(&self) -> &str { "Telephone" }
    fn id(&self) -> &str { "telephone" }
    fn is_enabled(&self) -> bool { self.enabled }
    fn set_enabled(&mut self, enabled: bool) { self.enabled = enabled; }

    fn process(&mut self, samples: &mut [f32], sample_rate: u32) {
        if !self.enabled { return; }
        if self.built_for != (self.low_cut, self.high_cut, sample_rate) {
            self.hp = Biquad::highpass(self.low_cut, sample_rate as f32);
            self.lp = Biquad::lowpass(self.high_cut, sample_rate as f32);
            self.built_for = (self.low_cut, self.high_cut, sample_rate);
        }
        self.hp.process(samples);
        self.lp.process(samples);
        for s in samples.iter_mut() {
            *s = s.clamp(-1.0, 1.0);
        }
    }

    fn params(&self) -> Vec<PluginParam> {
        vec![
            PluginParam {
                id: "low_cut".into(),
                label: "Low cut (Hz)".into(),
                min: 20.0,
                max: 2000.0,
                default: 300.0,
                value: self.low_cut,
                kind: ParamKind::Integer,
            },
            PluginParam {
                id: "high_cut".into(),
                label: "High cut (Hz)".into(),
                min: 200.0,
                max: 20000.0,
                default: 3400.0,
                value: self.high_cut,
                kind: ParamKind::Integer,
            },
        ]
    }

    fn set_param(&mut self, id: &str, value: f32) {
        match id {
            "low_cut" => self.low_cut = value.clamp(20.0, 2000.0),
            "high_cut" => self.high_cut = value.clamp(200.0, 20000.0),
            _ => {}
        }
    }
}
