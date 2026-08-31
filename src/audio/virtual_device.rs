//! Soundboard → PipeWire bridge.
//!
//! The virtual microphone itself (the Resonate sink + "Resonate Microphone" source,
//! plus the physical-mic pass-through) is created in the PipeWire graph by
//! [`crate::audio::pw_config`]. PipeWire does all the mixing.
//!
//! This module only runs ONE PipeWire stream: a playback stream that feeds decoded
//! soundboard PCM into the Resonate sink. Whatever we write here is mixed with the
//! mic by PipeWire and exposed on the virtual source. Underruns just produce brief
//! silence — there is no hand-clocked mixing and no rate matching to get wrong.
//!
//!   soundboard PCM ─▶ audio_queue ─▶ [this playback stream] ─▶ "resonate_sink"
//!                                                                     │
//!                              physical mic (loopback) ──────────────▶┤
//!                                                                     ▼
//!                                                     "Resonate Microphone" source
use std::collections::VecDeque;
use std::sync::{
    atomic::{AtomicBool, AtomicU32, Ordering},
    Arc, Mutex,
};
use std::thread::{self, JoinHandle};

use anyhow::{anyhow, Result};

use crate::plugins::host::PluginChain;

pub const SAMPLE_RATE: u32 = 48000;
pub const CHANNELS: usize = 2;

/// PipeWire node names of the graph nodes created by `pw_config`.
pub const SINK_NAME: &str = "resonate_sink";
pub const SOURCE_NAME: &str = "resonate_source";

/// 3 seconds of audio at 48 kHz stereo — overflow cap for the soundboard queue.
const QUEUE_CAP: usize = SAMPLE_RATE as usize * CHANNELS * 3;

/// ~0.5 s — overflow cap for the mic capture→playback bridge (keeps latency low).
const MIC_CAP: usize = SAMPLE_RATE as usize * CHANNELS / 2;

// ── Control messages ──────────────────────────────────────────────────────────

pub enum PwCtrl {
    Terminate,
    Flush,
}

// ── Public handle ─────────────────────────────────────────────────────────────

pub struct VirtualDeviceHandle {
    pub audio_queue: Arc<Mutex<VecDeque<f32>>>,
    pub volume: Arc<AtomicU32>,
    pub enabled: Arc<AtomicBool>,
    /// Linear gain applied to the captured microphone before the effects chain.
    pub mic_volume: Arc<AtomicU32>,
    /// Post-effects RMS of the mic capture (f32 bits), written by the PW thread.
    pub mic_level: Arc<AtomicU32>,
    pub ctrl: pipewire::channel::Sender<PwCtrl>,
    _thread: JoinHandle<()>,
}

impl VirtualDeviceHandle {
    /// Push mixed soundboard samples into the queue consumed by the PW playback
    /// callback. Blocking lock — this is the UI thread, so blocking is fine.
    pub fn push_samples(&self, samples: &[f32]) {
        if let Ok(mut q) = self.audio_queue.lock() {
            q.extend(samples.iter().copied());
            while q.len() > QUEUE_CAP {
                q.pop_front();
            }
        }
    }

    pub fn set_volume(&self, v: f32) {
        self.volume.store(v.to_bits(), Ordering::Relaxed);
    }

    pub fn set_mic_volume(&self, v: f32) {
        self.mic_volume.store(v.to_bits(), Ordering::Relaxed);
    }

    pub fn set_enabled(&self, v: bool) {
        self.enabled.store(v, Ordering::Relaxed);
    }

    /// Latest post-effects mic RMS level, 0.0–1.0.
    pub fn mic_level(&self) -> f32 {
        f32::from_bits(self.mic_level.load(Ordering::Relaxed))
    }
}

impl Drop for VirtualDeviceHandle {
    fn drop(&mut self) {
        let _ = self.ctrl.send(PwCtrl::Terminate);
    }
}

// ── Device enumeration ────────────────────────────────────────────────────────

#[derive(Clone, Debug)]
pub struct AudioNode {
    pub id: u32,
    pub name: String,
    pub media_class: String,
    pub description: String,
}

/// Start the soundboard→PipeWire bridge and the in-process mic effects path.
///
/// `device_name`  — display name for the virtual mic ("Resonate Microphone").
/// `input_device` — description of the physical mic; empty = auto-detect.
/// `effects`      — shared mic-effects chain, run on the capture callback.
/// `mic_volume`   — initial linear mic gain.
pub fn start(
    device_name: &str,
    input_device: &str,
    effects: Arc<Mutex<PluginChain>>,
    mic_volume: f32,
) -> Result<VirtualDeviceHandle> {
    // Decide routing: tear down the raw daemon loopback (avoids double mic), find
    // the mic to capture, and learn whether we must create the bridge ourselves.
    let plan = crate::audio::pw_config::claim_routing(device_name, input_device);
    let mic_target = plan.mic_target;
    let bridge_args = if plan.create_bridge {
        Some(crate::audio::pw_config::bridge_module_args(device_name))
    } else {
        None
    };

    let audio_queue: Arc<Mutex<VecDeque<f32>>> = Arc::new(Mutex::new(VecDeque::new()));
    let volume = Arc::new(AtomicU32::new(1.0f32.to_bits()));
    let enabled = Arc::new(AtomicBool::new(true));
    let mic_vol = Arc::new(AtomicU32::new(mic_volume.to_bits()));
    let mic_level = Arc::new(AtomicU32::new(0.0f32.to_bits()));

    let (ctrl_tx, ctrl_rx) = pipewire::channel::channel::<PwCtrl>();

    let queue_clone = audio_queue.clone();
    let vol_clone = volume.clone();
    let en_clone = enabled.clone();
    let mic_vol_clone = mic_vol.clone();
    let mic_level_clone = mic_level.clone();

    let thread = thread::Builder::new()
        .name("resonate-pw".into())
        .spawn(move || {
            if let Err(e) = run_pw_thread(
                queue_clone,
                vol_clone,
                en_clone,
                mic_vol_clone,
                mic_level_clone,
                effects,
                mic_target,
                bridge_args,
                ctrl_rx,
            ) {
                log::error!("PipeWire thread: {e}");
            }
        })?;

    Ok(VirtualDeviceHandle {
        audio_queue,
        volume,
        enabled,
        mic_volume: mic_vol,
        mic_level,
        ctrl: ctrl_tx,
        _thread: thread,
    })
}

// ── PipeWire thread ───────────────────────────────────────────────────────────

struct PwState {
    audio_queue: Arc<Mutex<VecDeque<f32>>>,
    volume: Arc<AtomicU32>,
    enabled: Arc<AtomicBool>,
    flush_pending: bool,
}

fn build_format_pod() -> Result<Vec<u8>> {
    use pipewire::spa;
    use spa::param::audio::{AudioFormat, AudioInfoRaw};
    use spa::pod::{serialize::PodSerializer, Object, Value};

    let mut info = AudioInfoRaw::new();
    info.set_format(AudioFormat::F32LE);
    info.set_rate(SAMPLE_RATE);
    info.set_channels(CHANNELS as u32);
    let mut pos = [0u32; spa::param::audio::MAX_CHANNELS];
    pos[0] = spa::sys::SPA_AUDIO_CHANNEL_FL;
    pos[1] = spa::sys::SPA_AUDIO_CHANNEL_FR;
    info.set_position(pos);

    let bytes = PodSerializer::serialize(
        std::io::Cursor::new(Vec::new()),
        &Value::Object(Object {
            type_: spa::sys::SPA_TYPE_OBJECT_Format,
            id: spa::sys::SPA_PARAM_EnumFormat,
            properties: info.into(),
        }),
    )
    .map_err(|e| anyhow!("POD serialize: {e}"))?
    .0
    .into_inner();

    Ok(bytes)
}

#[allow(clippy::too_many_arguments)]
fn run_pw_thread(
    audio_queue: Arc<Mutex<VecDeque<f32>>>,
    volume: Arc<AtomicU32>,
    enabled: Arc<AtomicBool>,
    mic_volume: Arc<AtomicU32>,
    mic_level: Arc<AtomicU32>,
    effects: Arc<Mutex<PluginChain>>,
    mic_target: Option<String>,
    bridge_args: Option<String>,
    ctrl_rx: pipewire::channel::Receiver<PwCtrl>,
) -> Result<()> {
    use pipewire as pw;
    use pw::{properties::properties, spa};
    use spa::pod::Pod;
    use spa::utils::Direction;

    pw::init();

    let mainloop = pw::main_loop::MainLoopRc::new(None).map_err(|e| anyhow!("MainLoop: {e:?}"))?;
    let context =
        pw::context::ContextRc::new(&mainloop, None).map_err(|e| anyhow!("Context: {e:?}"))?;
    let core = context.connect_rc(None).map_err(|e| anyhow!("Core: {e:?}"))?;

    // Create the virtual sink/source bridge inside our own (long-lived) context.
    // `pw-cli load-module` does not persist, so we load the module here; it stays
    // alive for the app's lifetime and is cleaned up when the context drops.
    if let Some(args) = bridge_args {
        let name = std::ffi::CString::new("libpipewire-module-loopback").unwrap();
        let cargs = std::ffi::CString::new(args).unwrap();
        let module = unsafe {
            pw::sys::pw_context_load_module(
                context.as_raw_ptr(),
                name.as_ptr(),
                cargs.as_ptr(),
                std::ptr::null_mut(),
            )
        };
        if module.is_null() {
            log::warn!("Failed to create Resonate virtual audio bridge (loopback module)");
        } else {
            log::info!("Created Resonate virtual audio bridge in-process");
        }
    }

    // Playback stream that feeds the Resonate sink.
    let mut props = properties! {
        *pw::keys::MEDIA_TYPE        => "Audio",
        *pw::keys::MEDIA_CATEGORY    => "Playback",
        *pw::keys::MEDIA_ROLE        => "Production",
        *pw::keys::NODE_NAME         => "resonate-soundboard",
        *pw::keys::NODE_DESCRIPTION  => "Resonate Soundboard",
        *pw::keys::APP_NAME          => "Resonate",
    };
    // Route into the Resonate sink (node.name target).
    props.insert("target.object", SINK_NAME);
    props.insert("node.target", SINK_NAME);

    let stream = pw::stream::StreamBox::new(&core, "resonate-soundboard", props)
        .map_err(|e| anyhow!("Soundboard stream: {e:?}"))?;

    let state = std::rc::Rc::new(std::cell::RefCell::new(PwState {
        audio_queue,
        volume,
        enabled,
        flush_pending: false,
    }));

    let state_proc = state.clone();
    let _listener = stream
        .add_local_listener_with_user_data(())
        .process(move |stream, _| {
            let Some(mut buf) = stream.dequeue_buffer() else { return };
            let datas = buf.datas_mut();
            let d = &mut datas[0];
            let stride = std::mem::size_of::<f32>() * CHANNELS;

            if let Some(slice) = d.data() {
                let n_frames = slice.len() / stride;
                let n_samples = n_frames * CHANNELS;
                let out = unsafe {
                    std::slice::from_raw_parts_mut(slice.as_mut_ptr() as *mut f32, n_samples)
                };

                let mut st = state_proc.borrow_mut();

                if st.flush_pending {
                    if let Ok(mut q) = st.audio_queue.lock() {
                        q.clear();
                    }
                    st.flush_pending = false;
                }

                let vol = if st.enabled.load(Ordering::Relaxed) {
                    f32::from_bits(st.volume.load(Ordering::Relaxed))
                } else {
                    0.0
                };

                // Drain soundboard PCM (try_lock so we never block the PW thread).
                let mut filled = 0usize;
                if let Ok(mut q) = st.audio_queue.try_lock() {
                    let take = q.len().min(n_samples);
                    for sample in out.iter_mut().take(take) {
                        if let Some(s) = q.pop_front() {
                            *sample = (s * vol).clamp(-1.0, 1.0);
                        }
                    }
                    filled = take;
                }
                // Pad the rest with silence.
                for sample in out.iter_mut().skip(filled) {
                    *sample = 0.0;
                }

                let chunk = d.chunk_mut();
                *chunk.offset_mut() = 0;
                *chunk.stride_mut() = stride as i32;
                *chunk.size_mut() = (stride * n_frames) as u32;
            }
        })
        .register()
        .map_err(|e| anyhow!("Soundboard listener: {e:?}"))?;

    let fmt_bytes = build_format_pod()?;
    let mut params = [Pod::from_bytes(&fmt_bytes).ok_or_else(|| anyhow!("Bad POD"))?];
    stream
        .connect(
            Direction::Output,
            None,
            pw::stream::StreamFlags::AUTOCONNECT
                | pw::stream::StreamFlags::MAP_BUFFERS
                | pw::stream::StreamFlags::RT_PROCESS,
            &mut params,
        )
        .map_err(|e| anyhow!("Soundboard connect: {e:?}"))?;

    // ── Mic effects path: physical mic → capture → PluginChain → bridge → sink ──
    //
    // The capture and playback callbacks both run on this PipeWire thread, so the
    // bridge between them is a plain `Rc<RefCell<VecDeque>>` (no locking). The
    // effects chain is shared with the UI thread, so it uses `try_lock`.
    let mic_bridge = std::rc::Rc::new(std::cell::RefCell::new(VecDeque::<f32>::new()));
    let mic_scratch = std::rc::Rc::new(std::cell::RefCell::new(Vec::<f32>::new()));

    // Capture stream from the physical mic.
    let mut cap_props = properties! {
        *pw::keys::MEDIA_TYPE       => "Audio",
        *pw::keys::MEDIA_CATEGORY   => "Capture",
        *pw::keys::MEDIA_ROLE       => "Production",
        *pw::keys::NODE_NAME        => "resonate-mic-capture",
        *pw::keys::NODE_DESCRIPTION => "Resonate Mic Capture",
        *pw::keys::APP_NAME         => "Resonate",
    };
    if let Some(t) = &mic_target {
        // Pin to the physical mic with both keys (matches the working playback
        // streams) and refuse to follow the default source — otherwise, once the
        // virtual mic becomes the default input, this would capture our own
        // source and form a feedback loop (sink → source → here → sink).
        cap_props.insert("target.object", t.as_str());
        cap_props.insert("node.target", t.as_str());
        cap_props.insert("node.dont-reconnect", "true");
        log::info!("Mic capture target: {t}");
    } else {
        log::warn!("No mic target; capture will not be connected to avoid feedback");
    }
    let mic_capture = pw::stream::StreamBox::new(&core, "resonate-mic-capture", cap_props)
        .map_err(|e| anyhow!("Mic capture stream: {e:?}"))?;

    let bridge_cap = mic_bridge.clone();
    let scratch_cap = mic_scratch.clone();
    let effects_cap = effects.clone();
    let mic_vol_cap = mic_volume.clone();
    let mic_level_cb = mic_level.clone();
    let _mic_cap_listener = mic_capture
        .add_local_listener_with_user_data(())
        .process(move |stream, _| {
            let Some(mut buf) = stream.dequeue_buffer() else { return };
            let datas = buf.datas_mut();
            let d = &mut datas[0];
            let bps = std::mem::size_of::<f32>();
            let (offset, size, stride) = {
                let c = d.chunk_mut();
                (
                    *c.offset_mut() as usize,
                    *c.size_mut() as usize,
                    *c.stride_mut() as usize,
                )
            };
            let Some(raw) = d.data() else { return };
            // The capture may negotiate mono (e.g. a mono mic), so derive the real
            // channel count from the frame stride and upmix to the stereo the chain
            // and the sink expect.
            let channels = if stride >= bps { stride / bps } else { CHANNELS };
            let start = offset.min(raw.len());
            let avail = size.min(raw.len() - start);
            let frames = (avail / bps) / channels.max(1);
            if frames == 0 {
                return;
            }
            let in_f32 = unsafe {
                std::slice::from_raw_parts(raw[start..].as_ptr() as *const f32, frames * channels)
            };

            let g = f32::from_bits(mic_vol_cap.load(Ordering::Relaxed));
            let mut scratch = scratch_cap.borrow_mut();
            scratch.clear();
            for f in 0..frames {
                let base = f * channels;
                let (l, r) = if channels == 1 {
                    let s = in_f32[base];
                    (s, s)
                } else {
                    (in_f32[base], in_f32[base + 1])
                };
                scratch.push(l * g);
                scratch.push(r * g);
            }

            if let Ok(mut chain) = effects_cap.try_lock() {
                chain.process(&mut scratch, SAMPLE_RATE);
            }

            // Post-effects RMS for the UI level meter (lock-free store).
            if !scratch.is_empty() {
                let sum_sq: f32 = scratch.iter().map(|s| s * s).sum();
                let rms = (sum_sq / scratch.len() as f32).sqrt();
                mic_level_cb.store(rms.to_bits(), Ordering::Relaxed);
            }

            let mut bridge = bridge_cap.borrow_mut();
            bridge.extend(scratch.iter().copied());
            while bridge.len() > MIC_CAP {
                bridge.pop_front();
            }
        })
        .register()
        .map_err(|e| anyhow!("Mic capture listener: {e:?}"))?;

    // Only connect when we have a real mic target — never autoconnect to the
    // default, which may be our own virtual source.
    if mic_target.is_some() {
        let cap_fmt = build_format_pod()?;
        let mut cap_params = [Pod::from_bytes(&cap_fmt).ok_or_else(|| anyhow!("Bad POD"))?];
        mic_capture
            .connect(
                Direction::Input,
                None,
                pw::stream::StreamFlags::AUTOCONNECT
                    | pw::stream::StreamFlags::MAP_BUFFERS
                    | pw::stream::StreamFlags::RT_PROCESS,
                &mut cap_params,
            )
            .map_err(|e| anyhow!("Mic capture connect: {e:?}"))?;
    }

    // Playback stream: processed mic → Resonate sink (PipeWire sums it with the
    // soundboard stream there).
    let mut mic_play_props = properties! {
        *pw::keys::MEDIA_TYPE       => "Audio",
        *pw::keys::MEDIA_CATEGORY   => "Playback",
        *pw::keys::MEDIA_ROLE       => "Production",
        *pw::keys::NODE_NAME        => "resonate-mic",
        *pw::keys::NODE_DESCRIPTION => "Resonate Mic",
        *pw::keys::APP_NAME         => "Resonate",
    };
    mic_play_props.insert("target.object", SINK_NAME);
    mic_play_props.insert("node.target", SINK_NAME);
    let mic_playback = pw::stream::StreamBox::new(&core, "resonate-mic", mic_play_props)
        .map_err(|e| anyhow!("Mic playback stream: {e:?}"))?;

    let bridge_play = mic_bridge.clone();
    let _mic_play_listener = mic_playback
        .add_local_listener_with_user_data(())
        .process(move |stream, _| {
            let Some(mut buf) = stream.dequeue_buffer() else { return };
            let datas = buf.datas_mut();
            let d = &mut datas[0];
            let stride = std::mem::size_of::<f32>() * CHANNELS;
            if let Some(slice) = d.data() {
                let n_frames = slice.len() / stride;
                let n_samples = n_frames * CHANNELS;
                let out = unsafe {
                    std::slice::from_raw_parts_mut(slice.as_mut_ptr() as *mut f32, n_samples)
                };
                let mut bridge = bridge_play.borrow_mut();
                let mut filled = 0usize;
                for sample in out.iter_mut() {
                    match bridge.pop_front() {
                        Some(s) => {
                            *sample = s;
                            filled += 1;
                        }
                        None => break,
                    }
                }
                for sample in out.iter_mut().skip(filled) {
                    *sample = 0.0;
                }
                let chunk = d.chunk_mut();
                *chunk.offset_mut() = 0;
                *chunk.stride_mut() = stride as i32;
                *chunk.size_mut() = (stride * n_frames) as u32;
            }
        })
        .register()
        .map_err(|e| anyhow!("Mic playback listener: {e:?}"))?;

    let mic_fmt = build_format_pod()?;
    let mut mic_params = [Pod::from_bytes(&mic_fmt).ok_or_else(|| anyhow!("Bad POD"))?];
    mic_playback
        .connect(
            Direction::Output,
            None,
            pw::stream::StreamFlags::AUTOCONNECT
                | pw::stream::StreamFlags::MAP_BUFFERS
                | pw::stream::StreamFlags::RT_PROCESS,
            &mut mic_params,
        )
        .map_err(|e| anyhow!("Mic playback connect: {e:?}"))?;

    // Control channel.
    let ml = mainloop.clone();
    let state_ctrl = state.clone();
    let _ctrl = ctrl_rx.attach(mainloop.loop_(), move |cmd| match cmd {
        PwCtrl::Terminate => ml.quit(),
        PwCtrl::Flush => state_ctrl.borrow_mut().flush_pending = true,
    });

    mainloop.run();
    Ok(())
}

// ── Node enumeration ──────────────────────────────────────────────────────────

pub fn enumerate_nodes() -> Vec<AudioNode> {
    use pipewire as pw;

    pw::init();

    let nodes = Arc::new(Mutex::new(Vec::<AudioNode>::new()));
    let nodes_w = nodes.clone();

    let result = std::panic::catch_unwind(move || {
        let ml = pw::main_loop::MainLoopRc::new(None).ok()?;
        let ctx = pw::context::ContextRc::new(&ml, None).ok()?;
        let core = ctx.connect_rc(None).ok()?;
        let registry = core.get_registry().ok()?;

        let _reg_listener = registry
            .add_listener_local()
            .global(move |global| {
                if global.type_ != pw::types::ObjectType::Node {
                    return;
                }
                let Some(props) = global.props.as_ref() else { return };
                let class = props.get("media.class").unwrap_or("");
                // Keep every node that has a name (callers filter by class). The
                // loopback's Stream/* nodes must be visible for mic-handoff teardown.
                let name = props.get("node.name").unwrap_or("").to_string();
                if name.is_empty() {
                    return;
                }
                let desc = props
                    .get("node.description")
                    .or_else(|| props.get("node.name"))
                    .unwrap_or("Unknown")
                    .to_string();
                if let Ok(mut v) = nodes_w.lock() {
                    v.push(AudioNode {
                        id: global.id,
                        name,
                        media_class: class.to_string(),
                        description: desc,
                    });
                }
            })
            .register();

        let done = std::rc::Rc::new(std::cell::Cell::new(false));
        let done2 = done.clone();
        let ml2 = ml.clone();
        let _pending = core.sync(0).ok()?;
        let _core_listener = core
            .add_listener_local()
            .done(move |_, _| {
                done2.set(true);
                ml2.quit();
            })
            .register();

        ml.run();
        Some(())
    });

    if result.is_err() {
        log::warn!("enumerate_nodes: PipeWire roundtrip panicked");
    }

    Arc::try_unwrap(nodes)
        .unwrap_or_default()
        .into_inner()
        .unwrap_or_default()
}
