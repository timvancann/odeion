//! Playlists: named lists of songs, filled song by song (`+`), each shown in the list
//! like an album. Kept in `playlists.json` in the data folder, in order of creation.
//! Songs that have gone stay in the file but aren't listed, so one coming back isn't
//! lost.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Named {
    pub name: String,
    pub songs: Vec<PathBuf>,
}

#[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Playlists(Vec<Named>);

impl Playlists {
    pub fn all(&self) -> &[Named] {
        &self.0
    }

    pub fn get(&self, i: usize) -> Option<&Named> {
        self.0.get(i)
    }

    /// Where the playlist called `name` is, case aside.
    pub fn find(&self, name: &str) -> Option<usize> {
        self.0.iter().position(|p| p.name.eq_ignore_ascii_case(name.trim()))
    }

    /// A name a playlist may take: trimmed, not empty, not another's (`except` may keep
    /// its own).
    fn check(&self, name: &str, except: Option<usize>) -> Result<String, String> {
        let name = name.trim();
        if name.is_empty() {
            return Err("a playlist needs a name".into());
        }
        match self.find(name) {
            Some(i) if Some(i) != except => Err(format!("there is a playlist called {name} already")),
            _ => Ok(name.to_string()),
        }
    }

    /// A new, empty playlist. Returns its place.
    pub fn create(&mut self, name: &str) -> Result<usize, String> {
        let name = self.check(name, None)?;
        self.0.push(Named { name, songs: Vec::new() });
        Ok(self.0.len() - 1)
    }

    pub fn rename(&mut self, i: usize, name: &str) -> Result<(), String> {
        let name = self.check(name, Some(i))?;
        match self.0.get_mut(i) {
            Some(p) => {
                p.name = name;
                Ok(())
            }
            None => Err("no such playlist".into()),
        }
    }

    pub fn delete(&mut self, i: usize) -> Option<Named> {
        (i < self.0.len()).then(|| self.0.remove(i))
    }

    /// Adds `song` at the end of playlist `i`. False when it was in it already (or
    /// there's no such playlist), which changes nothing.
    pub fn add(&mut self, i: usize, song: &Path) -> bool {
        match self.0.get_mut(i) {
            Some(p) if !p.songs.iter().any(|s| s == song) => {
                p.songs.push(song.to_path_buf());
                true
            }
            _ => false,
        }
    }

    pub fn has(&self, i: usize, song: &Path) -> bool {
        self.0.get(i).is_some_and(|p| p.songs.iter().any(|s| s == song))
    }

    pub fn remove(&mut self, i: usize, song: &Path) {
        if let Some(p) = self.0.get_mut(i) {
            p.songs.retain(|s| s != song);
        }
    }

    /// Moves `song` one place up (towards the start) or down. Returns whether it moved.
    pub fn shift(&mut self, i: usize, song: &Path, up: bool) -> bool {
        let Some(p) = self.0.get_mut(i) else { return false };
        let Some(at) = p.songs.iter().position(|s| s == song) else { return false };
        let to = if up { at.checked_sub(1) } else { Some(at + 1).filter(|&t| t < p.songs.len()) };
        match to {
            Some(to) => {
                p.songs.swap(at, to);
                true
            }
            None => false,
        }
    }

    pub fn load(file: &Path) -> Playlists {
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

    fn songs(p: &Playlists, i: usize) -> Vec<&str> {
        p.get(i).unwrap().songs.iter().map(|s| s.to_str().unwrap()).collect()
    }

    #[test]
    fn names_are_trimmed_unique_and_not_empty() {
        let mut p = Playlists::default();
        assert_eq!(p.create(" Warm-up "), Ok(0));
        assert_eq!(p.get(0).unwrap().name, "Warm-up");
        assert!(p.create("warm-up").is_err());
        assert!(p.create("  ").is_err());
        assert_eq!(p.create("Gig"), Ok(1));
        assert!(p.rename(1, "WARM-UP").is_err());
        assert!(p.rename(0, "warm-up").is_ok());
        assert_eq!(p.find("Warm-Up"), Some(0));
        assert_eq!(p.delete(1).unwrap().name, "Gig");
        assert_eq!(p.delete(5), None);
    }

    #[test]
    fn songs_go_in_once_and_move_up_and_down() {
        let mut p = Playlists::default();
        p.create("Gig").unwrap();
        for s in ["a", "b", "c"] {
            assert!(p.add(0, Path::new(s)));
        }
        assert!(!p.add(0, Path::new("b")));
        assert!(!p.add(7, Path::new("b")));
        assert!(p.has(0, Path::new("c")));
        assert!(p.shift(0, Path::new("c"), true));
        assert_eq!(songs(&p, 0), ["a", "c", "b"]);
        assert!(!p.shift(0, Path::new("a"), true));
        assert!(!p.shift(0, Path::new("b"), false));
        p.remove(0, Path::new("a"));
        assert_eq!(songs(&p, 0), ["c", "b"]);
    }

    #[test]
    fn playlists_survive_a_save_and_a_missing_file_is_none() {
        let file = std::env::temp_dir().join(format!("riff-playlists-{}.json", std::process::id()));
        let mut p = Playlists::default();
        p.create("Gig").unwrap();
        p.add(0, Path::new("/m/a.mp3"));
        p.store(&file).unwrap();
        assert_eq!(Playlists::load(&file), p);
        let _ = std::fs::remove_file(&file);
        assert_eq!(Playlists::load(&file), Playlists::default());
    }
}
