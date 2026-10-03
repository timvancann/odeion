//! Playback: one song at a time, seeking, and an A/B loop.

use rodio::source::SeekError;
use rodio::{ChannelCount, Decoder, DeviceSinkBuilder, MixerDeviceSink, Player, SampleRate, Source};
use std::fs::File;
use std::io::BufReader;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, channel};
use std::time::Duration;

/// A loop between two points. Without an end there is no loop; without a start it runs
/// from the beginning of the song. Switched off, it keeps its edges but doesn't loop.
#[derive(Debug, Default, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Loop {
    pub start: Option<Duration>,
    pub end: Option<Duration>,
    pub off: bool,
}

impl Loop {
    /// A start at or after the end drops the end: the old loop no longer makes sense.
    /// Placing an edge switches the loop on.
    pub fn set_start(&mut self, at: Duration) {
        self.start = Some(at);
        self.off = false;
        if self.end.is_some_and(|end| end <= at) {
            self.end = None;
        }
    }

    /// An end before the start is ignored.
    pub fn set_end(&mut self, at: Duration) {
        if at > self.start.unwrap_or_default() {
            self.end = Some(at);
            self.off = false;
        }
    }

    /// Both edges are there (or an end, looping from the song's start).
    pub fn active(&self) -> bool {
        self.end.is_some()
    }

    /// It has edges and loops.
    pub fn looping(&self) -> bool {
        self.active() && !self.off
    }

    /// Looping on or off, keeping the edges.
    pub fn switch(&mut self) {
        self.off = !self.off;
    }

    /// Where to jump to when playback has reached `at`, if anywhere.
    pub fn jump(&self, at: Duration) -> Option<Duration> {
        let end = self.end.filter(|_| !self.off)?;
        (at >= end).then(|| self.start.unwrap_or_default())
    }

    /// Moves the start to `at`, keeping it at least `MIN_LOOP` before the end.
    pub fn move_start(&mut self, at: Duration) {
        self.start = Some(match self.end {
            Some(end) => at.min(end.saturating_sub(MIN_LOOP)),
            None => at,
        });
    }

    /// Moves the end to `at`, keeping it at least `MIN_LOOP` after the start and within
    /// the song. Without an end there is nothing to move.
    pub fn move_end(&mut self, at: Duration, song: Option<Duration>) {
        if self.end.is_none() {
            return;
        }
        let at = at.max(self.start.unwrap_or_default() + MIN_LOOP);
        self.end = Some(song.map_or(at, |s| at.min(s)));
    }

    /// Moves the start by `by` seconds, negative for earlier.
    pub fn nudge_start(&mut self, by: f64) {
        self.move_start(shifted(self.start.unwrap_or_default(), by));
    }

    pub fn nudge_end(&mut self, by: f64, song: Option<Duration>) {
        if let Some(end) = self.end {
            self.move_end(shifted(end, by), song);
        }
    }

    /// Moves the whole loop by its own length, to the next phrase or the one before,
    /// stopping at the start or end of the song.
    pub fn slide(&mut self, forward: bool, song: Option<Duration>) {
        let Some(end) = self.end else { return };
        let start = self.start.unwrap_or_default();
        let length = end - start;
        let new_start = if forward {
            let latest = song.map_or(end, |s| s.saturating_sub(length));
            end.min(latest)
        } else {
            start.saturating_sub(length)
        };
        self.start = Some(new_start);
        self.end = Some(new_start + length);
    }

    /// Halves (`factor` 0.5) or doubles (2.0) the loop, keeping its start.
    pub fn resize(&mut self, factor: f64, song: Option<Duration>) {
        if let Some(end) = self.end {
            let start = self.start.unwrap_or_default();
            self.move_end(start + (end - start).mul_f64(factor), song);
        }
    }
}

/// A playhead that moves smoothly between the player's position updates, for drawing
/// at the screen's frame rate. It runs on its own clock and takes the player's word
/// again only when they disagree by more than `SLACK`: a seek, a loop jump, a pause.
#[derive(Debug, Default)]
pub struct Smooth {
    /// The position, and the clock time it was taken at.
    anchor: Option<(Duration, Duration)>,
}

/// More than a position update's step, less than anything a person would notice.
const SLACK: Duration = Duration::from_millis(60);

impl Smooth {
    /// Where to draw the playhead, given the player's position and a clock reading.
    pub fn at(&mut self, real: Duration, playing: bool, now: Duration, speed: f32) -> Duration {
        if playing && let Some((from, then)) = self.anchor {
            let gone = now.saturating_sub(then);
            let guess = from + if speed < 1.0 { gone.mul_f64(speed as f64) } else { gone };
            if guess.abs_diff(real) <= SLACK {
                return guess;
            }
        }
        self.anchor = playing.then_some((real, now));
        real
    }
}

/// The shortest loop: shorter than this, the edges can't be heard apart.
pub const MIN_LOOP: Duration = Duration::from_millis(100);

fn shifted(at: Duration, by: f64) -> Duration {
    Duration::from_secs_f64((at.as_secs_f64() + by).max(0.0))
}

type Song = Mix<Decoder<BufReader<File>>>;

/// Several recordings played as one, sample by sample: a stem set summed, each at its
/// own gain, read live so muting a stem is instant. A single file is a mix of one.
pub(crate) struct Mix<S> {
    stems: Vec<S>,
    gains: Arc<Vec<AtomicU32>>,
}

impl<S: Source> Mix<S> {
    fn new(stems: Vec<S>, gains: Arc<Vec<AtomicU32>>) -> Result<Self, String> {
        let first = stems.first().ok_or("nothing to play")?;
        let (channels, rate) = (first.channels(), first.sample_rate());
        if stems.iter().any(|s| s.channels() != channels || s.sample_rate() != rate) {
            return Err("the stems differ in channels or sample rate".into());
        }
        Ok(Mix { stems, gains })
    }
}

impl<S: Source> Iterator for Mix<S> {
    type Item = rodio::Sample;

    fn next(&mut self) -> Option<rodio::Sample> {
        let mut sum = 0.0;
        let mut any = false;
        for (stem, gain) in self.stems.iter_mut().zip(self.gains.iter()) {
            // A stem that ends early is silence from there on.
            if let Some(v) = stem.next() {
                sum += v * f32::from_bits(gain.load(Ordering::Relaxed));
                any = true;
            }
        }
        any.then_some(sum)
    }
}

impl<S: Source> Source for Mix<S> {
    fn current_span_len(&self) -> Option<usize> {
        None
    }

    fn channels(&self) -> ChannelCount {
        self.stems[0].channels()
    }

    fn sample_rate(&self) -> SampleRate {
        self.stems[0].sample_rate()
    }

    fn total_duration(&self) -> Option<Duration> {
        self.stems.iter().filter_map(|s| s.total_duration()).max()
    }

    fn try_seek(&mut self, at: Duration) -> Result<(), SeekError> {
        for stem in &mut self.stems {
            stem.try_seek(at)?;
        }
        Ok(())
    }
}

/// Keeps slowed audio inside full scale. Stretching re-aligns phases, and peaks that
/// were apart can land together: on loud masters 0.02 to 0.06% of samples went over,
/// up to 1.8, and the hardware would clip them into crackle. Below 0.9 nothing changes;
/// above it peaks bend smoothly toward 1 and never pass it. Only those rare peaks are
/// touched, so the level stays as it was.
fn soft_limit(x: f32) -> f32 {
    const KNEE: f32 = 0.9;
    let size = x.abs();
    if size <= KNEE {
        return x;
    }
    x.signum() * (KNEE + (1.0 - KNEE) * ((size - KNEE) / (1.0 - KNEE)).tanh())
}

/// Output frames the slow-down makes at a time.
const BLOCK: usize = 512;

/// The song slowed down without changing pitch (Signalsmith Stretch, a phase vocoder,
/// at its full-quality preset: half a percent of a core). At full speed it stays out of
/// the way: samples pass through untouched, with no delay. It comes after the loop, so
/// loops keep to the sample in the song's time.
struct Stretched<S> {
    inner: S,
    shared: Arc<Shared>,
    stretch: signalsmith_stretch::Stretch,
    channels: usize,
    /// Output waiting to be played, and how far into it.
    out: Vec<f32>,
    at: usize,
    input: Vec<f32>,
    /// Input frames owed from rounding blocks at the speed.
    carry: f64,
    /// Whether the last sample came through the stretcher.
    stretching: bool,
}

impl<S: Source> Stretched<S> {
    fn new(inner: S, shared: Arc<Shared>) -> Self {
        let channels = inner.channels().get() as usize;
        let stretch =
            signalsmith_stretch::Stretch::preset_default(channels as u32, inner.sample_rate().get());
        Stretched {
            inner,
            shared,
            stretch,
            channels,
            out: Vec::new(),
            at: 0,
            input: Vec::new(),
            carry: 0.0,
            stretching: false,
        }
    }

    /// Starts afresh: after a seek, or when slowing down begins.
    fn restart(&mut self) {
        self.stretch.reset();
        self.out.clear();
        self.at = 0;
        self.carry = 0.0;
    }
}

impl<S: Source> Iterator for Stretched<S> {
    type Item = rodio::Sample;

    fn next(&mut self) -> Option<rodio::Sample> {
        let speed = f32::from_bits(self.shared.speed.load(Ordering::Relaxed)) as f64;
        if speed >= 0.999 {
            if self.stretching {
                // Back to full speed: what the stretcher held back is dropped.
                self.stretching = false;
                self.restart();
                self.shared.delay.store(0, Ordering::Relaxed);
            }
            return self.inner.next();
        }
        if !self.stretching {
            self.stretching = true;
            self.restart();
        }
        if self.at >= self.out.len() {
            let wanted = BLOCK as f64 * speed + self.carry;
            let frames = wanted.floor() as usize;
            self.carry = wanted - frames as f64;
            self.input.clear();
            for _ in 0..frames * self.channels {
                match self.inner.next() {
                    Some(v) => self.input.push(v),
                    None => break,
                }
            }
            if self.input.is_empty() {
                return None;
            }
            // A last, partial frame is padded so the stretcher gets whole frames.
            let whole = self.input.len().div_ceil(self.channels) * self.channels;
            self.input.resize(whole, 0.0);
            self.out.resize(BLOCK * self.channels, 0.0);
            self.stretch.process(&self.input, &mut self.out);
            self.at = 0;
            let held = self.stretch.input_latency() as f64 + self.stretch.output_latency() as f64 * speed;
            self.shared.delay.store(held as u64, Ordering::Relaxed);
        }
        let v = soft_limit(self.out[self.at]);
        self.at += 1;
        Some(v)
    }
}

impl<S: Source> Source for Stretched<S> {
    fn current_span_len(&self) -> Option<usize> {
        None
    }

    fn channels(&self) -> ChannelCount {
        self.inner.channels()
    }

    fn sample_rate(&self) -> SampleRate {
        self.inner.sample_rate()
    }

    fn total_duration(&self) -> Option<Duration> {
        self.inner.total_duration()
    }

    fn try_seek(&mut self, at: Duration) -> Result<(), SeekError> {
        self.inner.try_seek(at)?;
        if self.stretching {
            self.restart();
        }
        Ok(())
    }
}

fn gains(of: &[f32]) -> Arc<Vec<AtomicU32>> {
    Arc::new(of.iter().map(|g| AtomicU32::new(g.to_bits())).collect())
}

/// What the audio thread and the window share: the loop to keep, in frames (`NONE` for
/// no loop), and how far the song has played.
#[derive(Debug)]
struct Shared {
    start: AtomicU64,
    end: AtomicU64,
    played: AtomicU64,
    /// Playback speed (f32 bits): 1 plays as recorded, below slows down.
    speed: AtomicU32,
    /// Frames the slow-down holds back: played but not heard yet.
    delay: AtomicU64,
}

const NONE: u64 = u64::MAX;

impl Default for Shared {
    fn default() -> Self {
        Shared {
            start: AtomicU64::new(0),
            end: AtomicU64::new(NONE),
            played: AtomicU64::new(0),
            speed: AtomicU32::new(1.0f32.to_bits()),
            delay: AtomicU64::new(0),
        }
    }
}

/// The song as the player hears it, looping at the exact sample. Checking for the loop's
/// end from the window, every 40 ms, let it play up to 40 ms past it. It also keeps the
/// position itself: the player can't see a jump made in here.
struct Looped<S> {
    inner: S,
    shared: Arc<Shared>,
    channels: u16,
    rate: u32,
    /// Which sample of the current frame comes next.
    sample: u16,
}

impl<S: Source> Looped<S> {
    fn new(inner: S, shared: Arc<Shared>) -> Self {
        let (channels, rate) = (inner.channels().get(), inner.sample_rate().get());
        Looped { inner, shared, channels, rate, sample: 0 }
    }

    fn frames(&self, at: Duration) -> u64 {
        (at.as_secs_f64() * self.rate as f64).round() as u64
    }
}

impl<S: Source> Iterator for Looped<S> {
    type Item = rodio::Sample;

    fn next(&mut self) -> Option<rodio::Sample> {
        if self.sample == 0 {
            let end = self.shared.end.load(Ordering::Relaxed);
            if end != NONE && self.shared.played.load(Ordering::Relaxed) >= end {
                let start = self.shared.start.load(Ordering::Relaxed);
                let to = Duration::from_secs_f64(start as f64 / self.rate as f64);
                if self.inner.try_seek(to).is_ok() {
                    self.shared.played.store(start, Ordering::Relaxed);
                }
            }
        }
        let sample = self.inner.next()?;
        self.sample += 1;
        if self.sample == self.channels {
            self.sample = 0;
            self.shared.played.fetch_add(1, Ordering::Relaxed);
        }
        Some(sample)
    }
}

impl<S: Source> Source for Looped<S> {
    fn current_span_len(&self) -> Option<usize> {
        // A jump can land mid-span, so make no promises about span boundaries.
        None
    }

    fn channels(&self) -> ChannelCount {
        self.inner.channels()
    }

    fn sample_rate(&self) -> SampleRate {
        self.inner.sample_rate()
    }

    fn total_duration(&self) -> Option<Duration> {
        self.inner.total_duration()
    }

    fn try_seek(&mut self, at: Duration) -> Result<(), SeekError> {
        self.inner.try_seek(at)?;
        self.sample = 0;
        self.shared.played.store(self.frames(at), Ordering::Relaxed);
        Ok(())
    }
}

pub struct Audio {
    /// None when there is no output device; everything else still works, silently.
    sink: Option<MixerDeviceSink>,
    player: Option<Player>,
    loading: Option<Receiver<Result<Song, String>>>,
    /// What plays: one file, or the stems of a set.
    pub files: Vec<PathBuf>,
    /// Each file's gain, read live by the audio thread.
    gains: Arc<Vec<AtomicU32>>,
    /// Stay paused once the files open (a switch made while paused).
    start_paused: bool,
    /// Playback speed, below 1 slowed down; kept across songs.
    speed: f32,
    pub duration: Option<Duration>,
    pub looping: Loop,
    pub error: Option<String>,
    /// Where to start once the song has opened.
    start_at: Option<Duration>,
    /// The loop and position, shared with the audio thread.
    shared: Arc<Shared>,
    rate: u32,
}

impl Audio {
    pub fn new() -> Audio {
        let sink = DeviceSinkBuilder::open_default_sink()
            .map(|mut s| {
                s.log_on_drop(false);
                s
            })
            .ok();
        Audio {
            error: sink.is_none().then(|| "no audio output".to_string()),
            sink,
            player: None,
            loading: None,
            start_at: None,
            shared: Arc::default(),
            rate: 44_100,
            files: Vec::new(),
            gains: gains(&[]),
            start_paused: false,
            speed: 1.0,
            duration: None,
            looping: Loop::default(),
        }
    }

    /// Starts a song: one file, or a stem set summed at `gains_now`. Opening happens on
    /// a thread: a file still syncing can take a while.
    pub fn play(&mut self, files: &[PathBuf], gains_now: &[f32]) {
        self.player = None;
        self.duration = None;
        self.error = None;
        self.looping = Loop::default();
        self.start_paused = false;
        self.files = files.to_vec();
        self.gains = gains(gains_now);
        let (tx, rx) = channel();
        let (files, gains) = (files.to_vec(), self.gains.clone());
        std::thread::spawn(move || {
            let stems = files
                .iter()
                .map(|path| {
                    File::open(path)
                        .map_err(|e| e.to_string())
                        .and_then(|f| Decoder::try_from(f).map_err(|e| e.to_string()))
                })
                .collect::<Result<Vec<_>, _>>();
            let _ = tx.send(stems.and_then(|stems| Mix::new(stems, gains)));
        });
        self.loading = Some(rx);
        self.start_at = None;
    }

    /// Switches to other files (a variation) at `at`, with `looping`, playing or paused
    /// as before.
    pub fn switch(&mut self, files: &[PathBuf], gains_now: &[f32], at: Duration, looping: Loop) {
        let paused = !self.playing();
        self.play(files, gains_now);
        self.looping = looping;
        self.start_at = Some(at);
        self.start_paused = paused;
    }

    /// Plays at `speed` (1 as recorded, below slower) from now on, pitch unchanged.
    pub fn set_speed(&mut self, speed: f32) {
        self.speed = speed;
        self.shared.speed.store(speed.to_bits(), Ordering::Relaxed);
    }

    pub fn speed(&self) -> f32 {
        self.speed
    }

    /// The stems' gains, from now on.
    pub fn set_gains(&self, now: &[f32]) {
        for (gain, g) in self.gains.iter().zip(now) {
            gain.store(g.to_bits(), Ordering::Relaxed);
        }
    }

    /// Called often: picks up a song that finished opening and keeps the loop.
    /// Returns whether anything visible changed.
    pub fn tick(&mut self) -> bool {
        let mut changed = false;
        if let Some(result) = self.loading.as_ref().and_then(|rx| rx.try_recv().ok()) {
            self.loading = None;
            changed = true;
            match (result, &self.sink) {
                (Ok(song), Some(sink)) => {
                    self.duration = song.total_duration();
                    self.rate = song.sample_rate().get();
                    self.shared = Arc::default();
                    self.keep_loop();
                    let player = Player::connect_new(sink.mixer());
                    self.shared.speed.store(self.speed.to_bits(), Ordering::Relaxed);
                    let looped = Looped::new(song, self.shared.clone());
                    player.append(Stretched::new(looped, self.shared.clone()));
                    if let Some(at) = self.start_at.take() {
                        let _ = player.try_seek(at);
                    }
                    if std::mem::take(&mut self.start_paused) {
                        player.pause();
                    }
                    self.player = Some(player);
                }
                (Ok(_), None) => {}
                (Err(e), _) => self.error = Some(e),
            }
        }
        self.keep_loop();
        changed || self.playing()
    }

    /// Hands the loop to the audio thread, which keeps it to the sample.
    fn keep_loop(&self) {
        let frames = |d: Duration| (d.as_secs_f64() * self.rate as f64).round() as u64;
        let looping = self.looping;
        let (start, end) = match looping.end.filter(|_| looping.looping()) {
            Some(end) => (frames(looping.start.unwrap_or_default()), frames(end)),
            None => (0, NONE),
        };
        self.shared.start.store(start, Ordering::Relaxed);
        self.shared.end.store(end, Ordering::Relaxed);
    }

    /// Whether the song played to its end.
    pub fn ended(&self) -> bool {
        self.loading.is_none() && self.player.as_ref().is_some_and(|p| p.empty())
    }

    pub fn loading(&self) -> bool {
        self.loading.is_some()
    }

    pub fn playing(&self) -> bool {
        self.player
            .as_ref()
            .is_some_and(|p| !p.is_paused() && !p.empty())
    }

    pub fn position(&self) -> Duration {
        match &self.player {
            // What's heard: what the song has played, less what the slow-down still holds.
            Some(p) if !p.empty() => {
                let played = self.shared.played.load(Ordering::Relaxed);
                let held = self.shared.delay.load(Ordering::Relaxed);
                Duration::from_secs_f64(played.saturating_sub(held) as f64 / self.rate as f64)
            }
            Some(_) => self.duration.unwrap_or_default(),
            None => Duration::ZERO,
        }
    }

    /// Pause or resume. A song that has ended starts again, keeping its loop points.
    pub fn toggle(&mut self) {
        match &self.player {
            Some(p) if p.empty() => {
                if !self.files.is_empty() {
                    let looping = self.looping;
                    let now: Vec<f32> =
                        self.gains.iter().map(|g| f32::from_bits(g.load(Ordering::Relaxed))).collect();
                    self.play(&self.files.clone(), &now);
                    self.looping = looping;
                }
            }
            Some(p) if p.is_paused() => p.play(),
            Some(p) => p.pause(),
            None => {}
        }
    }

    /// Starts the song that is opening at `at`, rather than the top.
    pub fn resume_at(&mut self, at: Duration) {
        self.start_at = Some(at).filter(|at| !at.is_zero());
    }

    /// Plays from `at`, whether paused, playing or ended.
    pub fn play_from(&mut self, at: Duration) {
        if self.ended() {
            // Starting again opens the song anew; the seek waits for that.
            self.toggle();
            self.start_at = Some(at);
            return;
        }
        self.seek(at);
        if let Some(p) = &self.player {
            p.play();
        }
    }

    pub fn pause(&self) {
        if let Some(p) = &self.player {
            p.pause();
        }
    }

    pub fn seek(&self, to: Duration) {
        // The audio thread gets the loop first: with the old one it would jump right back
        // to the old loop's start (playing a region landed in the previous one).
        self.keep_loop();
        let to = match self.duration {
            Some(d) => to.min(d.saturating_sub(Duration::from_millis(250))),
            None => to,
        };
        if let Some(p) = &self.player {
            let _ = p.try_seek(to);
        }
    }

    pub fn seek_by(&self, seconds: f64) {
        let now = self.position().as_secs_f64();
        self.seek(Duration::from_secs_f64((now + seconds).max(0.0)));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(secs: u64) -> Duration {
        Duration::from_secs(secs)
    }

    #[test]
    fn no_end_no_loop() {
        let mut l = Loop::default();
        l.set_start(s(10));
        assert!(!l.active());
        assert_eq!(l.jump(s(500)), None);
    }

    #[test]
    fn jumps_back_to_the_start_at_the_end() {
        let mut l = Loop::default();
        l.set_start(s(10));
        l.set_end(s(20));
        assert_eq!(l.jump(s(19)), None);
        assert_eq!(l.jump(s(20)), Some(s(10)));
    }

    #[test]
    fn an_end_alone_loops_from_the_beginning() {
        let mut l = Loop::default();
        l.set_end(s(20));
        assert_eq!(l.jump(s(21)), Some(s(0)));
    }

    #[test]
    fn an_end_before_the_start_is_ignored() {
        let mut l = Loop::default();
        l.set_start(s(10));
        l.set_end(s(5));
        assert_eq!(l.end, None);
    }

    fn ms(millis: u64) -> Duration {
        Duration::from_millis(millis)
    }

    fn looped(start: u64, end: u64) -> Loop {
        Loop { start: Some(s(start)), end: Some(s(end)), off: false }
    }

    #[test]
    fn nudged_edges_keep_their_order_and_stay_in_the_song() {
        let mut l = looped(10, 20);
        l.nudge_start(-0.05);
        assert_eq!(l.start, Some(ms(9950)));
        l.nudge_start(-100.0);
        assert_eq!(l.start, Some(s(0)));
        l.nudge_start(30.0);
        assert_eq!(l.start, Some(s(20) - MIN_LOOP));
        l.nudge_end(-100.0, Some(s(30)));
        assert_eq!(l.end, Some(s(20)));
        l.nudge_end(100.0, Some(s(30)));
        assert_eq!(l.end, Some(s(30)));
    }

    #[test]
    fn without_an_end_only_the_start_moves() {
        let mut l = Loop::default();
        l.nudge_end(1.0, None);
        assert_eq!(l.end, None);
        l.nudge_start(0.5);
        assert_eq!(l.start, Some(ms(500)));
    }

    #[test]
    fn sliding_moves_a_loop_by_its_length_within_the_song() {
        let mut l = looped(10, 14);
        l.slide(true, Some(s(60)));
        assert_eq!(l, looped(14, 18));
        l.slide(false, Some(s(60)));
        l.slide(false, Some(s(60)));
        l.slide(false, Some(s(60)));
        assert_eq!(l, looped(2, 6));
        let mut l = looped(50, 58);
        l.slide(true, Some(s(60)));
        assert_eq!(l, looped(52, 60));
    }

    #[test]
    fn sliding_back_stops_at_the_start_of_the_song() {
        let mut l = looped(3, 7);
        l.slide(false, None);
        assert_eq!(l, looped(0, 4));
    }

    #[test]
    fn halving_and_doubling_keep_the_start() {
        let mut l = looped(10, 14);
        l.resize(0.5, None);
        assert_eq!(l, looped(10, 12));
        l.resize(2.0, Some(s(13)));
        assert_eq!(l, looped(10, 13));
    }

    #[test]
    fn the_smooth_playhead_glides_between_steps_and_follows_jumps() {
        let mut p = Smooth::default();
        assert_eq!(p.at(s(10), true, ms(0), 1.0), s(10));
        // The player still says 10 s; 16 ms later the playhead has moved on anyway.
        assert_eq!(p.at(s(10), true, ms(16), 1.0), s(10) + ms(16));
        assert_eq!(p.at(s(10) + ms(40), true, ms(32), 1.0), s(10) + ms(32));
        // A loop jump back to 4 s is followed at once.
        assert_eq!(p.at(s(4), true, ms(48), 1.0), s(4));
        assert_eq!(p.at(s(4), true, ms(64), 1.0), s(4) + ms(16));
        // Paused, it stays where the player is.
        assert_eq!(p.at(s(7), false, ms(80), 1.0), s(7));
        assert_eq!(p.at(s(7), false, ms(96), 1.0), s(7));
        // Slowed to half speed, the playhead moves at half speed.
        let mut p = Smooth::default();
        assert_eq!(p.at(s(10), true, ms(0), 0.5), s(10));
        assert_eq!(p.at(s(10), true, ms(40), 0.5), s(10) + ms(20));
    }

    #[test]
    fn switched_off_a_loop_keeps_its_edges_but_doesnt_loop() {
        let mut l = looped(10, 20);
        l.switch();
        assert_eq!(l.jump(s(25)), None);
        assert!(l.active() && !l.looping());
        l.nudge_end(1.0, None);
        assert_eq!(l.end, Some(s(21)));
        // Placing an edge means looping again.
        l.set_end(s(22));
        assert!(l.looping());
        assert_eq!(l.jump(s(25)), Some(s(10)));
    }

    /// A source of `frames` stereo frames whose samples count up, seekable.
    struct Counting {
        at: usize,
        frames: usize,
    }

    impl Iterator for Counting {
        type Item = rodio::Sample;
        fn next(&mut self) -> Option<rodio::Sample> {
            (self.at < self.frames * 2).then(|| {
                self.at += 1;
                ((self.at - 1) / 2) as rodio::Sample
            })
        }
    }

    impl Source for Counting {
        fn current_span_len(&self) -> Option<usize> {
            None
        }
        fn channels(&self) -> ChannelCount {
            ChannelCount::new(2).unwrap()
        }
        fn sample_rate(&self) -> SampleRate {
            SampleRate::new(1000).unwrap()
        }
        fn total_duration(&self) -> Option<Duration> {
            None
        }
        fn try_seek(&mut self, at: Duration) -> Result<(), SeekError> {
            self.at = (at.as_secs_f64() * 1000.0).round() as usize * 2;
            Ok(())
        }
    }

    #[test]
    fn the_audio_loops_at_the_exact_frame_and_knows_where_it_is() {
        let shared = Arc::new(Shared::default());
        shared.start.store(100, Ordering::Relaxed);
        shared.end.store(110, Ordering::Relaxed);
        let mut song = Looped::new(Counting { at: 0, frames: 1000 }, shared.clone());
        let frames: Vec<rodio::Sample> = song.by_ref().take(2 * 125).step_by(2).collect();
        // Frames 0..110, then 100.. again: never frame 110 itself.
        assert_eq!(frames[109], 109.0);
        assert_eq!(frames[110], 100.0);
        assert!(!frames.contains(&110.0));
        // 125 frames: 0..110, then 100..110 once more, then 100..105.
        assert_eq!(shared.played.load(Ordering::Relaxed), 105);
        assert_eq!(frames[124], 104.0);
        // Seeking from outside keeps the position honest too.
        song.try_seek(Duration::from_millis(500)).unwrap();
        assert_eq!(shared.played.load(Ordering::Relaxed), 500);
    }

    #[test]
    fn a_seek_hands_the_new_loop_over_first() {
        // No output device: the loop handover happens all the same.
        let mut audio = Audio {
            sink: None,
            player: None,
            loading: None,
            files: Vec::new(),
            gains: gains(&[]),
            start_paused: false,
            speed: 1.0,
            duration: None,
            looping: looped(0, 16),
            error: None,
            start_at: None,
            shared: Arc::default(),
            rate: 1000,
        };
        audio.seek(s(1));
        assert_eq!(audio.shared.end.load(Ordering::Relaxed), 16_000);
        audio.looping = looped(120, 140);
        audio.seek(s(120));
        assert_eq!(audio.shared.start.load(Ordering::Relaxed), 120_000);
        assert_eq!(audio.shared.end.load(Ordering::Relaxed), 140_000);
    }

    #[test]
    fn stems_are_summed_at_their_gains_live() {
        let g = gains(&[1.0, 0.5]);
        let mut mix = Mix::new(
            vec![Counting { at: 0, frames: 10 }, Counting { at: 0, frames: 4 }],
            g.clone(),
        )
        .unwrap();
        // Frame 2 (both channels): 2 + 0.5 * 2.
        let first: Vec<rodio::Sample> = mix.by_ref().take(6).collect();
        assert_eq!(first[4], 3.0);
        // Muted from here on; the second stem ends after frame 3 anyway.
        g[1].store(0.0f32.to_bits(), Ordering::Relaxed);
        assert_eq!(mix.next(), Some(3.0));
        let rest: Vec<rodio::Sample> = mix.collect();
        assert_eq!(rest.len(), 2 * 10 - 7);
        assert_eq!(rest.last(), Some(&9.0));
    }

    #[test]
    fn the_limiter_leaves_normal_levels_alone_and_keeps_peaks_under_full_scale() {
        for x in [0.0, 0.3, -0.75, 0.9, -0.9] {
            assert_eq!(soft_limit(x), x);
        }
        for x in [0.95f32, 1.0, 1.8, -1.8, 40.0] {
            let y = soft_limit(x);
            assert!(y.abs() <= 1.0 && y.abs() > 0.9 && y.signum() == x.signum(), "{x} gave {y}");
        }
        // Smooth: bigger stays bigger.
        assert!(soft_limit(1.2) < soft_limit(1.5));
    }

    #[test]
    fn at_full_speed_samples_pass_through_untouched() {
        let shared = Arc::new(Shared::default());
        let song = Stretched::new(Counting { at: 0, frames: 50 }, shared.clone());
        let straight: Vec<rodio::Sample> = Counting { at: 0, frames: 50 }.collect();
        assert_eq!(song.collect::<Vec<_>>(), straight);
        assert_eq!(shared.delay.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn slowed_down_the_song_lasts_longer_and_reports_what_it_holds_back() {
        let shared = Arc::new(Shared::default());
        shared.speed.store(0.5f32.to_bits(), Ordering::Relaxed);
        let mut song = Stretched::new(Counting { at: 0, frames: 4000 }, shared.clone());
        let heard = song.by_ref().count();
        // At half speed 4,000 frames take about twice as long; blocks round it up.
        let frames = heard / 2;
        assert!((7900..=8400).contains(&frames), "{frames} frames");
        assert!(shared.delay.load(Ordering::Relaxed) > 0);
        // Back at full speed it passes straight through again.
        shared.speed.store(1.0f32.to_bits(), Ordering::Relaxed);
        song.try_seek(Duration::ZERO).unwrap();
        assert_eq!(song.next(), Some(0.0));
        assert_eq!(shared.delay.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn a_start_past_the_end_drops_the_end() {
        let mut l = Loop::default();
        l.set_end(s(20));
        l.set_start(s(30));
        assert_eq!(l, Loop { start: Some(s(30)), end: None, off: false });
    }
}

