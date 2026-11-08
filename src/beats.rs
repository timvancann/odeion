//! Beats: where they fall in a song, and the grid rules built on them (`g` in the
//! practice view). Found in the background when a song starts, like the waveform.
//!
//! Old rock was rarely played to a click, so there is no fixed tempo here: each beat is
//! placed on its own, near a real onset and close to the beats around it. The grid is
//! those beats, never "BPM times n".
//!
//! Onsets are SuperFlux-style spectral flux (Böck 2013: vibrato and bends don't count as
//! hits); the tempo is Ellis's weighted autocorrelation and the beats his dynamic
//! programming tracker (2007). Tightness 300 suited a rock library best: measured on
//! twelve songs it kept 75% of beats on onsets with 6% jitter, where librosa's 100 gave
//! 76% and 8%, and 1000 lost songs that drift.

use rustfft::{FftPlanner, num_complex::Complex};
use serde::{Deserialize, Serialize};
use std::hash::{DefaultHasher, Hash, Hasher};
use std::path::Path;
use std::time::Duration;
use symphonia::core::audio::SampleBuffer;
use symphonia::core::codecs::DecoderOptions;

const FRAME: usize = 2048;
/// How strongly beats keep to the tempo rather than chase onsets.
const TIGHTNESS: f64 = 300.0;
/// Previous goes to the start of the beat you're in, unless you're within this of it.
pub const GRACE: Duration = Duration::from_millis(150);

#[derive(Debug, Default, Clone, PartialEq)]
pub struct Beats {
    pub times: Vec<Duration>,
}

impl Beats {
    /// The beat nearest `at`.
    pub fn nearest(&self, at: Duration) -> Option<Duration> {
        self.times.iter().copied().min_by_key(|b| b.abs_diff(at))
    }

    /// The first beat after `at`.
    pub fn next(&self, at: Duration) -> Option<Duration> {
        let i = self.times.partition_point(|&b| b <= at);
        self.times.get(i).copied()
    }

    /// The start of the beat `at` is in, or the beat before when `at` is within `GRACE`
    /// of that start, so pressing back twice quickly steps back two beats.
    pub fn previous(&self, at: Duration) -> Option<Duration> {
        let i = self.times.partition_point(|&b| b <= at).checked_sub(1)?;
        if at - self.times[i] > GRACE {
            Some(self.times[i])
        } else {
            i.checked_sub(1).map(|j| self.times[j])
        }
    }

    /// The beat `by` beats from the one nearest `at`.
    pub fn step(&self, at: Duration, by: isize) -> Option<Duration> {
        let i = self.index(at)? as isize + by;
        self.times.get(usize::try_from(i).ok()?).copied()
    }

    /// The loop from `start` to `end` moved on (or back) by its own number of beats.
    pub fn slide(&self, start: Duration, end: Duration, forward: bool) -> Option<(Duration, Duration)> {
        let (a, b) = (self.index(start)?, self.index(end)?);
        let n = b.checked_sub(a).filter(|&n| n > 0)?;
        let (a, b) = if forward { (a + n, b + n) } else { (a.checked_sub(n)?, b - n) };
        Some((*self.times.get(a)?, *self.times.get(b)?))
    }

    /// The tempo around `at` in beats per minute, from the four beats on either side, so
    /// it follows a band that speeds up.
    pub fn tempo_at(&self, at: Duration) -> Option<f64> {
        let i = self.index(at)?;
        let (a, b) = (i.saturating_sub(4), (i + 4).min(self.times.len() - 1));
        (b > a).then(|| 60.0 * (b - a) as f64 / (self.times[b] - self.times[a]).as_secs_f64())
    }

    fn index(&self, at: Duration) -> Option<usize> {
        (0..self.times.len()).min_by_key(|&i| self.times[i].abs_diff(at))
    }
}

/// The detector's version. Raise it when detection changes, and every song's beats are
/// found again rather than read from the cache.
const DETECTOR: u32 = 1;

/// A song's beats as cached: valid while the file and the detector are unchanged.
#[derive(Serialize, Deserialize)]
struct Cached {
    size: u64,
    modified: u64,
    detector: u32,
    /// Beat times in microseconds.
    beats: Vec<u64>,
}

/// A song's beats from the cache in `dir`, or found and cached when the song (by its
/// size and modification time) or the detector changed since.
pub fn cached(path: &Path, size: u64, modified: u64, dir: &Path) -> Result<Beats, String> {
    let mut hasher = DefaultHasher::new();
    path.hash(&mut hasher);
    let file = dir.join(format!("{:016x}.json", hasher.finish()));
    let hit = std::fs::read(&file)
        .ok()
        .and_then(|b| serde_json::from_slice::<Cached>(&b).ok())
        .filter(|c| c.size == size && c.modified == modified && c.detector == DETECTOR);
    if let Some(c) = hit {
        return Ok(Beats { times: c.beats.into_iter().map(Duration::from_micros).collect() });
    }
    let beats = read(path)?;
    let entry = Cached {
        size,
        modified,
        detector: DETECTOR,
        beats: beats.times.iter().map(|t| t.as_micros() as u64).collect(),
    };
    // A cache that can't be written only costs time on the next play.
    let _ = std::fs::create_dir_all(dir);
    if let Ok(json) = serde_json::to_vec(&entry) {
        let _ = std::fs::write(&file, json);
    }
    Ok(beats)
}

/// The beats of a stem set, found in the stems summed.
pub fn read_mix(files: &[std::path::PathBuf]) -> Result<Beats, String> {
    let (mix, rate) = mono_mix(files)?;
    Ok(Beats { times: detect(&mix, rate) })
}

/// Stems decoded to mono and summed, as heard together.
pub(crate) fn mono_mix(files: &[std::path::PathBuf]) -> Result<(Vec<f32>, u32), String> {
    let mut mix: Vec<f32> = Vec::new();
    let mut rate = 0;
    for f in files {
        let (x, r) = mono(f)?;
        rate = r;
        if mix.len() < x.len() {
            mix.resize(x.len(), 0.0);
        }
        for (m, v) in mix.iter_mut().zip(x) {
            *m += v;
        }
    }
    Ok((mix, rate))
}

/// Finds the beats in a song.
pub fn read(path: &Path) -> Result<Beats, String> {
    let (mono, rate) = mono(path)?;
    Ok(Beats { times: detect(&mono, rate) })
}

pub(crate) fn mono(path: &Path) -> Result<(Vec<f32>, u32), String> {
    let mut format = crate::wave::open(path)?;
    let track = format.default_track().ok_or("no audio track")?.clone();
    let rate = track.codec_params.sample_rate.ok_or("unknown sample rate")?;
    let mut decoder = symphonia::default::get_codecs()
        .make(&track.codec_params, &DecoderOptions::default())
        .map_err(|e| e.to_string())?;
    let mut mono = Vec::new();
    let mut buffer: Option<SampleBuffer<f32>> = None;
    while let Ok(packet) = format.next_packet() {
        if packet.track_id() != track.id {
            continue;
        }
        // A damaged packet is skipped, as a player would.
        let Ok(decoded) = decoder.decode(&packet) else { continue };
        let spec = *decoded.spec();
        let channels = spec.channels.count();
        if buffer.as_ref().is_none_or(|b| b.capacity() < decoded.capacity() * channels) {
            buffer = Some(SampleBuffer::new(decoded.capacity() as u64, spec));
        }
        let buffer = buffer.as_mut().expect("just made");
        buffer.copy_interleaved_ref(decoded);
        mono.extend(buffer.samples().chunks(channels).map(|f| f.iter().sum::<f32>() / channels as f32));
    }
    Ok((mono, rate))
}

/// The beat times in mono audio at `rate`.
/// Onset strength every 10 ms (and that step in samples), normalised.
pub(crate) fn strength(mono: &[f32], rate: u32) -> (Vec<f32>, usize) {
    let hop = ((rate as f64 / 100.0).round() as usize).max(1); // 10 ms
    let fps = rate as f64 / hop as f64;
    (normalise(&onsets(mono, hop), (fps * 0.25) as usize), hop)
}

pub fn detect(mono: &[f32], rate: u32) -> Vec<Duration> {
    let (strength, hop) = strength(mono, rate);
    let fps = rate as f64 / hop as f64;
    if strength.len() < 2 * (fps as usize) {
        return Vec::new();
    }
    let period = period(&strength, fps);
    let block = (rate as usize / 1000).max(1);
    let rise = rises(mono, block);
    let seconds_per_block = block as f64 / rate as f64;
    // The chain of beats runs on into silence before and after the music; those beats
    // mean nothing, so only beats from the first real onset to the last stay.
    let strong = |f: &usize| strength[*f] > 1.0;
    let first = (0..strength.len()).find(strong).unwrap_or(0);
    let last = (0..strength.len()).rev().find(strong).unwrap_or(strength.len());
    let reach = (period / 2.0) as usize;
    track(&strength, period)
        .into_iter()
        .filter(|&f| f + reach >= first && f <= last + reach)
        .map(|f| attack(&rise, (f * hop + FRAME / 2) as f64 / rate as f64, seconds_per_block))
        .map(Duration::from_secs_f64)
        .collect()
}

/// How sharply the level rises into each block (about a millisecond) of the song.
pub(crate) fn rises(mono: &[f32], block: usize) -> Vec<f32> {
    let level: Vec<f32> = mono
        .chunks(block)
        .map(|c| (c.iter().map(|v| v * v).sum::<f32>() / c.len() as f32 + 1e-9).ln())
        .collect();
    // Against the loudest of the few milliseconds before, so a hit stands out and a
    // sustained note doesn't.
    (0..level.len())
        .map(|i| level[i] - level[i.saturating_sub(4)..i.max(1)].iter().fold(f32::MIN, |a, &b| a.max(b)))
        .collect()
}

/// A beat moved onto its attack: the sharpest rise within 35 ms. Onset strength is
/// measured on 46 ms windows 10 ms apart, which places hits tens of milliseconds early
/// and coarsely; a loop edge wants the attack itself.
fn attack(rise: &[f32], at: f64, per_block: f64) -> f64 {
    let (i, reach) = ((at / per_block) as usize, (0.035 / per_block) as usize);
    (i.saturating_sub(reach)..(i + reach).min(rise.len()))
        .max_by(|&a, &b| rise[a].total_cmp(&rise[b]))
        .map_or(at, |best| best as f64 * per_block)
}

/// Onset strength per hop: log spectra, each bin against the loudest of its neighbours
/// two frames back, rises summed.
fn onsets(x: &[f32], hop: usize) -> Vec<f32> {
    let fft = FftPlanner::<f32>::new().plan_fft_forward(FRAME);
    let window: Vec<f32> = (0..FRAME)
        .map(|i| 0.5 - 0.5 * (std::f32::consts::TAU * i as f32 / FRAME as f32).cos())
        .collect();
    let bins = FRAME / 2;
    let mut buffer = vec![Complex::new(0.0, 0.0); FRAME];
    let spectra: Vec<Vec<f32>> = (0..x.len().saturating_sub(FRAME) / hop)
        .map(|f| {
            for i in 0..FRAME {
                buffer[i] = Complex::new(x[f * hop + i] * window[i], 0.0);
            }
            fft.process(&mut buffer);
            (0..bins).map(|k| (1.0 + 100.0 * buffer[k].norm()).ln()).collect()
        })
        .collect();
    (0..spectra.len())
        .map(|t| {
            let Some(before) = t.checked_sub(2).map(|p| &spectra[p]) else { return 0.0 };
            (1..bins - 1)
                .map(|k| (spectra[t][k] - before[k - 1].max(before[k]).max(before[k + 1])).max(0.0))
                .sum()
        })
        .collect()
}

/// Above a moving average only, scaled to unit spread: quiet and loud passages count alike.
fn normalise(o: &[f32], width: usize) -> Vec<f32> {
    let mut sum = vec![0.0f64; o.len() + 1];
    for (i, v) in o.iter().enumerate() {
        sum[i + 1] = sum[i] + *v as f64;
    }
    let out: Vec<f32> = (0..o.len())
        .map(|i| {
            let (a, b) = (i.saturating_sub(width), (i + width + 1).min(o.len()));
            (o[i] as f64 - (sum[b] - sum[a]) / (b - a) as f64).max(0.0) as f32
        })
        .collect();
    let spread = (out.iter().map(|v| v * v).sum::<f32>() / out.len().max(1) as f32).sqrt();
    out.iter().map(|v| v / spread.max(1e-9)).collect()
}

/// The beat period in frames: the strongest repetition between 50 and 220 BPM, leaning
/// toward 120 as people's sense of the beat does.
fn period(o: &[f32], fps: f64) -> f64 {
    let (lo, hi) = ((fps * 60.0 / 220.0) as usize, (fps * 60.0 / 50.0) as usize);
    (lo..=hi.min(o.len() - 1))
        .map(|lag| {
            let together: f32 = (0..o.len() - lag).map(|i| o[i] * o[i + lag]).sum();
            let bpm = 60.0 * fps / lag as f64;
            (lag, together as f64 * (-0.5 * (bpm / 120.0).log2().powi(2)).exp())
        })
        .max_by(|a, b| a.1.total_cmp(&b.1))
        .map_or(fps / 2.0, |(lag, _)| lag as f64)
}

/// Each frame's best score as a beat: its onset plus the best earlier beat, less a cost
/// for a gap unlike the period. The beats are the best chain back from the end.
fn track(o: &[f32], period: f64) -> Vec<usize> {
    let (lo, hi) = (((period / 2.0).round() as usize).max(1), (period * 2.0).round() as usize);
    let mut score = vec![0.0f64; o.len()];
    let mut back: Vec<Option<usize>> = vec![None; o.len()];
    for t in 0..o.len() {
        let best = (lo..=hi.min(t))
            .map(|gap| (t - gap, score[t - gap] - TIGHTNESS * (gap as f64 / period).ln().powi(2)))
            .max_by(|a, b| a.1.total_cmp(&b.1));
        score[t] = o[t] as f64 + best.map_or(0.0, |b| b.1);
        back[t] = best.map(|b| b.0);
    }
    let tail = o.len().saturating_sub(period as usize);
    let Some(mut t) = (tail..o.len()).max_by(|&a, &b| score[a].total_cmp(&score[b])) else {
        return Vec::new();
    };
    let mut beats = vec![t];
    while let Some(earlier) = back[t] {
        beats.push(earlier);
        t = earlier;
    }
    beats.reverse();
    beats
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(secs: f64) -> Duration {
        Duration::from_secs_f64(secs)
    }

    fn grid(times: &[f64]) -> Beats {
        Beats { times: times.iter().map(|&t| s(t)).collect() }
    }

    /// Clicks (short noise bursts) at the given times, mono at 22.05 kHz.
    fn clicks(times: &[f64], length: f64) -> (Vec<f32>, u32) {
        let rate = 22_050u32;
        let mut x = vec![0.0f32; (length * rate as f64) as usize];
        let mut seed = 1u32;
        for &t in times {
            let at = (t * rate as f64) as usize;
            for i in 0..400.min(x.len() - at) {
                seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                let noise = (seed >> 9) as f32 / (1u32 << 23) as f32 - 1.0;
                x[at + i] = noise * (1.0 - i as f32 / 400.0);
            }
        }
        (x, rate)
    }

    /// Within 15 ms: closer than anyone hears a loop edge.
    fn found_near(beats: &[Duration], click: f64) -> bool {
        beats.iter().any(|b| (b.as_secs_f64() - click).abs() < 0.015)
    }

    #[test]
    fn finds_every_click_of_a_steady_beat() {
        let times: Vec<f64> = (0..40).map(|i| 1.0 + i as f64 * 0.5).collect(); // 120 BPM
        let (x, rate) = clicks(&times, 22.0);
        let beats = detect(&x, rate);
        let missed: Vec<f64> = times.iter().copied().filter(|&t| !found_near(&beats, t)).collect();
        assert!(missed.is_empty(), "missed {missed:?}");
    }

    #[test]
    fn follows_a_band_that_speeds_up() {
        // From 100 BPM to about 120 over the song, as Gallows Pole does (measured 100
        // to 118).
        let mut times = vec![1.0];
        while *times.last().unwrap() < 30.0 {
            let t = *times.last().unwrap();
            times.push(t + 0.6 - 0.1 * t / 30.0);
        }
        let (x, rate) = clicks(&times, 32.0);
        let beats = detect(&x, rate);
        let found = times.iter().filter(|&&t| found_near(&beats, t)).count();
        assert!(found as f64 >= times.len() as f64 * 0.95, "{found} of {}", times.len());
        let b = Beats { times: beats };
        assert!((b.tempo_at(s(3.0)).unwrap() - 101.0).abs() < 4.0);
        assert!((b.tempo_at(s(28.0)).unwrap() - 119.0).abs() < 4.0);
    }

    #[test]
    fn beats_come_from_the_cache_until_the_song_changes() {
        let dir = std::env::temp_dir().join(format!("riff-beats-{}", std::process::id()));
        let song = dir.join("song.wav");
        std::fs::create_dir_all(&dir).unwrap();
        // Not audio at all: only a cache hit can produce beats for it.
        std::fs::write(&song, b"not audio").unwrap();
        let mut hasher = DefaultHasher::new();
        song.hash(&mut hasher);
        let entry = Cached { size: 9, modified: 5, detector: DETECTOR, beats: vec![500_000, 1_000_000] };
        std::fs::write(dir.join(format!("{:016x}.json", hasher.finish())), serde_json::to_vec(&entry).unwrap()).unwrap();
        assert_eq!(cached(&song, 9, 5, &dir).unwrap(), grid(&[0.5, 1.0]));
        // Changed since: found again, which fails for a file that isn't audio.
        assert!(cached(&song, 9, 6, &dir).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn back_restarts_the_beat_then_steps_back() {
        let b = grid(&[1.0, 1.5, 2.0, 2.5]);
        assert_eq!(b.previous(s(2.3)), Some(s(2.0)));
        assert_eq!(b.previous(s(2.0)), Some(s(1.5)));
        assert_eq!(b.previous(s(2.1)), Some(s(1.5)));
        assert_eq!(b.previous(s(1.05)), None);
        assert_eq!(b.next(s(2.0)), Some(s(2.5)));
        assert_eq!(b.next(s(2.6)), None);
        assert_eq!(b.nearest(s(1.7)), Some(s(1.5)));
    }

    #[test]
    fn loops_slide_in_beats_even_as_the_tempo_drifts() {
        let b = grid(&[0.0, 0.5, 1.0, 1.5, 2.0, 2.4, 2.8, 3.2, 3.6]);
        // Two beats from 1.0, slid on: the next two, shorter in seconds.
        assert_eq!(b.slide(s(1.0), s(2.0), true), Some((s(2.0), s(2.8))));
        assert_eq!(b.slide(s(1.0), s(2.0), false), Some((s(0.0), s(1.0))));
        assert_eq!(b.slide(s(0.5), s(1.5), false), None);
        assert_eq!(b.step(s(1.1), 1), Some(s(1.5)));
        assert_eq!(b.step(s(1.1), -1), Some(s(0.5)));
        assert_eq!(b.step(s(0.1), -1), None);
    }
}
