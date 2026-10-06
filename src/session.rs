//! What riff was showing when it closed, so it opens the same way: the song that was
//! playing (its recording, playhead and loops are in `resume.rs`), the highlighted
//! song, the view and the practice view's grid and zoom. Kept in `session.json` in the
//! data folder. Songs are named by path, so one that has gone is simply not restored.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::Duration;

#[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Session {
    /// The song that was playing (or paused).
    pub playing: Option<PathBuf>,
    /// The highlighted song.
    pub selected: Option<PathBuf>,
    /// The practice view was open.
    pub practice: bool,
    /// The album view was open, on the album of this song.
    pub album: Option<PathBuf>,
    /// The artist view was open, on the artist of this song.
    pub artist: Option<PathBuf>,
    /// The beat grid was on.
    pub grid: bool,
    /// How much the practice view showed.
    pub span: Option<Duration>,
}

impl Session {
    pub fn load(file: &Path) -> Session {
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

    #[test]
    fn a_session_survives_a_save_and_a_missing_file_is_a_fresh_start() {
        let file = std::env::temp_dir().join(format!("riff-session-{}.json", std::process::id()));
        let session = Session {
            playing: Some("/m/song.mp3".into()),
            selected: Some("/m/other.mp3".into()),
            practice: true,
            album: None,
            artist: Some("/m/song.mp3".into()),
            grid: true,
            span: Some(Duration::from_secs(8)),
        };
        session.store(&file).unwrap();
        assert_eq!(Session::load(&file), session);
        let _ = std::fs::remove_file(&file);
        assert_eq!(Session::load(&file), Session::default());
    }
}
