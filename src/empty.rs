//! What odeion says where there is nothing to show: the song list with no songs in it
//! (each reason in its own words), the practice view before a waveform, the loop bar
//! without a loop, the now-playing card before the first song, and the messages for a
//! folder or song that can't be read. Pure: the window hands in names and keys (from
//! `keys::keys_for`, never written here by hand).

use crate::ui::count;

/// Why the song list is empty.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Empty {
    /// No music folder yet.
    NoFolder,
    /// The first scan is still walking the folders.
    Looking,
    /// None of the listed folders could be opened.
    Unreadable,
    /// The scan found no songs.
    NoSongs,
    /// The search finds nothing; `scoped` inside an album, artist or playlist.
    NoMatch { scoped: bool },
    /// The open playlist has no songs.
    EmptyPlaylist,
}

/// What the list knows, to tell why it is empty.
#[derive(Debug, Clone, Copy, Default)]
pub struct List {
    pub rows: usize,
    pub tracks: usize,
    pub folders: usize,
    pub unreadable: usize,
    pub scanning: bool,
    /// Something is typed in the search.
    pub query: bool,
    /// The search finds only in an open album, artist or playlist.
    pub scoped: bool,
    /// A playlist is open (not an album or artist).
    pub playlist: bool,
}

/// Why the list is empty, or nothing when it isn't (or the reason is too rare to name).
pub fn why(l: List) -> Option<Empty> {
    if l.rows > 0 {
        return None;
    }
    Some(if l.folders == 0 {
        Empty::NoFolder
    } else if l.query {
        Empty::NoMatch { scoped: l.scoped }
    } else if l.playlist {
        Empty::EmptyPlaylist
    } else if l.tracks > 0 {
        return None;
    } else if l.scanning {
        Empty::Looking
    } else if l.unreadable >= l.folders {
        Empty::Unreadable
    } else {
        Empty::NoSongs
    })
}

/// What a button under the message does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Button {
    AddFolder,
    Settings,
    SearchEverything,
}

/// The message: a line, a quieter explanation, and maybe a button with its label.
#[derive(Debug, Clone, PartialEq)]
pub struct Said {
    pub title: String,
    pub detail: String,
    pub button: Option<(Button, &'static str)>,
}

/// The keys the messages mention, each the first of its action's keys, if it has one.
#[derive(Debug, Clone, Copy, Default)]
pub struct Keys<'a> {
    pub play: Option<&'a str>,
    pub settings: Option<&'a str>,
    pub add_to_playlist: Option<&'a str>,
}

/// What the list's message is about.
#[derive(Debug, Clone, Copy, Default)]
pub struct About<'a> {
    /// The listed folders, as shown.
    pub folders: &'a [String],
    /// Those that couldn't be opened: name and why.
    pub unreadable: &'a [(String, String)],
    /// Songs the first scan has found so far.
    pub found: usize,
    pub query: &'a str,
    /// The open album, artist or playlist.
    pub scope: Option<&'a str>,
    pub formats: &'a [String],
    pub keys: Keys<'a>,
}

/// " or press p", or nothing without a key.
fn or_press(key: Option<&str>) -> String {
    key.map_or(String::new(), |k| format!(" or press {k}"))
}

/// "a", "a and b", "a, b and c".
pub fn and(items: &[String]) -> String {
    match items {
        [] => String::new(),
        [one] => one.clone(),
        [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
    }
}

fn sentence(text: &str) -> String {
    let mut chars = text.chars();
    chars.next().map_or(String::new(), |c| c.to_uppercase().chain(chars).collect())
}

/// The message for an empty list.
pub fn said(empty: Empty, a: &About) -> Said {
    let settings = format!("click Open settings{}", or_press(a.keys.settings));
    match empty {
        Empty::NoFolder => Said {
            title: "Where is your music?".into(),
            detail: format!(
                "Add the folder your songs are in: click Add music folder{}. Odeion finds every song inside it and never writes there. More folders can be added in settings.",
                or_press(a.keys.play)
            ),
            button: Some((Button::AddFolder, "Add music folder…")),
        },
        Empty::Looking => Said {
            title: format!("Looking for music in {}…", and(a.folders)),
            detail: format!(
                "{} Songs in a cloud folder (Google Drive, iCloud, Dropbox) that aren't on this Mac yet are downloaded to read their names and covers, which can take a while.",
                match a.found {
                    0 => "Nothing found yet.".to_string(),
                    1 => "1 song found so far.".to_string(),
                    n => format!("{} songs found so far.", count(n)),
                }
            ),
            button: None,
        },
        Empty::Unreadable => {
            let names: Vec<String> = a.unreadable.iter().map(|(name, _)| name.clone()).collect();
            let reasons = match a.unreadable {
                [(_, why)] => sentence(why),
                all => all.iter().map(|(name, why)| format!("{name}: {why}")).collect::<Vec<_>>().join("; "),
            };
            Said {
                title: format!("Can't read {}", and(&names)),
                detail: format!("{reasons}. To pick another folder, {settings}."),
                button: Some((Button::Settings, "Open settings")),
            }
        }
        Empty::NoSongs => Said {
            title: format!("No songs found in {}", and(a.folders)),
            detail: format!("Odeion plays {} files. To add another folder, {settings}.", and(a.formats)),
            button: Some((Button::Settings, "Open settings")),
        },
        Empty::NoMatch { scoped: false } => Said {
            title: format!("No songs match '{}'", a.query.trim()),
            detail: "The search looks in song titles, artists, albums and playlist names.".into(),
            button: None,
        },
        Empty::NoMatch { scoped: true } => Said {
            title: format!("No songs in {} match '{}'", a.scope.unwrap_or("here"), a.query.trim()),
            detail: "It might be somewhere else in your music.".into(),
            button: Some((Button::SearchEverything, "Search everything")),
        },
        Empty::EmptyPlaylist => Said {
            title: format!("{} has no songs yet", a.scope.unwrap_or("This playlist")),
            detail: format!(
                "To add one, right-click a song in the library and choose Add to playlist…{}.",
                a.keys.add_to_playlist.map_or(String::new(), |k| format!(", or highlight it and press {k}"))
            ),
            button: None,
        },
    }
}

/// What the practice view's waveform says instead of a waveform, or nothing once it
/// shows. `library` and `play` are the keys back to the library and to play a song.
pub fn waveform(shown: bool, error: Option<&str>, song: bool, library: Option<&str>, play: Option<&str>) -> Option<String> {
    match (shown, error, song) {
        (true, ..) => None,
        (false, Some(e), _) => Some(format!("couldn't read this song's waveform: {e}")),
        (false, None, true) => Some("reading the waveform…".into()),
        (false, None, false) => Some(format!(
            "Nothing playing: click Library{}, then click a song{} to play it",
            or_press(library),
            or_press(play)
        )),
    }
}

/// How to make a loop, for the loop bar without one and the Loop button pressed then.
pub fn no_loop(start: Option<&str>, end: Option<&str>) -> String {
    match (start, end) {
        (Some(a), Some(b)) => format!("no loop: drag across the waveform, or press {a} at the start and {b} at the end"),
        _ => "no loop: drag across the waveform".into(),
    }
}

/// The now-playing card before any song: how to start, get to practice and find every key.
pub fn first_steps(play: Option<&str>, practice: Option<&str>, help: Option<&str>) -> String {
    format!(
        "Click a song{} to play it, then click Practice{} to practise it. Every key: the keyboard button{}",
        or_press(play),
        or_press(practice),
        help.map_or(String::new(), |k| format!(" or {k}"))
    )
}

/// The message for listed folders that couldn't be opened, or nothing when all could.
pub fn cant_read(unreadable: &[(String, String)]) -> Option<String> {
    (!unreadable.is_empty()).then(|| {
        unreadable.iter().map(|(name, why)| format!("can't read {name}: {why}")).collect::<Vec<_>>().join("; ")
    })
}

/// The message for odeion's own files that couldn't be read when it opened, given where
/// each was moved aside, or nothing when all could.
pub fn set_aside(kept: &[String]) -> Option<String> {
    (!kept.is_empty()).then(|| {
        format!("couldn't read some of Odeion's own files, so they start afresh; the old ones are kept as {}", kept.join(", "))
    })
}

/// The message for a song that didn't open.
pub fn cant_play(title: &str, error: &str) -> String {
    format!("couldn't play {title}: {error}")
}

/// A favourite's way in, said when the favourites are asked for and there are none.
pub fn no_favourites(key: Option<&str>) -> String {
    format!("no favourites yet: click the star on a song (point at it to see it){}", or_press(key))
}

/// The queue's way in, under the empty queue in the sidebar.
pub fn queue_hint(key: Option<&str>) -> String {
    format!("Point at a song and click its queue button{}", key.map_or(String::new(), |k| format!(", or press {k}")))
}

/// The same, said when the keyboard is sent to the empty queue.
pub fn no_queue(key: Option<&str>) -> String {
    let hint = queue_hint(key);
    format!("the queue is empty: {}{}", hint[..1].to_lowercase(), &hint[1..])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn list() -> List {
        List { folders: 1, ..List::default() }
    }

    #[test]
    fn a_list_with_rows_is_not_empty() {
        assert_eq!(why(List { rows: 3, ..list() }), None);
        assert_eq!(why(List { rows: 3, folders: 0, ..list() }), None);
    }

    #[test]
    fn each_reason_for_an_empty_list_is_told_apart() {
        assert_eq!(why(List { folders: 0, scanning: true, ..list() }), Some(Empty::NoFolder));
        assert_eq!(why(List { scanning: true, ..list() }), Some(Empty::Looking));
        assert_eq!(why(list()), Some(Empty::NoSongs));
        assert_eq!(why(List { folders: 2, unreadable: 1, ..list() }), Some(Empty::NoSongs));
        assert_eq!(why(List { folders: 2, unreadable: 2, ..list() }), Some(Empty::Unreadable));
        assert_eq!(why(List { tracks: 9, query: true, ..list() }), Some(Empty::NoMatch { scoped: false }));
        assert_eq!(
            why(List { tracks: 9, query: true, scoped: true, playlist: true, ..list() }),
            Some(Empty::NoMatch { scoped: true })
        );
        assert_eq!(why(List { tracks: 9, playlist: true, scoped: true, ..list() }), Some(Empty::EmptyPlaylist));
        // Songs known but none listed (an album emptied by a rescan) says nothing.
        assert_eq!(why(List { tracks: 9, ..list() }), None);
        // A rescan with the cached list showing isn't a first scan.
        assert_eq!(why(List { tracks: 9, scanning: true, rows: 9, ..list() }), None);
    }

    fn about<'a>(folders: &'a [String], formats: &'a [String]) -> About<'a> {
        About {
            folders,
            formats,
            keys: Keys { play: Some("Enter"), settings: Some("⌘,"), add_to_playlist: Some("+") },
            ..About::default()
        }
    }

    #[test]
    fn the_messages_name_folders_formats_and_keys() {
        let folders = ["~/Music".to_string(), "~/Downloads".to_string()];
        let formats = ["MP3".to_string(), "FLAC".to_string(), "WAV".to_string()];
        let a = about(&folders, &formats);
        let none = said(Empty::NoSongs, &a);
        assert_eq!(none.title, "No songs found in ~/Music and ~/Downloads");
        assert_eq!(none.detail, "Odeion plays MP3, FLAC and WAV files. To add another folder, click Open settings or press ⌘,.");
        assert_eq!(none.button, Some((Button::Settings, "Open settings")));
        assert!(said(Empty::NoFolder, &a).detail.contains("click Add music folder or press Enter"));

        let looking = said(Empty::Looking, &About { found: 1234, ..a });
        assert_eq!(looking.title, "Looking for music in ~/Music and ~/Downloads…");
        assert!(looking.detail.starts_with("1,234 songs found so far."));
        assert!(said(Empty::Looking, &a).detail.starts_with("Nothing found yet."));
    }

    #[test]
    fn a_search_says_what_found_nothing_and_where() {
        let a = About { query: " zep ", scope: Some("Houses of the Holy"), ..about(&[], &[]) };
        assert_eq!(said(Empty::NoMatch { scoped: false }, &a).title, "No songs match 'zep'");
        let scoped = said(Empty::NoMatch { scoped: true }, &a);
        assert_eq!(scoped.title, "No songs in Houses of the Holy match 'zep'");
        assert_eq!(scoped.button, Some((Button::SearchEverything, "Search everything")));
        let a = About { scope: Some("Warm-ups"), ..about(&[], &[]) };
        let playlist = said(Empty::EmptyPlaylist, &a);
        assert_eq!(playlist.title, "Warm-ups has no songs yet");
        assert!(playlist.detail.ends_with("choose Add to playlist…, or highlight it and press +."));
    }

    #[test]
    fn unreadable_folders_say_why() {
        let one = [("~/Music".to_string(), "it isn't there".to_string())];
        let a = About { unreadable: &one, ..about(&[], &[]) };
        let s = said(Empty::Unreadable, &a);
        assert_eq!(s.title, "Can't read ~/Music");
        assert_eq!(s.detail, "It isn't there. To pick another folder, click Open settings or press ⌘,.");
        assert_eq!(cant_read(&one).as_deref(), Some("can't read ~/Music: it isn't there"));
        assert_eq!(cant_read(&[]), None);
    }

    #[test]
    fn keys_are_left_out_when_an_action_has_none() {
        assert_eq!(no_loop(Some("a"), Some("b")), "no loop: drag across the waveform, or press a at the start and b at the end");
        assert_eq!(no_loop(None, Some("b")), "no loop: drag across the waveform");
        assert_eq!(no_favourites(None), "no favourites yet: click the star on a song (point at it to see it)");
        assert_eq!(queue_hint(Some("q")), "Point at a song and click its queue button, or press q");
        assert_eq!(no_queue(None), "the queue is empty: point at a song and click its queue button");
        assert_eq!(
            first_steps(Some("Enter"), Some("p"), Some("⌘?")),
            "Click a song or press Enter to play it, then click Practice or press p to practise it. Every key: the keyboard button or ⌘?"
        );
    }

    #[test]
    fn the_waveform_says_loading_failed_or_how_to_start() {
        assert_eq!(waveform(true, Some("x"), true, None, None), None);
        assert_eq!(waveform(false, Some("bad file"), true, None, None).unwrap(), "couldn't read this song's waveform: bad file");
        assert_eq!(waveform(false, None, true, None, None).unwrap(), "reading the waveform…");
        assert_eq!(
            waveform(false, None, false, Some("Esc"), Some("Enter")).unwrap(),
            "Nothing playing: click Library or press Esc, then click a song or press Enter to play it"
        );
    }

    #[test]
    fn a_failed_song_says_which() {
        assert_eq!(cant_play("Black Dog", "unrecognized format"), "couldn't play Black Dog: unrecognized format");
    }
}
