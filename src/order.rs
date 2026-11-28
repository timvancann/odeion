//! The library's sort order (plan 05; Tim, 2026-11-28, after Winamp's `%artist - %year`
//! strings): fields in a row, put in order by dragging them, each one way round or the
//! other. It only sorts, nothing is shown from it. Each field compares as what it is
//! (numbers as numbers, text without case or accents, by the file's sort name when it has
//! one), an empty value goes last (but a song without a disc number is on the first
//! disc), and the grouped view groups by the first field. Kept
//! in settings as a line of field names, `-` before one that's turned around.

use crate::library::{fold, Track};
use std::cmp::Ordering;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// What a fresh install sorts by: each artist's albums oldest first, each in disc and
/// track order.
pub const DEFAULT: &str = "artist year album disc track title";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Field {
    Artist,
    Album,
    Title,
    Year,
    Disc,
    Track,
    Length,
    Genre,
    File,
    Folder,
}

impl Field {
    pub const ALL: [Field; 10] = [
        Field::Artist,
        Field::Album,
        Field::Title,
        Field::Year,
        Field::Disc,
        Field::Track,
        Field::Length,
        Field::Genre,
        Field::File,
        Field::Folder,
    ];

    /// Its name in settings.
    pub fn name(self) -> &'static str {
        match self {
            Field::Artist => "artist",
            Field::Album => "album",
            Field::Title => "title",
            Field::Year => "year",
            Field::Disc => "disc",
            Field::Track => "track",
            Field::Length => "length",
            Field::Genre => "genre",
            Field::File => "file",
            Field::Folder => "folder",
        }
    }

    /// Its name on screen.
    pub fn label(self) -> &'static str {
        match self {
            Field::Artist => "Artist",
            Field::Album => "Album",
            Field::Title => "Title",
            Field::Year => "Year",
            Field::Disc => "Disc",
            Field::Track => "Track",
            Field::Length => "Length",
            Field::Genre => "Genre",
            Field::File => "File name",
            Field::Folder => "Folder",
        }
    }

    fn named(word: &str) -> Option<Field> {
        let word = word.to_ascii_lowercase();
        Field::ALL.into_iter().find(|f| f.name() == word)
    }

    /// Whether it compares as a number, so it has no letter on the rail.
    pub fn is_number(self) -> bool {
        matches!(self, Field::Year | Field::Disc | Field::Track | Field::Length)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Key {
    pub field: Field,
    pub reversed: bool,
}

impl Key {
    /// Which way round it sorts, in a few letters: A–Z or Z–A, 1–9 or 9–1.
    pub fn way(self) -> &'static str {
        match (self.field.is_number(), self.reversed) {
            (false, false) => "A–Z",
            (false, true) => "Z–A",
            (true, false) => "1–9",
            (true, true) => "9–1",
        }
    }

    /// The same in words, for its tooltip.
    pub fn way_in_words(self) -> &'static str {
        match (self.field, self.reversed) {
            (Field::Year, false) => "oldest first",
            (Field::Year, true) => "newest first",
            (Field::Length, false) => "shortest first",
            (Field::Length, true) => "longest first",
            (_, false) if self.field.is_number() => "lowest first",
            (_, true) if self.field.is_number() => "highest first",
            (_, false) => "A to Z",
            (_, true) => "Z to A",
        }
    }
}

/// The fields to sort by, first one first. Never empty.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Order(pub Vec<Key>);

impl Default for Order {
    fn default() -> Order {
        Order::read(DEFAULT)
    }
}

impl Order {
    /// The order a settings line keeps: field names, `-` before one turned around. A name
    /// it doesn't know (a hand-edited file) is passed over, one named twice counts where
    /// it came first, and a line with none is the default.
    pub fn read(text: &str) -> Order {
        let mut keys: Vec<Key> = Vec::new();
        for word in text.split(|c: char| c.is_whitespace() || c == ',') {
            let (reversed, name) = match word.strip_prefix('-') {
                Some(name) => (true, name),
                None => (false, word),
            };
            if let Some(field) = Field::named(name)
                && !keys.iter().any(|k| k.field == field)
            {
                keys.push(Key { field, reversed });
            }
        }
        if keys.is_empty() { Order::read(DEFAULT) } else { Order(keys) }
    }

    /// The line kept in settings.
    pub fn text(&self) -> String {
        let words: Vec<String> =
            self.0.iter().map(|k| format!("{}{}", if k.reversed { "-" } else { "" }, k.field.name())).collect();
        words.join(" ")
    }

    pub fn first(&self) -> Key {
        self.0[0]
    }

    /// The fields it doesn't sort by, to add.
    pub fn unused(&self) -> Vec<Field> {
        Field::ALL.into_iter().filter(|f| !self.0.iter().any(|k| k.field == *f)).collect()
    }

    /// `field` dropped onto the field at `onto`: it takes that place, the others making
    /// room, as a song dragged in a playlist does. A field not in the order yet comes in
    /// there, sorting A to Z; `onto` past the end puts it last.
    pub fn place(&mut self, field: Field, onto: usize) {
        let from = self.0.iter().position(|k| k.field == field);
        let key = from.map_or(Key { field, reversed: false }, |i| self.0.remove(i));
        self.0.insert(onto.min(self.0.len()), key);
    }

    /// Turns the field at `i` the other way round.
    pub fn reverse(&mut self, i: usize) {
        if let Some(k) = self.0.get_mut(i) {
            k.reversed = !k.reversed;
        }
    }

    /// Takes the field at `i` out, unless it's the last one left.
    pub fn remove(&mut self, i: usize) {
        if self.0.len() > 1 && i < self.0.len() {
            self.0.remove(i);
        }
    }

    /// `a` against `b`, field by field. An empty value goes last either way round.
    pub fn compare(&self, a: &[Option<Value>], b: &[Option<Value>]) -> Ordering {
        for (k, (a, b)) in self.0.iter().zip(a.iter().zip(b)) {
            let by = match (a, b) {
                (Some(a), Some(b)) if k.reversed => b.cmp(a),
                (Some(a), Some(b)) => a.cmp(b),
                (Some(_), None) => Ordering::Less,
                (None, Some(_)) => Ordering::Greater,
                (None, None) => Ordering::Equal,
            };
            if by != Ordering::Equal {
                return by;
            }
        }
        Ordering::Equal
    }

    /// What `t` sorts by, one value per field.
    pub fn values(&self, t: &Track, years: &Years) -> Vec<Option<Value>> {
        self.0.iter().map(|k| value(t, k.field, years)).collect()
    }

    /// What `t` is filed under on the letter rail: the first field's text, by its sort
    /// name. None when that field is a number.
    pub fn filing(&self, t: &Track) -> Option<String> {
        match value(t, self.first().field, &Years::new()) {
            Some(Value::Text(text)) => Some(text),
            _ if self.first().field.is_number() => None,
            _ => Some(String::new()),
        }
    }
}

/// A value sorted by: text folded for comparing, or a number.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum Value {
    Number(u64),
    Text(String),
}

/// Each album's year, its earliest song's, so a remaster's bonus track can't split it:
/// by folder and folded album name.
pub type Years = HashMap<(Option<PathBuf>, String), u32>;

pub fn years(tracks: &[Track]) -> Years {
    let mut years = Years::new();
    for t in tracks {
        if let Some(year) = t.year {
            let entry = years.entry(album_of(t)).or_insert(year);
            *entry = (*entry).min(year);
        }
    }
    years
}

fn album_of(t: &Track) -> (Option<PathBuf>, String) {
    (t.path.parent().map(Path::to_path_buf), fold(&t.album))
}

fn year(t: &Track, years: &Years) -> Option<u32> {
    years.get(&album_of(t)).copied().or(t.year)
}

/// Text folded, by the sort name when the file has one; empty text is no value.
fn text(name: &str, sort: &Option<String>) -> Option<Value> {
    let text = fold(sort.as_deref().unwrap_or(name).trim());
    (!text.is_empty()).then_some(Value::Text(text))
}

fn value(t: &Track, field: Field, years: &Years) -> Option<Value> {
    let number = |n: Option<u32>| n.map(|n| Value::Number(n.into()));
    match field {
        Field::Artist => text(&t.artist, &t.sort_artist),
        Field::Album => text(&t.album, &t.sort_album),
        Field::Title => text(&t.title, &t.sort_title),
        Field::Year => number(year(t, years)),
        // No disc number is a single disc: before disc 2, as the library always had it.
        Field::Disc => Some(Value::Number(t.disc.unwrap_or(0).into())),
        Field::Track => number(t.number),
        Field::Length => t.seconds.map(Value::Number),
        Field::Genre => text(t.genre.as_deref().unwrap_or(""), &None),
        Field::File => text(&t.path.file_name().unwrap_or_default().to_string_lossy(), &None),
        // The whole folder, so two folders of the same name stay apart.
        Field::Folder => text(&t.path.parent().unwrap_or(Path::new("")).to_string_lossy(), &None),
    }
}

/// What songs group by under `field` in the grouped view, and the words on its row.
/// Names group ignoring case and accents, an album by its folder too.
pub fn group(t: &Track, field: Field, years: &Years) -> (String, String) {
    let or = |text: &str, none: &str| if text.trim().is_empty() { none.to_string() } else { text.to_string() };
    let numbered = |n: Option<u32>, what: &str, none: &str| n.map_or(none.to_string(), |n| format!("{what} {n}"));
    let label = match field {
        Field::Artist => or(&t.artist, "No artist"),
        Field::Album => or(&t.album, "No album"),
        Field::Title => or(&t.title, "No title"),
        Field::Year => year(t, years).map_or("No year".to_string(), |y| y.to_string()),
        Field::Disc => numbered(t.disc, "Disc", "No disc number"),
        Field::Track => numbered(t.number, "Track", "No track number"),
        Field::Length => t.seconds.map_or("No length".to_string(), |s| format!("{}:{:02}", s / 60, s % 60)),
        Field::Genre => or(t.genre.as_deref().unwrap_or(""), "No genre"),
        Field::File => t.path.file_name().unwrap_or_default().to_string_lossy().into_owned(),
        Field::Folder => t.path.parent().and_then(Path::file_name).unwrap_or_default().to_string_lossy().into_owned(),
    };
    let key = match field {
        Field::Album => format!("{}\u{0}{}", t.path.parent().unwrap_or(Path::new("")).display(), fold(&t.album)),
        Field::Folder => t.path.parent().unwrap_or(Path::new("")).display().to_string(),
        _ => fold(&label),
    };
    (key, label)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(order: &Order) -> String {
        order.text()
    }

    #[test]
    fn settings_keep_the_order_as_a_line_of_fields() {
        let order = Order::read("artist -year album");
        assert_eq!(order.0.len(), 3);
        assert!(order.0[1].reversed);
        assert_eq!(names(&order), "artist -year album");
        assert_eq!(Order::default().text(), DEFAULT);
        // A hand-edited file: unknown names are passed over, a name twice counts once.
        assert_eq!(names(&Order::read("Artist, bpm, year, artist")), "artist year");
        assert_eq!(Order::read("bpm"), Order::default());
        assert_eq!(Order::read(""), Order::default());
    }

    #[test]
    fn a_field_dropped_onto_another_takes_its_place() {
        let mut order = Order::read("artist year album track");
        // Dragged right, onto album: album makes room to the left.
        order.place(Field::Year, 2);
        assert_eq!(names(&order), "artist album year track");
        // Dragged left, onto artist: it goes first.
        order.place(Field::Track, 0);
        assert_eq!(names(&order), "track artist album year");
        // A new one comes in where it's dropped; past the end, last.
        order.place(Field::Genre, 1);
        order.place(Field::Length, 99);
        assert_eq!(names(&order), "track genre artist album year length");
        // A reversed field keeps its way round when it moves.
        order.reverse(4);
        order.place(Field::Year, 0);
        assert_eq!(names(&order), "-year track genre artist album length");
    }

    #[test]
    fn the_last_field_stays() {
        let mut order = Order::read("artist year");
        order.remove(0);
        order.remove(0);
        assert_eq!(names(&order), "year");
        assert_eq!(order.unused().len(), Field::ALL.len() - 1);
    }

    #[test]
    fn each_field_says_which_way_round_it_sorts() {
        let year = Key { field: Field::Year, reversed: true };
        assert_eq!((year.way(), year.way_in_words()), ("9–1", "newest first"));
        let artist = Key { field: Field::Artist, reversed: false };
        assert_eq!((artist.way(), artist.way_in_words()), ("A–Z", "A to Z"));
    }

    fn song(artist: &str, album: &str, year: Option<u32>, number: Option<u32>) -> Track {
        let mut t = Track::from_path(Path::new("/m"), Path::new(&format!("/m/{artist}/{album}/x.mp3")), 0, 0);
        (t.year, t.number, t.title) = (year, number, format!("{album} {}", number.unwrap_or(0)));
        t
    }

    /// The titles of `tracks` in library order under `order`, as the library sorts them.
    fn sorted(order: &str, tracks: &[Track]) -> Vec<String> {
        let mut all = tracks.to_vec();
        crate::library::sort(&mut all, &Order::read(order));
        all.into_iter().map(|t| t.title).collect()
    }

    #[test]
    fn a_song_without_a_disc_number_comes_before_disc_two() {
        let mut first = song("A", "Double", Some(1990), Some(1));
        first.title = "untagged".into();
        let mut second = song("A", "Double", Some(1990), Some(1));
        (second.disc, second.title) = (Some(2), "disc two".into());
        assert_eq!(sorted(DEFAULT, &[second, first]), ["untagged", "disc two"]);
    }

    #[test]
    fn numbers_compare_as_numbers_and_empty_values_go_last_either_way() {
        let tracks = [
            song("A", "X", Some(1980), Some(10)),
            song("A", "X", Some(1980), Some(2)),
            song("A", "X", Some(1980), None),
        ];
        assert_eq!(sorted("track", &tracks), ["X 2", "X 10", "X 0"]);
        assert_eq!(sorted("-track", &tracks), ["X 10", "X 2", "X 0"]);
    }

    #[test]
    fn newest_album_first_keeps_each_album_whole() {
        let mut tracks = vec![
            song("A", "Old", Some(1970), Some(1)),
            song("A", "New", Some(1990), Some(1)),
            song("A", "New", Some(1990), Some(2)),
            song("A", "Undated", None, Some(1)),
        ];
        // The remaster's bonus track says 2005; its album is still from 1970.
        tracks.push(song("A", "Old", Some(2005), Some(2)));
        assert_eq!(sorted("artist -year album track", &tracks), ["New 1", "New 2", "Old 1", "Old 2", "Undated 1"]);
    }

    #[test]
    fn a_sort_name_files_the_artist_where_the_tagger_said() {
        let mut beatles = song("The Beatles", "Help!", None, Some(1));
        beatles.sort_artist = Some("Beatles, The".into());
        let tracks = [song("Bowie", "Low", None, Some(1)), beatles.clone(), song("Abba", "Arrival", None, Some(1))];
        assert_eq!(sorted("artist", &tracks), ["Arrival 1", "Help! 1", "Low 1"]);
        assert_eq!(Order::default().filing(&beatles).as_deref(), Some("beatles, the"));
        assert_eq!(Order::read("year artist").filing(&beatles), None);
    }

    #[test]
    fn groups_are_named_by_the_first_field() {
        let years = years(&[song("A", "Old", Some(1970), Some(1))]);
        let t = song("A", "Old", Some(2005), Some(2));
        assert_eq!(group(&t, Field::Year, &years).1, "1970");
        assert_eq!(group(&song("A", "X", None, None), Field::Year, &years).1, "No year");
        assert_eq!(group(&t, Field::Genre, &years).1, "No genre");
        // Same album name, another folder: another group.
        let a = group(&song("A", "Greatest Hits", None, None), Field::Album, &years);
        let b = group(&song("B", "Greatest Hits", None, None), Field::Album, &years);
        assert_eq!(a.1, b.1);
        assert_ne!(a.0, b.0);
    }
}
