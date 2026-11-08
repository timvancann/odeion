//! Variations: other recordings of a song, to switch to in the practice view (`v`). A
//! backing track is one file; a stem set is several, played summed. Kept in
//! `variations.json` in the data folder, keyed by the song's path.
//!
//! A variation may not share the song's timeline: a backing track made separately can
//! start later or run longer. Each is aligned once, when added, by matching its onsets
//! to the song's; one that doesn't line up gets a timeline of its own.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Variation {
    pub label: String,
    /// One file, or the stems of a set, in order.
    pub files: Vec<PathBuf>,
    /// Each stem's name, from its file name ("Drums", "Bass").
    pub stems: Vec<String>,
    /// Seconds to add to a moment in the song to find it here; none for a variation on
    /// a timeline of its own.
    pub offset: Option<f64>,
}

#[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize)]
pub struct Variations(HashMap<PathBuf, Vec<Variation>>);

impl Variations {
    pub fn of(&self, song: &Path) -> &[Variation] {
        self.0.get(song).map_or(&[], |v| v.as_slice())
    }

    pub fn add(&mut self, song: &Path, variation: Variation) {
        self.0.entry(song.to_path_buf()).or_default().push(variation);
    }

    pub fn remove(&mut self, song: &Path, index: usize) -> Option<Variation> {
        let list = self.0.get_mut(song)?;
        let gone = (index < list.len()).then(|| list.remove(index));
        if list.is_empty() {
            self.0.remove(song);
        }
        gone
    }

    /// Whether a file belongs to some song as a variation, so the library leaves it out.
    pub fn is_variation(&self, file: &Path) -> bool {
        self.0.values().flatten().any(|v| v.files.iter().any(|f| f == file))
    }

    pub fn load(file: &Path) -> Variations {
        crate::state::load_json(file)
    }

    /// Saves through a temp file and a rename, so a crash can't leave half a file.
    pub fn store(&self, file: &Path) -> std::io::Result<()> {
        if let Some(dir) = file.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let json = serde_json::to_vec_pretty(self).map_err(std::io::Error::other)?;
        let tmp = file.with_extension("json.tmp");
        std::fs::write(&tmp, json)?;
        std::fs::rename(&tmp, file)
    }
}

/// A variation of `song` from what was dropped or picked: its audio files (one, or a
/// stem set), each stem named, and aligned to the song. Files much shorter than the rest
/// (a count-in) are left out: summed in, they'd play at the wrong moment.
pub fn build(song: &Path, dropped: &[PathBuf]) -> Result<Variation, String> {
    let files: Vec<PathBuf> = audio_files(dropped).into_iter().filter(|f| f != song).collect();
    if files.is_empty() {
        return Err("no audio among what was dropped".into());
    }
    let decoded = files
        .iter()
        .map(|f| crate::beats::mono(f))
        .collect::<Result<Vec<_>, _>>()?;
    let longest = decoded.iter().map(|(x, _)| x.len()).max().unwrap_or(0);
    let (files, decoded): (Vec<PathBuf>, Vec<(Vec<f32>, u32)>) = files
        .into_iter()
        .zip(decoded)
        .filter(|(_, (x, _))| x.len() * 2 >= longest)
        .unzip();
    let rate = decoded[0].1;
    let mut mix = vec![0.0f32; longest];
    for (x, _) in &decoded {
        for (m, v) in mix.iter_mut().zip(x) {
            *m += v;
        }
    }
    let (original, song_rate) = crate::beats::mono(song)?;
    let offset = align((&original, song_rate), (&mix, rate));
    let stems = stem_names(&files);
    let label = if files.len() == 1 { stems[0].clone() } else { "stems".to_string() };
    Ok(Variation { label, files, stems, offset })
}

/// Starting gains for a variation's stems: a click track starts muted.
pub fn default_gains(variation: &Variation) -> Vec<f32> {
    variation
        .stems
        .iter()
        .map(|name| {
            let name = name.to_lowercase();
            if name.contains("metronome") || name.contains("click") { 0.0 } else { 1.0 }
        })
        .collect()
}

/// The audio files among what was dropped or picked: files as they are, folders by the
/// audio files directly in them (a stem export's `peaks/` and the like are left out).
pub fn audio_files(paths: &[PathBuf]) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for path in paths {
        if path.is_dir() {
            let mut inside: Vec<PathBuf> = std::fs::read_dir(path)
                .into_iter()
                .flatten()
                .flatten()
                .map(|e| e.path())
                .filter(|p| p.is_file() && crate::scan::is_audio(p))
                .collect();
            inside.sort();
            out.extend(inside);
        } else if crate::scan::is_audio(path) {
            out.push(path.clone());
        }
    }
    out
}

/// What sets the stems apart in their file names: what's left once the part they all
/// share at the start and end is gone. "05 I_m the One-Drums-Ab minor-75bpm-444hz.mp3"
/// among its set is "Drums". A single file is named by its whole file name.
pub fn stem_names(files: &[PathBuf]) -> Vec<String> {
    let stems: Vec<String> = files
        .iter()
        .map(|f| f.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default())
        .collect();
    if stems.len() < 2 {
        return stems;
    }
    let first: Vec<char> = stems[0].chars().collect();
    let chars: Vec<Vec<char>> = stems.iter().map(|s| s.chars().collect()).collect();
    let shortest = chars.iter().map(|c| c.len()).min().unwrap_or(0);
    let front = (0..shortest).take_while(|&i| chars.iter().all(|c| c[i] == first[i])).count();
    let back = (0..shortest - front)
        .take_while(|&i| chars.iter().all(|c| c[c.len() - 1 - i] == first[first.len() - 1 - i]))
        .count();
    chars
        .iter()
        .zip(&stems)
        .map(|(c, whole)| {
            let middle: String = c[front..c.len() - back].iter().collect();
            let middle = middle.trim_matches(|ch: char| ch == '-' || ch == '_' || ch.is_whitespace());
            if middle.is_empty() { whole.clone() } else { middle.to_string() }
        })
        .collect()
}

/// How far apart two recordings sit: the seconds to add to a moment in `song` to find it
/// in `other`, or none when their onsets don't line up clearly (another recording, or
/// another tempo). Only shifts of up to 30 s are looked for.
///
/// First on 10 ms onset strength, which finds the match; then on 1 ms rises within 15 ms
/// of it, which places it: 10 ms steps plus an mp3 encoder's delay left stems 40 to 50 ms
/// off, which is heard when switching.
pub fn align(song: (&[f32], u32), other: (&[f32], u32)) -> Option<f64> {
    let ((a, rate), (b, other_rate)) = (song, other);
    if rate != other_rate {
        return None;
    }
    let ((sa, hop), (sb, _)) = (crate::beats::strength(a, rate), crate::beats::strength(b, rate));
    let fps = rate as f64 / hop as f64;
    let reach = (30.0 * fps) as i64;
    let score = |x: &[f32], y: &[f32], lag: i64| {
        let (mut sum, mut n) = (0.0f32, 0usize);
        for (i, v) in x.iter().enumerate() {
            let j = i as i64 + lag;
            if j >= 0 && (j as usize) < y.len() {
                sum += v * y[j as usize];
                n += 1;
            }
        }
        if n < x.len() / 4 { 0.0 } else { sum / n as f32 }
    };
    let scores: Vec<(i64, f32)> = (-reach..=reach).map(|lag| (lag, score(&sa, &sb, lag))).collect();
    let (lag, best) = scores.iter().copied().max_by(|x, y| x.1.total_cmp(&y.1))?;
    // Clearly better than any other shift, or it's no match at all: the guitar stem on its
    // own scored 0.17 against a next best of 0.16; real matches beat theirs 1.7 times.
    let next = scores
        .iter()
        .filter(|(l, _)| (l - lag).abs() > 20)
        .map(|s| s.1)
        .fold(0.0f32, f32::max);
    if best < next * 1.3 || best <= 0.0 {
        return None;
    }
    let block = (rate as usize / 1000).max(1);
    let (ra, rb) = (crate::beats::rises(a, block), crate::beats::rises(b, block));
    let rough = (lag as f64 * hop as f64 / block as f64).round() as i64;
    let (ra, rb): (Vec<f32>, Vec<f32>) =
        (ra.iter().map(|v| v.max(0.0)).collect(), rb.iter().map(|v| v.max(0.0)).collect());
    let fine = (rough - 15..=rough + 15).max_by(|&x, &y| score(&ra, &rb, x).total_cmp(&score(&ra, &rb, y)))?;
    Some(fine as f64 * block as f64 / rate as f64)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Clicks at seeded random times over `seconds`, mono at 22.05 kHz, starting `delay`
    /// seconds into the recording.
    fn clicks(seed: u32, seconds: f64, delay: f64) -> Vec<f32> {
        let rate = 22_050.0;
        let mut x = vec![0.0f32; ((seconds + delay + 1.0) * rate) as usize];
        let mut state = seed;
        let mut next = || {
            state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            state >> 8
        };
        let mut t = 0.5;
        while t < seconds {
            let at = ((t + delay) * rate) as usize;
            for i in 0..300 {
                x[at + i] += ((next() % 2000) as f32 / 1000.0 - 1.0) * (1.0 - i as f32 / 300.0);
            }
            t += 0.2 + (next() % 500) as f64 / 1000.0;
        }
        x
    }

    #[test]
    fn a_later_starting_copy_is_found_to_the_millisecond() {
        let song = clicks(7, 40.0, 0.0);
        let backing = clicks(7, 40.0, 1.9);
        let offset = align((&song, 22_050), (&backing, 22_050)).unwrap();
        assert!((offset - 1.9).abs() < 0.002, "offset {offset}");
        let earlier = clicks(7, 40.0, 0.0);
        let song_later = clicks(7, 40.0, 0.75);
        let offset = align((&song_later, 22_050), (&earlier, 22_050)).unwrap();
        assert!((offset + 0.75).abs() < 0.002, "offset {offset}");
    }

    #[test]
    fn another_recording_gets_a_timeline_of_its_own() {
        let song = clicks(7, 40.0, 0.0);
        let other = clicks(99, 40.0, 0.0);
        assert_eq!(align((&song, 22_050), (&other, 22_050)), None);
    }

    #[test]
    fn stems_are_named_by_what_sets_them_apart() {
        let files: Vec<PathBuf> = ["Bass", "Drums", "Guitars", "Count-in"]
            .iter()
            .map(|s| PathBuf::from(format!("/d/05 I_m the One-{s}-Ab minor-75bpm-444hz.mp3")))
            .collect();
        assert_eq!(stem_names(&files), ["Bass", "Drums", "Guitars", "Count-in"]);
        assert_eq!(
            stem_names(&[PathBuf::from("/r/I'm the One Backing Tack.mp3")]),
            ["I'm the One Backing Tack"]
        );
    }

    #[test]
    fn variation_files_are_known_and_removing_the_last_forgets_the_song() {
        let song = Path::new("/m/song.mp3");
        let mut v = Variations::default();
        v.add(song, Variation {
            label: "backing".into(),
            files: vec!["/r/backing.mp3".into()],
            stems: vec!["backing".into()],
            offset: Some(1.9),
        });
        assert!(v.is_variation(Path::new("/r/backing.mp3")));
        assert!(!v.is_variation(song));
        assert_eq!(v.remove(song, 3), None);
        assert!(v.remove(song, 0).is_some());
        assert_eq!(v, Variations::default());
    }
}
