use anyhow::Result;
use rodio::{Decoder, OutputStream, OutputStreamHandle, Sink, Source};
use std::collections::HashMap;
use std::io::BufReader;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use super::virtual_device::{VirtualDeviceHandle, CHANNELS, SAMPLE_RATE};
use crate::config::EffectEntry;
use crate::plugins::host::PluginChain;
use crate::plugins::PluginParam;

/// Shared, lockable mic-effects chain. Read by the PipeWire mic thread (via
/// `try_lock`), mutated from the UI thread.
pub type SharedChain = Arc<Mutex<PluginChain>>;

pub struct SoundInfo {
    pub name: String,
    pub fraction: f64,
    pub remaining_secs: Option<u32>,
}

// ── PCM decode ────────────────────────────────────────────────────────────────

type PcmSlot = Arc<Mutex<Option<Arc<Vec<f32>>>>>;

fn decode_to_pcm(path: &Path) -> Result<Arc<Vec<f32>>> {
    let file = std::fs::File::open(path)?;
    let dec = Decoder::new(BufReader::new(file))
        .map_err(|e| anyhow::anyhow!("Decode '{}': {}", path.display(), e))?;

    let src_rate = dec.sample_rate();
    let src_ch = dec.channels() as usize;
    let raw_f32: Vec<f32> = dec.convert_samples().collect();

    let stereo: Vec<f32> = if src_ch == 1 {
        raw_f32.iter().flat_map(|&s| [s, s]).collect()
    } else if src_ch == 2 {
        raw_f32
    } else {
        raw_f32
            .chunks(src_ch)
            .flat_map(|c| [c[0], c.get(1).copied().unwrap_or(0.0)])
            .collect()
    };

    let out = if src_rate == SAMPLE_RATE {
        stereo
    } else {
        resample_linear(&stereo, src_rate, SAMPLE_RATE)
    };

    Ok(Arc::new(out))
}

fn resample_linear(input: &[f32], src: u32, dst: u32) -> Vec<f32> {
    let ratio = src as f64 / dst as f64;
    let src_frames = input.len() / CHANNELS;
    let dst_frames = (src_frames as f64 / ratio).ceil() as usize;
    let mut out = Vec::with_capacity(dst_frames * CHANNELS);
    for i in 0..dst_frames {
        let pos = i as f64 * ratio;
        let idx = pos as usize;
        let frac = (pos - idx as f64) as f32;
        for ch in 0..CHANNELS {
            let a = input.get(idx * CHANNELS + ch).copied().unwrap_or(0.0);
            let b = input.get((idx + 1) * CHANNELS + ch).copied().unwrap_or(0.0);
            out.push(a + (b - a) * frac);
        }
    }
    out
}

// ── Playing sound ─────────────────────────────────────────────────────────────

struct PlayingSound {
    name: String,
    path: PathBuf,
    sink: Sink,
    total_duration: Option<Duration>,
    pcm: Option<Arc<Vec<f32>>>,
    pcm_slot: Option<PcmSlot>, // background-decode slot; cleared once PCM arrives
    pcm_pos: usize,
    /// Per-sound linear gain (from the tile's volume slider), 0.0–1.0.
    volume: f32,
}

struct QueuedSound {
    name: String,
    path: PathBuf,
    total_secs: Option<u32>,
    volume: f32,
}

fn probe_duration(path: &Path) -> Option<Duration> {
    let file = std::fs::File::open(path).ok()?;
    Decoder::new(BufReader::new(file)).ok()?.total_duration()
}

// ── Engine ────────────────────────────────────────────────────────────────────

pub struct AudioEngine {
    _stream: OutputStream,
    stream_handle: OutputStreamHandle,

    playing: Vec<PlayingSound>,
    queue: Vec<QueuedSound>,
    polyphonic: bool,

    pub virtual_device: Option<VirtualDeviceHandle>,
    pcm_cache: HashMap<PathBuf, Arc<Vec<f32>>>,
    last_tick: Instant,

    monitor_volume: f32,
    monitor_enabled: bool,
    /// When true, pressing play on an already-playing sound stops it instead.
    stop_on_play: bool,

    /// Mic effects chain, shared with the PipeWire mic capture thread.
    effects: SharedChain,
}

impl AudioEngine {
    pub fn new() -> Result<Self> {
        let (_stream, stream_handle) = OutputStream::try_default()
            .map_err(|e| anyhow::anyhow!("Audio output: {}", e))?;
        Ok(Self {
            _stream,
            stream_handle,
            playing: Vec::new(),
            queue: Vec::new(),
            polyphonic: true,
            virtual_device: None,
            pcm_cache: HashMap::new(),
            last_tick: Instant::now(),
            monitor_volume: 1.0,
            monitor_enabled: true,
            stop_on_play: true,
            effects: Arc::new(Mutex::new(PluginChain::new())),
        })
    }

    /// Clone of the shared mic-effects chain, to hand to the PipeWire mic thread.
    pub fn effects_handle(&self) -> SharedChain {
        self.effects.clone()
    }

    /// Rebuild the whole chain from config (used on startup and on add/remove).
    pub fn rebuild_effects(&self, entries: &[EffectEntry]) {
        let chain = PluginChain::from_entries(entries);
        if let Ok(mut guard) = self.effects.lock() {
            *guard = chain;
        }
    }

    /// Live parameters of the effect at `idx`, for the effects UI.
    pub fn effect_params(&self, idx: usize) -> Vec<PluginParam> {
        self.effects
            .lock()
            .map(|c| c.params_for(idx))
            .unwrap_or_default()
    }

    pub fn set_virtual_device(&mut self, dev: VirtualDeviceHandle) {
        self.virtual_device = Some(dev);
    }

    pub fn set_polyphonic(&mut self, v: bool) {
        self.polyphonic = v;
    }

    pub fn polyphonic(&self) -> bool {
        self.polyphonic
    }

    pub fn set_monitor_volume(&mut self, v: f32) {
        self.monitor_volume = v;
        let base = if self.monitor_enabled { v } else { 0.0 };
        for ps in &self.playing {
            ps.sink.set_volume(base * ps.volume);
        }
    }

    pub fn set_monitor_enabled(&mut self, enabled: bool) {
        self.monitor_enabled = enabled;
        let base = if enabled { self.monitor_volume } else { 0.0 };
        for ps in &self.playing {
            ps.sink.set_volume(base * ps.volume);
        }
        // Virtual device is NOT muted here — it always mirrors soundboard + mic.
    }

    pub fn set_stop_on_play(&mut self, v: bool) {
        self.stop_on_play = v;
    }

    pub fn set_mic_volume(&mut self, v: f32) {
        if let Some(vd) = &self.virtual_device {
            vd.set_mic_volume(v);
        }
    }

    // Mic effects run in-process on the PipeWire mic capture thread (see
    // virtual_device.rs). These mutate the live, shared chain.
    pub fn set_effect_enabled(&self, idx: usize, enabled: bool) {
        if let Ok(mut c) = self.effects.lock() {
            c.set_enabled(idx, enabled);
        }
    }

    pub fn set_effect_param(&self, idx: usize, param: &str, value: f32) {
        if let Ok(mut c) = self.effects.lock() {
            c.set_param(idx, param, value);
        }
    }

    // ── Playback ──────────────────────────────────────────────────────────────

    pub fn play(&mut self, path: &Path, name: &str, volume: f32) -> Result<()> {
        // "Stop on Second Press": pressing play on an already-playing sound stops it.
        if self.stop_on_play && self.playing.iter().any(|s| s.path == path) {
            self.playing.retain(|s| s.path != path);
            return Ok(());
        }
        if self.polyphonic || self.playing.is_empty() {
            self.start_sound(path, name, volume)?;
        } else {
            let total_secs = probe_duration(path).map(|d| d.as_secs() as u32);
            self.queue.push(QueuedSound {
                name: name.to_string(),
                path: path.to_path_buf(),
                total_secs,
                volume,
            });
        }
        Ok(())
    }

    pub fn cue(&mut self, path: &Path, name: &str, volume: f32) {
        let total_secs = probe_duration(path).map(|d| d.as_secs() as u32);
        self.queue.push(QueuedSound {
            name: name.to_string(),
            path: path.to_path_buf(),
            total_secs,
            volume,
        });
    }

    fn start_sound(&mut self, path: &Path, name: &str, volume: f32) -> Result<()> {
        // Start rodio immediately — no blocking on this path.
        let file = std::fs::File::open(path)?;
        let source = Decoder::new(BufReader::new(file))
            .map_err(|e| anyhow::anyhow!("Decode '{}': {}", path.display(), e))?;
        let total = source.total_duration();
        let sink =
            Sink::try_new(&self.stream_handle).map_err(|e| anyhow::anyhow!("Sink: {}", e))?;
        let base = if self.monitor_enabled { self.monitor_volume } else { 0.0 };
        sink.set_volume(base * volume);
        sink.append(source);

        // PCM for virtual mic: serve from cache or decode in background.
        let (pcm, pcm_slot) = if self.virtual_device.is_some() {
            if let Some(cached) = self.pcm_cache.get(path) {
                (Some(cached.clone()), None)
            } else {
                let slot: PcmSlot = Arc::new(Mutex::new(None));
                let slot_clone = slot.clone();
                let path_clone = path.to_path_buf();
                std::thread::Builder::new()
                    .name("resonate-decode".into())
                    .spawn(move || match decode_to_pcm(&path_clone) {
                        Ok(pcm) => {
                            if let Ok(mut g) = slot_clone.lock() {
                                *g = Some(pcm);
                            }
                        }
                        Err(e) => log::warn!("PCM decode: {e}"),
                    })
                    .ok();
                (None, Some(slot))
            }
        } else {
            (None, None)
        };

        self.playing.push(PlayingSound {
            name: name.to_string(),
            path: path.to_path_buf(),
            sink,
            total_duration: total,
            pcm,
            pcm_slot,
            pcm_pos: 0,
            volume,
        });
        Ok(())
    }

    // ── Tick ──────────────────────────────────────────────────────────────────

    pub fn tick(&mut self) -> (Vec<SoundInfo>, Vec<(String, Option<u32>)>) {
        self.playing.retain(|s| !s.sink.empty());

        if self.playing.is_empty() {
            while !self.queue.is_empty() {
                let next = self.queue.remove(0);
                if let Err(e) = self.start_sound(&next.path, &next.name, next.volume) {
                    log::error!("Failed to start queued '{}': {}", next.name, e);
                    continue;
                }
                break;
            }
        }

        // Virtual mic mixing — elapsed-time approach avoids sink.get_pos() quirks on PipeWire.
        if self.virtual_device.is_some() {
            let elapsed_secs = self.last_tick.elapsed().as_secs_f64().min(0.5);
            let n = (elapsed_secs * SAMPLE_RATE as f64).round() as usize * CHANNELS;

            // For sounds still decoding: advance pcm_pos speculatively so that when PCM
            // arrives, we start from the right position (not from the beginning).
            // For sounds whose decode just finished: set pcm, clamp pos, leave pos
            // for the mix block to advance (avoids a double-advance in the same tick).
            let mut cache_inserts: Vec<(PathBuf, Arc<Vec<f32>>)> = Vec::new();
            for ps in &mut self.playing {
                if ps.pcm.is_none() {
                    let ready = ps
                        .pcm_slot
                        .as_ref()
                        .and_then(|s| s.try_lock().ok())
                        .and_then(|g| g.as_ref().map(Arc::clone));
                    if let Some(pcm) = ready {
                        ps.pcm_pos = ps.pcm_pos.min(pcm.len());
                        cache_inserts.push((ps.path.clone(), pcm.clone()));
                        ps.pcm = Some(pcm);
                        ps.pcm_slot = None;
                    } else {
                        ps.pcm_pos = ps.pcm_pos.saturating_add(n);
                    }
                }
            }
            for (path, pcm) in cache_inserts {
                self.pcm_cache.insert(path, pcm);
            }

            // Mix and push only when at least one sound has decoded PCM.
            // Never push silence — that would pre-fill the ring buffer and delay playback.
            if n > 0 && self.playing.iter().any(|ps| ps.pcm.is_some()) {
                let mut mix = vec![0.0f32; n];
                for ps in &mut self.playing {
                    if let Some(pcm) = &ps.pcm {
                        let end = (ps.pcm_pos + n).min(pcm.len());
                        let avail = end.saturating_sub(ps.pcm_pos);
                        for i in 0..avail {
                            mix[i] = (mix[i] + pcm[ps.pcm_pos + i] * ps.volume).clamp(-1.0, 1.0);
                        }
                        ps.pcm_pos = end;
                    }
                }
                if let Some(vd) = &self.virtual_device {
                    vd.push_samples(&mix);
                }
            }

        }
        self.last_tick = Instant::now();

        // Build display info
        let playing_info: Vec<SoundInfo> = self
            .playing
            .iter()
            .map(|s| {
                let elapsed = s.sink.get_pos();
                if let Some(total) = s.total_duration {
                    let frac = if total.as_secs_f64() > 0.0 {
                        (elapsed.as_secs_f64() / total.as_secs_f64()).clamp(0.0, 1.0)
                    } else {
                        0.0
                    };
                    let remaining = total.as_secs().saturating_sub(elapsed.as_secs()) as u32;
                    SoundInfo {
                        name: s.name.clone(),
                        fraction: frac,
                        remaining_secs: Some(remaining),
                    }
                } else {
                    SoundInfo {
                        name: s.name.clone(),
                        fraction: 0.0,
                        remaining_secs: None,
                    }
                }
            })
            .collect();

        let queue_info: Vec<(String, Option<u32>)> = self
            .queue
            .iter()
            .map(|q| (q.name.clone(), q.total_secs))
            .collect();

        (playing_info, queue_info)
    }

    // ── Controls ──────────────────────────────────────────────────────────────

    pub fn stop_all(&mut self) {
        self.playing.clear();
        self.queue.clear();
        self.last_tick = Instant::now();
        if let Some(vd) = &self.virtual_device {
            let _ = vd.ctrl.send(super::virtual_device::PwCtrl::Flush);
        }
    }

    pub fn stop_sound_by_path(&mut self, path: &Path) {
        self.playing.retain(|s| s.path != path);
    }

    pub fn pause_all(&mut self) {
        for s in &self.playing {
            s.sink.pause();
        }
    }

    pub fn resume_all(&mut self) {
        for s in &self.playing {
            s.sink.play();
        }
    }

    pub fn skip_queue(&mut self) {
        self.playing.clear();
        if !self.queue.is_empty() {
            let next = self.queue.remove(0);
            if let Err(e) = self.start_sound(&next.path, &next.name, next.volume) {
                log::error!("Skip failed: {e}");
            }
        }
    }

    pub fn toggle_pause(&mut self) {
        if self.is_all_paused() {
            self.resume_all();
        } else {
            self.pause_all();
        }
    }

    pub fn is_all_paused(&self) -> bool {
        !self.playing.is_empty() && self.playing.iter().all(|s| s.sink.is_paused())
    }

    pub fn is_anything_playing(&self) -> bool {
        !self.playing.is_empty()
    }
}
