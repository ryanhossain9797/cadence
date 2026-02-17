use anyhow::{Context, Result};
use rodio::{Decoder, OutputStream, OutputStreamHandle, Sink, Source};
use serde::Serialize;
use std::time::Instant;
use std::{fs::File, io::BufReader, path::Path};

#[derive(Debug, Clone, Serialize)]
pub struct TrackInfo {
    pub path: String,
    pub duration_ms: Option<u64>,
}

pub struct Player {
    _stream: OutputStream,
    _handle: OutputStreamHandle,
    sink: Sink,
    last_playback_time: Option<Instant>,
    last_playback_position: u64,
}

impl Player {
    pub fn new() -> Result<Self> {
        let (stream, handle) =
            OutputStream::try_default().context("No default output device available")?;
        let sink = Sink::try_new(&handle).context("Failed to create sink")?;
        Ok(Self {
            _stream: stream,
            _handle: handle,
            sink,
            last_playback_time: None,
            last_playback_position: 0,
        })
    }

    pub fn current_position_ms(&self) -> u64 {
        let last_playback_position = self.last_playback_position;
        match self.last_playback_time {
            Some(instant) => last_playback_position + instant.elapsed().as_millis() as u64,
            None => last_playback_position, // paused
        }
    }

    pub fn load_and_play<P: AsRef<Path>>(&mut self, path: P) -> Result<TrackInfo> {
        self.last_playback_position = 0;
        self.last_playback_time = Some(Instant::now());
        let p = path.as_ref();

        // Open once for duration using the same decoder we’ll use for playback.
        let f1 = File::open(p).with_context(|| format!("Failed to open {:?}", p))?;
        let src = Decoder::new(BufReader::new(f1))
            .with_context(|| format!("Unsupported/invalid audio: {:?}", p))?;
        let dur = src.total_duration().map(|d| d.as_millis() as u64);

        self.sink.clear();
        self.sink.append(src);
        self.sink.play();

        Ok(TrackInfo {
            path: p.to_string_lossy().into_owned(),
            duration_ms: dur,
        })
    }

    pub fn pause(&mut self) {
        self.last_playback_position = self.current_position_ms();
        self.last_playback_time = None;
        self.sink.pause();
    }
    pub fn resume(&mut self) {
        self.last_playback_time = Some(Instant::now());
        self.sink.play();
    }

    pub fn stop(&self) {
        self.sink.stop();
    }

    /// Naive “seek”: stops + re-queues from an offset by skipping samples (approx).
    /// This is placeholder until we switch to a decoder with random access control.
    pub fn seek_approx<P: AsRef<Path>>(&mut self, path: P, to_ms: u64) -> Result<()> {
        use std::time::Duration;

        let path = path.as_ref();

        // Open once to query total duration
        let file = File::open(path)?;
        let src = Decoder::new(BufReader::new(file))?;
        let to = Duration::from_millis(to_ms);

        if let Some(total) = src.total_duration() {
            if to >= total {
                // Seeking past EOF: just stop.
                self.stop();
                return Ok(());
            }
        }

        let skipped = src.skip_duration(to); // returns a Source wrapper, not a Duration

        self.sink.clear();
        self.sink.append(skipped);
        self.sink.play();
        self.last_playback_position = to_ms;
        self.last_playback_time = Some(Instant::now());

        Ok(())
    }

    pub fn advance_or_rewind<P: AsRef<Path>>(&mut self, path: P, delta_ms: i64) -> Result<()> {
        let current = self.current_position_ms() as i64;
        let target = (current + delta_ms).max(0) as u64;
        self.seek_approx(path, target)
    }
}
