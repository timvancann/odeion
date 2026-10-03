//! Regions: loops saved under a name, per song (`r` saves, `n` `N` jump, `x` deletes).
//! Kept in `regions.json` in the data folder, keyed by the song's path, so nothing is
//! written next to the music.

use crate::audio::Loop;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

/// A loop this close to a region's edges is that region.
const SAME: Duration = Duration::from_millis(5);

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Region {
    pub name: String,
    pub start: Duration,
    pub end: Duration,
}

impl Region {
    /// The name a region gets when none is typed: where it is.
    pub fn default_name(start: Duration, end: Duration) -> String {
        format!("{} to {}", crate::ui::clock(start), crate::ui::clock(end))
    }

    fn is(&self, start: Duration, end: Duration) -> bool {
        self.start.abs_diff(start) <= SAME && self.end.abs_diff(end) <= SAME
    }
}

#[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize)]
pub struct Regions(HashMap<PathBuf, Vec<Region>>);

impl Regions {
    pub fn of(&self, song: &Path) -> &[Region] {
        self.0.get(song).map_or(&[], |r| r.as_slice())
    }

    /// The region a loop is, if it is one.
    pub fn matching(&self, song: &Path, looping: Loop) -> Option<&Region> {
        let end = looping.end?;
        let start = looping.start.unwrap_or_default();
        self.of(song).iter().find(|r| r.is(start, end))
    }

    /// Saves a region, in order of where it starts. The same stretch again is renamed.
    pub fn save(&mut self, song: &Path, region: Region) {
        let list = self.0.entry(song.to_path_buf()).or_default();
        list.retain(|r| !r.is(region.start, region.end));
        let at = list.partition_point(|r| r.start <= region.start);
        list.insert(at, region);
    }

    /// Moves a region (found by its stretch) to new edges, keeping its name and the
    /// song order. Returns the region as it is now.
    pub fn reshape(&mut self, song: &Path, from: &Region, start: Duration, end: Duration) -> Option<Region> {
        let list = self.0.get_mut(song)?;
        let i = list.iter().position(|r| r.is(from.start, from.end))?;
        let mut region = list.remove(i);
        (region.start, region.end) = (start, end);
        self.save(song, region.clone());
        Some(region)
    }

    /// Removes the region a loop is. Returns its name.
    pub fn remove(&mut self, song: &Path, looping: Loop) -> Option<String> {
        let name = self.matching(song, looping)?.name.clone();
        let (start, end) = (looping.start.unwrap_or_default(), looping.end?);
        let list = self.0.get_mut(song)?;
        list.retain(|r| !r.is(start, end));
        if list.is_empty() {
            self.0.remove(song);
        }
        Some(name)
    }

    /// The next region after `from` (the loop's start, or the playhead without one), or
    /// the one before it.
    pub fn step(&self, song: &Path, from: Duration, forward: bool) -> Option<&Region> {
        let list = self.of(song);
        if forward {
            list.iter().find(|r| r.start > from + SAME)
        } else {
            list.iter().rev().find(|r| r.start + SAME < from)
        }
    }

    pub fn load(file: &Path) -> Regions {
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
        let json = serde_json::to_vec_pretty(self).map_err(std::io::Error::other)?;
        let tmp = file.with_extension("json.tmp");
        std::fs::write(&tmp, json)?;
        std::fs::rename(&tmp, file)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(secs: f64) -> Duration {
        Duration::from_secs_f64(secs)
    }

    fn region(name: &str, start: f64, end: f64) -> Region {
        Region { name: name.into(), start: s(start), end: s(end) }
    }

    fn looped(start: f64, end: f64) -> Loop {
        Loop { start: Some(s(start)), end: Some(s(end)), off: false }
    }

    #[test]
    fn regions_keep_song_order_and_the_same_stretch_is_renamed() {
        let song = Path::new("/m/song.mp3");
        let mut r = Regions::default();
        r.save(song, region("Solo", 90.0, 110.0));
        r.save(song, region("Intro", 0.0, 12.0));
        r.save(song, region("Verse", 12.0, 40.0));
        let names: Vec<&str> = r.of(song).iter().map(|x| x.name.as_str()).collect();
        assert_eq!(names, ["Intro", "Verse", "Solo"]);
        r.save(song, region("Outro solo", 90.002, 110.0));
        assert_eq!(r.of(song).len(), 3);
        assert_eq!(r.matching(song, looped(90.0, 110.0)).unwrap().name, "Outro solo");
        assert!(r.of(Path::new("/m/other.mp3")).is_empty());
    }

    #[test]
    fn stepping_goes_from_the_loop_or_playhead() {
        let song = Path::new("/m/song.mp3");
        let mut r = Regions::default();
        for (name, a, b) in [("A", 0.0, 10.0), ("B", 10.0, 20.0), ("C", 30.0, 40.0)] {
            r.save(song, region(name, a, b));
        }
        assert_eq!(r.step(song, s(10.0), true).unwrap().name, "C");
        assert_eq!(r.step(song, s(10.0), false).unwrap().name, "A");
        assert_eq!(r.step(song, s(25.0), false).unwrap().name, "B");
        assert!(r.step(song, s(30.0), true).is_none());
        assert!(r.step(song, s(0.0), false).is_none());
    }

    #[test]
    fn a_reshaped_region_keeps_its_name_and_place_in_order() {
        let song = Path::new("/m/song.mp3");
        let mut r = Regions::default();
        r.save(song, region("Verse", 10.0, 20.0));
        r.save(song, region("Solo", 30.0, 40.0));
        let solo = r.of(song)[1].clone();
        let moved = r.reshape(song, &solo, s(5.0), s(9.5)).unwrap();
        assert_eq!(moved, region("Solo", 5.0, 9.5));
        let names: Vec<&str> = r.of(song).iter().map(|x| x.name.as_str()).collect();
        assert_eq!(names, ["Solo", "Verse"]);
        assert!(r.reshape(song, &solo, s(1.0), s(2.0)).is_none());
    }

    #[test]
    fn removing_takes_the_loops_region_only() {
        let song = Path::new("/m/song.mp3");
        let mut r = Regions::default();
        r.save(song, region("A", 0.0, 10.0));
        assert_eq!(r.remove(song, looped(0.0, 9.0)), None);
        assert_eq!(r.remove(song, looped(0.0, 10.0)), Some("A".into()));
        assert_eq!(r, Regions::default());
    }

    #[test]
    fn regions_survive_a_save() {
        let file = std::env::temp_dir().join(format!("riff-regions-{}.json", std::process::id()));
        let mut r = Regions::default();
        r.save(Path::new("/m/a.mp3"), region("Solo", 1.5, 2.25));
        r.store(&file).unwrap();
        assert_eq!(Regions::load(&file), r);
        let _ = std::fs::remove_file(&file);
        assert_eq!(Region::default_name(s(20.0), s(24.0)), "0:20 to 0:24");
    }
}
