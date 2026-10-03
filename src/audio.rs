//! Playback: one song at a time, seeking, and an A/B loop.

use rodio::source::SeekError;
use rodio::{ChannelCount, Decoder, DeviceSinkBuilder, MixerDeviceSink, Player, SampleRate, Source};
use std::fs::File;
use std::io::BufReader;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, channel};
use std::time::Duration;

/// A loop between two points. Without an end there is no loop; without a start it runs
/// from the beginning of the song. Switched off, it keeps its edges but doesn't loop.
#[derive(Debug, Default, Clone, Copy, PartialEq)]
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
    pub fn at(&mut self, real: Duration, playing: bool, now: Duration) -> Duration {
        if playing && let Some((from, then)) = self.anchor {
            let guess = from + now.saturating_sub(then);
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

type Song = Decoder<BufReader<File>>;

/// What the audio thread and the window share: the loop to keep, in frames (`NONE` for
/// no loop), and how far the song has played.
#[derive(Debug)]
struct Shared {
    start: AtomicU64,
    end: AtomicU64,
    played: AtomicU64,
}

const NONE: u64 = u64::MAX;

impl Default for Shared {
    fn default() -> Self {
        Shared { start: AtomicU64::new(0), end: AtomicU64::new(NONE), played: AtomicU64::new(0) }
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
    pub path: Option<PathBuf>,
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
            path: None,
            duration: None,
            looping: Loop::default(),
        }
    }

    /// Starts a song. Opening happens on a thread: a file still syncing can take a while.
    pub fn play(&mut self, path: &Path) {
        self.player = None;
        self.duration = None;
        self.error = None;
        self.looping = Loop::default();
        self.path = Some(path.to_path_buf());
        let (tx, rx) = channel();
        let path = path.to_path_buf();
        std::thread::spawn(move || {
            let song = File::open(&path)
                .map_err(|e| e.to_string())
                .and_then(|f| Decoder::try_from(f).map_err(|e| e.to_string()));
            let _ = tx.send(song);
        });
        self.loading = Some(rx);
        self.start_at = None;
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
                    player.append(Looped::new(song, self.shared.clone()));
                    if let Some(at) = self.start_at.take() {
                        let _ = player.try_seek(at);
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
            Some(p) if !p.empty() => Duration::from_secs_f64(
                self.shared.played.load(Ordering::Relaxed) as f64 / self.rate as f64,
            ),
            Some(_) => self.duration.unwrap_or_default(),
            None => Duration::ZERO,
        }
    }

    /// Pause or resume. A song that has ended starts again, keeping its loop points.
    pub fn toggle(&mut self) {
        match &self.player {
            Some(p) if p.empty() => {
                if let Some(path) = self.path.clone() {
                    let looping = self.looping;
                    self.play(&path);
                    self.looping = looping;
                }
            }
            Some(p) if p.is_paused() => p.play(),
            Some(p) => p.pause(),
            None => {}
        }
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
        assert_eq!(p.at(s(10), true, ms(0)), s(10));
        // The player still says 10 s; 16 ms later the playhead has moved on anyway.
        assert_eq!(p.at(s(10), true, ms(16)), s(10) + ms(16));
        assert_eq!(p.at(s(10) + ms(40), true, ms(32)), s(10) + ms(32));
        // A loop jump back to 4 s is followed at once.
        assert_eq!(p.at(s(4), true, ms(48)), s(4));
        assert_eq!(p.at(s(4), true, ms(64)), s(4) + ms(16));
        // Paused, it stays where the player is.
        assert_eq!(p.at(s(7), false, ms(80)), s(7));
        assert_eq!(p.at(s(7), false, ms(96)), s(7));
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
    fn a_start_past_the_end_drops_the_end() {
        let mut l = Loop::default();
        l.set_end(s(20));
        l.set_start(s(30));
        assert_eq!(l, Loop { start: Some(s(30)), end: None, off: false });
    }
}

