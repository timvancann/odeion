//! The waveform for the practice view (`P`): read in the background whenever a song
//! starts, kept in memory for that song only, and summarised to one column per pixel
//! at any zoom.

use crate::audio::Loop;
use std::fs::File;
use std::path::Path;
use std::time::Duration;
use symphonia::core::audio::SampleBuffer;
use symphonia::core::codecs::DecoderOptions;
use symphonia::core::formats::{FormatOptions, FormatReader, SeekMode, SeekTo};
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;
use symphonia::core::units::Time;

/// Audio frames per point: about 3 ms at 44.1 kHz, fine enough for the closest zoom.
pub const STEP: usize = 128;
/// Threads reading one song, each from its own stretch: a song in ~30 ms, not ~200.
const THREADS: usize = 8;
/// Each thread starts this much early and throws it away: an mp3 decoder needs a little
/// history after a seek before its output is right.
const WARM_UP: f64 = 1.0;

/// One point or column: the loudest sample and the mean square (for RMS), mono.
#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub struct Point {
    pub peak: f32,
    pub square: f32,
}

impl Point {
    pub fn rms(&self) -> f32 {
        self.square.sqrt()
    }
}

#[derive(Debug, Default, Clone, PartialEq)]
pub struct Wave {
    pub rate: u32,
    pub points: Vec<Point>,
}

impl Wave {
    fn seconds_per_point(&self) -> f64 {
        STEP as f64 / self.rate.max(1) as f64
    }

    pub fn length(&self) -> Duration {
        Duration::from_secs_f64(self.points.len() as f64 * self.seconds_per_point())
    }

    /// The loudest point's RMS, to scale the drawing by: RMS, not peaks, because a loud
    /// master's peaks all reach the top and hide its transients.
    pub fn loudest(&self) -> f32 {
        self.points.iter().map(|p| p.rms()).fold(0.0, f32::max)
    }

    /// `width` columns spanning `from` to `to`: each the loudest peak and the mean square
    /// of the points under it. Columns past the end are silent.
    pub fn columns(&self, from: Duration, to: Duration, width: usize) -> Vec<Point> {
        let per = self.seconds_per_point();
        let (a, b) = (from.as_secs_f64() / per, to.as_secs_f64() / per);
        (0..width)
            .map(|x| {
                let lo = (a + (b - a) * x as f64 / width as f64) as usize;
                let hi = ((a + (b - a) * (x + 1) as f64 / width as f64) as usize).max(lo + 1);
                let under = &self.points[lo.min(self.points.len())..hi.min(self.points.len())];
                if under.is_empty() {
                    return Point::default();
                }
                Point {
                    peak: under.iter().map(|p| p.peak).fold(0.0, f32::max),
                    square: under.iter().map(|p| p.square).sum::<f32>() / under.len() as f32,
                }
            })
            .collect()
    }
}

/// The stretch the detail view shows. With a loop, `span` centred on it; otherwise `span`
/// with the playhead a quarter in. Kept inside the song where it fits.
pub fn window(at: Duration, looping: Loop, span: Duration, song: Duration) -> (Duration, Duration) {
    let span = span.as_secs_f64();
    let from = match looping.end {
        Some(end) => {
            let start = looping.start.unwrap_or_default().as_secs_f64();
            (start + end.as_secs_f64()) / 2.0 - span / 2.0
        }
        None => at.as_secs_f64() - span / 4.0,
    };
    let from = from.min(song.as_secs_f64() - span).max(0.0);
    (Duration::from_secs_f64(from), Duration::from_secs_f64(from + span))
}

/// How much the detail view shows for a loop of `length`: `span` stays while the loop
/// fits with room and fills a fair part of it, so nudging doesn't make the zoom jitter;
/// otherwise the loop with 30% room.
pub fn fit(span: Duration, length: Duration) -> Duration {
    let (span, length) = (span.as_secs_f64(), length.as_secs_f64());
    if length * 1.1 <= span && length >= span * 0.4 {
        Duration::from_secs_f64(span)
    } else {
        Duration::from_secs_f64((length * 1.3).max(0.5))
    }
}

fn open(path: &Path) -> Result<Box<dyn FormatReader>, String> {
    let file = File::open(path).map_err(|e| e.to_string())?;
    let mut hint = Hint::new();
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        hint.with_extension(ext);
    }
    let stream = MediaSourceStream::new(Box::new(file), Default::default());
    symphonia::default::get_probe()
        .format(&hint, stream, &FormatOptions::default(), &MetadataOptions::default())
        .map(|p| p.format)
        .map_err(|e| e.to_string())
}

/// Reads a song's waveform, several stretches at once when its length is known and its
/// packets carry timestamps to place them by.
pub fn read(path: &Path) -> Result<Wave, String> {
    let format = open(path)?;
    let params = &format.default_track().ok_or("no audio track")?.codec_params;
    let rate = params.sample_rate.ok_or("unknown sample rate")?;
    let points = match params.n_frames.filter(|_| params.time_base.is_some()) {
        Some(frames) => {
            let total = (frames as usize).div_ceil(STEP);
            // Stretch boundaries fall on whole points, so no point is split between threads.
            let bounds: Vec<usize> = (0..=THREADS).map(|i| total * i / THREADS).collect();
            let parts = std::thread::scope(|s| {
                let handles: Vec<_> = bounds
                    .windows(2)
                    .map(|w| s.spawn(move || stretch(path, rate, w[0], Some(w[1]))))
                    .collect();
                handles
                    .into_iter()
                    .map(|h| h.join().map_err(|_| "a reader failed".to_string()).and_then(|r| r))
                    .collect::<Result<Vec<_>, _>>()
            })?;
            parts.concat()
        }
        None => stretch(path, rate, 0, None)?,
    };
    Ok(Wave { rate, points })
}

/// Points `first` up to `last` (or the end) of one song.
fn stretch(path: &Path, rate: u32, first: usize, last: Option<usize>) -> Result<Vec<Point>, String> {
    let mut format = open(path)?;
    let track = format.default_track().ok_or("no audio track")?.clone();
    let mut decoder = symphonia::default::get_codecs()
        .make(&track.codec_params, &DecoderOptions::default())
        .map_err(|e| e.to_string())?;
    let (lo, hi) = (first * STEP, last.map_or(usize::MAX, |l| l * STEP));
    if lo > 0 {
        let from = (lo as f64 / rate as f64 - WARM_UP).max(0.0);
        let to = SeekTo::Time { time: Time::from(from), track_id: Some(track.id) };
        format.seek(SeekMode::Accurate, to).map_err(|e| e.to_string())?;
        decoder.reset();
    }
    // Each packet is placed by its own timestamp: where a seek says it landed isn't
    // reliable for every format (ogg).
    let time_base = track.codec_params.time_base;
    let mut frame = 0;
    let mut points = vec![Point::default(); last.map_or(0, |l| l - first)];
    let mut buffer: Option<SampleBuffer<f32>> = None;
    while frame < hi {
        let packet = match format.next_packet() {
            Ok(p) => p,
            Err(_) => break, // the end of the file
        };
        if packet.track_id() != track.id {
            continue;
        }
        // A damaged packet is skipped, as a player would.
        if let Some(tb) = time_base {
            let t = tb.calc_time(packet.ts());
            frame = ((t.seconds as f64 + t.frac) * rate as f64).round() as usize;
        }
        let Ok(decoded) = decoder.decode(&packet) else { continue };
        let spec = *decoded.spec();
        let channels = spec.channels.count();
        let needed = decoded.capacity() * channels;
        if buffer.as_ref().is_none_or(|b| b.capacity() < needed) {
            buffer = Some(SampleBuffer::new(decoded.capacity() as u64, spec));
        }
        let buffer = buffer.as_mut().expect("just made");
        buffer.copy_interleaved_ref(decoded);
        for samples in buffer.samples().chunks(channels) {
            if frame >= lo && frame < hi {
                let i = frame / STEP - first;
                if i >= points.len() {
                    points.resize(i + 1, Point::default());
                }
                let mono = samples.iter().sum::<f32>() / channels as f32;
                let p = &mut points[i];
                p.peak = p.peak.max(mono.abs());
                p.square += mono * mono / STEP as f32;
            }
            frame += 1;
        }
    }
    Ok(points)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(secs: f64) -> Duration {
        Duration::from_secs_f64(secs)
    }

    /// A mono 16-bit WAV of `seconds` silence with one full-scale click at `click`.
    fn wav_with_click(seconds: f64, click: f64) -> std::path::PathBuf {
        let rate = 44_100u32;
        let frames = (seconds * rate as f64) as usize;
        let at = (click * rate as f64) as usize;
        let mut data = Vec::with_capacity(frames * 2);
        for i in 0..frames {
            let v: i16 = if (at..at + 64).contains(&i) { i16::MAX } else { 0 };
            data.extend_from_slice(&v.to_le_bytes());
        }
        let mut out = Vec::new();
        out.extend_from_slice(b"RIFF");
        out.extend_from_slice(&(36 + data.len() as u32).to_le_bytes());
        out.extend_from_slice(b"WAVEfmt ");
        out.extend_from_slice(&16u32.to_le_bytes());
        out.extend_from_slice(&1u16.to_le_bytes()); // PCM
        out.extend_from_slice(&1u16.to_le_bytes()); // mono
        out.extend_from_slice(&rate.to_le_bytes());
        out.extend_from_slice(&(rate * 2).to_le_bytes());
        out.extend_from_slice(&2u16.to_le_bytes());
        out.extend_from_slice(&16u16.to_le_bytes());
        out.extend_from_slice(b"data");
        out.extend_from_slice(&(data.len() as u32).to_le_bytes());
        out.extend_from_slice(&data);
        let path = std::env::temp_dir().join(format!("riff-wave-{}-{click}.wav", std::process::id()));
        std::fs::write(&path, out).unwrap();
        path
    }

    #[test]
    fn a_click_lands_where_it_is_in_the_song() {
        // Eight stretches of 2.5 s: 7.3 s is inside the third, which a thread seeks to, and
        // not on a boundary.
        let path = wav_with_click(20.0, 7.3);
        let wave = read(&path).unwrap();
        let _ = std::fs::remove_file(&path);
        assert!((wave.length().as_secs_f64() - 20.0).abs() < 0.01);
        let loudest = wave.points.iter().position(|p| p.peak > 0.9).unwrap();
        let at = loudest as f64 * STEP as f64 / 44_100.0;
        assert!((at - 7.3).abs() < 0.005, "click read at {at}");
        assert_eq!(wave.points.iter().filter(|p| p.peak > 0.9).count(), 1);
    }

    #[test]
    fn columns_keep_the_loudest_peak_under_them() {
        let mut wave = Wave { rate: STEP as u32, points: vec![Point::default(); 100] };
        wave.points[42] = Point { peak: 1.0, square: 0.5 };
        let cols = wave.columns(s(0.0), s(100.0), 10);
        assert_eq!(cols.len(), 10);
        assert_eq!(cols[4].peak, 1.0);
        assert_eq!(cols[4].square, 0.05);
        assert!(cols.iter().enumerate().all(|(i, c)| i == 4 || c.peak == 0.0));
        assert_eq!(wave.columns(s(200.0), s(210.0), 3), vec![Point::default(); 3]);
    }

    #[test]
    fn the_zoom_refits_only_when_the_loop_outgrows_or_shrinks_well_below_it() {
        assert_eq!(fit(s(8.0), s(6.0)), s(8.0));
        assert_eq!(fit(s(8.0), s(6.05)), s(8.0));
        assert_eq!(fit(s(8.0), s(7.5)), s(7.5 * 1.3));
        assert_eq!(fit(s(8.0), s(2.0)), s(2.0 * 1.3));
        assert_eq!(fit(s(8.0), s(0.1)), s(0.5));
    }

    #[test]
    fn the_window_frames_the_loop_or_follows_the_playhead() {
        let looped = Loop { start: Some(s(10.0)), end: Some(s(14.0)) };
        assert_eq!(window(s(50.0), looped, s(8.0), s(100.0)), (s(8.0), s(16.0)));
        assert_eq!(window(s(50.0), Loop::default(), s(8.0), s(100.0)), (s(48.0), s(56.0)));
        // Near the start or end it stays inside the song.
        assert_eq!(window(s(1.0), Loop::default(), s(8.0), s(100.0)), (s(0.0), s(8.0)));
        assert_eq!(window(s(99.0), Loop::default(), s(8.0), s(100.0)), (s(92.0), s(100.0)));
    }
}
