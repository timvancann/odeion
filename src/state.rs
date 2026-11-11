//! Where things live, the settings, and which songs are favourites.

use serde::{Deserialize, Serialize, de::DeserializeOwned};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// Files of odeion's that couldn't be read this run, each moved aside as it was found.
static SET_ASIDE: Mutex<Vec<PathBuf>> = Mutex::new(Vec::new());

/// Reads one of odeion's JSON files; none there gives the default. One that can't be read
/// or understood (a sync conflict, a hand edit gone wrong) is renamed to
/// `<name>.broken` first, so the next save can't overwrite what it held.
pub fn load_json<T: DeserializeOwned + Default>(file: &Path) -> T {
    let read = std::fs::read(file);
    if let Err(e) = &read
        && e.kind() == std::io::ErrorKind::NotFound
    {
        return T::default();
    }
    if let Some(value) = read.ok().and_then(|b| serde_json::from_slice(&b).ok()) {
        return value;
    }
    let aside = broken(file);
    if std::fs::rename(file, &aside).is_ok()
        && let Ok(mut list) = SET_ASIDE.lock()
    {
        list.push(aside);
    }
    T::default()
}

/// Where `load_json` moves a file it couldn't read.
pub fn broken(file: &Path) -> PathBuf {
    let mut name = file.as_os_str().to_owned();
    name.push(".broken");
    PathBuf::from(name)
}

/// The files moved aside so far, once: odeion says so when it opens.
pub fn take_set_aside() -> Vec<PathBuf> {
    SET_ASIDE.lock().map(|mut list| std::mem::take(&mut *list)).unwrap_or_default()
}


fn home() -> PathBuf {
    PathBuf::from(std::env::var_os("HOME").unwrap_or_default())
}

/// Where odeion keeps its data unless another folder is chosen in settings. It also holds
/// the note naming that other folder. Each system's usual place: Application Support on
/// a Mac, `%APPDATA%` on Windows, `$XDG_DATA_HOME` (or `~/.local/share`) on Linux.
pub fn default_data_dir() -> PathBuf {
    if cfg!(target_os = "macos") {
        home().join("Library/Application Support/odeion")
    } else if cfg!(windows) {
        PathBuf::from(std::env::var_os("APPDATA").unwrap_or_default()).join("odeion")
    } else {
        std::env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .filter(|p| p.is_absolute())
            .unwrap_or_else(|| home().join(".local/share"))
            .join("odeion")
    }
}

/// Where odeion keeps its data this run: the folder chosen in settings, or the default.
/// Read once, so a new choice only takes effect when odeion opens again.
pub fn data_dir() -> PathBuf {
    static DIR: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();
    DIR.get_or_init(|| {
        let default = move_from_riff(&default_data_dir());
        chosen_data_dir(&default).unwrap_or(default)
    })
    .clone()
}

/// Odeion was called riff until 2026-11-08: the default data folder of that name, beside
/// `default`, becomes this one, once, when this one doesn't exist yet. Its note naming a
/// chosen folder comes along, and covers are found again by `scan::load_cache`. Returns
/// the folder to use: the old one still, if it couldn't be moved, rather than starting
/// empty while everything sits beside it.
pub fn move_from_riff(default: &Path) -> PathBuf {
    let old = default.with_file_name("riff");
    if !default.exists() && old.is_dir() && std::fs::rename(&old, default).is_err() {
        return old;
    }
    default.to_path_buf()
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

/// What ⌘, changes: the folders to scan, the appearance and an editor theme by name, the
/// keyboard and the output device. Also where the window was left, and the volume.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub folders: Vec<PathBuf>,
    /// Odeion's own Light or Dark, or Auto. Files from before it get Dark, what they showed.
    #[serde(default = "crate::theme::Appearance::before")]
    pub appearance: crate::theme::Appearance,
    /// The scheme: "T3" (a new install's), empty for Gold, or an editor theme's name.
    /// Files from before T3 keep Gold, what they showed (and old files' "Gold" too).
    #[serde(default)]
    pub theme: String,
    pub window: Option<Placement>,
    /// Mac keys or Vim keys (`keys.rs`).
    pub keyboard: crate::keys::Keyboard,
    /// Read only: files from before the keyboard choice said `vim` instead.
    #[serde(skip_serializing)]
    pub vim: Option<bool>,
    /// The master volume, from 0 to 1.
    pub volume: f32,
    /// The output device chosen, by id; none plays through the system's.
    pub output: Option<String>,
    /// The speed trainer's settings.
    pub trainer: crate::trainer::Trainer,
    /// How the list shows songs.
    pub list: ListView,
    /// The global EQ, and the presets the user saved.
    pub eq: crate::eq::Eq,
    pub eq_presets: Vec<crate::eq::Preset>,
    /// What the export sheet was left saying, and the folder last exported to.
    pub export: crate::export::Choices,
    pub export_folder: Option<PathBuf>,
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
            folders: Vec::new(),
            appearance: crate::theme::Appearance::Auto,
            theme: crate::theme::T3.to_string(),
            window: None,
            keyboard: crate::keys::Keyboard::Mac,
            vim: None,
            volume: 1.0,
            output: None,
            trainer: Default::default(),
            list: ListView::Columns,
            eq: Default::default(),
            eq_presets: Vec::new(),
            export: Default::default(),
            export_folder: None,
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

    /// The settings file, or the defaults when there is none or it can't be read (then
    /// kept as `settings.json.broken`, see `load_json`). An old file's `vim` picks the
    /// keyboard: on is Vim, off is Mac.
    pub fn load(file: &Path) -> Settings {
        let mut settings: Settings = load_json(file);
        if let Some(vim) = settings.vim.take() {
            settings.keyboard = if vim { crate::keys::Keyboard::Vim } else { crate::keys::Keyboard::Mac };
        }
        if settings.theme == "Gold" {
            settings.theme.clear();
        }
        settings
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
        Favorites(load_json(file))
    }

    /// Saves through a temp file and a rename, so a crash can't leave half a file.
    pub fn store(&self, file: &Path) -> std::io::Result<()> {
        if let Some(dir) = file.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let json = serde_json::to_vec(&self.0).map_err(std::io::Error::other)?;
        let tmp = file.with_extension("json.tmp");
        std::fs::write(&tmp, json)?;
        std::fs::rename(&tmp, file)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_data_folder_from_before_the_rename_moves_once() {
        let parent = std::env::temp_dir().join(format!("odeion-rename-{}", std::process::id()));
        let (old, new) = (parent.join("riff"), parent.join("odeion"));
        let _ = std::fs::remove_dir_all(&parent);
        std::fs::create_dir_all(&old).unwrap();
        std::fs::write(old.join("settings.json"), b"{}").unwrap();
        assert_eq!(move_from_riff(&new), new);
        assert!(!old.exists());
        assert_eq!(std::fs::read(new.join("settings.json")).unwrap(), b"{}");
        // With both there, nothing moves: the new one is never overwritten.
        std::fs::create_dir_all(&old).unwrap();
        std::fs::write(old.join("settings.json"), b"old").unwrap();
        assert_eq!(move_from_riff(&new), new);
        assert_eq!(std::fs::read(new.join("settings.json")).unwrap(), b"{}");
        assert_eq!(std::fs::read(old.join("settings.json")).unwrap(), b"old");
        let _ = std::fs::remove_dir_all(&parent);
    }

    #[test]
    fn toggling_twice_unmarks() {
        let mut f = Favorites::default();
        assert!(f.toggle(Path::new("/a")));
        assert!(f.contains(Path::new("/a")));
        assert!(!f.toggle(Path::new("/a")));
        assert_eq!(f, Favorites::default());
    }

    #[test]
    fn favourites_survive_a_save_through_a_temp_file() {
        let dir = std::env::temp_dir().join(format!("odeion-favourites-{}", std::process::id()));
        let file = dir.join("favorites.json");
        let mut f = Favorites::default();
        f.toggle(Path::new("/a"));
        f.store(&file).unwrap();
        assert_eq!(Favorites::load(&file), f);
        assert!(!file.with_extension("json.tmp").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_chosen_data_folder_is_kept_and_the_default_clears_it() {
        let default = std::env::temp_dir().join(format!("odeion-data-{}", std::process::id()));
        let demo = PathBuf::from("/tmp/odeion demo");
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
        let file = std::env::temp_dir().join(format!("odeion-settings-{}.json", std::process::id()));
        let s = Settings {
            folders: vec!["/music".into(), "/more".into()],
            appearance: crate::theme::Appearance::Light,
            theme: "Tokyo Night".into(),
            window: Some(Placement { display: "abc".into(), x: 10., y: 20., width: 900., height: 800. }),
            keyboard: crate::keys::Keyboard::Vim,
            vim: None,
            volume: 0.6,
            output: Some("coreaudio:headphones".into()),
            trainer: crate::trainer::Trainer { from: 60, to: 95, step: 5, every: 4 },
            list: ListView::Grouped,
            eq: crate::eq::Eq { on: true, preamp: -2, bands: [3, 2, 0, 0, 0, 0, 0, 1, 2, 3] },
            eq_presets: vec![crate::eq::Preset { name: "Desk".into(), preamp: 0, bands: [1; 10] }],
            export: crate::export::Choices { speed: false, format: crate::export::Format::Ogg, ..Default::default() },
            export_folder: Some("/exports".into()),
        };
        s.save(&file).unwrap();
        assert_eq!(Settings::load(&file), s);
        std::fs::write(&file, br#"{"theme": "Tokyo Night"}"#).unwrap();
        // No folder until one is added: odeion starts empty.
        assert!(Settings::load(&file).folders.is_empty());
        // Settings from before volume and output play at full, through the system's.
        assert_eq!(Settings::load(&file).volume, 1.0);
        assert_eq!(Settings::load(&file).output, None);
        assert_eq!(Settings::load(&file).list, ListView::Columns);
        assert!(Settings::load(&file).eq.flat());
        let _ = std::fs::remove_file(&file);
    }

    #[test]
    fn a_file_that_cant_be_read_is_kept_aside_not_overwritten() {
        let file = std::env::temp_dir().join(format!("odeion-broken-{}.json", std::process::id()));
        let aside = broken(&file);
        let _ = std::fs::remove_file(&aside);
        std::fs::write(&file, br#"{"folders": ["/m"], "#).unwrap();
        let s = Settings::load(&file);
        assert!(s.folders.is_empty());
        assert!(!file.exists());
        assert_eq!(std::fs::read(&aside).unwrap(), br#"{"folders": ["/m"], "#);
        assert!(take_set_aside().contains(&aside));
        // Saving now writes a new file and leaves the old one alone.
        s.save(&file).unwrap();
        assert_eq!(std::fs::read(&aside).unwrap(), br#"{"folders": ["/m"], "#);
        // No file at all is a first start, with nothing to keep.
        std::fs::remove_file(&file).unwrap();
        assert_eq!(Settings::load(&file), Settings::default());
        assert!(!file.exists());
        let _ = std::fs::remove_file(&aside);
    }

    #[test]
    fn old_files_pick_the_keyboard_from_vim_and_new_ones_get_mac() {
        use crate::keys::Keyboard;
        let file = std::env::temp_dir().join(format!("odeion-keyboard-{}.json", std::process::id()));
        let _ = std::fs::remove_file(&file);
        assert_eq!(Settings::load(&file).keyboard, Keyboard::Mac);
        std::fs::write(&file, br#"{"vim": true}"#).unwrap();
        let old = Settings::load(&file);
        assert_eq!(old.keyboard, Keyboard::Vim);
        // Saved again, it says `keyboard` and no longer `vim`.
        old.save(&file).unwrap();
        let text = std::fs::read_to_string(&file).unwrap();
        assert!(text.contains(r#""keyboard": "vim""#) && !text.contains(r#""vim":"#));
        assert_eq!(Settings::load(&file).keyboard, Keyboard::Vim);
        std::fs::write(&file, br#"{"vim": false}"#).unwrap();
        assert_eq!(Settings::load(&file).keyboard, Keyboard::Mac);
        std::fs::write(&file, br#"{"theme": "Gold"}"#).unwrap();
        assert_eq!(Settings::load(&file).keyboard, Keyboard::Mac);
        let _ = std::fs::remove_file(&file);
    }

    #[test]
    fn old_files_keep_looking_as_they_did_and_new_installs_follow_macos() {
        use crate::theme::Appearance;
        let file = std::env::temp_dir().join(format!("odeion-appearance-{}.json", std::process::id()));
        let _ = std::fs::remove_file(&file);
        let new = Settings::load(&file);
        assert_eq!((new.appearance, new.theme.as_str()), (Appearance::Auto, crate::theme::T3));
        // Gold, the old built-in, is odeion's own Dark.
        std::fs::write(&file, br#"{"theme": "Gold"}"#).unwrap();
        let gold = Settings::load(&file);
        assert_eq!((gold.appearance, gold.theme.as_str()), (Appearance::Dark, ""));
        std::fs::write(&file, br#"{"folders": []}"#).unwrap();
        let old = Settings::load(&file);
        assert_eq!((old.appearance, old.theme.as_str()), (Appearance::Dark, ""));
        // A Zed theme stays chosen, as an editor theme.
        std::fs::write(&file, br#"{"theme": "Tokyo Night"}"#).unwrap();
        assert_eq!(Settings::load(&file).theme, "Tokyo Night");
        std::fs::write(&file, br#"{"appearance": "auto", "theme": ""}"#).unwrap();
        assert_eq!(Settings::load(&file).appearance, Appearance::Auto);
        let _ = std::fs::remove_file(&file);
    }
}
