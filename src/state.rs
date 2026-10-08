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

/// What ⌘, changes: the folders to scan, the colour theme by name, vim keys and the
/// output device. Also where the window was left, and the volume.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub folders: Vec<PathBuf>,
    pub theme: String,
    pub window: Option<Placement>,
    /// Bare letters as vim commands outside search (`keys.rs`).
    pub vim: bool,
    /// The master volume, from 0 to 1.
    pub volume: f32,
    /// The output device chosen, by id; none plays through the system's.
    pub output: Option<String>,
    /// The speed trainer's settings.
    pub trainer: crate::trainer::Trainer,
    /// How the list shows songs.
    pub list: ListView,
    /// The global EQ, and the presets Tim saved.
    pub eq: crate::eq::Eq,
    pub eq_presets: Vec<crate::eq::Preset>,
}

/// How the list shows songs: artist, title and album in columns (a repeated artist
/// faded), or the full list grouped under each artist, the artist pinned on top.
#[derive(Debug, Default, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ListView {
    #[default]
    Columns,
    Grouped,
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
            volume: 1.0,
            output: None,
            trainer: Default::default(),
            list: ListView::Columns,
            eq: Default::default(),
            eq_presets: Vec::new(),
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
            volume: 0.6,
            output: Some("coreaudio:headphones".into()),
            trainer: crate::trainer::Trainer { from: 60, to: 95, step: 5, every: 4 },
            list: ListView::Grouped,
            eq: crate::eq::Eq { on: true, preamp: -2, bands: [3, 2, 0, 0, 0, 0, 0, 1, 2, 3] },
            eq_presets: vec![crate::eq::Preset { name: "Desk".into(), preamp: 0, bands: [1; 10] }],
        };
        s.save(&file).unwrap();
        assert_eq!(Settings::load(&file), s);
        std::fs::write(&file, br#"{"theme": "Tokyo Night"}"#).unwrap();
        assert_eq!(Settings::load(&file).folders, Settings::default().folders);
        // Settings from before volume and output play at full, through the system's.
        assert_eq!(Settings::load(&file).volume, 1.0);
        assert_eq!(Settings::load(&file).output, None);
        assert_eq!(Settings::load(&file).list, ListView::Columns);
        assert!(Settings::load(&file).eq.flat());
        let _ = std::fs::remove_file(&file);
    }
}
