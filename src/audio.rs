//! Playback: one song at a time, seeking, and an A/B loop.

use rodio::{Decoder, DeviceSinkBuilder, MixerDeviceSink, Player};
use std::fs::File;
use std::io::BufReader;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, channel};
use std::time::Duration;

/// A loop between two points. Without an end there is no loop; without a start it runs
/// from the beginning of the song.
#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub struct Loop {
    pub start: Option<Duration>,
    pub end: Option<Duration>,
}

impl Loop {
    /// A start at or after the end drops the end: the old loop no longer makes sense.
    pub fn set_start(&mut self, at: Duration) {
        self.start = Some(at);
        if self.end.is_some_and(|end| end <= at) {
            self.end = None;
        }
    }

    /// An end before the start is ignored.
    pub fn set_end(&mut self, at: Duration) {
        if at > self.start.unwrap_or_default() {
            self.end = Some(at);
        }
    }

    pub fn active(&self) -> bool {
        self.end.is_some()
    }

    /// Where to jump to when playback has reached `at`, if anywhere.
    pub fn jump(&self, at: Duration) -> Option<Duration> {
        let end = self.end?;
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
                    use rodio::Source;
                    self.duration = song.total_duration();
                    let player = Player::connect_new(sink.mixer());
                    player.append(song);
                    if let Some(at) = self.start_at.take() {
                        let _ = player.try_seek(at);
                    }
                    self.player = Some(player);
                }
                (Ok(_), None) => {}
                (Err(e), _) => self.error = Some(e),
            }
        }
        if let Some(to) = self.looping.jump(self.position()) {
            self.seek(to);
        }
        changed || self.playing()
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
            Some(p) if !p.empty() => p.get_pos(),
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
        Loop { start: Some(s(start)), end: Some(s(end)) }
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
    fn a_start_past_the_end_drops_the_end() {
        let mut l = Loop::default();
        l.set_end(s(20));
        l.set_start(s(30));
        assert_eq!(l, Loop { start: Some(s(30)), end: None });
    }
}

