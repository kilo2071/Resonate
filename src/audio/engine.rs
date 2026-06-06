use anyhow::Result;
use rodio::{Decoder, OutputStream, OutputStreamHandle, Sink, Source};
use std::io::BufReader;
use std::path::Path;
use std::time::Duration;

pub struct AudioEngine {
    _stream: OutputStream,
    stream_handle: OutputStreamHandle,
    sink: Option<Sink>,
    current_name: Option<String>,
    total_duration: Option<Duration>,
}

impl AudioEngine {
    pub fn new() -> Result<Self> {
        let (_stream, stream_handle) = OutputStream::try_default()
            .map_err(|e| anyhow::anyhow!("Failed to open audio output: {}", e))?;
        Ok(Self {
            _stream,
            stream_handle,
            sink: None,
            current_name: None,
            total_duration: None,
        })
    }

    pub fn play(&mut self, path: &Path, name: &str) -> Result<()> {
        self.sink.take();

        let file = std::fs::File::open(path)?;
        let source = Decoder::new(BufReader::new(file))
            .map_err(|e| anyhow::anyhow!("Failed to decode '{}': {}", path.display(), e))?;
        let total = source.total_duration();

        let sink = Sink::try_new(&self.stream_handle)
            .map_err(|e| anyhow::anyhow!("Failed to create audio sink: {}", e))?;
        sink.append(source);

        self.sink = Some(sink);
        self.current_name = Some(name.to_string());
        self.total_duration = total;
        Ok(())
    }

    pub fn stop(&mut self) {
        self.sink.take();
        self.current_name = None;
        self.total_duration = None;
    }

    pub fn pause_or_resume(&mut self) {
        if let Some(sink) = &self.sink {
            if sink.is_paused() {
                sink.play();
            } else {
                sink.pause();
            }
        }
    }

    /// Returns (fraction 0..1, optional remaining secs, sound name) when playing.
    pub fn playback_progress(&self) -> Option<(f64, Option<u32>, String)> {
        let sink = self.sink.as_ref()?;
        if sink.empty() {
            return None;
        }
        let elapsed = sink.get_pos();
        let name = self.current_name.clone().unwrap_or_default();

        if let Some(total) = self.total_duration {
            let frac = if total.as_secs_f64() > 0.0 {
                (elapsed.as_secs_f64() / total.as_secs_f64()).clamp(0.0, 1.0)
            } else {
                0.0
            };
            let remaining = total.as_secs().saturating_sub(elapsed.as_secs()) as u32;
            Some((frac, Some(remaining), name))
        } else {
            Some((0.0, None, name))
        }
    }

    pub fn is_finished(&self) -> bool {
        self.sink.as_ref().map(|s| s.empty()).unwrap_or(true)
    }
}
