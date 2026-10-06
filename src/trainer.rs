//! The speed trainer: a loop starts slow and speeds up as it repeats, from one speed to
//! another in steps, every so many repeats, then stays at the top. The loop's wraps
//! are counted in the audio thread (`Looped`); this turns a count into a speed.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Trainer {
    /// Percent of the recorded speed to start at, and to end at.
    pub from: u16,
    pub to: u16,
    /// Percent added at each step.
    pub step: u16,
    /// Repeats of the loop between steps.
    pub every: u16,
}

impl Default for Trainer {
    fn default() -> Self {
        Trainer { from: 70, to: 100, step: 5, every: 2 }
    }
}

/// A setting on the trainer's sheet, in its order there.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Field {
    From,
    To,
    Step,
    Every,
}

pub const FIELDS: [Field; 4] = [Field::From, Field::To, Field::Step, Field::Every];

impl Trainer {
    /// The speed after `repeats` wraps of the loop.
    pub fn percent(&self, repeats: u64) -> u16 {
        let steps = repeats / self.every.max(1) as u64;
        let up = (steps * self.step as u64).min(u16::MAX as u64) as u16;
        self.from.saturating_add(up).min(self.to.max(self.from))
    }

    pub fn get(&self, field: Field) -> u16 {
        match field {
            Field::From => self.from,
            Field::To => self.to,
            Field::Step => self.step,
            Field::Every => self.every,
        }
    }

    /// One setting up (true) or down a notch: speeds by 5%, the step by 1%, repeats by
    /// one. Speeds stay within 25 to 110, `from` below `to`.
    pub fn nudge(&mut self, field: Field, up: bool) {
        use crate::speed::{FASTEST, SLOWEST, STEP};
        let by = |v: u16, n: u16, lo: u16, hi: u16| {
            if up { v.saturating_add(n).min(hi) } else { v.saturating_sub(n).max(lo) }
        };
        match field {
            Field::From => self.from = by(self.from, STEP, SLOWEST, self.to.saturating_sub(STEP)),
            Field::To => self.to = by(self.to, STEP, self.from + STEP, FASTEST),
            Field::Step => self.step = by(self.step, 1, 1, 25),
            Field::Every => self.every = by(self.every, 1, 1, 16),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn speeds_up_a_step_every_n_repeats_and_stays_at_the_top() {
        let t = Trainer::default();
        assert_eq!(t.percent(0), 70);
        assert_eq!(t.percent(1), 70);
        assert_eq!(t.percent(2), 75);
        assert_eq!(t.percent(5), 80);
        assert_eq!(t.percent(12), 100);
        assert_eq!(t.percent(1_000_000), 100);
        let odd = Trainer { from: 60, to: 87, step: 10, every: 1 };
        assert_eq!(odd.percent(3), 87);
    }

    #[test]
    fn settings_stay_sensible() {
        let mut t = Trainer::default();
        for _ in 0..20 {
            t.nudge(Field::From, true);
        }
        assert_eq!(t.from, 95);
        for _ in 0..20 {
            t.nudge(Field::To, false);
        }
        assert_eq!(t.to, 100);
        for _ in 0..30 {
            t.nudge(Field::Every, false);
            t.nudge(Field::Step, true);
        }
        assert_eq!((t.every, t.step), (1, 25));
        t.nudge(Field::To, true);
        t.nudge(Field::To, true);
        t.nudge(Field::To, true);
        assert_eq!(t.to, 110);
        assert_eq!(t.get(Field::To), 110);
    }
}
