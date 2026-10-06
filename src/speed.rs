//! How the song plays: its speed, in whole percent of the recorded speed, and its
//! pitch, in cents, each apart from the other. A song remembers both even while it
//! plays as recorded, so `s` goes back and forth between the two. `,` `.` step the
//! speed by 5%, the slider (and the trainer) set any whole percent from 25 to 110;
//! `k j` move the pitch a semitone and `K J` 5 cents, up to an octave either way.

use serde::{Deserialize, Serialize};

pub const SLOWEST: u16 = 25;
pub const FASTEST: u16 = 110;
/// `,` and `.` move this many percent, landing on the 5% grid.
pub const STEP: u16 = 5;
/// The slowed speed a song starts with, before it was ever slowed.
const FIRST: u16 = 70;
/// The pitch goes no further than an octave up or down.
pub const OCTAVE: i16 = 1200;
/// The old slow-down presets, by index, as `resume.json` kept them before percentages.
pub const PRESETS: [u16; 5] = [50, 60, 70, 80, 90];

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Speed {
    /// The slowed (or sped up) speed, kept while `on` is off.
    pub percent: u16,
    /// The pitch, up or down, kept while `on` is off.
    #[serde(default)]
    pub cents: i16,
    /// Playing at `percent` and `cents`; off plays as recorded.
    pub on: bool,
}

impl Default for Speed {
    fn default() -> Self {
        Speed { percent: FIRST, cents: 0, on: false }
    }
}

impl Speed {
    /// What plays now, in percent.
    pub fn now(&self) -> u16 {
        if self.on { self.percent } else { 100 }
    }

    /// What plays now, as a factor for the audio: 1 as recorded.
    pub fn factor(&self) -> f32 {
        self.now() as f32 / 100.0
    }

    /// The pitch now, in cents.
    pub fn pitch(&self) -> i16 {
        if self.on { self.cents } else { 0 }
    }

    /// `s`: as recorded, and again back to the kept speed and pitch.
    pub fn toggle(&mut self) {
        self.on = !self.on && (self.percent != 100 || self.cents != 0);
    }

    /// Plays at `percent` (clamped to 25 to 110), at the pitch playing now. Back to as
    /// recorded switches off, keeping what it came from for `s`.
    pub fn set(&mut self, percent: u16) {
        self.play(percent.clamp(SLOWEST, FASTEST), self.pitch());
    }

    /// Plays at `cents` (up to an octave either way), at the speed playing now.
    pub fn set_pitch(&mut self, cents: i16) {
        self.play(self.now(), cents.clamp(-OCTAVE, OCTAVE));
    }

    /// `k j` (by a semitone, 100) and `K J` (by 5 cents).
    pub fn shift(&mut self, by: i16) {
        self.set_pitch(self.pitch().saturating_add(by));
    }

    fn play(&mut self, percent: u16, cents: i16) {
        if percent == 100 && cents == 0 {
            self.on = false;
        } else {
            (self.percent, self.cents, self.on) = (percent, cents, true);
        }
    }

    /// `,` (slower) and `.` (faster): to the next 5% from what plays now.
    pub fn step(&mut self, faster: bool) {
        let now = self.now();
        let to = if faster { (now / STEP + 1) * STEP } else { now.div_ceil(STEP).saturating_sub(1) * STEP };
        self.set(to);
    }

    /// From the old `resume.json`: slow-down on or off and a preset's index.
    pub fn from_preset(slow: bool, preset: usize) -> Speed {
        Speed { percent: PRESETS[preset.min(PRESETS.len() - 1)], cents: 0, on: slow }
    }
}

/// A pitch as musicians say it: "+2 st", "−35 c", "+1 st +15 c", or "0" as recorded.
pub fn pitch_label(cents: i16) -> String {
    let sign = |n: i16| if n < 0 { "−" } else { "+" };
    let (st, c) = (cents / 100, cents % 100);
    match (st, c) {
        (0, 0) => "0".to_string(),
        (0, c) => format!("{}{} c", sign(c), c.abs()),
        (st, 0) => format!("{}{} st", sign(st), st.abs()),
        (st, c) => format!("{}{} st {}{} c", sign(st), st.abs(), sign(c), c.abs()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn steps_go_5_percent_from_what_plays_between_25_and_110() {
        let mut s = Speed::default();
        s.step(false);
        assert_eq!((s.now(), s.on), (95, true));
        s.step(true);
        assert_eq!((s.now(), s.on), (100, false));
        s.step(true);
        assert_eq!(s.now(), 105);
        s.step(true);
        s.step(true);
        assert_eq!(s.now(), FASTEST);
        s.set(27);
        s.step(false);
        assert_eq!(s.now(), SLOWEST);
        s.step(false);
        assert_eq!(s.now(), SLOWEST);
        // Off the grid (the slider, the trainer), a step lands back on it.
        s.set(73);
        s.step(true);
        assert_eq!(s.now(), 75);
        s.set(73);
        s.step(false);
        assert_eq!(s.now(), 70);
    }

    #[test]
    fn s_goes_to_full_speed_and_back_to_the_kept_speed() {
        let mut s = Speed::default();
        s.set(65);
        s.toggle();
        assert_eq!((s.now(), s.percent), (100, 65));
        s.toggle();
        assert_eq!(s.now(), 65);
        // Stepping up to full speed keeps where it came from.
        s.set(95);
        s.step(true);
        assert_eq!(s.now(), 100);
        s.toggle();
        assert_eq!(s.now(), 95);
        assert_eq!(s.factor(), 0.95);
    }

    #[test]
    fn pitch_moves_by_semitones_and_cents_up_to_an_octave() {
        let mut s = Speed::default();
        s.shift(100);
        // A pitch alone plays at full speed: the kept 70% doesn't come along.
        assert_eq!((s.now(), s.pitch(), s.on), (100, 100, true));
        s.shift(-5);
        assert_eq!(s.pitch(), 95);
        for _ in 0..20 {
            s.shift(100);
        }
        assert_eq!(s.pitch(), OCTAVE);
        s.set_pitch(-2000);
        assert_eq!(s.pitch(), -OCTAVE);
        // Speed and pitch change apart.
        s.set_pitch(200);
        s.set(80);
        assert_eq!((s.now(), s.pitch()), (80, 200));
    }

    #[test]
    fn s_puts_speed_and_pitch_back_and_brings_both_back() {
        let mut s = Speed::default();
        s.set(80);
        s.shift(-200);
        s.toggle();
        assert_eq!((s.now(), s.pitch(), s.factor()), (100, 0, 1.0));
        s.toggle();
        assert_eq!((s.now(), s.pitch()), (80, -200));
        // Back to as recorded by hand keeps both for `s`.
        s.set(100);
        s.set_pitch(0);
        assert!(!s.on);
        s.set(100);
        s.toggle();
        assert_eq!((s.now(), s.pitch()), (100, -200));
    }

    #[test]
    fn pitches_read_as_musicians_say_them() {
        assert_eq!(pitch_label(0), "0");
        assert_eq!(pitch_label(200), "+2 st");
        assert_eq!(pitch_label(-35), "−35 c");
        assert_eq!(pitch_label(115), "+1 st +15 c");
        assert_eq!(pitch_label(-105), "−1 st −5 c");
    }

    #[test]
    fn old_presets_become_percentages() {
        assert_eq!(Speed::from_preset(true, 2), Speed { percent: 70, cents: 0, on: true });
        assert_eq!(Speed::from_preset(false, 4), Speed { percent: 90, cents: 0, on: false });
        assert_eq!(Speed::from_preset(true, 9).percent, 90);
    }
}
