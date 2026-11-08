//! The global EQ: ten bands in octaves from 31 Hz to 16 kHz, like Winamp's, plus a
//! preamp, each in whole dB from -12 to +12. It's there to make the speakers and
//! headphones sound right, so it comes last in the sound, after the stretcher. Built-in
//! presets and the user's own saved ones; kept in `settings.json`.

use serde::{Deserialize, Serialize};

/// Each band's centre, in Hz.
pub const BANDS: [f32; 10] = [31., 62., 125., 250., 500., 1000., 2000., 4000., 8000., 16000.];
/// About an octave wide, so neighbouring bands meet.
const Q: f32 = 1.41;
/// The most a band or the preamp lifts or cuts, in dB.
pub const RANGE: i8 = 12;

#[derive(Debug, Default, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Eq {
    pub on: bool,
    pub preamp: i8,
    pub bands: [i8; 10],
}

impl Eq {
    /// Off, or on with nothing lifted or cut: samples pass through untouched.
    pub fn flat(&self) -> bool {
        !self.on || (self.preamp == 0 && self.bands == [0; 10])
    }

    /// The preamp (`at` 0) or band `at - 1`, as the sheet numbers them.
    pub fn get(&self, at: usize) -> i8 {
        if at == 0 { self.preamp } else { self.bands[at - 1] }
    }

    /// Moves the preamp or a band by `by` dB, within the range.
    pub fn nudge(&mut self, at: usize, by: i8) {
        self.set(at, self.get(at).saturating_add(by));
    }

    pub fn set(&mut self, at: usize, db: i8) {
        let db = db.clamp(-RANGE, RANGE);
        match at {
            0 => self.preamp = db,
            n if n <= 10 => self.bands[n - 1] = db,
            _ => {}
        }
    }

    /// The EQ in two words, for the audio thread to read without a lock: the preamp
    /// and bands as bytes, and whether it's on.
    pub fn pack(&self) -> (u64, u64) {
        let mut bytes = [0u8; 16];
        bytes[0] = self.preamp as u8;
        for (b, &db) in bytes[1..11].iter_mut().zip(&self.bands) {
            *b = db as u8;
        }
        bytes[11] = self.on as u8;
        let (lo, hi) = bytes.split_at(8);
        (u64::from_le_bytes(lo.try_into().unwrap()), u64::from_le_bytes(hi.try_into().unwrap()))
    }

    pub fn unpack((lo, hi): (u64, u64)) -> Eq {
        let mut bytes = [0u8; 16];
        bytes[..8].copy_from_slice(&lo.to_le_bytes());
        bytes[8..].copy_from_slice(&hi.to_le_bytes());
        let mut bands = [0i8; 10];
        for (db, &b) in bands.iter_mut().zip(&bytes[1..11]) {
            *db = b as i8;
        }
        Eq { on: bytes[11] != 0, preamp: bytes[0] as i8, bands }
    }

    /// The gains of `preset`, switching the EQ on.
    pub fn apply(&mut self, preset: &Preset) {
        (self.on, self.preamp, self.bands) = (true, preset.preamp, preset.bands);
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Preset {
    pub name: String,
    pub preamp: i8,
    pub bands: [i8; 10],
}

fn preset(name: &str, preamp: i8, bands: [i8; 10]) -> Preset {
    Preset { name: name.into(), preamp, bands }
}

/// The presets Odeion comes with. Boosting ones lower the preamp a little to leave room.
pub fn built_in() -> Vec<Preset> {
    vec![
        preset("Flat", 0, [0; 10]),
        preset("More bass", -3, [6, 5, 4, 2, 0, 0, 0, 0, 0, 0]),
        preset("Less bass", 0, [-6, -5, -4, -2, 0, 0, 0, 0, 0, 0]),
        preset("More treble", -3, [0, 0, 0, 0, 0, 0, 2, 4, 5, 6]),
        preset("Less treble", 0, [0, 0, 0, 0, 0, 0, -2, -4, -5, -6]),
        preset("Voice", -2, [-3, -2, -1, 0, 2, 4, 4, 2, 0, -1]),
        preset("Loudness", -3, [5, 4, 2, 0, -1, -1, 0, 2, 4, 5]),
    ]
}

/// Every preset: the built-in ones, then the saved ones.
pub fn all(saved: &[Preset]) -> Vec<Preset> {
    built_in().into_iter().chain(saved.iter().cloned()).collect()
}

/// Which preset `eq` is, by its gains: its name, or "Custom".
pub fn name_of(eq: &Eq, saved: &[Preset]) -> String {
    all(saved)
        .into_iter()
        .find(|p| p.preamp == eq.preamp && p.bands == eq.bands)
        .map_or_else(|| "Custom".into(), |p| p.name)
}

/// The preset after (or before) the one `eq` is, round the list; from "Custom", the
/// first (or last).
pub fn step(eq: &Eq, saved: &[Preset], forward: bool) -> Preset {
    let all = all(saved);
    let n = all.len();
    let at = all.iter().position(|p| p.preamp == eq.preamp && p.bands == eq.bands);
    let next = match (at, forward) {
        (Some(i), true) => (i + 1) % n,
        (Some(i), false) => (i + n - 1) % n,
        (None, true) => 0,
        (None, false) => n - 1,
    };
    all[next].clone()
}

/// Saves `eq`'s gains as `name`, overwriting a saved preset of that name. A built-in
/// name is refused, as is an empty one.
pub fn save(saved: &mut Vec<Preset>, name: &str, eq: &Eq) -> Result<(), String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("a preset needs a name".into());
    }
    if built_in().iter().any(|p| p.name.eq_ignore_ascii_case(name)) {
        return Err(format!("{name} is built in"));
    }
    let preset = Preset { name: name.into(), preamp: eq.preamp, bands: eq.bands };
    match saved.iter_mut().find(|p| p.name.eq_ignore_ascii_case(name)) {
        Some(old) => *old = preset,
        None => saved.push(preset),
    }
    Ok(())
}

/// Removes the saved preset `eq` is, if it is one. Returns its name.
pub fn remove(saved: &mut Vec<Preset>, eq: &Eq) -> Option<String> {
    let at = saved.iter().position(|p| p.preamp == eq.preamp && p.bands == eq.bands)?;
    Some(saved.remove(at).name)
}

/// A band's peaking filter (RBJ's cookbook), normalised so a0 is 1.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Coefficients {
    b: [f32; 3],
    a: [f32; 2],
}

impl Coefficients {
    pub fn peaking(rate: f32, centre: f32, db: f32) -> Coefficients {
        let a = 10f32.powf(db / 40.);
        let w = 2. * std::f32::consts::PI * centre / rate;
        let alpha = w.sin() / (2. * Q);
        let cos = w.cos();
        let a0 = 1. + alpha / a;
        Coefficients {
            b: [(1. + alpha * a) / a0, -2. * cos / a0, (1. - alpha * a) / a0],
            a: [-2. * cos / a0, (1. - alpha / a) / a0],
        }
    }
}

/// One channel's memory of one band (transposed direct form II).
#[derive(Debug, Clone, Copy, Default)]
pub struct State([f32; 2]);

impl State {
    pub fn run(&mut self, c: &Coefficients, x: f32) -> f32 {
        let y = c.b[0] * x + self.0[0];
        self.0[0] = c.b[1] * x - c.a[0] * y + self.0[1];
        self.0[1] = c.b[2] * x - c.a[1] * y;
        y
    }
}

/// `eq` as filters at `rate`: the preamp's linear gain and each band's coefficients,
/// leaving out bands too high for the rate. A band at 0 dB passes samples unchanged
/// but stays, so moving a band never resets the others' memory (that would click).
pub fn design(eq: &Eq, rate: f32) -> (f32, Vec<Coefficients>) {
    let gain = 10f32.powf(eq.preamp as f32 / 20.);
    let bands = BANDS
        .iter()
        .zip(eq.bands)
        .filter(|&(&f, _)| f < rate * 0.45)
        .map(|(&f, db)| Coefficients::peaking(rate, f, db as f32))
        .collect();
    (gain, bands)
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATE: f32 = 44_100.;

    /// The gain in dB `eq` gives a sine at `hz`, measured after it settles.
    fn gain_at(eq: &Eq, hz: f32) -> f32 {
        let (pre, bands) = design(eq, RATE);
        let mut states = vec![State::default(); bands.len()];
        let n = RATE as usize;
        let (mut inp, mut out) = (0f64, 0f64);
        for i in 0..n {
            let x = (2. * std::f32::consts::PI * hz * i as f32 / RATE).sin();
            let mut y = x * pre;
            for (c, s) in bands.iter().zip(&mut states) {
                y = s.run(c, y);
            }
            if i > n / 2 {
                inp += (x * x) as f64;
                out += (y * y) as f64;
            }
        }
        (10. * (out / inp).log10()) as f32
    }

    #[test]
    fn each_band_gives_its_gain_at_its_own_frequency() {
        for (i, &hz) in BANDS.iter().enumerate() {
            for db in [-12, 6, 12] {
                let mut eq = Eq { on: true, ..Eq::default() };
                eq.bands[i] = db;
                let got = gain_at(&eq, hz);
                assert!((got - db as f32).abs() < 0.5, "{hz} Hz at {db} dB gave {got}");
            }
        }
    }

    #[test]
    fn a_band_leaves_frequencies_far_from_it_alone() {
        let mut eq = Eq { on: true, ..Eq::default() };
        eq.bands[0] = 12;
        assert!(gain_at(&eq, 4000.).abs() < 0.5);
    }

    #[test]
    fn the_preamp_moves_everything() {
        let eq = Eq { on: true, preamp: -6, ..Eq::default() };
        assert!((gain_at(&eq, 1000.) + 6.).abs() < 0.1);
    }

    #[test]
    fn it_packs_for_the_audio_thread_and_back() {
        let eq = Eq { on: true, preamp: -3, bands: [-12, -1, 0, 1, 12, 5, -5, 2, -2, 7] };
        assert_eq!(Eq::unpack(eq.pack()), eq);
        assert_eq!(Eq::unpack(Eq::default().pack()), Eq::default());
    }

    #[test]
    fn a_band_at_zero_passes_samples_unchanged() {
        let (_, bands) = design(&Eq { on: true, ..Eq::default() }, RATE);
        let mut s = State::default();
        for x in [0.5f32, -0.25, 0.9, 0.0, -1.0] {
            assert!((s.run(&bands[3], x) - x).abs() < 1e-6);
        }
    }

    #[test]
    fn off_or_all_zero_is_flat() {
        assert!(Eq::default().flat());
        assert!(Eq { on: true, ..Eq::default() }.flat());
        assert!(Eq { on: false, preamp: 3, bands: [6; 10] }.flat());
        assert!(!Eq { on: true, preamp: 1, ..Eq::default() }.flat());
    }

    #[test]
    fn gains_stay_within_twelve_db() {
        let mut eq = Eq::default();
        eq.nudge(0, 20);
        eq.nudge(3, -20);
        assert_eq!((eq.preamp, eq.bands[2]), (12, -12));
        assert_eq!(eq.get(3), -12);
    }

    #[test]
    fn a_preset_is_named_by_its_gains_and_stepping_goes_round() {
        let mut eq = Eq::default();
        assert_eq!(name_of(&eq, &[]), "Flat");
        eq.bands[4] = 1;
        assert_eq!(name_of(&eq, &[]), "Custom");
        let first = step(&eq, &[], true);
        assert_eq!(first.name, "Flat");
        eq.apply(&built_in()[6]);
        assert!(eq.on);
        assert_eq!(step(&eq, &[], true).name, "Flat");
        assert_eq!(step(&eq, &[], false).name, "Voice");
    }

    #[test]
    fn saving_overwrites_by_name_and_spares_the_built_in_ones() {
        let mut saved = Vec::new();
        let mut eq = Eq { on: true, preamp: -1, bands: [1; 10] };
        save(&mut saved, "Desk", &eq).unwrap();
        eq.bands[0] = 4;
        save(&mut saved, "desk", &eq).unwrap();
        assert_eq!(saved.len(), 1);
        assert_eq!(saved[0].bands[0], 4);
        assert_eq!(name_of(&eq, &saved), "desk");
        assert!(save(&mut saved, "Flat", &eq).is_err());
        assert!(save(&mut saved, "  ", &eq).is_err());
        assert_eq!(remove(&mut saved, &eq), Some("desk".into()));
        assert!(saved.is_empty());
        assert_eq!(remove(&mut saved, &Eq::default()), None);
    }
}
