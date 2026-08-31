//! Shared waveform-peak data, used by the sound editor dialog and the LCD
//! waveform on the soundboard page.

use std::path::Path;
use std::sync::{Arc, Mutex};

use super::virtual_device::{CHANNELS, SAMPLE_RATE};

pub const WAVE_BUCKETS: usize = 1024;

#[derive(Clone)]
pub struct Wave {
    /// Per-bucket peak (max abs), 0.0–1.0.
    pub peaks: Vec<f32>,
    pub duration_secs: f32,
    /// Peak of the whole file (for import normalisation).
    pub peak: f32,
}

pub fn build_wave(pcm: &[f32]) -> Wave {
    let frames = pcm.len() / CHANNELS;
    let duration_secs = frames as f32 / SAMPLE_RATE as f32;
    let mut peaks = vec![0.0f32; WAVE_BUCKETS];
    let mut peak = 0.0f32;
    if frames > 0 {
        for (i, frame) in pcm.chunks(CHANNELS).enumerate() {
            let bucket = i * WAVE_BUCKETS / frames;
            let amp = frame.iter().fold(0.0f32, |m, s| m.max(s.abs()));
            let b = &mut peaks[bucket.min(WAVE_BUCKETS - 1)];
            *b = b.max(amp);
            peak = peak.max(amp);
        }
    }
    Wave { peaks, duration_secs, peak }
}

/// Decode + build peaks on a background thread. Poll the returned slot from a
/// glib timeout (the codebase's standard cross-thread pattern — no async runtime).
pub fn load_async(path: &Path) -> Arc<Mutex<Option<Wave>>> {
    let slot: Arc<Mutex<Option<Wave>>> = Arc::new(Mutex::new(None));
    let slot_clone = slot.clone();
    let path = path.to_path_buf();
    std::thread::Builder::new()
        .name("resonate-wave".into())
        .spawn(move || match super::engine::decode_to_pcm(&path) {
            Ok(pcm) => {
                let wave = build_wave(&pcm);
                if let Ok(mut g) = slot_clone.lock() {
                    *g = Some(wave);
                }
            }
            Err(e) => log::warn!("Waveform decode: {e}"),
        })
        .ok();
    slot
}

pub fn fmt_secs(s: f32) -> String {
    let m = (s / 60.0) as u32;
    format!("{}:{:05.2}", m, s - m as f32 * 60.0)
}
