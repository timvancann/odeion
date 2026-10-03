//! The library: what a track is, how one is read from its path before its tags are
//! known, the order tracks are listed in, and search. Pure; no I/O.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Track {
    pub path: PathBuf,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub number: Option<u32>,
    /// A thumbnail of the embedded cover, in the cache.
    pub cover: Option<PathBuf>,
    /// Whether the fields above come from the file's tags rather than its path.
    pub tagged: bool,
    /// Size and modification time when the tags were read, to notice a changed file.
    pub size: u64,
    pub modified: u64,
    /// The length in seconds, once the tags are read.
    #[serde(default)]
    pub seconds: Option<u64>,
    /// The release year and disc number, from the tags.
    #[serde(default)]
    pub year: Option<u32>,
    #[serde(default)]
    pub disc: Option<u32>,
    /// Which version of the tag reader read this track (`scan::READER`).
    #[serde(default)]
    pub reader: u32,
}

impl Track {
    /// A track as far as its path tells: `Artist/Album/NN Title.ext`.
    pub fn from_path(root: &Path, path: &Path, size: u64, modified: u64) -> Track {
        let parts: Vec<String> = path
            .strip_prefix(root)
            .unwrap_or(path)
            .parent()
            .into_iter()
            .flat_map(|p| p.components())
            .map(|c| c.as_os_str().to_string_lossy().into_owned())
            .collect();
        let stem = path
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        let (number, title) = split_number(&stem);
        Track {
            path: path.to_path_buf(),
            title,
            artist: parts.first().cloned().unwrap_or_default(),
            album: parts.get(1).cloned().unwrap_or_default(),
            number,
            cover: None,
            tagged: false,
            size,
            modified,
            seconds: None,
            year: None,
            disc: None,
            reader: 0,
        }
    }

    /// The Transcribe! document for this track: same name, `.xsc`.
    pub fn xsc(&self) -> PathBuf {
        self.path.with_extension("xsc")
    }
}

/// "05 I'm the One" → (5, "I'm the One"); "1-03 Song" → (3, "Song"); no number → whole stem.
fn split_number(stem: &str) -> (Option<u32>, String) {
    let Some((head, rest)) = stem.split_once(' ') else {
        return (None, stem.to_string());
    };
    let head = head.trim_end_matches(['.', '-']);
    let last = head.rsplit('-').next().unwrap_or(head);
    let digits = |s: &str| !s.is_empty() && s.chars().all(|c| c.is_ascii_digit());
    if head.split('-').all(digits) {
        let rest = rest.trim_start_matches(['-', '.', ' ']);
        (last.parse().ok(), rest.to_string())
    } else {
        (None, stem.to_string())
    }
}

/// Artist, then album, then track number, then title, ignoring case.
pub fn sort(tracks: &mut [Track]) {
    // An album's year is its earliest track's, so a remastered bonus track can't split it.
    let mut years: HashMap<(Option<PathBuf>, String), u32> = HashMap::new();
    for t in tracks.iter() {
        if let Some(year) = t.year {
            let key = (t.path.parent().map(Path::to_path_buf), fold(&t.album));
            let entry = years.entry(key).or_insert(year);
            *entry = (*entry).min(year);
        }
    }
    tracks.sort_by_cached_key(|t| {
        let album = fold(&t.album);
        let year = years
            .get(&(t.path.parent().map(Path::to_path_buf), album.clone()))
            .copied()
            .unwrap_or(u32::MAX);
        (
            fold(&t.artist),
            year,
            album,
            t.disc.unwrap_or(0),
            t.number.unwrap_or(u32::MAX),
            fold(&t.title),
        )
    });
}

/// Lowercase, without accents, with curly apostrophes made straight, so "jamies cryin"
/// finds "Jamie’s Cryin’" and "motorhead" finds "Motörhead".
pub fn fold(text: &str) -> String {
    text.chars()
        .flat_map(char::to_lowercase)
        .filter_map(|c| {
            Some(match c {
                'à' | 'á' | 'â' | 'ã' | 'ä' | 'å' => 'a',
                'è' | 'é' | 'ê' | 'ë' => 'e',
                'ì' | 'í' | 'î' | 'ï' => 'i',
                'ò' | 'ó' | 'ô' | 'õ' | 'ö' | 'ø' => 'o',
                'ù' | 'ú' | 'û' | 'ü' => 'u',
                'ý' | 'ÿ' => 'y',
                'ñ' => 'n',
                'ç' => 'c',
                '’' | '‘' | '\'' | '`' => return None,
                _ => c,
            })
        })
        .collect()
}

/// The tracks matching a query, best first. Every word has to appear in the artist, album
/// or title. Titles that start with the query come first, then titles containing every
/// word, then the rest; library order within each.
pub fn search(tracks: &[Track], query: &str) -> Vec<usize> {
    let query = fold(query.trim());
    let words: Vec<&str> = query.split_whitespace().collect();
    if words.is_empty() {
        return (0..tracks.len()).collect();
    }
    let mut hits: Vec<(u8, usize)> = tracks
        .iter()
        .enumerate()
        .filter_map(|(i, t)| {
            let title = fold(&t.title);
            let all = format!("{} {} {}", fold(&t.artist), fold(&t.album), title);
            if !words.iter().all(|w| all.contains(w)) {
                return None;
            }
            let rank = if title.starts_with(&query) {
                0
            } else if words.iter().all(|w| title.contains(w)) {
                1
            } else {
                2
            };
            Some((rank, i))
        })
        .collect();
    hits.sort();
    hits.into_iter().map(|(_, i)| i).collect()
}

/// The tracks of `of`'s album, in track order: same album name, same folder.
pub fn album(tracks: &[Track], of: &Track) -> Vec<usize> {
    let name = fold(&of.album);
    let mut hits: Vec<usize> = (0..tracks.len())
        .filter(|&i| {
            let t = &tracks[i];
            t.path.parent() == of.path.parent() && fold(&t.album) == name
        })
        .collect();
    hits.sort_by_key(|&i| {
        let t = &tracks[i];
        (t.disc.unwrap_or(0), t.number.unwrap_or(u32::MAX), fold(&t.title))
    });
    hits
}

#[cfg(test)]
mod tests {
    use super::*;

    fn track(artist: &str, album: &str, number: u32, title: &str) -> Track {
        Track {
            path: PathBuf::from(format!("/m/{artist}/{album}/{number:02} {title}.mp3")),
            title: title.into(),
            artist: artist.into(),
            album: album.into(),
            number: Some(number),
            cover: None,
            tagged: true,
            size: 0,
            modified: 0,
            seconds: None,
            year: None,
            disc: None,
            reader: 0,
        }
    }

    #[test]
    fn reads_artist_album_number_and_title_from_the_path() {
        let t = Track::from_path(
            Path::new("/m"),
            Path::new("/m/Van Halen/Van Halen/05 I’m the One.mp3"),
            9,
            1,
        );
        assert_eq!(t.artist, "Van Halen");
        assert_eq!(t.album, "Van Halen");
        assert_eq!(t.number, Some(5));
        assert_eq!(t.title, "I’m the One");
        assert!(!t.tagged);
        assert_eq!((t.size, t.modified), (9, 1));
    }

    #[test]
    fn numbers_come_in_several_shapes() {
        assert_eq!(split_number("1-03 Song"), (Some(3), "Song".into()));
        assert_eq!(split_number("07. Song"), (Some(7), "Song".into()));
        assert_eq!(split_number("07 - Song"), (Some(7), "Song".into()));
        assert_eq!(split_number("Song Two"), (None, "Song Two".into()));
        assert_eq!(split_number("Eruption"), (None, "Eruption".into()));
        assert_eq!(split_number("1999 Live"), (Some(1999), "Live".into()));
    }

    #[test]
    fn a_loose_file_has_only_what_its_folders_give() {
        let t = Track::from_path(Path::new("/m"), Path::new("/m/Solo/Jam.wav"), 0, 0);
        assert_eq!((t.artist.as_str(), t.album.as_str()), ("Solo", ""));
        let t = Track::from_path(Path::new("/m"), Path::new("/m/Jam.wav"), 0, 0);
        assert_eq!((t.artist.as_str(), t.title.as_str()), ("", "Jam"));
    }

    #[test]
    fn the_transcribe_document_sits_next_to_the_song() {
        let t = track("Van Halen", "Van Halen", 5, "I’m the One");
        assert_eq!(t.xsc(), PathBuf::from("/m/Van Halen/Van Halen/05 I’m the One.xsc"));
    }

    #[test]
    fn sorts_by_artist_album_and_number_ignoring_case() {
        let mut tracks = vec![
            track("b", "x", 1, "z"),
            track("A", "y", 2, "a"),
            track("a", "y", 1, "b"),
        ];
        sort(&mut tracks);
        let titles: Vec<&str> = tracks.iter().map(|t| t.title.as_str()).collect();
        assert_eq!(titles, ["b", "a", "z"]);
    }

    #[test]
    fn an_artists_albums_go_oldest_first_and_undated_last() {
        let dated = |album: &str, year: Option<u32>, n: u32| {
            let mut t = track("Rush", album, n, &format!("{album} {n}"));
            t.year = year;
            t
        };
        // The bonus track's later year doesn't move or split its album.
        let mut tracks = vec![
            dated("Signals", Some(1982), 1),
            dated("Archives", None, 1),
            dated("Moving Pictures", Some(2011), 2),
            dated("Moving Pictures", Some(1981), 1),
        ];
        sort(&mut tracks);
        let titles: Vec<&str> = tracks.iter().map(|t| t.title.as_str()).collect();
        assert_eq!(
            titles,
            ["Moving Pictures 1", "Moving Pictures 2", "Signals 1", "Archives 1"]
        );
    }

    #[test]
    fn discs_play_in_order() {
        let mut one = track("A", "Double", 2, "one");
        one.disc = Some(1);
        let mut two = track("A", "Double", 1, "two");
        two.disc = Some(2);
        let mut tracks = vec![two, one];
        sort(&mut tracks);
        assert_eq!(tracks[0].title, "one");
        assert_eq!(album(&tracks, &tracks[0]), [0, 1]);
    }

    #[test]
    fn folds_case_accents_and_apostrophes() {
        assert_eq!(fold("Jamie’s Cryin’"), "jamies cryin");
        assert_eq!(fold("Motörhead"), "motorhead");
    }

    #[test]
    fn every_word_must_match_somewhere() {
        let tracks = vec![
            track("Van Halen", "Van Halen", 2, "Eruption"),
            track("Van Halen", "1984", 3, "Jump"),
            track("Queen", "Jazz", 1, "Mustapha"),
        ];
        assert_eq!(search(&tracks, "halen jump"), [1]);
        assert_eq!(search(&tracks, "van"), [0, 1]);
        assert_eq!(search(&tracks, "nothing"), Vec::<usize>::new());
        assert_eq!(search(&tracks, "  "), [0, 1, 2]);
    }

    #[test]
    fn titles_rank_above_artist_and_album_hits() {
        let tracks = vec![
            track("Jump Band", "Album", 1, "Other"),
            track("X", "Y", 1, "High Jump"),
            track("Z", "Y", 1, "Jump Around"),
        ];
        assert_eq!(search(&tracks, "jump"), [2, 1, 0]);
    }

    #[test]
    fn an_album_is_its_folder_and_name_in_track_order() {
        let mut live = track("Van Halen", "Van Halen", 1, "Live One");
        live.path = PathBuf::from("/m/Live/Van Halen/01 Live One.mp3");
        let tracks = vec![
            track("Van Halen", "Van Halen", 5, "I’m the One"),
            track("Van Halen", "1984", 1, "1984"),
            track("Van Halen", "Van Halen", 2, "Eruption"),
            live,
        ];
        assert_eq!(album(&tracks, &tracks[0]), [2, 0]);
    }
}
