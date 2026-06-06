use anyhow::Result;
use rodio::{Decoder, OutputStream, OutputStreamHandle, Sink, Source};
use std::io::BufReader;
use std::path::{Path, PathBuf};
use std::time::Duration;

pub struct SoundInfo {
    pub name: String,
    pub fraction: f64,
    pub remaining_secs: Option<u32>,
}

struct PlayingSound {
    name: String,
    path: PathBuf,
    sink: Sink,
    total_duration: Option<Duration>,
}

struct QueuedSound {
    name: String,
    path: PathBuf,
    total_secs: Option<u32>,
}

pub struct AudioEngine {
    _stream: OutputStream,
    stream_handle: OutputStreamHandle,
    playing: Vec<PlayingSound>,
    queue: Vec<QueuedSound>,
    polyphonic: bool,
}

fn probe_duration(path: &Path) -> Option<Duration> {
    let file = std::fs::File::open(path).ok()?;
    Decoder::new(BufReader::new(file)).ok()?.total_duration()
}

impl AudioEngine {
    pub fn new() -> Result<Self> {
        let (_stream, stream_handle) = OutputStream::try_default()
            .map_err(|e| anyhow::anyhow!("Failed to open audio output: {}", e))?;
        Ok(Self {
            _stream,
            stream_handle,
            playing: Vec::new(),
            queue: Vec::new(),
            polyphonic: true,
        })
    }

    pub fn set_polyphonic(&mut self, v: bool) {
        self.polyphonic = v;
    }

    pub fn polyphonic(&self) -> bool {
        self.polyphonic
    }

    pub fn play(&mut self, path: &Path, name: &str) -> Result<()> {
        if self.polyphonic || self.playing.is_empty() {
            self.start_sound(path, name)?;
        } else {
            let total_secs = probe_duration(path).map(|d| d.as_secs() as u32);
            self.queue.push(QueuedSound {
                name: name.to_string(),
                path: path.to_path_buf(),
                total_secs,
            });
        }
        Ok(())
    }

    /// Always append to queue regardless of polyphonic mode.
    pub fn cue(&mut self, path: &Path, name: &str) {
        let total_secs = probe_duration(path).map(|d| d.as_secs() as u32);
        self.queue.push(QueuedSound {
            name: name.to_string(),
            path: path.to_path_buf(),
            total_secs,
        });
    }

    fn start_sound(&mut self, path: &Path, name: &str) -> Result<()> {
        let file = std::fs::File::open(path)?;
        let source = Decoder::new(BufReader::new(file))
            .map_err(|e| anyhow::anyhow!("Decode '{}': {}", path.display(), e))?;
        let total = source.total_duration();
        let sink = Sink::try_new(&self.stream_handle)
            .map_err(|e| anyhow::anyhow!("Create sink: {}", e))?;
        sink.append(source);
        self.playing.push(PlayingSound {
            name: name.to_string(),
            path: path.to_path_buf(),
            sink,
            total_duration: total,
        });
        Ok(())
    }

    /// Call every timer tick. Returns (playing_info, queue_info).
    /// queue_info entries: (name, total_duration_secs)
    pub fn tick(&mut self) -> (Vec<SoundInfo>, Vec<(String, Option<u32>)>) {
        self.playing.retain(|s| !s.sink.empty());

        if self.playing.is_empty() {
            while !self.queue.is_empty() {
                let next = self.queue.remove(0);
                if let Err(e) = self.start_sound(&next.path, &next.name) {
                    log::error!("Failed to start queued sound '{}': {}", next.name, e);
                    continue;
                }
                break;
            }
        }

        let playing_info: Vec<SoundInfo> = self.playing.iter().map(|s| {
            let elapsed = s.sink.get_pos();
            if let Some(total) = s.total_duration {
                let frac = if total.as_secs_f64() > 0.0 {
                    (elapsed.as_secs_f64() / total.as_secs_f64()).clamp(0.0, 1.0)
                } else {
                    0.0
                };
                let remaining = total.as_secs().saturating_sub(elapsed.as_secs()) as u32;
                SoundInfo { name: s.name.clone(), fraction: frac, remaining_secs: Some(remaining) }
            } else {
                SoundInfo { name: s.name.clone(), fraction: 0.0, remaining_secs: None }
            }
        }).collect();

        let queue_info: Vec<(String, Option<u32>)> = self.queue.iter()
            .map(|q| (q.name.clone(), q.total_secs))
            .collect();

        (playing_info, queue_info)
    }

    pub fn stop_all(&mut self) {
        self.playing.clear();
        self.queue.clear();
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

    /// Stop all playing sounds then start the next item from the queue.
    pub fn skip_queue(&mut self) {
        self.playing.clear();
        if !self.queue.is_empty() {
            let next = self.queue.remove(0);
            if let Err(e) = self.start_sound(&next.path, &next.name) {
                log::error!("Skip failed to start '{}': {}", next.name, e);
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
