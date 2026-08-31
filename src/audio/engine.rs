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

/// ~85 ms of mono audio at 48 kHz for the LCD oscilloscope window.
const ANALYSIS_LEN: usize = 4096;

pub struct SoundInfo {
    pub name: String,
    pub fraction: f64,
    pub remaining_secs: Option<u32>,
}

// ── PCM decode ────────────────────────────────────────────────────────────────

type PcmSlot = Arc<Mutex<Option<Arc<Vec<f32>>>>>;

// Also used by the sound editor (on its own thread) to build waveform peaks.
pub(crate) fn decode_to_pcm(path: &Path) -> Result<Arc<Vec<f32>>> {
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

// ── Playback parameters ───────────────────────────────────────────────────────

/// Everything that shapes one playback: per-sound gain, trim window and fades.
#[derive(Clone, Copy, Debug)]
pub struct PlayParams {
    /// Per-sound linear gain 0.0–1.0.
    pub volume: f32,
    pub start_secs: f32,
    /// 0.0 = play to the end.
    pub end_secs: f32,
    pub fade_in_ms: f32,
    pub fade_out_ms: f32,
}

impl Default for PlayParams {
    fn default() -> Self {
        Self { volume: 1.0, start_secs: 0.0, end_secs: 0.0, fade_in_ms: 0.0, fade_out_ms: 0.0 }
    }
}

/// Fade-in/out envelope wrapper for the monitor (rodio) path. Fade-out needs the
/// total length; when unknown the fade-out is skipped.
struct EnvelopeSource<S: Source<Item = f32>> {
    inner: S,
    pos: usize,
    fade_in: usize,        // samples
    fade_out: usize,       // samples
    total: Option<usize>,  // samples
}

impl<S: Source<Item = f32>> EnvelopeSource<S> {
    fn new(inner: S, fade_in_ms: f32, fade_out_ms: f32, total_secs: Option<f32>) -> Self {
        let per_sec = inner.sample_rate() as f32 * inner.channels() as f32;
        Self {
            fade_in: (fade_in_ms / 1000.0 * per_sec) as usize,
            fade_out: (fade_out_ms / 1000.0 * per_sec) as usize,
            total: total_secs.map(|t| (t * per_sec) as usize),
            pos: 0,
            inner,
        }
    }
}

impl<S: Source<Item = f32>> Iterator for EnvelopeSource<S> {
    type Item = f32;
    fn next(&mut self) -> Option<f32> {
        let s = self.inner.next()?;
        let mut g = 1.0f32;
        if self.fade_in > 0 && self.pos < self.fade_in {
            g *= self.pos as f32 / self.fade_in as f32;
        }
        if let Some(total) = self.total {
            if self.fade_out > 0 && self.pos + self.fade_out >= total {
                g *= (total.saturating_sub(self.pos)) as f32 / self.fade_out as f32;
            }
        }
        self.pos += 1;
        Some(s * g)
    }
}

impl<S: Source<Item = f32>> Source for EnvelopeSource<S> {
    fn current_frame_len(&self) -> Option<usize> {
        self.inner.current_frame_len()
    }
    fn channels(&self) -> u16 {
        self.inner.channels()
    }
    fn sample_rate(&self) -> u32 {
        self.inner.sample_rate()
    }
    fn total_duration(&self) -> Option<Duration> {
        self.inner.total_duration()
    }
}

/// Build the monitor-path (rodio) source chain for one playback: decode → skip
/// to `play_from` → optional end trim → fade envelope. Returns the sink (volume
/// not yet set) and the file's total duration if the decoder knows it.
fn build_monitor_sink(
    handle: &OutputStreamHandle,
    path: &Path,
    params: &PlayParams,
    play_from: f32,
) -> Result<(Sink, Option<f32>)> {
    let file = std::fs::File::open(path)?;
    let source = Decoder::new(BufReader::new(file))
        .map_err(|e| anyhow::anyhow!("Decode '{}': {}", path.display(), e))?;
    let file_secs = source.total_duration().map(|t| t.as_secs_f32());
    let start = Duration::from_secs_f32(play_from.max(0.0));
    let played_secs = if params.end_secs > play_from {
        Some(params.end_secs - play_from.max(0.0))
    } else {
        file_secs.map(|t| (t - play_from).max(0.0))
    };
    let sink = Sink::try_new(handle).map_err(|e| anyhow::anyhow!("Sink: {}", e))?;
    let src = source.convert_samples::<f32>().skip_duration(start);
    if params.end_secs > play_from {
        let len = Duration::from_secs_f32(params.end_secs - play_from.max(0.0));
        sink.append(EnvelopeSource::new(
            src.take_duration(len),
            params.fade_in_ms,
            params.fade_out_ms,
            played_secs,
        ));
    } else {
        sink.append(EnvelopeSource::new(
            src,
            params.fade_in_ms,
            params.fade_out_ms,
            played_secs,
        ));
    }
    Ok((sink, file_secs))
}

// ── Playing sound ─────────────────────────────────────────────────────────────

struct PlayingSound {
    name: String,
    path: PathBuf,
    sink: Sink,
    /// Original play request — defines the display window (start..end) even
    /// after seeks.
    params: PlayParams,
    /// Where the current sink actually started in file time (changes on seek).
    play_offset_secs: f32,
    file_secs: Option<f32>,
    pcm: Option<Arc<Vec<f32>>>,
    pcm_slot: Option<PcmSlot>, // background-decode slot; cleared once PCM arrives
    pcm_pos: usize,
    /// Per-sound linear gain (from the tile's volume slider), 0.0–1.0.
    volume: f32,
    // Envelope for the virtual-mic PCM path (samples at SAMPLE_RATE·CHANNELS)
    start_pos: usize,
    end_pos: Option<usize>,
    fade_in_samples: usize,
    fade_out_samples: usize,
}

impl PlayingSound {
    /// Display window in file time: (start, end) — end needs a known duration.
    fn window(&self) -> (f32, Option<f32>) {
        let start = self.params.start_secs.max(0.0);
        let end = if self.params.end_secs > start {
            Some(self.params.end_secs)
        } else {
            self.file_secs
        };
        (start, end)
    }

    /// Absolute position in file time.
    fn position_secs(&self) -> f32 {
        self.play_offset_secs + self.sink.get_pos().as_secs_f32()
    }
}

struct QueuedSound {
    name: String,
    path: PathBuf,
    total_secs: Option<u32>,
    params: PlayParams,
}

/// Monitor-only playback used by the sound editor (never fed to the virtual mic).
struct Preview {
    sink: Sink,
    start_secs: f32,
    started: Instant,
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
    /// Level of the soundboard mix into the virtual mic (monitor path is unaffected).
    soundboard_mic_volume: f32,

    /// Mono tail of the soundboard mix, feeding the LCD oscilloscope.
    analysis: std::collections::VecDeque<f32>,

    preview: Option<Preview>,

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
            soundboard_mic_volume: 1.0,
            analysis: std::collections::VecDeque::new(),
            preview: None,
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

    pub fn set_mic_volume(&mut self, v: f32) {
        if let Some(vd) = &self.virtual_device {
            vd.set_mic_volume(v);
        }
    }

    pub fn set_soundboard_mic_volume(&mut self, v: f32) {
        self.soundboard_mic_volume = v;
    }

    /// Post-effects mic RMS for the level meter (0.0 when no virtual device).
    pub fn mic_level(&self) -> f32 {
        self.virtual_device.as_ref().map(|vd| vd.mic_level()).unwrap_or(0.0)
    }

    /// The last ~85 ms of the playing soundboard mix, mono, for the LCD
    /// oscilloscope. Empty when idle.
    pub fn scope(&self) -> Vec<f32> {
        self.analysis.iter().copied().collect()
    }

    /// Live-apply a sound's volume to every playing/queued instance of it.
    pub fn set_sound_volume(&mut self, path: &Path, volume: f32) {
        let base = if self.monitor_enabled { self.monitor_volume } else { 0.0 };
        for ps in &mut self.playing {
            if ps.path == path {
                ps.volume = volume;
                ps.sink.set_volume(base * volume);
            }
        }
        for q in &mut self.queue {
            if q.path == path {
                q.params.volume = volume;
            }
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

    pub fn play(&mut self, path: &Path, name: &str, params: PlayParams) -> Result<()> {
        if self.polyphonic || self.playing.is_empty() {
            self.start_sound(path, name, params)?;
        } else {
            let total_secs = probe_duration(path).map(|d| d.as_secs() as u32);
            self.queue.push(QueuedSound {
                name: name.to_string(),
                path: path.to_path_buf(),
                total_secs,
                params,
            });
        }
        Ok(())
    }

    pub fn cue(&mut self, path: &Path, name: &str, params: PlayParams) {
        let total_secs = probe_duration(path).map(|d| d.as_secs() as u32);
        self.queue.push(QueuedSound {
            name: name.to_string(),
            path: path.to_path_buf(),
            total_secs,
            params,
        });
    }

    fn start_sound(&mut self, path: &Path, name: &str, params: PlayParams) -> Result<()> {
        let volume = params.volume;
        // Start rodio immediately — no blocking on this path.
        let (sink, file_secs) =
            build_monitor_sink(&self.stream_handle, path, &params, params.start_secs)?;
        let base = if self.monitor_enabled { self.monitor_volume } else { 0.0 };
        sink.set_volume(base * volume);

        // PCM for the virtual mic and the spectrum display: cache or background decode.
        let (pcm, pcm_slot) = if let Some(cached) = self.pcm_cache.get(path) {
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
        };

        // Virtual-mic PCM starts at the same offset as the monitor sink.
        let per_sec = SAMPLE_RATE as f64 * CHANNELS as f64;
        let start_pos = (params.start_secs.max(0.0) as f64 * per_sec) as usize / CHANNELS * CHANNELS;
        let end_pos = (params.end_secs > params.start_secs)
            .then(|| (params.end_secs as f64 * per_sec) as usize / CHANNELS * CHANNELS);

        self.playing.push(PlayingSound {
            name: name.to_string(),
            path: path.to_path_buf(),
            sink,
            params,
            play_offset_secs: params.start_secs.max(0.0),
            file_secs,
            pcm,
            pcm_slot,
            pcm_pos: start_pos,
            volume,
            start_pos,
            end_pos,
            fade_in_samples: (params.fade_in_ms as f64 / 1000.0 * per_sec) as usize,
            fade_out_samples: (params.fade_out_ms as f64 / 1000.0 * per_sec) as usize,
        });
        Ok(())
    }

    /// Seek the first playing sound to `frac` of its display window (progress
    /// bar scrub). Rebuilds the monitor sink at the target — the display window
    /// (and thus the progress bar mapping) is unchanged.
    pub fn seek_playing(&mut self, frac: f64) {
        let Some((path, params, win_start, win_end)) = self.playing.first().map(|ps| {
            let (s, e) = ps.window();
            (ps.path.clone(), ps.params, s, e)
        }) else {
            return;
        };
        let Some(win_end) = win_end else { return }; // unknown duration → no mapping
        let target = win_start + frac.clamp(0.0, 1.0) as f32 * (win_end - win_start).max(0.0);

        // No fade-in on a seek — re-fading mid-sound sounds like a dropout.
        let mut seek_params = params;
        seek_params.fade_in_ms = 0.0;
        let built = build_monitor_sink(&self.stream_handle, &path, &seek_params, target);
        let base = if self.monitor_enabled { self.monitor_volume } else { 0.0 };
        match built {
            Ok((sink, _)) => {
                if let Some(ps) = self.playing.first_mut() {
                    sink.set_volume(base * ps.volume);
                    ps.sink = sink; // old sink drops → stops
                    ps.play_offset_secs = target;
                    let per_sec = SAMPLE_RATE as f64 * CHANNELS as f64;
                    let pos = (target as f64 * per_sec) as usize / CHANNELS * CHANNELS;
                    ps.pcm_pos = pos;
                    ps.start_pos = pos;
                    ps.fade_in_samples = 0;
                }
            }
            Err(e) => log::warn!("Seek failed: {e}"),
        }
    }

    // ── Preview (sound editor) ────────────────────────────────────────────────

    /// Play a sound on the monitor output only — never fed to the virtual mic.
    /// Honours the full params (start, trim, fades). Replaces any running preview.
    pub fn preview(&mut self, path: &Path, params: PlayParams) -> Result<()> {
        self.stop_preview();
        let file = std::fs::File::open(path)?;
        let source = Decoder::new(BufReader::new(file))
            .map_err(|e| anyhow::anyhow!("Decode '{}': {}", path.display(), e))?;
        let start = Duration::from_secs_f32(params.start_secs.max(0.0));
        let file_total = source.total_duration();
        let played_secs = if params.end_secs > params.start_secs {
            Some(params.end_secs - params.start_secs.max(0.0))
        } else {
            file_total.map(|t| t.saturating_sub(start).as_secs_f32())
        };
        let sink =
            Sink::try_new(&self.stream_handle).map_err(|e| anyhow::anyhow!("Sink: {}", e))?;
        sink.set_volume(self.monitor_volume * params.volume);
        let src = source.convert_samples::<f32>().skip_duration(start);
        if params.end_secs > params.start_secs {
            let len = Duration::from_secs_f32(params.end_secs - params.start_secs.max(0.0));
            sink.append(EnvelopeSource::new(
                src.take_duration(len),
                params.fade_in_ms,
                params.fade_out_ms,
                played_secs,
            ));
        } else {
            sink.append(EnvelopeSource::new(
                src,
                params.fade_in_ms,
                params.fade_out_ms,
                played_secs,
            ));
        }
        self.preview = Some(Preview { sink, start_secs: params.start_secs, started: Instant::now() });
        Ok(())
    }

    pub fn stop_preview(&mut self) {
        self.preview = None;
    }

    /// Current preview position in seconds from the file start, if previewing.
    pub fn preview_position(&mut self) -> Option<f32> {
        if let Some(p) = &self.preview {
            if p.sink.empty() {
                self.preview = None;
                return None;
            }
            return Some(p.start_secs + p.started.elapsed().as_secs_f32());
        }
        None
    }

    // ── Tick ──────────────────────────────────────────────────────────────────

    pub fn tick(&mut self) -> (Vec<SoundInfo>, Vec<(String, Option<u32>)>) {
        self.playing.retain(|s| !s.sink.empty());

        if self.playing.is_empty() {
            while !self.queue.is_empty() {
                let next = self.queue.remove(0);
                if let Err(e) = self.start_sound(&next.path, &next.name, next.params) {
                    log::error!("Failed to start queued '{}': {}", next.name, e);
                    continue;
                }
                break;
            }
        }

        // Mixing — elapsed-time approach avoids sink.get_pos() quirks on PipeWire.
        // Runs even without a virtual device: the mix also feeds the spectrum.
        {
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
                        let end_limit = ps.end_pos.map(|e| e.min(pcm.len())).unwrap_or(pcm.len());
                        let end = (ps.pcm_pos + n).min(end_limit);
                        let avail = end.saturating_sub(ps.pcm_pos);
                        let gain = ps.volume * self.soundboard_mic_volume;
                        for i in 0..avail {
                            let pos = ps.pcm_pos + i;
                            let mut g = gain;
                            if ps.fade_in_samples > 0 && pos < ps.start_pos + ps.fade_in_samples {
                                g *= (pos - ps.start_pos) as f32 / ps.fade_in_samples as f32;
                            }
                            if ps.fade_out_samples > 0 && pos + ps.fade_out_samples >= end_limit {
                                g *= (end_limit - pos) as f32 / ps.fade_out_samples as f32;
                            }
                            mix[i] = (mix[i] + pcm[pos] * g).clamp(-1.0, 1.0);
                        }
                        if avail > 0 {
                            ps.pcm_pos = end;
                        }
                    }
                }
                if let Some(vd) = &self.virtual_device {
                    vd.push_samples(&mix);
                }
                // Mono tail of the mix feeds the LCD oscilloscope.
                for frame in mix.chunks(CHANNELS) {
                    self.analysis
                        .push_back(frame.iter().sum::<f32>() / CHANNELS as f32);
                }
                while self.analysis.len() > ANALYSIS_LEN {
                    self.analysis.pop_front();
                }
            } else if n > 0 && !self.playing.is_empty() {
                // Sounds are playing but their PCM is still decoding: advance the
                // scope with silence so it doesn't freeze on stale content.
                for _ in 0..(n / CHANNELS) {
                    self.analysis.push_back(0.0);
                }
                while self.analysis.len() > ANALYSIS_LEN {
                    self.analysis.pop_front();
                }
            }
        }
        if self.playing.is_empty() {
            self.analysis.clear();
        }
        self.last_tick = Instant::now();

        // Build display info. Fractions map into the display window (start..end),
        // which survives seeks.
        let playing_info: Vec<SoundInfo> = self
            .playing
            .iter()
            .map(|s| {
                let (win_start, win_end) = s.window();
                match win_end {
                    Some(end) if end > win_start => {
                        let pos = s.position_secs();
                        let span = end - win_start;
                        SoundInfo {
                            name: s.name.clone(),
                            fraction: (((pos - win_start) / span) as f64).clamp(0.0, 1.0),
                            remaining_secs: Some((end - pos).max(0.0) as u32),
                        }
                    }
                    _ => SoundInfo {
                        name: s.name.clone(),
                        fraction: 0.0,
                        remaining_secs: None,
                    },
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
            if let Err(e) = self.start_sound(&next.path, &next.name, next.params) {
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
