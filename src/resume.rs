//! Where each song was left: which recording played (the song or a variation), the
//! slow-down, the playhead, and each recording's loop, marker and stem levels. Picking
//! the song again picks up there. Kept in `resume.json` in the data folder.

use crate::audio::Loop;
use crate::speed::Speed;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

/// A position this close to the end starts the song from the top instead: resuming a
/// song that played out would end it at once.
const NEAR_THE_END: Duration = Duration::from_secs(5);

#[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Spot {
    /// 0 for the song itself, n for its n-th variation.
    pub variation: usize,
    /// How fast it played, and the slowed speed kept while at full speed.
    pub pace: Option<Speed>,
    /// Before percentages: slow-down on or off, and which preset. Read, never written.
    #[serde(skip_serializing)]
    pub slow: bool,
    #[serde(skip_serializing)]
    pub speed: Option<usize>,
    /// The playhead, in the playing recording's own time.
    pub at: Duration,
    /// Each recording's loop, by the recording's first file.
    pub loops: HashMap<PathBuf, Loop>,
    /// Each recording's marker, where `o` plays from.
    pub marks: HashMap<PathBuf, Duration>,
    /// Each stem set's levels, by its first file; full ones aren't kept.
    pub levels: HashMap<PathBuf, Vec<f32>>,
}

impl Spot {
    /// The song's speed: as kept, from an old preset, or the first slowed speed.
    pub fn pace(&self) -> Speed {
        self.pace.or_else(|| self.speed.map(|p| Speed::from_preset(self.slow, p))).unwrap_or_default()
    }

    /// Where to start: the remembered playhead, or the top when that was near the end.
    pub fn start(&self, length: Option<Duration>) -> Duration {
        match length {
            Some(length) if self.at + NEAR_THE_END >= length => Duration::ZERO,
            _ => self.at,
        }
    }
}

#[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize)]
pub struct Resume(HashMap<PathBuf, Spot>);

impl Resume {
    pub fn of(&self, song: &Path) -> Option<&Spot> {
        self.0.get(song)
    }

    pub fn keep(&mut self, song: &Path, spot: Spot) {
        self.0.insert(song.to_path_buf(), spot);
    }

    pub fn load(file: &Path) -> Resume {
        std::fs::read(file)
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default()
    }

    /// Saves through a temp file and a rename, so a crash can't leave half a file.
    pub fn store(&self, file: &Path) -> std::io::Result<()> {
        if let Some(dir) = file.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let json = serde_json::to_vec(self).map_err(std::io::Error::other)?;
        let tmp = file.with_extension("json.tmp");
        std::fs::write(&tmp, json)?;
        std::fs::rename(&tmp, file)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(secs: u64) -> Duration {
        Duration::from_secs(secs)
    }

    #[test]
    fn a_song_left_near_its_end_starts_from_the_top() {
        let spot = Spot { at: s(100), ..Spot::default() };
        assert_eq!(spot.start(Some(s(200))), s(100));
        assert_eq!(spot.start(None), s(100));
        assert_eq!(Spot { at: s(196), ..Spot::default() }.start(Some(s(200))), s(0));
    }

    #[test]
    fn old_slow_down_presets_read_as_percentages() {
        let old: Spot = serde_json::from_str(r#"{"variation": 0, "slow": true, "speed": 3, "at": {"secs": 5, "nanos": 0}}"#).unwrap();
        assert_eq!(old.pace(), Speed { percent: 80, on: true });
        assert_eq!(Spot::default().pace(), Speed::default());
        // Written anew, only the percentage is kept.
        let json = serde_json::to_string(&Spot { pace: Some(old.pace()), ..old }).unwrap();
        assert!(!json.contains("\"slow\"") && json.contains("\"pace\""));
    }

    #[test]
    fn where_songs_were_left_survives_a_save() {
        let file = std::env::temp_dir().join(format!("riff-resume-{}.json", std::process::id()));
        let mut r = Resume::default();
        let mut loops = HashMap::new();
        loops.insert(PathBuf::from("/d/stems-Bass.mp3"), Loop { start: Some(s(10)), end: Some(s(14)), off: true });
        let marks = HashMap::from([(PathBuf::from("/m/song.mp3"), s(31))]);
        let levels = HashMap::from([(PathBuf::from("/d/stems-Bass.mp3"), vec![1.0, 0.5, 0.25])]);
        let pace = Some(Speed { percent: 65, on: true });
        r.keep(Path::new("/m/song.mp3"), Spot { variation: 2, pace, at: s(42), loops, marks, levels, ..Spot::default() });
        r.store(&file).unwrap();
        assert_eq!(Resume::load(&file), r);
        assert_eq!(Resume::load(&file).of(Path::new("/m/song.mp3")).unwrap().variation, 2);
        assert!(Resume::load(&file).of(Path::new("/m/other.mp3")).is_none());
        let _ = std::fs::remove_file(&file);
    }
}
