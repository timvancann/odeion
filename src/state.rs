//! Where things live, the settings, and which songs are favourites.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};


fn home() -> PathBuf {
    PathBuf::from(std::env::var_os("HOME").unwrap_or_default())
}

/// The music folder before any are chosen in settings: `RIFF_MUSIC`, or `~/Music`.
pub fn music_root() -> PathBuf {
    std::env::var_os("RIFF_MUSIC").map_or_else(|| home().join("Music"), PathBuf::from)
}

/// Where riff keeps its data unless another folder is chosen in settings. It also holds
/// the note naming that other folder.
pub fn default_data_dir() -> PathBuf {
    home().join("Library/Application Support/riff")
}

/// Where riff keeps its data this run: the folder chosen in settings, or the default.
/// Read once, so a new choice only takes effect when riff opens again.
pub fn data_dir() -> PathBuf {
    static DIR: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();
    DIR.get_or_init(|| {
        let default = default_data_dir();
        chosen_data_dir(&default).unwrap_or(default)
    })
    .clone()
}

/// The note in the default data folder that names the folder chosen in settings.
const CHOSEN: &str = "data-folder";

/// The data folder chosen in settings, if any, from the note in `default`.
pub fn chosen_data_dir(default: &Path) -> Option<PathBuf> {
    let text = std::fs::read_to_string(default.join(CHOSEN)).ok()?;
    let dir = text.trim();
    (!dir.is_empty()).then(|| PathBuf::from(dir)).filter(|d| d != default)
}

/// Chooses the data folder for the next run: `dir`, or the default when it's `None` or
/// the default itself.
pub fn choose_data_dir(default: &Path, dir: Option<&Path>) -> std::io::Result<()> {
    let note = default.join(CHOSEN);
    match dir.filter(|d| *d != default) {
        Some(dir) => {
            std::fs::create_dir_all(default)?;
            std::fs::write(note, dir.to_string_lossy().as_bytes())
        }
        None => match std::fs::remove_file(note) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e),
            _ => Ok(()),
        },
    }
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
    fn a_chosen_data_folder_is_kept_and_the_default_clears_it() {
        let default = std::env::temp_dir().join(format!("riff-data-{}", std::process::id()));
        let demo = PathBuf::from("/tmp/riff demo");
        assert_eq!(chosen_data_dir(&default), None);
        choose_data_dir(&default, Some(&demo)).unwrap();
        assert_eq!(chosen_data_dir(&default), Some(demo));
        choose_data_dir(&default, Some(&default)).unwrap();
        assert_eq!(chosen_data_dir(&default), None);
        choose_data_dir(&default, None).unwrap();
        assert_eq!(chosen_data_dir(&default), None);
        let _ = std::fs::remove_dir_all(&default);
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
