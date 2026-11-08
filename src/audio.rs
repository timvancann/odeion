//! Playback: one song at a time, seeking, and an A/B loop. Also the volume, the stems'
//! levels, and which output device plays: the one chosen in settings while it is
//! connected, otherwise the system's, followed when it changes.

use rodio::cpal::traits::HostTrait;
use rodio::source::SeekError;
use rodio::{ChannelCount, Decoder, DeviceSinkBuilder, DeviceTrait, MixerDeviceSink, Player, SampleRate, Source};
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

    /// The loop a drag across the waveform draws: pressed at `from`, the pointer now at
    /// `to` and `moved` pixels away. Either way round, each end through `snap` (onto a
    /// beat on the grid), within the song, looping. None when it is a click: it moved
    /// less than `SWEEP`, or is shorter than `MIN_LOOP` once snapped.
    pub fn swept(
        from: Duration,
        to: Duration,
        moved: f32,
        song: Option<Duration>,
        snap: impl Fn(Duration) -> Duration,
    ) -> Option<Loop> {
        if moved < SWEEP {
            return None;
        }
        let within = |d: Duration| song.map_or(d, |s| d.min(s));
        let (start, end) = (within(snap(from.min(to))), within(snap(from.max(to))));
        (end >= start + MIN_LOOP).then_some(Loop { start: Some(start), end: Some(end), off: false })
    }
}

/// How far a press on the waveform moves, in pixels, before it draws a loop rather
/// than being a click.
pub const SWEEP: f32 = 4.0;

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

/// The song slowed down or sped up without changing pitch, and its pitch moved without
/// changing speed (Signalsmith Stretch, a phase vocoder, at its full-quality preset:
/// half a percent of a core). At exactly 100% and no pitch shift it stays out of the
/// way: samples pass through untouched, with no delay. It comes after the loop, so
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
    /// The pitch shift the stretcher has, in semitones.
    transposed: f32,
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
            transposed: 0.0,
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
        let pitch = f32::from_bits(self.shared.pitch.load(Ordering::Relaxed));
        if (speed - 1.0).abs() < 1e-4 && pitch == 0.0 {
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
        if pitch != self.transposed {
            self.stretch.set_transpose_factor_semitones(pitch, None);
            self.transposed = pitch;
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

/// The global EQ, last in the sound (after the stretcher), so it colours what's heard.
/// It reads the EQ from `Shared` once a block and passes samples straight through while
/// it's flat. Boosts can push peaks over full scale, so its output is soft-limited.
struct Equalized<S> {
    inner: S,
    shared: Arc<Shared>,
    channels: usize,
    rate: f32,
    /// The packed EQ last read, and what it makes.
    seen: Option<(u64, u64)>,
    flat: bool,
    gain: f32,
    bands: Vec<crate::eq::Coefficients>,
    /// Each channel's memory of each band, channel by channel.
    states: Vec<crate::eq::State>,
    channel: usize,
    /// Samples until the EQ is read again.
    until: usize,
}

impl<S: Source> Equalized<S> {
    fn new(inner: S, shared: Arc<Shared>) -> Self {
        let channels = inner.channels().get() as usize;
        let rate = inner.sample_rate().get() as f32;
        Equalized {
            inner,
            shared,
            channels,
            rate,
            seen: None,
            flat: true,
            gain: 1.,
            bands: Vec::new(),
            states: Vec::new(),
            channel: 0,
            until: 0,
        }
    }

    fn read_eq(&mut self) {
        let packed = (self.shared.eq.0.load(Ordering::Relaxed), self.shared.eq.1.load(Ordering::Relaxed));
        if self.seen == Some(packed) {
            return;
        }
        self.seen = Some(packed);
        let eq = crate::eq::Eq::unpack(packed);
        let was_flat = self.flat;
        self.flat = eq.flat();
        (self.gain, self.bands) = crate::eq::design(&eq, self.rate);
        // Coming back from flat, the filters start from silence, not stale memory.
        if was_flat || self.states.len() != self.bands.len() * self.channels {
            self.states = vec![Default::default(); self.bands.len() * self.channels];
        }
    }
}

impl<S: Source> Iterator for Equalized<S> {
    type Item = rodio::Sample;

    fn next(&mut self) -> Option<rodio::Sample> {
        if self.until == 0 && self.channel == 0 {
            self.read_eq();
            self.until = BLOCK * self.channels;
        }
        self.until = self.until.saturating_sub(1);
        let x = self.inner.next()?;
        let ch = self.channel;
        self.channel = (self.channel + 1) % self.channels;
        if self.flat {
            return Some(x);
        }
        let n = self.bands.len();
        let mut y = x * self.gain;
        for (c, s) in self.bands.iter().zip(&mut self.states[ch * n..(ch + 1) * n]) {
            y = s.run(c, y);
        }
        Some(soft_limit(y))
    }
}

impl<S: Source> Source for Equalized<S> {
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
        self.states.iter_mut().for_each(|s| *s = Default::default());
        (self.channel, self.until) = (0, 0);
        Ok(())
    }
}

/// ⌥↑ and ⌥↓ change the volume by this much.
pub const VOLUME_STEP: f32 = 0.05;

/// The volume a step up or down from `volume`, on the 5% grid, from silent to full.
pub fn step_volume(volume: f32, up: bool) -> f32 {
    let steps = (volume / VOLUME_STEP).round() + if up { 1.0 } else { -1.0 };
    (steps * VOLUME_STEP).clamp(0.0, 1.0)
}

/// A level from a slider: how far along it the pointer is, as a whole percent.
pub fn level_at(fraction: f32) -> f32 {
    (fraction.clamp(0.0, 1.0) * 100.0).round() / 100.0
}

/// Each stem's gain: its level, the soloed stem alone, the muted ones silent.
pub fn stem_gains(levels: &[f32], muted: &[bool], solo: Option<usize>) -> Vec<f32> {
    (0..muted.len())
        .map(|i| {
            let heard = match solo {
                Some(s) => s == i,
                None => !muted[i],
            };
            if heard { levels.get(i).copied().unwrap_or(1.0) } else { 0.0 }
        })
        .collect()
}

/// An output device: its id, stable across runs and reconnections, and its name.
#[derive(Debug, Clone, PartialEq)]
pub struct Output {
    pub id: String,
    pub name: String,
}

fn output(device: &rodio::Device) -> Option<Output> {
    let id = device.id().ok()?.to_string();
    let name = device.description().map(|d| d.name().to_string()).unwrap_or_else(|_| id.clone());
    Some(Output { id, name })
}

/// The output devices connected now, for the picker. Slow (half a second here): cpal
/// asks each device which formats it plays, which also disturbed playback when done
/// every second. Only when the picker opens, never on a timer.
pub fn outputs() -> Vec<Output> {
    let host = rodio::cpal::default_host();
    host.output_devices().map(|d| d.filter_map(|d| output(&d)).collect()).unwrap_or_default()
}

/// The system's output device now, with its name.
pub fn system_output() -> Option<Output> {
    rodio::cpal::default_host().default_output_device().and_then(|d| output(&d))
}

/// A connected device's name, by its id.
pub fn name_of(id: &str) -> Option<String> {
    device(id).and_then(|d| output(&d)).map(|o| o.name)
}

/// The id of the system's output device: one cheap property read, fine every second.
fn system_id() -> Option<String> {
    let device = rodio::cpal::default_host().default_output_device()?;
    device.id().ok().map(|id| id.to_string())
}

/// A connected device by its id, comparing ids only: no formats asked, so cheap enough
/// to check every second.
fn device(id: &str) -> Option<rodio::Device> {
    let mut devices = rodio::cpal::default_host().devices().ok()?;
    devices.find(|d| d.id().is_ok_and(|d| d.to_string() == id))
}

/// The device to play through: the chosen one while it is connected, otherwise the
/// system's.
pub fn wanted(chosen: Option<&str>, chosen_connected: bool, system: Option<String>) -> Option<String> {
    chosen.filter(|_| chosen_connected).map(str::to_string).or(system)
}

/// Opens the device with id `want`, or the system's when that fails. Returns the id of
/// what opened.
fn open(want: Option<&str>) -> Option<(MixerDeviceSink, Option<String>)> {
    let chosen = want.and_then(|id| {
        let found = device(id)?;
        DeviceSinkBuilder::from_device(found).and_then(|b| b.open_stream()).ok()
    });
    let (mut sink, id) = match chosen {
        Some(sink) => (sink, want.map(str::to_string)),
        None => (DeviceSinkBuilder::open_default_sink().ok()?, system_id()),
    };
    sink.log_on_drop(false);
    Some((sink, id))
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
    /// Pitch shift in semitones (f32 bits): 0 as recorded.
    pitch: AtomicU32,
    /// Frames the slow-down holds back: played but not heard yet.
    delay: AtomicU64,
    /// Times the loop has jumped back to its start, for the speed trainer.
    wraps: AtomicU64,
    /// The EQ, packed (`Eq::pack`).
    eq: (AtomicU64, AtomicU64),
}

const NONE: u64 = u64::MAX;

impl Default for Shared {
    fn default() -> Self {
        Shared {
            start: AtomicU64::new(0),
            end: AtomicU64::new(NONE),
            played: AtomicU64::new(0),
            speed: AtomicU32::new(1.0f32.to_bits()),
            pitch: AtomicU32::new(0.0f32.to_bits()),
            delay: AtomicU64::new(0),
            wraps: AtomicU64::new(0),
            eq: (AtomicU64::new(0), AtomicU64::new(0)),
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
                    self.shared.wraps.fetch_add(1, Ordering::Relaxed);
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
    /// The id of the device `sink` plays through.
    output: Option<String>,
    /// The device chosen in settings, by id; none follows the system's.
    pub chosen: Option<String>,
    /// Pitch shift in semitones, kept across songs like the speed.
    pitch: f32,
    /// From 0 (silent) to 1 (as recorded).
    volume: f32,
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
    /// The global EQ, handed to each song's sound.
    eq: crate::eq::Eq,
    pub duration: Option<Duration>,
    pub looping: Loop,
    pub error: Option<String>,
    /// Why the last song didn't open, until the window takes it to say so.
    pub failed: Option<String>,
    /// Where to start once the song has opened.
    start_at: Option<Duration>,
    /// The loop and position, shared with the audio thread.
    shared: Arc<Shared>,
    rate: u32,
}

impl Audio {
    pub fn new(volume: f32, chosen: Option<String>) -> Audio {
        let connected = chosen.as_deref().is_some_and(|id| device(id).is_some());
        let want = wanted(chosen.as_deref(), connected, system_id());
        let (sink, output) = open(want.as_deref()).map_or((None, None), |(s, id)| (Some(s), id));
        Audio {
            error: sink.is_none().then(|| "no audio output".to_string()),
            failed: None,
            sink,
            output,
            chosen,
            volume,
            pitch: 0.0,
            player: None,
            loading: None,
            start_at: None,
            shared: Arc::default(),
            rate: 44_100,
            files: Vec::new(),
            gains: gains(&[]),
            start_paused: false,
            speed: 1.0,
            eq: Default::default(),
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

    /// Moves the pitch by `cents` from now on, speed unchanged.
    pub fn set_pitch(&mut self, cents: i16) {
        self.pitch = cents as f32 / 100.0;
        self.shared.pitch.store(self.pitch.to_bits(), Ordering::Relaxed);
    }

    /// The global EQ from now on.
    pub fn set_eq(&mut self, eq: crate::eq::Eq) {
        self.eq = eq;
        self.store_eq();
    }

    fn store_eq(&self) {
        let (lo, hi) = self.eq.pack();
        self.shared.eq.0.store(lo, Ordering::Relaxed);
        self.shared.eq.1.store(hi, Ordering::Relaxed);
    }

    /// How often the loop has jumped back to its start since the song opened.
    pub fn wraps(&self) -> u64 {
        self.shared.wraps.load(Ordering::Relaxed)
    }

    pub fn volume(&self) -> f32 {
        self.volume
    }

    pub fn set_volume(&mut self, volume: f32) {
        self.volume = volume.clamp(0.0, 1.0);
        if let Some(p) = &self.player {
            p.set_volume(self.volume);
        }
    }

    /// Moves to the device that should play, when that is not the one playing: the
    /// chosen one was connected or unplugged, or the system's changed. What plays
    /// carries on from the same moment, playing or paused as it was. Returns the
    /// device's name when it moved.
    pub fn follow_output(&mut self) -> Option<String> {
        let connected = self.chosen.as_deref().is_some_and(|id| device(id).is_some());
        let want = wanted(self.chosen.as_deref(), connected, system_id())?;
        if self.output.as_ref() == Some(&want) {
            return None;
        }
        let (at, playing, looping) = (self.position(), self.playing(), self.looping);
        let at = if self.ended() { Duration::ZERO } else { at };
        let now: Vec<f32> = self.gains.iter().map(|g| f32::from_bits(g.load(Ordering::Relaxed))).collect();
        // The player goes before its device does.
        self.player = None;
        self.sink = None;
        let (sink, opened) = open(Some(&want)).map_or((None, None), |(s, id)| (Some(s), id));
        (self.sink, self.output) = (sink, opened);
        self.error = self.sink.is_none().then(|| "no audio output".to_string());
        if !self.files.is_empty() && self.loading.is_none() {
            self.switch(&self.files.clone(), &now, at, looping);
            self.start_paused = !playing;
        }
        let name = self.output.as_deref().and_then(device).and_then(|d| output(&d)).map(|o| o.name);
        Some(name.unwrap_or_else(|| "the system's output".into()))
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
                    player.set_volume(self.volume);
                    self.shared.speed.store(self.speed.to_bits(), Ordering::Relaxed);
                    self.shared.pitch.store(self.pitch.to_bits(), Ordering::Relaxed);
                    self.store_eq();
                    // Paused before anything is added, so not a sample slips out.
                    if std::mem::take(&mut self.start_paused) {
                        player.pause();
                    }
                    let looped = Looped::new(song, self.shared.clone());
                    player.append(Equalized::new(Stretched::new(looped, self.shared.clone()), self.shared.clone()));
                    if let Some(at) = self.start_at.take() {
                        let _ = player.try_seek(at);
                    }
                    self.player = Some(player);
                }
                (Ok(_), None) => {}
                (Err(e), _) => self.failed = Some(e),
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

    /// The recording being opened starts paused, at its start point, ready for Space.
    pub fn open_paused(&mut self) {
        self.start_paused = true;
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

/// `m` at `at`: drops the marker there, or clears it, wherever it is (dragging moves it).
pub fn place_marker(marker: Option<Duration>, at: Duration) -> Option<Duration> {
    match marker {
        Some(_) => None,
        None => Some(at),
    }
}

/// Where `o` plays from: the marker when there is one, else the loop's start, else the
/// top.
pub fn replay_from(marker: Option<Duration>, looping: Loop) -> Duration {
    marker.or(looping.start).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_marker_is_dropped_and_then_cleared_from_anywhere() {
        let s = Duration::from_secs_f64;
        assert_eq!(place_marker(None, s(12.0)), Some(s(12.0)));
        assert_eq!(place_marker(Some(s(12.0)), s(30.0)), None);
        assert_eq!(place_marker(Some(s(12.0)), s(12.2)), None);
        let looping = Loop { start: Some(s(40.0)), end: Some(s(50.0)), off: false };
        assert_eq!(replay_from(Some(s(12.0)), looping), s(12.0));
        assert_eq!(replay_from(None, looping), s(40.0));
        assert_eq!(replay_from(None, Loop::default()), Duration::ZERO);
    }

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
    fn a_drag_across_the_waveform_draws_a_loop_and_a_short_one_is_a_click() {
        let free = |d: Duration| d;
        // Either way round, looping.
        assert_eq!(Loop::swept(s(12), s(8), 40.0, None, free), Some(looped(8, 12)));
        assert_eq!(Loop::swept(s(8), s(12), 40.0, None, free), Some(looped(8, 12)));
        // A press that hardly moved is a click, however far apart the moments.
        assert_eq!(Loop::swept(s(8), s(12), SWEEP - 0.5, None, free), None);
        // Shorter than the shortest loop is a click too.
        assert_eq!(Loop::swept(ms(8000), ms(8050), 40.0, None, free), None);
        assert!(Loop::swept(ms(8000), ms(8100), 40.0, None, free).is_some());
        // Each end snaps (to the nearest whole second here, like beats).
        let beat = |d: Duration| s(d.as_secs_f64().round() as u64);
        assert_eq!(Loop::swept(ms(7800), ms(12300), 40.0, None, beat), Some(looped(8, 12)));
        // Both ends on the same beat: a click.
        assert_eq!(Loop::swept(ms(7800), ms(8300), 40.0, None, beat), None);
        // Never past the song's end.
        assert_eq!(Loop::swept(s(8), s(40), 40.0, Some(s(30)), free), Some(looped(8, 30)));
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
    fn volume_steps_on_a_5_percent_grid_from_silent_to_full() {
        assert_eq!(step_volume(1.0, true), 1.0);
        assert!((step_volume(1.0, false) - 0.95).abs() < 1e-6);
        assert!((step_volume(0.42, true) - 0.45).abs() < 1e-6);
        assert_eq!(step_volume(0.02, false), 0.0);
        assert_eq!(level_at(1.4), 1.0);
        assert_eq!(level_at(0.333), 0.33);
        assert_eq!(level_at(-0.2), 0.0);
    }

    #[test]
    fn stems_play_at_their_level_unless_muted_or_another_is_soloed() {
        let levels = [1.0, 0.5, 0.25];
        assert_eq!(stem_gains(&levels, &[false, false, true], None), [1.0, 0.5, 0.0]);
        // Solo plays a stem at its own level, even a muted one.
        assert_eq!(stem_gains(&levels, &[false, true, false], Some(1)), [0.0, 0.5, 0.0]);
        // Levels not known yet are full.
        assert_eq!(stem_gains(&[], &[false, false], None), [1.0, 1.0]);
    }

    #[test]
    fn the_chosen_output_plays_while_connected_else_the_systems() {
        let id = |s: &str| Some(s.to_string());
        assert_eq!(wanted(Some("headphones"), true, id("speakers")), id("headphones"));
        assert_eq!(wanted(Some("interface"), false, id("speakers")), id("speakers"));
        assert_eq!(wanted(None, false, id("headphones")), id("headphones"));
        assert_eq!(wanted(None, false, None), None);
    }

    #[test]
    fn a_seek_hands_the_new_loop_over_first() {
        // No output device: the loop handover happens all the same.
        let mut audio = Audio {
            sink: None,
            output: None,
            chosen: None,
            volume: 1.0,
            pitch: 0.0,
            player: None,
            loading: None,
            files: Vec::new(),
            gains: gains(&[]),
            start_paused: false,
            speed: 1.0,
            eq: Default::default(),
            duration: None,
            looping: looped(0, 16),
            error: None,
            failed: None,
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
    fn a_flat_eq_passes_samples_untouched_and_a_preamp_scales_them() {
        let shared = Arc::new(Shared::default());
        let song = Equalized::new(Counting { at: 0, frames: 600 }, shared.clone());
        let straight: Vec<rodio::Sample> = Counting { at: 0, frames: 600 }.collect();
        assert_eq!(song.collect::<Vec<_>>(), straight);
        let eq = crate::eq::Eq { on: true, preamp: -6, ..Default::default() };
        let (lo, hi) = eq.pack();
        shared.eq.0.store(lo, Ordering::Relaxed);
        shared.eq.1.store(hi, Ordering::Relaxed);
        // Frames 0 to 0.5, quiet enough that the limiter stays out of it.
        let quiet = rodio::source::Source::amplify(Counting { at: 0, frames: 600 }, 1. / 1200.);
        let heard: Vec<rodio::Sample> = Equalized::new(quiet, shared).collect();
        let half = 10f32.powf(-6. / 20.);
        for (i, y) in heard.iter().enumerate().step_by(97) {
            let x = (i / 2) as f32 / 1200.;
            assert!((y - x * half).abs() < 1e-4, "sample {i}: {y} for {x}");
        }
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
    fn sped_up_the_song_is_over_sooner_and_only_exactly_100_passes_through() {
        let shared = Arc::new(Shared::default());
        shared.speed.store(1.1f32.to_bits(), Ordering::Relaxed);
        let heard = Stretched::new(Counting { at: 0, frames: 4400 }, shared.clone()).count() / 2;
        // 4,400 frames at 110% take about 4,000; blocks round it up.
        assert!((3900..=4300).contains(&heard), "{heard} frames");
        // 99% is slowed, not passed through as full speed was before 25 to 110%.
        shared.speed.store(0.99f32.to_bits(), Ordering::Relaxed);
        let mut song = Stretched::new(Counting { at: 0, frames: 4000 }, shared.clone());
        song.next();
        assert!(shared.delay.load(Ordering::Relaxed) > 0);
    }

    #[test]
    fn a_pitch_shift_alone_goes_through_the_stretcher_at_full_length() {
        let shared = Arc::new(Shared::default());
        shared.pitch.store(2.0f32.to_bits(), Ordering::Relaxed);
        let heard = Stretched::new(Counting { at: 0, frames: 4000 }, shared.clone()).count() / 2;
        assert!((3900..=4300).contains(&heard), "{heard} frames");
        assert!(shared.delay.load(Ordering::Relaxed) > 0);
    }

    #[test]
    fn a_start_past_the_end_drops_the_end() {
        let mut l = Loop::default();
        l.set_end(s(20));
        l.set_start(s(30));
        assert_eq!(l, Loop { start: Some(s(30)), end: None, off: false });
    }
}

