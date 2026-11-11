//! Sections of a song (called regions in the code and the data file): named stretches,
//! saved per recording (`r` saves the loop as one, `n` `N` play the next or previous,
//! `dd` deletes, or a section's right-click menu). A section is not the loop: playing
//! one sets the loop to it, and after that the two are apart, so the loop can shrink
//! inside a section or leave it without changing it. Sections change by dragging their edges. Kept in `regions.json` in the
//! data folder, keyed by the recording's path, so nothing is written next to the music.

use crate::audio::Loop;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

/// A loop this close to a region's edges is that region.
const SAME: Duration = Duration::from_millis(5);
/// A section dragged shorter stops at this.
const MIN_LENGTH: Duration = Duration::from_millis(100);

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

    /// The section in play: the one the loop is, or else the shortest one holding the
    /// playhead (a solo inside a verse is the solo). Its place in the list.
    pub fn current(&self, song: &Path, looping: Loop, at: Duration) -> Option<usize> {
        let list = self.of(song);
        if let (Some(end), start) = (looping.end, looping.start.unwrap_or_default())
            && let Some(i) = list.iter().position(|r| r.is(start, end))
        {
            return Some(i);
        }
        (0..list.len())
            .filter(|&i| list[i].start <= at && at < list[i].end)
            .min_by_key(|&i| list[i].end - list[i].start)
    }

    /// Where "section start" plays from: the section in play's start, else the loop's
    /// start. The marker plays no part (that is `o`).
    pub fn restart(&self, song: &Path, looping: Loop, at: Duration) -> Option<Duration> {
        match self.current(song, looping, at) {
            Some(i) => Some(self.of(song)[i].start),
            None => looping.end.map(|_| looping.start.unwrap_or_default()),
        }
    }

    /// Moves section `i`'s start (or end) to `at`, keeping it at least `MIN_LENGTH` long
    /// and the list in song order. Returns where the section is in the list now.
    pub fn move_edge(&mut self, song: &Path, i: usize, start: bool, at: Duration) -> Option<usize> {
        let list = self.0.get_mut(song)?;
        if i >= list.len() {
            return None;
        }
        let mut region = list.remove(i);
        if start {
            region.start = at.min(region.end.saturating_sub(MIN_LENGTH));
        } else {
            region.end = at.max(region.start + MIN_LENGTH);
        }
        let at = list.partition_point(|r| r.start <= region.start);
        list.insert(at, region);
        Some(at)
    }

    /// Renames section `i`; an empty name gives it one saying where it is. Returns the
    /// name it got.
    pub fn rename(&mut self, song: &Path, i: usize, name: &str) -> Option<String> {
        let region = self.0.get_mut(song)?.get_mut(i)?;
        region.name = match name.trim() {
            "" => Region::default_name(region.start, region.end),
            name => name.to_string(),
        };
        Some(region.name.clone())
    }

    /// Removes section `i`. Returns its name.
    pub fn remove(&mut self, song: &Path, i: usize) -> Option<String> {
        let list = self.0.get_mut(song)?;
        if i >= list.len() {
            return None;
        }
        let name = list.remove(i).name;
        if list.is_empty() {
            self.0.remove(song);
        }
        Some(name)
    }

    /// Where stepping to the next or previous section counts from. Next: the playhead,
    /// so a jump to a section inside a bigger loop goes on to the one after it. Previous:
    /// the loop's start while the playhead is in the loop with no section starting
    /// between them, so a section looped goes to the one before rather than to its own
    /// start; with a section starting in between (a bigger loop) the playhead, as without
    /// a loop. A loop left behind (the playhead moved before or past it) doesn't count.
    pub fn step_from(&self, song: &Path, looping: Loop, at: Duration, forward: bool) -> Duration {
        let (start, Some(end)) = (looping.start.unwrap_or_default(), looping.end) else { return at };
        let in_loop = start <= at + SAME && at <= end + SAME;
        let between = self.of(song).iter().any(|r| r.start > start + SAME && r.start <= at);
        if !forward && in_loop && !between { start } else { at }
    }

    /// The next region after `from` (see `step_from`), or the one before it.
    pub fn step(&self, song: &Path, from: Duration, forward: bool) -> Option<&Region> {
        let list = self.of(song);
        if forward {
            list.iter().find(|r| r.start > from + SAME)
        } else {
            list.iter().rev().find(|r| r.start + SAME < from)
        }
    }

    pub fn load(file: &Path) -> Regions {
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
    fn a_section_renamed_keeps_its_stretch_and_an_empty_name_says_where() {
        let song = Path::new("/m/song.mp3");
        let mut r = Regions::default();
        r.save(song, region("Verse", 10.0, 20.0));
        assert_eq!(r.rename(song, 0, " Chorus "), Some("Chorus".into()));
        assert_eq!(r.of(song)[0], region("Chorus", 10.0, 20.0));
        assert_eq!(r.rename(song, 0, ""), Some(Region::default_name(s(10.0), s(20.0))));
        assert_eq!(r.rename(song, 1, "Gone"), None);
    }

    #[test]
    fn section_start_is_the_section_in_play_or_else_the_loop() {
        let song = Path::new("/m/song.mp3");
        let mut r = Regions::default();
        r.save(song, region("Verse", 10.0, 40.0));
        r.save(song, region("Lick", 20.0, 24.0));
        // The loop shrunk inside the verse: the verse, not where the loop starts.
        assert_eq!(r.restart(song, looped(30.0, 34.0), s(31.0)), Some(s(10.0)));
        assert_eq!(r.restart(song, Loop::default(), s(22.0)), Some(s(20.0)));
        assert_eq!(r.restart(song, looped(50.0, 60.0), s(55.0)), Some(s(50.0)));
        assert_eq!(r.restart(song, Loop::default(), s(55.0)), None);
    }

    #[test]
    fn a_loop_left_behind_doesnt_count_when_stepping() {
        let song = Path::new("/m/song.mp3");
        let mut r = Regions::default();
        for (name, a, b) in [("A", 10.0, 20.0), ("B", 20.0, 30.0)] {
            r.save(song, region(name, a, b));
        }
        // The loop is still A, the playhead back before it: next is A, not B.
        let from = r.step_from(song, looped(10.0, 20.0), s(4.0), true);
        assert_eq!(r.step(song, from, true).unwrap().name, "A");
        // In the loop: next is the one after the playhead, previous the one before the
        // loop's start, not A's own start.
        let from = r.step_from(song, looped(10.0, 20.0), s(19.0), true);
        assert_eq!(r.step(song, from, true).unwrap().name, "B");
        let from = r.step_from(song, looped(10.0, 20.0), s(19.0), false);
        assert!(r.step(song, from, false).is_none());
        assert_eq!(r.step_from(song, Loop::default(), s(25.0), false), s(25.0));
        // Jumped to B inside a bigger loop around both: next isn't B again.
        r.save(song, region("C", 30.0, 40.0));
        let from = r.step_from(song, looped(5.0, 35.0), s(20.0), true);
        assert_eq!(r.step(song, from, true).unwrap().name, "C");
        // And previous goes to A, not before the loop's start: just after B's start...
        let from = r.step_from(song, looped(5.0, 35.0), s(20.0), false);
        assert_eq!(r.step(song, from, false).unwrap().name, "A");
        // ...and partway into B it starts B again first, as without a loop.
        let from = r.step_from(song, looped(5.0, 35.0), s(25.0), false);
        assert_eq!(r.step(song, from, false).unwrap().name, "B");
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
    fn a_dragged_edge_keeps_the_name_the_order_and_a_length() {
        let song = Path::new("/m/song.mp3");
        let mut r = Regions::default();
        r.save(song, region("Verse", 10.0, 20.0));
        r.save(song, region("Solo", 30.0, 40.0));
        assert_eq!(r.move_edge(song, 1, true, s(5.0)), Some(0));
        let names: Vec<&str> = r.of(song).iter().map(|x| x.name.as_str()).collect();
        assert_eq!(names, ["Solo", "Verse"]);
        assert_eq!(r.of(song)[0], region("Solo", 5.0, 40.0));
        r.move_edge(song, 0, false, s(1.0));
        assert_eq!(r.of(song)[0], region("Solo", 5.0, 5.1));
        assert_eq!(r.move_edge(song, 7, true, s(1.0)), None);
    }

    #[test]
    fn the_current_section_is_the_loops_or_the_tightest_around_the_playhead() {
        let song = Path::new("/m/song.mp3");
        let mut r = Regions::default();
        r.save(song, region("Verse", 10.0, 40.0));
        r.save(song, region("Lick", 20.0, 24.0));
        r.save(song, region("Chorus", 40.0, 60.0));
        let none = Loop::default();
        assert_eq!(r.current(song, looped(40.0, 60.0), s(15.0)), Some(2));
        assert_eq!(r.current(song, looped(21.0, 22.0), s(21.5)), Some(1));
        assert_eq!(r.current(song, none, s(12.0)), Some(0));
        assert_eq!(r.current(song, none, s(70.0)), None);
    }

    #[test]
    fn removing_takes_one_section() {
        let song = Path::new("/m/song.mp3");
        let mut r = Regions::default();
        r.save(song, region("A", 0.0, 10.0));
        assert_eq!(r.remove(song, 3), None);
        assert_eq!(r.remove(song, 0), Some("A".into()));
        assert_eq!(r, Regions::default());
    }

    #[test]
    fn regions_survive_a_save() {
        let file = std::env::temp_dir().join(format!("odeion-regions-{}.json", std::process::id()));
        let mut r = Regions::default();
        r.save(Path::new("/m/a.mp3"), region("Solo", 1.5, 2.25));
        r.store(&file).unwrap();
        assert_eq!(Regions::load(&file), r);
        let _ = std::fs::remove_file(&file);
        assert_eq!(Region::default_name(s(20.0), s(24.0)), "0:20 to 0:24");
    }
}
