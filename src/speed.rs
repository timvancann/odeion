//! How fast the song plays, in whole percent of its recorded speed, pitch kept. A song
//! remembers a slowed speed even while it plays at full speed, so `s` goes back and
//! forth between the two. `,` `.` step by 5%; the slider (and the trainer) set any
//! whole percent from 25 to 110.

use serde::{Deserialize, Serialize};

pub const SLOWEST: u16 = 25;
pub const FASTEST: u16 = 110;
/// `,` and `.` move this many percent, landing on the 5% grid.
pub const STEP: u16 = 5;
/// The slowed speed a song starts with, before it was ever slowed.
const FIRST: u16 = 70;
/// The old slow-down presets, by index, as `resume.json` kept them before percentages.
pub const PRESETS: [u16; 5] = [50, 60, 70, 80, 90];

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Speed {
    /// The slowed (or sped up) speed, kept while `on` is off.
    pub percent: u16,
    /// Playing at `percent`; off plays as recorded.
    pub on: bool,
}

impl Default for Speed {
    fn default() -> Self {
        Speed { percent: FIRST, on: false }
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

    /// `s`: full speed, and again back to the kept speed.
    pub fn toggle(&mut self) {
        self.on = !self.on && self.percent != 100;
    }

    /// Plays at `percent` (clamped to 25 to 110). Full speed switches off, keeping the
    /// speed it came from for `s`.
    pub fn set(&mut self, percent: u16) {
        let percent = percent.clamp(SLOWEST, FASTEST);
        if percent == 100 {
            self.on = false;
        } else {
            (self.percent, self.on) = (percent, true);
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
        Speed { percent: PRESETS[preset.min(PRESETS.len() - 1)], on: slow }
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
    fn old_presets_become_percentages() {
        assert_eq!(Speed::from_preset(true, 2), Speed { percent: 70, on: true });
        assert_eq!(Speed::from_preset(false, 4), Speed { percent: 90, on: false });
        assert_eq!(Speed::from_preset(true, 9).percent, 90);
    }
}
