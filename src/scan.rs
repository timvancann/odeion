//! Finding the music and reading its tags. The cache makes a start instant: the library
//! shows what was known last time, and only new or changed files are read again, in the
//! background.

use crate::library::Track;
use lofty::config::ParseOptions;
use lofty::picture::PictureType;
use lofty::prelude::*;
use lofty::probe::Probe;
use std::collections::{HashMap, VecDeque};
use std::hash::{DefaultHasher, Hash, Hasher};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{Receiver, channel};
use std::sync::{Arc, Mutex};
use std::time::UNIX_EPOCH;

const AUDIO: &[&str] = &["mp3", "flac", "wav", "aif", "aiff", "m4a", "ogg"];
const READERS: usize = 4;
/// The tag reader's version. Raise it when `read_tags` learns something new, and every
/// track is read once more. 1 added the length, 2 the year and disc.
pub const READER: u32 = 2;
const COVER_SIZE: u32 = 256;

/// An audio file on disk: path, size and modification time in seconds.
pub type Found = (PathBuf, u64, u64);

/// A listed folder that couldn't be opened, and why, in plain words.
pub type Unreadable = (PathBuf, String);

/// Every audio file under `root`, each counted in `counted` as it is found, so the
/// first scan can say how far it got, and the paths it couldn't look into this time.
/// Hidden folders and Music.app's import inbox are skipped. A `root` that can't be
/// opened is an error; a folder or file below it that can't is passed over, and listed
/// so the songs known there aren't taken for deleted.
pub fn walk(root: &Path, counted: &AtomicUsize) -> std::io::Result<(Vec<Found>, Vec<PathBuf>)> {
    std::fs::read_dir(root)?;
    let (mut found, mut missed) = (Vec::new(), Vec::new());
    let mut dirs = vec![root.to_path_buf()];
    while let Some(dir) = dirs.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            missed.push(dir);
            continue;
        };
        for entry in entries {
            let Ok(entry) = entry else {
                missed.push(dir.clone());
                continue;
            };
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().into_owned();
            let Ok(meta) = entry.metadata() else {
                missed.push(path);
                continue;
            };
            if meta.is_dir() {
                // Only Music.app's inbox: its library itself sits in `Media.localized`.
                if !name.starts_with('.') && name != "Automatically Add to Music.localized" {
                    dirs.push(path);
                }
            } else if is_audio(&path) {
                let modified = meta
                    .modified()
                    .ok()
                    .and_then(|m| m.duration_since(UNIX_EPOCH).ok())
                    .map_or(0, |d| d.as_secs());
                found.push((path, meta.len(), modified));
                counted.fetch_add(1, Ordering::Relaxed);
            }
        }
    }
    Ok((found, missed))
}

/// What a walk over the listed folders found.
#[derive(Debug, Default)]
pub struct Walked {
    /// Every audio file, once, even where one listed folder sits inside another.
    pub found: Vec<Found>,
    /// The listed folders that couldn't be opened, and why.
    pub unreadable: Vec<Unreadable>,
    /// Every path that couldn't be looked into, those folders included: the songs known
    /// under them are kept as they were (an unplugged drive, a cloud folder that didn't
    /// answer), not dropped as deleted.
    pub missed: Vec<PathBuf>,
}

/// Walks each of `roots`.
pub fn walk_all(roots: &[PathBuf], counted: &AtomicUsize) -> Walked {
    let mut seen = std::collections::HashSet::new();
    let mut walked = Walked::default();
    for root in roots {
        match walk(root, counted) {
            Ok((files, missed)) => {
                walked.found.extend(files.into_iter().filter(|f| seen.insert(f.0.clone())));
                walked.missed.extend(missed);
            }
            Err(e) => {
                walked.unreadable.push((root.clone(), why(&e)));
                walked.missed.push(root.clone());
            }
        }
    }
    walked
}

/// Why a folder can't be opened, in words a reader without a terminal follows.
pub fn why(e: &std::io::Error) -> String {
    use std::io::ErrorKind;
    match e.kind() {
        ErrorKind::NotFound => "it isn't there (moved, renamed, or on a drive that isn't connected)".into(),
        // macOS's privacy settings refuse with "operation not permitted".
        ErrorKind::PermissionDenied if cfg!(target_os = "macos") => {
            "macOS doesn't let riff in (System Settings, Privacy & Security, Files and Folders)".into()
        }
        ErrorKind::PermissionDenied => "riff isn't allowed to read it".into(),
        ErrorKind::NotADirectory => "it's a file, not a folder".into(),
        _ => plain(&e.to_string()),
    }
}

/// A system error without its code: "Input/output error (os error 5)" reads
/// "input/output error".
pub fn plain(error: &str) -> String {
    let text = match error.rfind(" (os error ") {
        Some(at) if error.ends_with(')') => &error[..at],
        _ => error,
    };
    let mut chars = text.chars();
    match chars.next() {
        Some(first) if !chars.as_str().starts_with(|c: char| c.is_uppercase()) => {
            first.to_lowercase().chain(chars).collect()
        }
        _ => text.to_string(),
    }
}

/// The kinds of file riff plays, as their names are usually written: "MP3", "FLAC".
pub fn formats() -> Vec<String> {
    AUDIO.iter().map(|e| e.to_uppercase()).collect()
}

/// The folder a file's Artist/Album path is read from: the deepest of `roots` holding it.
fn root_of<'a>(roots: &'a [PathBuf], path: &Path) -> &'a Path {
    roots
        .iter()
        .filter(|r| path.starts_with(r))
        .max_by_key(|r| r.components().count())
        .map_or(Path::new(""), |r| r.as_path())
}

/// What a scan changed: songs that are new, and songs that are gone.
pub fn changes(before: &[Track], after: &[Track]) -> (usize, usize) {
    let old: std::collections::HashSet<&Path> = before.iter().map(|t| t.path.as_path()).collect();
    let new: std::collections::HashSet<&Path> = after.iter().map(|t| t.path.as_path()).collect();
    (new.difference(&old).count(), old.difference(&new).count())
}

/// How many tracks each of `roots` holds, a track counting for the deepest root it is in.
pub fn per_root(roots: &[PathBuf], tracks: &[Track]) -> Vec<usize> {
    let mut counts = vec![0; roots.len()];
    for t in tracks {
        let root = root_of(roots, &t.path);
        if let Some(i) = roots.iter().position(|r| r == root) {
            counts[i] += 1;
        }
    }
    counts
}

pub fn is_audio(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| AUDIO.contains(&e.to_ascii_lowercase().as_str()))
}

/// The library for the files found: a cached track where the file is unchanged, otherwise
/// one read from the path. An unchanged track an older reader read keeps showing what the
/// cache knows while it is read again. A cached track under a `missed` path stays as it
/// was. Returns the library and the tracks that still need their tags.
pub fn merge(
    roots: &[PathBuf],
    found: Vec<Found>,
    cached: Vec<Track>,
    missed: &[PathBuf],
) -> (Vec<Track>, Vec<Track>) {
    let mut cached: HashMap<PathBuf, Track> =
        cached.into_iter().map(|t| (t.path.clone(), t)).collect();
    let mut todo = Vec::new();
    let mut tracks: Vec<Track> = found
        .into_iter()
        .map(|(path, size, modified)| match cached.remove(&path) {
            Some(t) if t.tagged && t.size == size && t.modified == modified => {
                if t.reader != READER {
                    todo.push(t.clone());
                }
                t
            }
            _ => {
                let t = Track::from_path(root_of(roots, &path), &path, size, modified);
                todo.push(t.clone());
                t
            }
        })
        .collect();
    tracks.extend(
        cached
            .into_values()
            .filter(|t| missed.iter().any(|m| t.path.starts_with(m))),
    );
    (tracks, todo)
}

/// Reads the tags of `todo` on a few threads. Each track arrives on the receiver as it is
/// done; the channel closes when all are.
pub fn tag_in_background(todo: Vec<Track>, covers: PathBuf) -> Receiver<Track> {
    let (tx, rx) = channel();
    let queue = Arc::new(Mutex::new(VecDeque::from(todo)));
    let _ = std::fs::create_dir_all(&covers);
    for _ in 0..READERS {
        let (queue, tx, covers) = (queue.clone(), tx.clone(), covers.clone());
        std::thread::spawn(move || {
            loop {
                let Some(track) = queue.lock().ok().and_then(|mut q| q.pop_front()) else {
                    break;
                };
                if tx.send(read_tags(track, &covers)).is_err() {
                    break;
                }
            }
        });
    }
    rx
}

/// The track with what its tags say. A file that can't be read keeps what its path said,
/// and is marked read so it isn't tried again until it changes.
fn read_tags(mut track: Track, covers: &Path) -> Track {
    track.tagged = true;
    track.reader = READER;
    let file = Probe::open(&track.path).and_then(|p| p.options(ParseOptions::new()).read());
    let Ok(file) = file else { return track };
    let length = file.properties().duration();
    track.seconds = (!length.is_zero()).then(|| length.as_secs_f64().round() as u64);
    let Some(tag) = file.primary_tag().or_else(|| file.first_tag()) else {
        return track;
    };
    let text = |s: Option<std::borrow::Cow<str>>| s.map(|s| s.trim().to_string()).filter(|s| !s.is_empty());
    if let Some(title) = text(tag.title()) {
        track.title = title;
    }
    if let Some(artist) = text(tag.artist()) {
        track.artist = artist;
    }
    if let Some(album) = text(tag.album()) {
        track.album = album;
    }
    if let Some(number) = tag.track() {
        track.number = Some(number);
    }
    track.year = tag.date().map(|d| u32::from(d.year)).filter(|&y| y > 0);
    track.disc = tag.disk();
    let pictures = tag.pictures();
    let picture = pictures
        .iter()
        .find(|p| p.pic_type() == PictureType::CoverFront)
        .or(pictures.first());
    track.cover = picture.and_then(|p| thumbnail(p.data(), covers));
    track
}

/// A small JPEG of the cover in the cache, named after its content so an album's tracks
/// share one file.
fn thumbnail(data: &[u8], covers: &Path) -> Option<PathBuf> {
    let mut hasher = DefaultHasher::new();
    data.hash(&mut hasher);
    let path = covers.join(format!("{:016x}.jpg", hasher.finish()));
    if !path.exists() {
        let image = image::load_from_memory(data).ok()?;
        image
            .thumbnail(COVER_SIZE, COVER_SIZE)
            .to_rgb8()
            .save(&path)
            .ok()?;
    }
    Some(path)
}

pub fn load_cache(path: &Path) -> Vec<Track> {
    std::fs::read(path)
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
}

pub fn save_cache(path: &Path, tracks: &[Track]) {
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if let Ok(json) = serde_json::to_vec(tracks) {
        let tmp = path.with_extension("tmp");
        if std::fs::write(&tmp, json).is_ok() {
            let _ = std::fs::rename(tmp, path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn found(path: &str, size: u64, modified: u64) -> Found {
        (PathBuf::from(path), size, modified)
    }

    fn cached(path: &str, size: u64, modified: u64) -> Track {
        let mut t = Track::from_path(Path::new("/m"), Path::new(path), size, modified);
        t.title = "From tags".into();
        t.tagged = true;
        t.reader = READER;
        t
    }

    #[test]
    fn unchanged_files_come_from_the_cache() {
        let (tracks, todo) = merge(
            &["/m".into()],
            vec![found("/m/A/B/01 X.mp3", 10, 5)],
            vec![cached("/m/A/B/01 X.mp3", 10, 5)],
            &[],
        );
        assert_eq!(tracks[0].title, "From tags");
        assert!(todo.is_empty());
    }

    #[test]
    fn new_and_changed_files_are_read_again() {
        let (tracks, todo) = merge(
            &["/m".into()],
            vec![found("/m/A/B/01 X.mp3", 11, 5), found("/m/A/B/02 Y.mp3", 1, 1)],
            vec![cached("/m/A/B/01 X.mp3", 10, 5)],
            &[],
        );
        let titles: Vec<&str> = tracks.iter().map(|t| t.title.as_str()).collect();
        assert_eq!(titles, ["X", "Y"]);
        assert_eq!(todo.len(), 2);
    }

    #[test]
    fn tracks_read_by_an_older_reader_are_read_again() {
        let mut old = cached("/m/A/B/01 X.mp3", 10, 5);
        old.reader = READER - 1;
        let (tracks, todo) = merge(&["/m".into()], vec![found("/m/A/B/01 X.mp3", 10, 5)], vec![old], &[]);
        assert_eq!(todo.len(), 1);
        // Meanwhile it shows what was known, not what the path says.
        assert_eq!(tracks[0].title, "From tags");
    }

    #[test]
    fn changes_count_new_and_gone_songs() {
        let before = [cached("/m/a.mp3", 1, 1), cached("/m/b.mp3", 1, 1)];
        let after = [cached("/m/b.mp3", 2, 2), cached("/m/c.mp3", 1, 1), cached("/m/d.mp3", 1, 1)];
        assert_eq!(changes(&before, &after), (2, 1));
    }

    #[test]
    fn songs_count_for_the_deepest_folder_holding_them() {
        let roots = ["/m".into(), "/m/Downloads".into(), "/empty".into()];
        let tracks = [cached("/m/A/x.mp3", 1, 1), cached("/m/Downloads/y.mp3", 1, 1)];
        assert_eq!(per_root(&roots, &tracks), [1, 1, 0]);
    }

    #[test]
    fn deleted_files_drop_out() {
        let (tracks, _) = merge(&["/m".into()], vec![], vec![cached("/m/A/B/01 X.mp3", 1, 1)], &[]);
        assert!(tracks.is_empty());
    }

    #[test]
    fn songs_where_the_walk_couldnt_look_stay_as_they_were() {
        let (tracks, todo) = merge(
            &["/m".into(), "/drive".into()],
            vec![found("/m/A/B/01 X.mp3", 1, 1)],
            vec![
                cached("/m/A/B/01 X.mp3", 1, 1),
                cached("/m/A/B/02 Gone.mp3", 1, 1),
                cached("/m/C/D/03 Cloud.mp3", 1, 1),
                cached("/drive/E/F/04 Unplugged.mp3", 1, 1),
            ],
            &["/m/C".into(), "/drive".into()],
        );
        let mut paths: Vec<&str> = tracks.iter().map(|t| t.path.to_str().unwrap()).collect();
        paths.sort();
        assert_eq!(paths, ["/drive/E/F/04 Unplugged.mp3", "/m/A/B/01 X.mp3", "/m/C/D/03 Cloud.mp3"]);
        assert!(tracks.iter().all(|t| t.title == "From tags"));
        assert!(todo.is_empty());
    }

    #[test]
    fn artist_and_album_come_from_the_deepest_folder_holding_the_file() {
        let roots = ["/m".into(), "/other".into(), "/m/Downloads".into()];
        let (tracks, _) = merge(
            &roots,
            vec![
                found("/other/A/B/01 X.mp3", 1, 1),
                found("/m/Downloads/C/D/02 Y.mp3", 1, 1),
            ],
            vec![],
            &[],
        );
        assert_eq!((tracks[0].artist.as_str(), tracks[0].album.as_str()), ("A", "B"));
        assert_eq!((tracks[1].artist.as_str(), tracks[1].album.as_str()), ("C", "D"));
    }

    #[test]
    fn walks_audio_only_and_skips_the_music_inbox() {
        let root = std::env::temp_dir().join(format!("riff-walk-{}", std::process::id()));
        let album = root.join("Artist/Album");
        let inbox = root.join("Automatically Add to Music.localized");
        let media = root.join("Media.localized/Music/Band/Record");
        std::fs::create_dir_all(&album).unwrap();
        std::fs::create_dir_all(&inbox).unwrap();
        std::fs::create_dir_all(&media).unwrap();
        std::fs::write(album.join("01 Song.MP3"), b"x").unwrap();
        std::fs::write(media.join("02 Tune.m4a"), b"x").unwrap();
        std::fs::write(album.join("01 Song.xsc"), b"x").unwrap();
        std::fs::write(album.join("cover.jpg"), b"x").unwrap();
        std::fs::write(inbox.join("Other.mp3"), b"x").unwrap();
        let counted = AtomicUsize::new(0);
        let (found, missed) = walk(&root, &counted).unwrap();
        let mut paths: Vec<PathBuf> = found.into_iter().map(|f| f.0).collect();
        paths.sort();
        assert_eq!(paths, [album.join("01 Song.MP3"), media.join("02 Tune.m4a")]);
        assert!(missed.is_empty());
        assert_eq!(counted.load(Ordering::Relaxed), 2);
        let walked = walk_all(&[root.clone(), album.clone()], &AtomicUsize::new(0));
        let mut nested: Vec<PathBuf> = walked.found.into_iter().map(|f| f.0).collect();
        nested.sort();
        assert_eq!(nested, paths);
        assert!(walked.unreadable.is_empty() && walked.missed.is_empty());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    #[cfg(unix)] // shuts the folder with Unix permissions
    fn a_folder_below_that_cant_be_opened_is_listed_as_missed() {
        use std::os::unix::fs::PermissionsExt;
        let root = std::env::temp_dir().join(format!("riff-missed-{}", std::process::id()));
        let shut = root.join("Shut");
        std::fs::create_dir_all(&shut).unwrap();
        std::fs::write(root.join("01 Song.mp3"), b"x").unwrap();
        std::fs::set_permissions(&shut, std::fs::Permissions::from_mode(0o000)).unwrap();
        let walked = walk_all(std::slice::from_ref(&root), &AtomicUsize::new(0));
        std::fs::set_permissions(&shut, std::fs::Permissions::from_mode(0o755)).unwrap();
        assert_eq!(walked.found.len(), 1);
        assert!(walked.unreadable.is_empty());
        assert_eq!(walked.missed, [shut]);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_listed_folder_that_cant_be_opened_is_reported() {
        let root = std::env::temp_dir().join(format!("riff-unreadable-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("01 Song.mp3"), b"x").unwrap();
        let gone = root.join("Not here");
        let file = root.join("01 Song.mp3");
        let walked = walk_all(&[gone.clone(), root.clone(), file.clone()], &AtomicUsize::new(0));
        assert_eq!(walked.found.len(), 1);
        assert_eq!(walked.missed, [gone.clone(), file.clone()]);
        assert_eq!(
            walked.unreadable,
            [
                (gone, "it isn't there (moved, renamed, or on a drive that isn't connected)".to_string()),
                (file, "it's a file, not a folder".to_string()),
            ]
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn errors_lose_their_codes() {
        assert_eq!(plain("Input/output error (os error 5)"), "input/output error");
        assert_eq!(plain("Unrecognized format"), "unrecognized format");
        assert_eq!(plain("MP3 frame broken"), "MP3 frame broken");
        let denied = std::io::Error::from(std::io::ErrorKind::PermissionDenied);
        let says = if cfg!(target_os = "macos") { "macOS doesn't let riff in" } else { "riff isn't allowed" };
        assert!(why(&denied).starts_with(says));
    }

    #[test]
    fn formats_come_from_the_extensions_riff_reads() {
        let formats = formats();
        assert!(formats.contains(&"MP3".to_string()) && formats.contains(&"FLAC".to_string()));
        assert!(formats.iter().all(|f| is_audio(Path::new(&format!("x.{f}")))));
    }
}
