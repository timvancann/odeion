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

/// Every audio file under `root`. Hidden folders and Music.app's import inbox are skipped.
pub fn walk(root: &Path) -> Vec<Found> {
    let mut found = Vec::new();
    let mut dirs = vec![root.to_path_buf()];
    while let Some(dir) = dirs.pop() {
        for entry in std::fs::read_dir(&dir).into_iter().flatten().flatten() {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().into_owned();
            let Ok(meta) = entry.metadata() else { continue };
            if meta.is_dir() {
                if !name.starts_with('.') && !name.ends_with(".localized") {
                    dirs.push(path);
                }
            } else if is_audio(&path) {
                let modified = meta
                    .modified()
                    .ok()
                    .and_then(|m| m.duration_since(UNIX_EPOCH).ok())
                    .map_or(0, |d| d.as_secs());
                found.push((path, meta.len(), modified));
            }
        }
    }
    found
}

/// Every audio file under any of `roots`, once, even where one folder sits inside another.
pub fn walk_all(roots: &[PathBuf]) -> Vec<Found> {
    let mut seen = std::collections::HashSet::new();
    roots
        .iter()
        .flat_map(|root| walk(root))
        .filter(|f| seen.insert(f.0.clone()))
        .collect()
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
/// cache knows while it is read again. Returns the library and the tracks that still need their tags.
pub fn merge(roots: &[PathBuf], found: Vec<Found>, cached: Vec<Track>) -> (Vec<Track>, Vec<Track>) {
    let mut cached: HashMap<PathBuf, Track> =
        cached.into_iter().map(|t| (t.path.clone(), t)).collect();
    let mut todo = Vec::new();
    let tracks = found
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
        );
        let titles: Vec<&str> = tracks.iter().map(|t| t.title.as_str()).collect();
        assert_eq!(titles, ["X", "Y"]);
        assert_eq!(todo.len(), 2);
    }

    #[test]
    fn tracks_read_by_an_older_reader_are_read_again() {
        let mut old = cached("/m/A/B/01 X.mp3", 10, 5);
        old.reader = READER - 1;
        let (tracks, todo) = merge(&["/m".into()], vec![found("/m/A/B/01 X.mp3", 10, 5)], vec![old]);
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
        let (tracks, _) = merge(&["/m".into()], vec![], vec![cached("/m/A/B/01 X.mp3", 1, 1)]);
        assert!(tracks.is_empty());
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
        );
        assert_eq!((tracks[0].artist.as_str(), tracks[0].album.as_str()), ("A", "B"));
        assert_eq!((tracks[1].artist.as_str(), tracks[1].album.as_str()), ("C", "D"));
    }

    #[test]
    fn walks_audio_only_and_skips_the_music_inbox() {
        let root = std::env::temp_dir().join(format!("riff-walk-{}", std::process::id()));
        let album = root.join("Artist/Album");
        let inbox = root.join("Automatically Add to Music.localized");
        std::fs::create_dir_all(&album).unwrap();
        std::fs::create_dir_all(&inbox).unwrap();
        std::fs::write(album.join("01 Song.MP3"), b"x").unwrap();
        std::fs::write(album.join("01 Song.xsc"), b"x").unwrap();
        std::fs::write(album.join("cover.jpg"), b"x").unwrap();
        std::fs::write(inbox.join("Other.mp3"), b"x").unwrap();
        let paths: Vec<PathBuf> = walk(&root).into_iter().map(|f| f.0).collect();
        assert_eq!(paths, [album.join("01 Song.MP3")]);
        let nested: Vec<PathBuf> = walk_all(&[root.clone(), album.clone()])
            .into_iter()
            .map(|f| f.0)
            .collect();
        assert_eq!(nested, [album.join("01 Song.MP3")]);
        let _ = std::fs::remove_dir_all(&root);
    }
}
