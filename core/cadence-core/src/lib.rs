use anyhow::{Context, Result};
use parking_lot::Mutex;
use rodio::{buffer::SamplesBuffer, Decoder, OutputStream, OutputStreamHandle, Sink, Source};
use serde::Serialize;
use symphonia::core::{
    audio::SampleBuffer,
    codecs::DecoderOptions,
    formats::FormatOptions,
    io::MediaSourceStream,
    meta::MetadataOptions,
    probe::Hint,
};
use std::{fs::File, io::BufReader, path::Path, sync::Arc};

#[derive(Debug, Clone, Serialize)]
pub struct TrackInfo {
    pub path: String,
    pub duration_ms: Option<u64>, 
}

pub struct Player {
    _stream: OutputStream,
    _handle: OutputStreamHandle,
    sink: Arc<Mutex<Sink>>,
}

impl Player {
    pub fn new_default() -> Result<Self> {
        let (_stream, handle) = OutputStream::try_default()
            .context("No default output device available")?;
        let sink = Sink::try_new(&handle).context("Failed to create sink")?;
        Ok(Self { _stream, _handle: handle, sink: Arc::new(Mutex::new(sink)) })
    }

    pub fn load_and_play<P: AsRef<Path>>(&self, path: P) -> Result<TrackInfo> {
        let p = path.as_ref();

        // Open once for duration using the same decoder we’ll use for playback.
        let f1 = File::open(p).with_context(|| format!("Failed to open {:?}", p))?;
        let src1 = Decoder::new(BufReader::new(f1))
            .with_context(|| format!("Unsupported/invalid audio: {:?}", p))?;
        let dur = src1.total_duration().map(|d| d.as_millis() as u64);
        drop(src1);

        // Re-open and append the same type of decoder.
        let f2 = File::open(p)?;
        let src2 = Decoder::new(BufReader::new(f2))
            .with_context(|| format!("Unsupported/invalid audio: {:?}", p))?;

        let sink = self.sink.lock();
        sink.stop();
        sink.append(src2);
        sink.play();

        Ok(TrackInfo {
            path: p.to_string_lossy().into_owned(),
            duration_ms: dur,
        })
    }

    pub fn load_and_play_symphonia<P: AsRef<Path>>(&self, path: P) -> Result<TrackInfo> {
        let p = path.as_ref();
        let file = File::open(p).with_context(|| format!("open {:?}", p))?;
        let mss = MediaSourceStream::new(Box::new(file), Default::default());

        // Hint by extension if present.
        let mut hint = Hint::new();
        if let Some(ext) = p.extension().and_then(|e| e.to_str()) {
            hint.with_extension(ext);
        }

        // Probe + demux.
        let probed = symphonia::default::get_probe()
            .format(&hint, mss, &FormatOptions::default(), &MetadataOptions::default())
            .context("probe format")?;
        let mut format = probed.format;

        // Choose the default audio track, clone its parameters, and make a decoder.
        let (track_id, codec_params) = {
            let track = format.default_track().context("no audio track")?;
            (track.id, track.codec_params.clone())
        };
        let mut decoder = symphonia::default::get_codecs()
            .make(&codec_params, &DecoderOptions::default())?;

        // We'll accumulate interleaved f32 PCM here.
        let mut pcm: Vec<f32> = Vec::new();
        let mut sr = codec_params.sample_rate.unwrap_or(48_000);
        let mut ch = codec_params.channels.map(|c| c.count()).unwrap_or(2);

        loop {
            let pkt = match format.next_packet() {
                Ok(p) => p,
                Err(symphonia::core::errors::Error::ResetRequired) => { decoder.reset(); continue; }
                Err(symphonia::core::errors::Error::IoError(e))
                    if e.kind() == std::io::ErrorKind::UnexpectedEof => break,
                Err(e) => return Err(e.into()),
            };

            if pkt.track_id() != track_id { continue; }

            match decoder.decode(&pkt) {
                Ok(decoded) => {
                    // Update stream properties from the decoded buffer.
                    sr = decoded.spec().rate;
                    ch = decoded.spec().channels.count();

                    // Make a SampleBuffer (interleaved target) and copy into it.
                    let frames = decoded.frames() as u64;
                    let mut buf = SampleBuffer::<f32>::new(frames, *decoded.spec());
                    buf.copy_interleaved_ref(decoded);
                    pcm.extend_from_slice(buf.samples());
                }
                Err(symphonia::core::errors::Error::DecodeError(_)) => continue, // tolerate bad frames
                Err(e) => return Err(e.into()),
            }
        }

        let duration_ms = if ch > 0 && sr > 0 {
            Some((pcm.len() as u64 * 1000) / (sr as u64 * ch as u64))
        } else {
            None
        };

        // Play the interleaved buffer via rodio.
        let sink = self.sink.lock();
        sink.stop();
        sink.append(SamplesBuffer::new(ch as u16, sr, pcm));
        sink.play();

        Ok(TrackInfo {
            path: p.to_string_lossy().into_owned(),
            duration_ms,
        })
    }

    pub fn pause(&self) { self.sink.lock().pause(); }
    pub fn resume(&self) { self.sink.lock().play(); }
    pub fn stop(&self) { self.sink.lock().stop(); }

    /// Naive “seek”: stops + re-queues from an offset by skipping samples (approx).
    /// This is placeholder until we switch to a decoder with random access control.
    pub fn seek_approx<P: AsRef<Path>>(&self, path: P, to_ms: u64) -> Result<()> {
        use std::time::Duration;

        let path = path.as_ref();

        // Open once to query total duration
        let file = File::open(path)?;
        let src0 = Decoder::new(BufReader::new(file))?;
        let to = Duration::from_millis(to_ms);

        if let Some(total) = src0.total_duration() {
            if to >= total {
                // Seeking past EOF: just stop.
                self.stop();
                return Ok(());
            }
        }
        drop(src0); // close before reopening

        // Reopen and build the skipped stream
        let file = File::open(path)?;
        let src = Decoder::new(BufReader::new(file))?;
        let skipped = src.skip_duration(to); // returns a Source wrapper, not a Duration

        let sink = self.sink.lock();
        sink.stop();
        sink.append(skipped);
        sink.play();

        Ok(())
    }

    pub fn sleep_until_end(&self) {
        self.sink.lock().sleep_until_end();
    }
}
