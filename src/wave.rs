//! The waveform for the practice view (`p`): read in the background whenever a song
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
/// The most points read, 64 MB of them: over 6 hours at 44.1 kHz, 1.5 at 192. A damaged
/// or crafted file can claim any length, sample rate or timestamp; trusting it would size
/// the waveform to match and abort odeion. A file claiming more is read from its packets,
/// and stops here.
const MOST: usize = 1 << 23;

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

    /// Columns for `width` pixels showing `from` to `to`: each the loudest peak and the
    /// mean square of the points under it, silent past the end.
    ///
    /// Columns are anchored to the song, not the view: column `k` always covers the same
    /// audio, so a view sliding with the playhead moves columns without reshaping them
    /// (summarising from `from` made every frame group the points differently, and the
    /// waveform shimmered). Returns where the first column starts, in columns left of
    /// `from` (0 to 1), and one column more than fits, to cover the right edge.
    pub fn columns(&self, from: Duration, to: Duration, width: usize) -> (f64, Vec<Point>) {
        // Rounded, so float noise in the span can't move the boundaries between frames.
        let wide = ((to - from).as_secs_f64() / width.max(1) as f64 * 1e6).round() / 1e6;
        if wide <= 0.0 {
            return (0.0, Vec::new());
        }
        let first = (from.as_secs_f64() / wide).floor();
        let behind = from.as_secs_f64() / wide - first;
        let columns = (0..=width).map(|i| self.column((first as usize + i) as f64 * wide, wide));
        (behind, columns.collect())
    }

    /// The points from `start` for `wide` seconds, summarised.
    fn column(&self, start: f64, wide: f64) -> Point {
        let per = self.seconds_per_point();
        let lo = (start / per) as usize;
        let hi = (((start + wide) / per) as usize).max(lo + 1);
        let under = &self.points[lo.min(self.points.len())..hi.min(self.points.len())];
        if under.is_empty() {
            return Point::default();
        }
        Point {
            peak: under.iter().map(|p| p.peak).fold(0.0, f32::max),
            square: under.iter().map(|p| p.square).sum::<f32>() / under.len() as f32,
        }
    }
}

/// The stretch the detail view shows. With a loop that loops and the playhead in it,
/// `span` centred on the loop; otherwise (no loop, one switched off, or the playhead
/// outside it) `span` with the playhead a quarter in, so the playhead is always in
/// sight. Kept inside the song where it fits.
pub fn window(at: Duration, looping: Loop, span: Duration, song: Duration) -> (Duration, Duration) {
    // A little slack: the drawn playhead is smoothed and may run a hair past the end.
    const SLACK: f64 = 0.1;
    let (span, at) = (span.as_secs_f64(), at.as_secs_f64());
    let start = looping.start.unwrap_or_default().as_secs_f64();
    let from = match looping.end.filter(|_| looping.looping()).map(|e| e.as_secs_f64()) {
        Some(end) if start - SLACK <= at && at <= end + SLACK => (start + end) / 2.0 - span / 2.0,
        _ => at - span / 4.0,
    };
    inside(from, span, song)
}

/// The stretch the detail view shows once it is dragged away from the playhead: `span`
/// from `from`, kept inside the song. It stays there until it follows again.
pub fn moved(from: Duration, span: Duration, song: Duration) -> (Duration, Duration) {
    inside(from.as_secs_f64(), span.as_secs_f64(), song)
}

/// Where a view dragged on the overview starts: as far from `from` as the pointer has
/// moved (`grabbed` to `now`), kept inside the song.
pub fn pan(from: Duration, grabbed: Duration, now: Duration, span: Duration, song: Duration) -> Duration {
    let from = from.as_secs_f64() + now.as_secs_f64() - grabbed.as_secs_f64();
    inside(from, span.as_secs_f64(), song).0
}

/// Where a view held away from the playhead starts after a zoom from `span` to `to`:
/// stretched or shrunk about its middle, so what was in view stays in view.
pub fn zoomed(from: Duration, span: Duration, to: Duration, song: Duration) -> Duration {
    let middle = from.as_secs_f64() + span.as_secs_f64() / 2.0;
    inside(middle - to.as_secs_f64() / 2.0, to.as_secs_f64(), song).0
}

/// Where a view starts after a sideways scroll of `by` seconds (negative goes back),
/// kept inside the song.
pub fn scrolled(from: Duration, by: f64, span: Duration, song: Duration) -> Duration {
    inside(from.as_secs_f64() + by, span.as_secs_f64(), song).0
}

/// `span` seconds from `from`, moved inside the song where it fits.
fn inside(from: f64, span: f64, song: Duration) -> (Duration, Duration) {
    let from = from.min(song.as_secs_f64() - span).max(0.0);
    (Duration::from_secs_f64(from), Duration::from_secs_f64(from + span))
}

/// The span that frames a loop of `length`: the loop with 30% room.
pub fn frame(length: Duration) -> Duration {
    Duration::from_secs_f64((length.as_secs_f64() * 1.3).max(0.5))
}

/// The view after a jump to the section from `start` to `end`: its span and where it
/// starts, the section centred and about two thirds of it (Tim, 2026-11-11), kept inside
/// the song.
pub fn section_view(start: Duration, end: Duration, song: Duration) -> (Duration, Duration) {
    let most = song.max(Duration::from_millis(500));
    let span = end.saturating_sub(start).mul_f64(1.5).clamp(Duration::from_millis(500), most);
    let middle = (start + end).as_secs_f64() / 2.0;
    (span, inside(middle - span.as_secs_f64() / 2.0, span.as_secs_f64(), song).0)
}

/// The view held at `from` shows `at`.
pub fn shows(from: Duration, span: Duration, song: Duration, at: Duration) -> bool {
    let (from, to) = moved(from, span, song);
    from <= at && at <= to
}

/// A view held on a section after a jump, as the playhead moves.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Held {
    /// Not yet seen the playhead: right after the seek it may still read where it was.
    Waiting,
    /// The playhead has shown in it.
    Seen,
    /// The playhead has left it: follow again.
    Left,
}

impl Held {
    /// What the hold is after the playhead read `shown` in the view (or not).
    pub fn next(self, shown: bool) -> Held {
        match (self, shown) {
            (Held::Waiting, true) => Held::Seen,
            (Held::Seen, false) | (Held::Left, _) => Held::Left,
            (held, _) => held,
        }
    }
}

/// What ⌘0 shows: the loop framed with room, or without a loop the whole song. Gives
/// the span and, when following the playhead wouldn't show the loop (looping off, or
/// the playhead outside it), where the view has to move to.
pub fn zoom_fit(at: Duration, looping: Loop, song: Duration) -> (Duration, Option<Duration>) {
    let Some(end) = looping.end else { return (song, None) };
    let start = looping.start.unwrap_or_default();
    let span = frame(end - start);
    let framed = inside((start + end).as_secs_f64() / 2.0 - span.as_secs_f64() / 2.0, span.as_secs_f64(), song);
    let followed = window(at, looping, span, song);
    (span, (followed != framed).then_some(framed.0))
}

/// How much the detail view shows while there is a loop. The zoom stays as set, however
/// far out (nudges and drags leave it alone), and only grows when the loop no longer fits.
pub fn fit(span: Duration, length: Duration) -> Duration {
    if length.as_secs_f64() * 1.1 > span.as_secs_f64() { frame(length) } else { span }
}

/// A song opened for reading its waveform or beats. Gapless, as rodio plays it: an MP3's
/// encoder delay is left out, or the waveform and beats would run behind the music by it
/// (about 25 ms with LAME).
pub(crate) fn open(path: &Path) -> Result<Box<dyn FormatReader>, String> {
    let file = File::open(path).map_err(|e| e.to_string())?;
    let mut hint = Hint::new();
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        hint.with_extension(ext);
    }
    let stream = MediaSourceStream::new(Box::new(file), Default::default());
    let options = FormatOptions { enable_gapless: true, ..Default::default() };
    symphonia::default::get_probe()
        .format(&hint, stream, &options, &MetadataOptions::default())
        .map(|p| p.format)
        .map_err(|e| e.to_string())
}

/// Reads a song's waveform, several stretches at once when its length is known and its
/// packets carry timestamps to place them by.
pub fn read(path: &Path) -> Result<Wave, String> {
    let format = open(path)?;
    let params = &format.default_track().ok_or("no audio track")?.codec_params;
    let rate = params.sample_rate.filter(|&r| r > 0).ok_or("unknown sample rate")?;
    let claimed = params.n_frames.map(|n| n.div_ceil(STEP as u64));
    let points = match claimed.filter(|&n| params.time_base.is_some() && n <= MOST as u64) {
        Some(total) => {
            let total = total as usize;
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

/// The waveform of a stem set as heard summed, from each stem's: combined point by point
/// (the loudest peak; mean squares add, as uncorrelated parts do). One wave is itself.
pub fn mix(parts: &[Wave]) -> Wave {
    let mut parts = parts.iter();
    let Some(mut mix) = parts.next().cloned() else { return Wave::default() };
    for wave in parts {
        if wave.points.len() > mix.points.len() {
            mix.points.resize(wave.points.len(), Point::default());
        }
        for (m, p) in mix.points.iter_mut().zip(&wave.points) {
            m.peak = m.peak.max(p.peak);
            m.square += p.square;
        }
    }
    mix
}

/// Points `first` up to `last` (or the end) of one song.
fn stretch(path: &Path, rate: u32, first: usize, last: Option<usize>) -> Result<Vec<Point>, String> {
    let mut format = open(path)?;
    let track = format.default_track().ok_or("no audio track")?.clone();
    let mut decoder = symphonia::default::get_codecs()
        .make(&track.codec_params, &DecoderOptions::default())
        .map_err(|e| e.to_string())?;
    let (lo, hi) = (first * STEP, last.unwrap_or(MOST).min(MOST) * STEP);
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

    #[test]
    fn zoom_to_fit_frames_the_loop_or_shows_the_whole_song() {
        let song = s(200.);
        // No loop: the whole song, following as ever.
        assert_eq!(zoom_fit(s(50.), Loop::default(), song), (song, None));
        // A loop from 40 to 50 s with the playhead in it: framed, and following shows it.
        let looping = Loop { start: Some(s(40.)), end: Some(s(50.)), off: false };
        let (span, moved) = zoom_fit(s(45.), looping, song);
        assert!((span.as_secs_f64() - 13.).abs() < 1e-9);
        assert_eq!(moved, None);
        // The playhead elsewhere: the view moves onto the loop, centred.
        let (span, moved) = zoom_fit(s(120.), looping, song);
        assert!((span.as_secs_f64() - 13.).abs() < 1e-9);
        assert!((moved.unwrap().as_secs_f64() - 38.5).abs() < 1e-9);
        // Looping switched off: following wouldn't show it either.
        let off = Loop { off: true, ..looping };
        assert!(zoom_fit(s(45.), off, song).1.is_some());
    }

    #[test]
    fn a_section_jumped_to_is_centred_in_two_thirds_of_the_view() {
        let song = s(200.);
        let (span, from) = section_view(s(90.), s(110.), song);
        assert!((span.as_secs_f64() - 30.).abs() < 1e-9);
        assert!((from.as_secs_f64() - 85.).abs() < 1e-9);
        // At the song's start or end it stays inside the song, the section still whole.
        for start in [0., 1., 180.] {
            let (span, from) = section_view(s(start), s(start + 20.), song);
            assert!(shows(from, span, song, s(start)) && shows(from, span, song, s(start + 20.)), "section at {start}");
        }
        // Played past its end, the view no longer shows the playhead.
        assert!(!shows(from, span, song, s(116.)));
        // The hold waits for the playhead to show (a seek may still read the old spot),
        // then lets go when it leaves.
        let held = Held::Waiting.next(shows(from, span, song, s(10.)));
        assert_eq!(held, Held::Waiting);
        let held = held.next(shows(from, span, song, s(90.)));
        assert_eq!(held, Held::Seen);
        assert_eq!(held.next(shows(from, span, song, s(110.))), Held::Seen);
        assert_eq!(held.next(shows(from, span, song, s(116.))), Held::Left);
        // Never smaller than half a second, nor past the song.
        assert_eq!(section_view(s(10.), s(10.1), song).0, s(0.5));
        assert_eq!(section_view(s(10.), s(190.), song), (song, Duration::ZERO));
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
        let path = std::env::temp_dir().join(format!("odeion-wave-{}-{click}.wav", std::process::id()));
        std::fs::write(&path, out).unwrap();
        path
    }

    #[test]
    fn a_file_claiming_to_be_endless_is_read_for_what_it_holds() {
        // The header says 4 GB of samples (over 13 hours, 2^24 points); the file has 2 s.
        let path = wav_with_click(2.0, 1.0);
        let mut bytes = std::fs::read(&path).unwrap();
        bytes[4..8].copy_from_slice(&u32::MAX.to_le_bytes());
        bytes[40..44].copy_from_slice(&(u32::MAX - 36).to_le_bytes());
        std::fs::write(&path, bytes).unwrap();
        let wave = read(&path).unwrap();
        let _ = std::fs::remove_file(&path);
        assert!((wave.length().as_secs_f64() - 2.0).abs() < 0.01, "read {:?}", wave.length());
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
        let (behind, cols) = wave.columns(s(0.0), s(100.0), 10);
        assert_eq!((behind, cols.len()), (0.0, 11));
        assert_eq!(cols[4].peak, 1.0);
        assert_eq!(cols[4].square, 0.05);
        assert!(cols.iter().enumerate().all(|(i, c)| i == 4 || c.peak == 0.0));
        assert_eq!(wave.columns(s(200.0), s(210.0), 3).1, vec![Point::default(); 4]);
    }

    #[test]
    fn a_sliding_view_moves_columns_without_reshaping_them() {
        let points = (0..1000).map(|i| Point { peak: (i % 7) as f32, square: (i % 13) as f32 }).collect();
        let wave = Wave { rate: STEP as u32, points };
        // A 100 s view over 40 columns, slid by a fraction of a column at a time.
        let (_, still) = wave.columns(s(10.0), s(110.0), 40);
        for slide in [0.3, 1.1, 2.5, 7.49] {
            let (behind, moved) = wave.columns(s(10.0 + slide), s(110.0 + slide), 40);
            // Columns are 2.5 s, numbered from the start of the song.
            let whole = ((10.0 + slide) / 2.5) as usize - (10.0 / 2.5) as usize;
            assert_eq!(moved[..30], still[whole..whole + 30], "slid {slide}");
            assert!((0.0..1.0).contains(&behind));
        }
    }

    #[test]
    fn the_zoom_stays_as_set_and_only_grows_when_the_loop_outgrows_it() {
        assert_eq!(fit(s(8.0), s(6.0)), s(8.0));
        assert_eq!(fit(s(8.0), s(6.05)), s(8.0));
        // Zoomed far out around a short loop: left alone.
        assert_eq!(fit(s(60.0), s(2.0)), s(60.0));
        assert_eq!(fit(s(8.0), s(7.5)), s(7.5 * 1.3));
        assert_eq!(frame(s(2.0)), s(2.0 * 1.3));
        assert_eq!(frame(s(0.1)), s(0.5));
    }

    #[test]
    fn stems_mix_into_one_wave() {
        let one = Wave { rate: 100, points: vec![Point { peak: 0.5, square: 0.1 }; 3] };
        let two = Wave { rate: 100, points: vec![Point { peak: 0.8, square: 0.2 }; 4] };
        let m = mix(&[one, two]);
        assert_eq!(m.points.len(), 4);
        assert_eq!(m.points[0], Point { peak: 0.8, square: 0.3 });
        assert_eq!(m.points[3], Point { peak: 0.8, square: 0.2 });
    }

    #[test]
    fn the_window_frames_the_loop_or_follows_the_playhead() {
        let looped = Loop { start: Some(s(10.0)), end: Some(s(14.0)), off: false };
        assert_eq!(window(s(12.0), looped, s(8.0), s(100.0)), (s(8.0), s(16.0)));
        // The playhead outside the loop is followed, so it is never out of sight.
        assert_eq!(window(s(50.0), looped, s(8.0), s(100.0)), (s(48.0), s(56.0)));
        assert_eq!(window(s(50.0), Loop::default(), s(8.0), s(100.0)), (s(48.0), s(56.0)));
        // A loop switched off is left behind: the view follows the playhead.
        let off = Loop { off: true, ..looped };
        assert_eq!(window(s(50.0), off, s(8.0), s(100.0)), (s(48.0), s(56.0)));
        // Near the start or end it stays inside the song.
        assert_eq!(window(s(1.0), Loop::default(), s(8.0), s(100.0)), (s(0.0), s(8.0)));
        assert_eq!(window(s(99.0), Loop::default(), s(8.0), s(100.0)), (s(92.0), s(100.0)));
    }

    #[test]
    fn a_held_view_zooms_about_its_middle_and_scrolls_sideways() {
        // 20 to 28 s shown, zoomed in to 4 s: 22 to 26, the middle kept at 24.
        assert_eq!(zoomed(s(20.0), s(8.0), s(4.0), s(100.0)), s(22.0));
        // Zoomed out to 16 s: 16 to 32.
        assert_eq!(zoomed(s(20.0), s(8.0), s(16.0), s(100.0)), s(16.0));
        // Near the start it stays inside the song.
        assert_eq!(zoomed(s(1.0), s(8.0), s(16.0), s(100.0)), s(0.0));
        // Sideways: forward and back, never past either end.
        assert_eq!(scrolled(s(20.0), 3.0, s(8.0), s(100.0)), s(23.0));
        assert_eq!(scrolled(s(20.0), -30.0, s(8.0), s(100.0)), s(0.0));
        assert_eq!(scrolled(s(90.0), 30.0, s(8.0), s(100.0)), s(92.0));
    }

    #[test]
    fn a_dragged_view_moves_with_the_pointer_and_stays_inside_the_song() {
        // Grabbed at 22 s with the view at 20 s, the pointer now at 52 s.
        assert_eq!(pan(s(20.0), s(22.0), s(52.0), s(8.0), s(100.0)), s(50.0));
        assert_eq!(pan(s(20.0), s(22.0), s(2.0), s(8.0), s(100.0)), s(0.0));
        assert_eq!(pan(s(20.0), s(22.0), s(99.0), s(8.0), s(100.0)), s(92.0));
        // Zoomed out after the drag, it widens from where it was left.
        assert_eq!(moved(s(50.0), s(16.0), s(100.0)), (s(50.0), s(66.0)));
        assert_eq!(moved(s(90.0), s(16.0), s(100.0)), (s(84.0), s(100.0)));
    }
}
