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

/// The songs among `songs` that match `query` (every word somewhere in artist, album or
/// title), in their own order: searching inside an album, an artist or a playlist.
pub fn filter(tracks: &[Track], songs: &[usize], query: &str) -> Vec<usize> {
    let query = fold(query.trim());
    let words: Vec<&str> = query.split_whitespace().collect();
    songs
        .iter()
        .copied()
        .filter(|&i| {
            let t = &tracks[i];
            let all = format!("{} {} {}", fold(&t.artist), fold(&t.album), fold(&t.title));
            words.iter().all(|w| all.contains(w))
        })
        .collect()
}

/// Whether a name (a playlist's) has every word of `query` in it. An empty query
/// matches nothing, so the full list doesn't list every playlist.
pub fn named(name: &str, query: &str) -> bool {
    let (name, query) = (fold(name), fold(query.trim()));
    let words: Vec<&str> = query.split_whitespace().collect();
    !words.is_empty() && words.iter().all(|w| name.contains(w))
}

/// The library's index letters, in its order: digits and symbols sort first.
pub const ALPHABET: &str = "#ABCDEFGHIJKLMNOPQRSTUVWXYZ";

/// The letter an artist is filed under: their first letter, `#` for anything else.
pub fn initial(artist: &str) -> char {
    match fold(artist).chars().next() {
        Some(c) if c.is_ascii_alphabetic() => c.to_ascii_uppercase(),
        _ => '#',
    }
}

/// Where each letter begins in a list of artists in library order: the letter and the
/// position of its first artist, once per letter.
pub fn letters<'a>(artists: impl IntoIterator<Item = (usize, &'a str)>) -> Vec<(char, usize)> {
    let mut found: Vec<(char, usize)> = Vec::new();
    for (at, artist) in artists {
        let letter = initial(artist);
        if !found.iter().any(|&(l, _)| l == letter) {
            found.push((letter, at));
        }
    }
    found
}

/// Where picking `letter` goes: its first artist or, when it has none, the next
/// letter's; past the last letter, the last one there is.
pub fn jump(letters: &[(char, usize)], letter: char) -> Option<usize> {
    let rank = |c: char| ALPHABET.find(c).unwrap_or(0);
    letters
        .iter()
        .filter(|&&(l, _)| rank(l) >= rank(letter))
        .min_by_key(|&&(l, _)| rank(l))
        .or(letters.iter().max_by_key(|&&(l, _)| rank(l)))
        .map(|&(_, at)| at)
}

/// Everything by `of`'s artist, in library order (albums oldest first, each in track
/// order), ignoring case and accents in the name.
pub fn artist(tracks: &[Track], of: &Track) -> Vec<usize> {
    let name = fold(&of.artist);
    (0..tracks.len()).filter(|&i| fold(&tracks[i].artist) == name).collect()
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

/// One row of a grouped list: the artist (or, in an artist's view, the album), said once
/// with how many songs follow, or a song.
#[derive(Debug, Clone, PartialEq)]
pub enum Grouped {
    Artist(String, usize),
    Album(String, usize),
    Track(usize),
}

/// `songs` (in library order) with each run by one artist under a row naming it.
/// Names match ignoring case and accents; the first song's spelling is shown.
pub fn grouped(tracks: &[Track], songs: &[usize]) -> Vec<Grouped> {
    runs(tracks, songs, |t| (None, fold(&t.artist)), |t| Grouped::Artist(t.artist.clone(), 0))
}

/// An artist's `songs` (in library order) with each album's run under a row naming it.
/// An album is as in `album`: the same name in the same folder.
pub fn by_album(tracks: &[Track], songs: &[usize]) -> Vec<Grouped> {
    runs(tracks, songs, |t| (t.path.parent(), fold(&t.album)), |t| Grouped::Album(t.album.clone(), 0))
}

/// `songs` with a `head` row above each run of the same `key`, counting the run.
fn runs<'t>(
    tracks: &'t [Track],
    songs: &[usize],
    key: impl Fn(&'t Track) -> (Option<&'t Path>, String),
    head: impl Fn(&Track) -> Grouped,
) -> Vec<Grouped> {
    let mut rows = Vec::with_capacity(songs.len() * 5 / 4);
    let mut at = 0;
    let mut last = None;
    for &i in songs {
        let this = key(&tracks[i]);
        if last.as_ref() != Some(&this) {
            at = rows.len();
            rows.push(head(&tracks[i]));
            last = Some(this);
        }
        if let Grouped::Artist(_, n) | Grouped::Album(_, n) = &mut rows[at] {
            *n += 1;
        }
        rows.push(Grouped::Track(i));
    }
    rows
}

/// What a row of the list is, for pinning artist rows.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Kind {
    /// "Favourites", "Library": the list's sections.
    Section,
    /// An artist's row, or an album's in an artist's view: the rows that pin.
    Artist,
    Track,
}

/// The artist row pinned to the top of a grouped list scrolled `scrolled` rows down
/// (fractions count): the one whose songs are at the top, and how far up the next
/// artist or section, arriving under it, pushes it (0 to 1 row). None above the first
/// artist of a section.
pub fn pinned(kinds: &[Kind], scrolled: f32) -> Option<(usize, f32)> {
    let top = scrolled.max(0.).floor() as usize;
    let head = (0..=top.min(kinds.len().checked_sub(1)?))
        .rev()
        .take_while(|&r| kinds[r] != Kind::Section)
        .find(|&r| kinds[r] == Kind::Artist)?;
    let push = match kinds.get(top + 1) {
        Some(Kind::Artist | Kind::Section) => scrolled.max(0.) - top as f32,
        _ => 0.,
    };
    Some((head, push))
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
    fn searching_inside_a_list_keeps_its_order() {
        let tracks = vec![
            track("Van Halen", "Van Halen", 2, "Eruption"),
            track("Van Halen", "1984", 3, "Jump"),
            track("Queen", "Jazz", 1, "Mustapha"),
        ];
        // A playlist's order, not the library's.
        assert_eq!(filter(&tracks, &[2, 1, 0], "a"), [2, 1, 0]);
        assert_eq!(filter(&tracks, &[2, 1, 0], "halen"), [1, 0]);
        assert_eq!(filter(&tracks, &[2, 0], "jump"), Vec::<usize>::new());
        assert_eq!(filter(&tracks, &[2, 0], " "), [2, 0]);
    }

    #[test]
    fn playlists_are_found_by_every_word_of_their_name() {
        assert!(named("Gig warm-up", "warm gig"));
        assert!(named("Motörhead covers", "motorhead"));
        assert!(!named("Gig", "gig set"));
        assert!(!named("Gig", "  "));
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
    fn artists_are_filed_under_their_first_letter_or_hash() {
        assert_eq!(initial("Ángela Aguilar"), 'A');
        assert_eq!(initial("rush"), 'R');
        assert_eq!(initial("3 Doors Down"), '#');
        assert_eq!(initial(""), '#');
        let artists = ["3 Doors Down", "AC/DC", "Aerosmith", "Def Leppard", "Rush", "Rush"];
        let found = letters(artists.iter().enumerate().map(|(i, a)| (i + 10, *a)));
        assert_eq!(found, vec![('#', 10), ('A', 11), ('D', 13), ('R', 14)]);
    }

    #[test]
    fn a_letter_without_artists_jumps_to_the_next_one() {
        let found = [('#', 0), ('A', 5), ('D', 9), ('R', 20)];
        assert_eq!(jump(&found, 'A'), Some(5));
        assert_eq!(jump(&found, 'B'), Some(9));
        assert_eq!(jump(&found, 'S'), Some(20));
        assert_eq!(jump(&[], 'S'), None);
    }

    #[test]
    fn an_artist_is_every_track_under_the_name_in_library_order() {
        let mut tracks = vec![
            track("Rush", "Signals", 1, "Subdivisions"),
            track("Dire Straits", "Dire Straits", 1, "Down to the Waterline"),
            track("rush", "Moving Pictures", 2, "Red Barchetta"),
            track("Rush", "Moving Pictures", 1, "Tom Sawyer"),
        ];
        tracks[1].year = Some(1978);
        sort(&mut tracks);
        let titles: Vec<&str> =
            artist(&tracks, &tracks[1]).into_iter().map(|i| tracks[i].title.as_str()).collect();
        assert!(titles.iter().all(|t| *t != "Down to the Waterline"));
        assert_eq!(titles.len(), 3);
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

    #[test]
    fn grouping_says_each_artist_once_with_its_count() {
        let tracks = vec![
            track("Dio", "Holy Diver", 1, "Stand Up and Shout"),
            track("dio", "Holy Diver", 2, "Holy Diver"),
            track("Iron Maiden", "Powerslave", 1, "Aces High"),
        ];
        assert_eq!(
            grouped(&tracks, &[0, 1, 2]),
            vec![
                Grouped::Artist("Dio".into(), 2),
                Grouped::Track(0),
                Grouped::Track(1),
                Grouped::Artist("Iron Maiden".into(), 1),
                Grouped::Track(2),
            ]
        );
        assert!(grouped(&tracks, &[]).is_empty());
    }

    #[test]
    fn an_artists_songs_group_under_each_album() {
        let mut tracks = vec![
            track("Dio", "Holy Diver", 1, "Stand Up and Shout"),
            track("Dio", "Holy Diver", 2, "Holy Diver"),
            track("Dio", "The Last in Line", 1, "We Rock"),
        ];
        // The same name in another folder is another album.
        let mut elsewhere = track("Dio", "The Last in Line", 2, "The Last in Line");
        elsewhere.path = PathBuf::from("/m/Dio/Live/02 The Last in Line.mp3");
        tracks.push(elsewhere);
        assert_eq!(
            by_album(&tracks, &[0, 1, 2, 3]),
            vec![
                Grouped::Album("Holy Diver".into(), 2),
                Grouped::Track(0),
                Grouped::Track(1),
                Grouped::Album("The Last in Line".into(), 1),
                Grouped::Track(2),
                Grouped::Album("The Last in Line".into(), 1),
                Grouped::Track(3),
            ]
        );
    }

    #[test]
    fn the_artist_of_the_top_row_stays_pinned_until_the_next_pushes_it_up() {
        use Kind::*;
        let kinds = [Section, Artist, Track, Track, Artist, Track, Section, Artist, Track];
        assert_eq!(pinned(&kinds, 0.0), None);
        assert_eq!(pinned(&kinds, 1.0), Some((1, 0.0)));
        assert_eq!(pinned(&kinds, 2.5), Some((1, 0.0)));
        // Row 4, the next artist, is arriving under the pinned one.
        assert_eq!(pinned(&kinds, 3.25), Some((1, 0.25)));
        assert_eq!(pinned(&kinds, 4.0), Some((4, 0.0)));
        assert_eq!(pinned(&kinds, 5.5), Some((4, 0.5)));
        // A section starts afresh.
        assert_eq!(pinned(&kinds, 6.0), None);
        assert_eq!(pinned(&kinds, 7.5), Some((7, 0.0)));
        assert_eq!(pinned(&[], 0.0), None);
    }
}
