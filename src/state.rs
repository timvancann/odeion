//! Where things live, the settings, which songs are favourites, and handing a song to
//! Transcribe!.

use crate::library::Track;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

const DEFAULT_MUSIC: &str =
    "Library/CloudStorage/GoogleDrive-timvancann@gmail.com/Mijn Drive/Music";

fn home() -> PathBuf {
    PathBuf::from(std::env::var_os("HOME").unwrap_or_default())
}

/// The music folder before any are chosen in settings: `RIFF_MUSIC`, or the Drive folder.
pub fn music_root() -> PathBuf {
    std::env::var_os("RIFF_MUSIC").map_or_else(|| home().join(DEFAULT_MUSIC), PathBuf::from)
}

pub fn data_dir() -> PathBuf {
    home().join("Library/Application Support/riff")
}

/// What ⌘, changes: the folders to scan and the colour theme, by name. Also where the
/// window was left.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub folders: Vec<PathBuf>,
    pub theme: String,
    pub window: Option<Placement>,
    /// Bare letters as vim commands outside search (`keys.rs`).
    pub vim: bool,
}

/// The window's place: which display (by its UUID) and where on it, in points from the
/// display's top left.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Placement {
    pub display: String,
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            folders: vec![music_root()],
            theme: crate::theme::DEFAULT_NAME.to_string(),
            window: None,
            vim: true,
        }
    }
}

impl Settings {
    /// Adds folders not listed yet. Returns whether any were new.
    pub fn add_folders(&mut self, folders: Vec<PathBuf>) -> bool {
        let before = self.folders.len();
        for folder in folders {
            if !self.folders.contains(&folder) {
                self.folders.push(folder);
            }
        }
        self.folders.len() != before
    }

    /// The settings file, or the defaults when there is none or it can't be read.
    pub fn load(file: &Path) -> Settings {
        std::fs::read(file)
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default()
    }

    /// Saves through a temp file and a rename, so a crash can't leave half a file.
    pub fn save(&self, file: &Path) -> std::io::Result<()> {
        if let Some(dir) = file.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let json = serde_json::to_vec_pretty(self).map_err(std::io::Error::other)?;
        let tmp = file.with_extension("json.tmp");
        std::fs::write(&tmp, json)?;
        std::fs::rename(&tmp, file)
    }
}

/// The songs marked as favourite.
#[derive(Debug, Default, PartialEq)]
pub struct Favorites(pub Vec<PathBuf>);

impl Favorites {
    /// Marks or unmarks a song. Returns whether it is a favourite now.
    pub fn toggle(&mut self, path: &Path) -> bool {
        let was = self.contains(path);
        if was {
            self.0.retain(|p| p != path);
        } else {
            self.0.push(path.to_path_buf());
        }
        !was
    }

    pub fn contains(&self, path: &Path) -> bool {
        self.0.iter().any(|p| p == path)
    }

    pub fn load(file: &Path) -> Favorites {
        Favorites(
            std::fs::read(file)
                .ok()
                .and_then(|b| serde_json::from_slice(&b).ok())
                .unwrap_or_default(),
        )
    }

    pub fn save(&self, file: &Path) {
        if let Some(dir) = file.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        if let Ok(json) = serde_json::to_vec(&self.0) {
            let _ = std::fs::write(file, json);
        }
    }
}

/// Opens the track in Transcribe!: its saved `.xsc` (loops, markers) when there is one,
/// otherwise the song itself.
pub fn open_in_transcribe(track: &Track) -> Result<(), String> {
    let xsc = track.xsc();
    let target = if xsc.exists() { xsc } else { track.path.clone() };
    let status = std::process::Command::new("open")
        .args(["-a", "Transcribe!"])
        .arg(&target)
        .status()
        .map_err(|e| e.to_string())?;
    if status.success() {
        Ok(())
    } else {
        Err("Transcribe! didn't open".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn toggling_twice_unmarks() {
        let mut f = Favorites::default();
        assert!(f.toggle(Path::new("/a")));
        assert!(f.contains(Path::new("/a")));
        assert!(!f.toggle(Path::new("/a")));
        assert_eq!(f, Favorites::default());
    }

    #[test]
    fn adding_a_listed_folder_changes_nothing() {
        let mut s = Settings { folders: vec!["/a".into()], ..Settings::default() };
        assert!(!s.add_folders(vec!["/a".into()]));
        assert!(s.add_folders(vec!["/b".into(), "/a".into(), "/b".into()]));
        assert_eq!(s.folders, [PathBuf::from("/a"), PathBuf::from("/b")]);
    }

    #[test]
    fn settings_survive_a_save_and_missing_fields_default() {
        let file = std::env::temp_dir().join(format!("riff-settings-{}.json", std::process::id()));
        let s = Settings {
            folders: vec!["/music".into(), "/more".into()],
            theme: "Tokyo Night".into(),
            window: Some(Placement { display: "abc".into(), x: 10., y: 20., width: 900., height: 800. }),
            vim: false,
        };
        s.save(&file).unwrap();
        assert_eq!(Settings::load(&file), s);
        std::fs::write(&file, br#"{"theme": "Tokyo Night"}"#).unwrap();
        assert_eq!(Settings::load(&file).folders, Settings::default().folders);
        let _ = std::fs::remove_file(&file);
    }
}
