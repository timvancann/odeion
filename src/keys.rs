//! What each key does, and the ⌘? sheet that lists them, side by side so they can't
//! disagree. Pure: `view.rs` turns a keystroke into a `Press` and performs the `Action`.
//!
//! Riff has modes, like vim. The library is for finding and playing songs, search is
//! its sub-mode where keys type, and practice is the waveform with the loop, sections
//! and stems. A key means what the current mode's map says, after the few keys that
//! work everywhere. There are two keyboards, chosen in settings: Mac (the default),
//! where typing a letter in the library starts a search, and Vim, where letters there
//! are commands (`j k`, `gg`, `dd`). Each mode's map is the keys both share, then the
//! keyboard's own, so the two differ only in bare letters and plain arrows.

use serde::{Deserialize, Serialize};

/// The loop bar's widen and narrow buttons move an edge this far.
pub const NUDGE: f64 = 0.05;
/// Each zoom step shows this many times more or less.
pub const ZOOM: f64 = 1.5;
/// ⌃D and ⌃U move this many rows.
pub const HALF_PAGE: isize = 6;
const SEEK: f64 = 5.0;

/// Which keys riff answers to, chosen in settings.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Keyboard {
    /// What a Mac user already knows: typing in the library starts a search.
    #[default]
    Mac,
    /// Letters in the library are commands, as in vim.
    Vim,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// Finding and playing songs. Riff opens here.
    Library,
    /// Typing a search, within the library.
    Search,
    /// The practice view: the waveform, the loop, sections and stems.
    Practice,
    /// The keyboard in the sidebar's queue, from the library or practice.
    Queue,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Action {
    Move(isize),
    Top,
    Bottom,
    /// Highlight the playing song.
    ToPlaying,
    /// Search on or off: Esc in search stops typing and keeps the results.
    Search,
    /// Into the search to type, from anywhere; already typing, nothing changes.
    Find,
    /// Esc in the library: clear the search and leave the album.
    Clear,
    ClearSearch,
    Play,
    Toggle,
    Seek(f64),
    ToStart,
    /// Plays from the marker, or the loop's start without one.
    FromLoopStart,
    /// Drops the marker at the playhead, or clears it there.
    Marker,
    Previous,
    Next,
    Shuffle,
    Favorite,
    Album,
    /// Everything by the highlighted song's artist, or back.
    Artist,
    Help,
    Settings,
    Quit,
    /// Hide riff, as ⌘H does in every Mac app (no menu bar does it for riff).
    Hide,
    LoopStart,
    LoopEnd,
    LoopClear,
    /// Looping on or off, keeping the edges.
    LoopSwitch,
    /// Widen (below 0) or narrow the loop at its start; buttons only.
    NudgeStart(f64),
    /// Widen (above 0) or narrow the loop at its end; buttons only.
    NudgeEnd(f64),
    /// Slide the loop on (true) or back by its length; buttons only.
    Slide(bool),
    /// Practice mode on or off.
    Practice,
    /// Practice mode on (true) or the library, whichever it was.
    SetPractice(bool),
    /// Zoom the practice view: below 1 closer in, above 1 further out.
    Zoom(f64),
    /// Zoom to fit the loop, or the whole song without one.
    ZoomFit,
    /// The beat grid on or off: loop edges and steps follow the beats.
    Grid,
    /// Save the loop as a named region (or rename the region it is).
    SaveRegion,
    /// The next (true) or previous region, as the loop.
    Region(bool),
    /// The section in play as the loop; from a section's menu, that section. No key.
    LoopRegion,
    /// Play from the start of the section in play, or the loop's; a button, no key.
    SectionStart,
    /// The practice view follows the playhead again after being dragged away.
    Follow,
    /// Delete the region in play.
    RemoveRegion,
    /// The playing song's next recording: the song, then each variation.
    Variation,
    /// Pick files to add as a variation of the playing song.
    AddVariation,
    /// Remove the playing variation.
    RemoveVariation,
    /// Minimise the window, or make it full screen (no menu bar does it for riff).
    Minimize,
    FullScreen,
    /// Speed and pitch as recorded, and again back to the song's kept ones.
    Slow,
    /// 5% slower (false) or faster (true), from 25 to 110%.
    Speed(bool),
    /// Play at this percent; from the speed menu, no key.
    SetSpeed(u16),
    /// The pitch up (above 0) or down by this many cents: a semitone or 5 cents.
    Pitch(i16),
    /// The pitch at this many cents; from the pitch menu, no key.
    SetPitch(i16),
    /// The speed trainer on or off: the loop speeds up as it repeats.
    Trainer,
    /// The trainer's settings sheet.
    TrainerSettings,
    /// The volume up (true) or down a step.
    Volume(bool),
    /// Stem n's level, in percent; from its menu, no key.
    Level(usize, u8),
    /// Mute or unmute stem n (from 0) of a stem set.
    Mute(usize),
    /// Solo stem n, or end the solo.
    Solo(usize),
    /// Queue the highlighted song: at the end, or at the top (true).
    Queue(bool),
    /// The keyboard into the queue, or back out of it.
    QueueFocus,
    /// The sidebar shown or hidden.
    Sidebar,
    /// In the queue: play the highlighted song now.
    QueuePlay,
    /// In the queue: the highlighted song to the top.
    QueueTop,
    /// In the queue: remove the highlighted song.
    QueueRemove,
    /// The list in columns or grouped by artist; the title bar's switch, no key yet.
    ListView,
    /// The EQ sheet open or closed.
    EqSheet,
    /// Add a song to a playlist (the picker): the highlighted one, or the playing one
    /// in practice.
    AddToPlaylist,
    /// In a playlist's view: remove the highlighted song from it.
    PlaylistRemove,
    /// In a playlist's view: move the highlighted song up (true) or down.
    PlaylistMove(bool),
    /// Play the songs listed (the open album, artist or playlist) from the first, or
    /// shuffled; from a menu, the songs it was opened on. No key.
    PlayAll,
    ShuffleAll,
    /// Queue every song listed, or those a menu was opened on. No key.
    QueueAll,
    /// Add every song listed, or those a menu was opened on, to a playlist. No key.
    AddAllToPlaylist,
    /// Open the highlighted playlist, or the one a menu was opened on. No key.
    OpenPlaylist,
    /// Rename or delete (asking first) the open playlist, or the one a menu was opened
    /// on. No key.
    RenamePlaylist,
    DeletePlaylist,
}

/// A key as GPUI reports it: `key` is the character or a name like "left", with shift
/// already applied to symbols ("{", "?") but not to letters ("g" plus `shift`).
#[derive(Debug, Clone, Copy, Default)]
pub struct Press<'a> {
    pub key: &'a str,
    pub cmd: bool,
    pub ctrl: bool,
    pub shift: bool,
    pub alt: bool,
}

/// What a key does in `mode`. `pending` holds the first key of `gg`, `zz` and `dd`.
pub fn press(mode: Mode, p: Press, keyboard: Keyboard, pending: &mut Option<char>) -> Option<Action> {
    let first = pending.take();
    if let Some(action) = anywhere(p) {
        return Some(action);
    }
    let vim = keyboard == Keyboard::Vim;
    match mode {
        Mode::Search => search(p),
        Mode::Library => outside(p)
            .or_else(|| library(p))
            .or_else(|| if vim { vim_library(p, first, pending) } else { None }),
        Mode::Practice => outside(p).or_else(|| practice(p)).or_else(|| match vim {
            true => vim_practice(p, first, pending),
            false => mac_practice(p),
        }),
        Mode::Queue => outside(p)
            .or_else(|| queue(p))
            .or_else(|| if vim { vim_queue(p, first, pending) } else { None }),
    }
}

/// Whether a key in the library starts a search with what it types: ⌘V (pasting) with
/// either keyboard, and with the Mac keys a letter or digit. Space and symbols keep
/// their commands.
pub fn starts_search(p: Press, mode: Mode, keyboard: Keyboard) -> bool {
    let mut chars = p.key.chars();
    let letter = matches!((chars.next(), chars.next()), (Some(c), None) if c.is_alphanumeric());
    let typed = letter && !p.cmd && !p.ctrl && !p.alt;
    mode == Mode::Library && (pastes(p) || (keyboard == Keyboard::Mac && typed))
}

/// ⌘V: paste into the search or the name being typed.
pub fn pastes(p: Press) -> bool {
    p.cmd && !p.ctrl && !p.alt && p.key == "v"
}

/// Pasted text as one line, for a search or a name: its lines joined by spaces.
pub fn one_line(text: &str) -> String {
    text.split(['\r', '\n']).map(str::trim).filter(|l| !l.is_empty()).collect::<Vec<_>>().join(" ")
}

/// Keys that work in every mode, even while typing a search: ⌘ shortcuts, ← → and the
/// volume. Both keyboards share them.
pub fn anywhere(p: Press) -> Option<Action> {
    use Action::*;
    if !p.cmd {
        return Some(match (p.key, p.ctrl, p.alt) {
            ("up", false, true) => Volume(true),
            ("down", false, true) => Volume(false),
            ("left", false, true) => ToStart,
            ("left", false, false) => Seek(-SEEK),
            ("right", false, false) => Seek(SEEK),
            _ => return None,
        });
    }
    Some(match (p.key, p.ctrl, p.alt) {
        ("p", ..) => Search,
        ("f", true, _) => FullScreen,
        ("f", false, _) => Find,
        ("m", false, _) => Minimize,
        ("d", false, false) => Favorite,
        ("a", false, false) if p.shift => Artist,
        ("a", false, false) => Album,
        ("s", true, false) => Sidebar,
        ("s", false, false) => Shuffle,
        ("h", false, false) => Hide,
        ("left", ..) => Previous,
        ("right", ..) => Next,
        ("1", false, false) => SetPractice(false),
        ("2", false, false) => SetPractice(true),
        ("o", false, false) => AddVariation,
        ("?", ..) => Help,
        ("/", ..) if p.shift => Help,
        (",", ..) => Settings,
        ("q", ..) => Quit,
        ("b", false, false) if !p.shift => Sidebar,
        ("e", false, _) if !p.shift => EqSheet,
        _ => return None,
    })
}

/// Bare keys that work in the library, practice and the queue, though not while typing.
fn outside(p: Press) -> Option<Action> {
    if p.cmd || p.ctrl || p.alt {
        return None;
    }
    Some(match (p.key, p.shift) {
        ("space", _) => Action::Toggle,
        ("`", false) => Action::QueueFocus,
        _ => return None,
    })
}

/// The start, the marker and playing from it: practice keys, which Vim's library has too.
fn marker(p: Press) -> Option<Action> {
    if p.cmd || p.ctrl || p.alt || p.shift {
        return None;
    }
    Some(match p.key {
        "0" => Action::ToStart,
        "m" => Action::Marker,
        "o" => Action::FromLoopStart,
        _ => return None,
    })
}

/// Moving through a list: the library's, the search results and the queue.
fn moving(p: Press) -> Option<Action> {
    use Action::Move;
    Some(match (p.key, p.cmd, p.ctrl, p.alt) {
        ("j", false, true, _) | ("down", false, false, false) => Move(1),
        ("k", false, true, _) | ("up", false, false, false) => Move(-1),
        ("pagedown", false, ..) => Move(10),
        ("pageup", false, ..) => Move(-10),
        _ => return None,
    })
}

/// Keys on the song list, in the library and while searching.
fn list(p: Press) -> Option<Action> {
    use Action::*;
    if let Some(action) = moving(p) {
        return Some(action);
    }
    Some(match (p.key, p.cmd, p.ctrl, p.alt) {
        ("home", false, false, false) | ("up", true, false, false) => Top,
        ("end", false, false, false) | ("down", true, false, false) => Bottom,
        ("enter", false, false, true) => Queue(p.shift),
        ("enter", false, false, false) if !p.shift => Play,
        ("l", true, false, false) => ToPlaying,
        ("n", true, false, false) => AddToPlaylist,
        ("backspace", true, false, false) => ClearSearch,
        _ => return None,
    })
}

fn search(p: Press) -> Option<Action> {
    match p.key {
        // Stops typing, keeping the results; Esc again in the library clears them.
        "escape" => Some(Action::Search),
        _ => list(p),
    }
}

fn library(p: Press) -> Option<Action> {
    use Action::*;
    // Before `moving`, which ignores shift.
    match (p.key, p.shift, p.cmd || p.ctrl || p.alt) {
        ("up", true, false) => return Some(PlaylistMove(true)),
        ("down", true, false) => return Some(PlaylistMove(false)),
        _ => {}
    }
    if let Some(action) = list(p) {
        return Some(action);
    }
    Some(match (p.key, p.shift, p.cmd || p.ctrl || p.alt) {
        ("escape", ..) => Clear,
        ("tab", false, false) => Practice,
        ("+", _, false) => AddToPlaylist,
        ("backspace", false, false) => PlaylistRemove,
        ("?", _, false) => Help,
        _ => return None,
    })
}

fn vim_library(p: Press, first: Option<char>, pending: &mut Option<char>) -> Option<Action> {
    use Action::*;
    if p.cmd || p.alt {
        return None;
    }
    if let Some(action) = marker(p).filter(|_| !p.ctrl) {
        return Some(action);
    }
    Some(match (p.key, p.shift, p.ctrl) {
        ("p", false, false) => Practice,
        ("q", shift, false) => Queue(shift),
        ("x", false, false) => PlaylistRemove,
        ("g", false, false) if first == Some('g') => Top,
        ("z", false, false) if first == Some('z') => ToPlaying,
        ("g" | "z", false, false) => {
            *pending = p.key.chars().next();
            return None;
        }
        ("g", true, _) => Bottom,
        ("k", true, false) => PlaylistMove(true),
        ("j", true, false) => PlaylistMove(false),
        ("d", _, true) => Move(HALF_PAGE),
        ("u", _, true) => Move(-HALF_PAGE),
        ("j", false, false) => Move(1),
        ("k", false, false) => Move(-1),
        ("h", false, false) => Seek(-SEEK),
        ("l", false, false) => Seek(SEEK),
        ("h", true, _) => Previous,
        ("l", true, _) => Next,
        ("/", ..) => Search,
        ("f", false, false) => Favorite,
        ("a", false, false) => Album,
        ("a", true, _) => Artist,
        ("i", false, false) => Search,
        ("s", false, false) => Shuffle,
        _ => return None,
    })
}

/// In the sidebar's queue.
fn queue(p: Press) -> Option<Action> {
    use Action::*;
    if let Some(action) = moving(p) {
        return Some(action);
    }
    if p.cmd || p.ctrl || p.alt {
        return None;
    }
    if let Some(action) = marker(p) {
        return Some(action);
    }
    Some(match (p.key, p.shift) {
        ("enter", _) => QueuePlay,
        ("t", false) => QueueTop,
        ("x", false) | ("backspace", _) => QueueRemove,
        ("escape" | "tab", _) => QueueFocus,
        ("?", _) => Help,
        _ => return None,
    })
}

fn vim_queue(p: Press, first: Option<char>, pending: &mut Option<char>) -> Option<Action> {
    use Action::*;
    if p.cmd || p.ctrl || p.alt {
        return None;
    }
    Some(match (p.key, p.shift) {
        ("j", false) => Move(1),
        ("k", false) => Move(-1),
        ("d", false) if first == Some('d') => QueueRemove,
        ("d", false) => {
            *pending = Some('d');
            return None;
        }
        _ => return None,
    })
}

/// Practice keys sit on the Glove80's base layer (shift counts as base). Brackets are
/// kept as quiet aliases for the loop edges, off the sheet.
fn practice(p: Press) -> Option<Action> {
    use Action::*;
    if p.cmd {
        return match (p.key, p.alt) {
            ("=" | "+", false) => Some(Zoom(1.0 / ZOOM)),
            ("-", false) => Some(Zoom(ZOOM)),
            ("0", false) => Some(ZoomFit),
            ("l", false) => Some(Follow),
            ("l", true) => Some(LoopSwitch),
            ("n", false) => Some(AddToPlaylist),
            _ => None,
        };
    }
    if p.ctrl {
        return None;
    }
    // Stems by number: 1 to 9 mute, ⌥ solos.
    if let Some(n) = p.key.parse::<usize>().ok().filter(|n| (1..=9).contains(n)) {
        return Some(if p.alt { Solo(n - 1) } else { Mute(n - 1) });
    }
    if let Some(action) = marker(p) {
        return Some(action);
    }
    Some(match (p.key, p.shift) {
        ("escape" | "tab", _) => Practice,
        ("a", false) | ("[", _) => LoopStart,
        ("b", false) | ("]", _) => LoopEnd,
        ("\\", _) => LoopSwitch,
        ("|", _) | ("backspace", _) => LoopClear,
        ("r", false) => SaveRegion,
        ("n", false) => Region(true),
        ("n", true) => Region(false),
        ("g", false) => Grid,
        ("s", false) => Slow,
        ("t", false) => Trainer,
        ("t", true) => TrainerSettings,
        (",", _) => Speed(false),
        (".", _) => Speed(true),
        ("-", _) => Zoom(ZOOM),
        ("=", _) => Zoom(1.0 / ZOOM),
        ("v", false) => Variation,
        ("v", true) => AddVariation,
        ("x", true) => RemoveVariation,
        ("+", _) => AddToPlaylist,
        ("?", _) => Help,
        _ => return None,
    })
}

fn vim_practice(p: Press, first: Option<char>, pending: &mut Option<char>) -> Option<Action> {
    use Action::*;
    if p.cmd || p.ctrl || p.alt {
        return None;
    }
    Some(match (p.key, p.shift) {
        ("p", false) => Practice,
        ("d", false) if first == Some('d') => RemoveRegion,
        ("d", false) => {
            *pending = Some('d');
            return None;
        }
        ("h", false) => Seek(-SEEK),
        ("l", false) => Seek(SEEK),
        ("k", false) => Pitch(100),
        ("j", false) => Pitch(-100),
        ("k", true) => Pitch(5),
        ("j", true) => Pitch(-5),
        _ => return None,
    })
}

/// The Mac keys' practice: the arrows move the pitch (up is higher), `l` loops.
fn mac_practice(p: Press) -> Option<Action> {
    use Action::*;
    if p.cmd || p.ctrl || p.alt {
        return None;
    }
    Some(match (p.key, p.shift) {
        ("up", shift) => Pitch(if shift { 5 } else { 100 }),
        ("down", shift) => Pitch(if shift { -5 } else { -100 }),
        ("l", false) => LoopSwitch,
        _ => return None,
    })
}

/// What a keycap on the sheet does in `mode` ("⌘←", "G", "⌃⌘F", "gg"), read through the
/// same keymap as a real key press, so the sheet and tooltips can't drift from it.
pub fn resolve(cap: &str, mode: Mode, keyboard: Keyboard) -> Option<Action> {
    // Two of the same letter, like `gg` or `dd`: pressed twice.
    let mut letters = cap.chars();
    if let (Some(a), Some(b), None) = (letters.next(), letters.next(), letters.next())
        && a == b
        && a.is_ascii_lowercase()
    {
        let mut pending = None;
        let press = Press { key: &cap[..1], ..Press::default() };
        self::press(mode, press, keyboard, &mut pending);
        return self::press(mode, press, keyboard, &mut pending);
    }
    // Modifiers in any order: "⌃⌘F" as macOS writes it.
    let (mut cmd, mut ctrl, mut alt, mut shift, mut rest) = (false, false, false, false, cap);
    loop {
        if let Some(r) = rest.strip_prefix('⇧') {
            (shift, rest) = (true, r);
        } else if let Some(r) = rest.strip_prefix('⌘') {
            (cmd, rest) = (true, r);
        } else if let Some(r) = rest.strip_prefix('⌃') {
            (ctrl, rest) = (true, r);
        } else if let Some(r) = rest.strip_prefix('⌥') {
            (alt, rest) = (true, r);
        } else {
            break;
        }
    }
    let named = match rest {
        "←" => "left",
        "→" => "right",
        "↑" => "up",
        "↓" => "down",
        "⌫" => "backspace",
        "Enter" => "enter",
        "Space" => "space",
        "Esc" => "escape",
        "Tab" => "tab",
        "PgUp" => "pageup",
        "PgDn" => "pagedown",
        "Home" => "home",
        "End" => "end",
        other => other,
    };
    let upper = named.len() == 1 && named.chars().all(|c| c.is_ascii_uppercase());
    let key = if upper || ctrl || cmd { named.to_lowercase() } else { named.to_string() };
    let press = Press { key: &key, cmd, ctrl, shift: shift || (upper && !ctrl && !cmd), alt };
    self::press(mode, press, keyboard, &mut None)
}

/// The keys for an action in `mode` with `keyboard`, as the sheet writes them: for
/// tooltips and menus.
pub fn keys_for(action: Action, mode: Mode, keyboard: Keyboard) -> Vec<&'static str> {
    sheet(mode)
        .flat_map(|g| g.lines.iter())
        .flat_map(|&line| caps(line, keyboard))
        .filter(|cap| resolve(cap, mode, keyboard) == Some(action))
        .collect()
}

/// One line of the ⌘? sheet: the keys both keyboards share, Vim's own, the Mac's own,
/// and what they do.
pub type Line = (&'static str, &'static str, &'static str, &'static str);

/// The keycaps a line shows with `keyboard`: the shared ones, then the keyboard's own.
pub fn caps(line: Line, keyboard: Keyboard) -> impl Iterator<Item = &'static str> {
    let (shared, vim, mac, _) = line;
    let own = match keyboard {
        Keyboard::Vim => vim,
        Keyboard::Mac => mac,
    };
    shared.split_whitespace().chain(own.split_whitespace())
}

/// A group of lines on the ⌘? sheet, and the modes whose sheet shows it.
pub struct Group {
    pub title: &'static str,
    pub modes: &'static [Mode],
    pub lines: &'static [Line],
}

/// Whether a sheet line is found by `query`, typed on the ⌘? sheet: every word of it
/// somewhere in what the line does, its group's title or the keys it shows with
/// `keyboard`, case aside.
pub fn found(title: &str, line: Line, keyboard: Keyboard, query: &str) -> bool {
    let keys: Vec<&str> = caps(line, keyboard).collect();
    let hay = format!("{title} {} {}", keys.join(" "), line.3).to_lowercase();
    query.to_lowercase().split_whitespace().all(|word| hay.contains(word))
}

/// The groups the ⌘? sheet shows in `mode`.
pub fn sheet(mode: Mode) -> impl Iterator<Item = &'static Group> {
    SHEET.iter().filter(move |g| g.modes.contains(&mode))
}

use Mode::{Library, Practice, Queue, Search};

/// Every shortcut, as the ⌘? sheet lists them, one keycap per space-separated key.
pub const SHEET: &[Group] = &[
    Group {
        title: "Playing",
        modes: &[Library, Practice],
        lines: &[
            ("Space", "", "", "pause or resume"),
            ("← →", "", "", "back or forward 5 seconds, a beat on the grid"),
            ("⌥↑ ⌥↓", "", "", "volume up or down"),
        ],
    },
    Group {
        title: "Library",
        modes: &[Library],
        lines: &[
            ("Enter", "", "", "play the highlighted song"),
            ("", "h l", "", "back or forward 5 seconds"),
            ("⌥←", "0", "", "back to the start"),
            ("", "m", "", "marker here, again to clear it"),
            ("", "o", "", "play from the marker, or the loop start"),
            ("⌘← ⌘→", "H L", "", "previous or next song"),
            ("⌘S", "s", "", "shuffle on or off"),
            ("Tab ⌘2", "p", "", "practice mode"),
            ("⌘? ?", "", "", "these keys"),
        ],
    },
    Group {
        title: "Songs",
        modes: &[Library],
        lines: &[
            ("⌘D", "f", "", "favourite or not"),
            ("⌘A", "a", "", "only this album, again to leave"),
            ("⇧⌘A", "A", "", "only this artist, again to leave"),
            ("⌥Enter ⇧⌥Enter", "q Q", "", "queue it, or at the top"),
            ("+ ⌘N", "", "", "add it to a playlist"),
            ("⌘O", "", "", "add a variation to the playing song"),
        ],
    },
    Group {
        title: "Playlist",
        modes: &[Library],
        lines: &[
            ("⌫", "x", "", "remove it from the open playlist"),
            ("⇧↑ ⇧↓", "K J", "", "move it up or down in the playlist"),
        ],
    },
    Group {
        title: "Finding",
        modes: &[Library],
        lines: &[
            ("", "⌘F ⌘P / i", "", "search"),
            ("", "", "⌘F ⌘P", "search, or just start typing"),
            ("↑ ↓ ⌃J ⌃K", "j k", "", "move"),
            ("Home End ⌘↑ ⌘↓", "gg G", "", "top or bottom"),
            ("", "⌃U ⌃D", "", "half a page"),
            ("PgUp PgDn", "", "", "move 10"),
            ("⌘L", "zz", "", "to the playing song"),
            ("Esc", "", "", "clear the search, leave the album or artist"),
            ("⌘⌫", "", "", "clear the search"),
        ],
    },
    Group {
        title: "Searching",
        modes: &[Search],
        lines: &[
            ("Enter", "", "", "play the highlighted song"),
            ("⌥Enter ⇧⌥Enter", "", "", "queue it, or at the top"),
            ("↑ ↓ ⌃J ⌃K", "", "", "move"),
            ("PgUp PgDn", "", "", "move 10"),
            ("Home End ⌘↑ ⌘↓", "", "", "top or bottom"),
            ("⌘L", "", "", "to the playing song"),
            ("Esc ⌘P", "", "", "stop typing, keep the results"),
            ("⌥↑ ⌥↓", "", "", "volume up or down"),
            ("⌘⌫", "", "", "clear the search"),
            ("⌘?", "", "", "these keys"),
        ],
    },
    Group {
        title: "Loop",
        modes: &[Practice],
        lines: &[
            ("a b", "", "", "loop starts or ends here"),
            ("\\ ⌥⌘L", "", "l", "loop on or off, keeping its edges"),
            ("| ⌫", "", "", "clear the loop"),
            ("r", "", "", "save it as a section, or rename its section"),
            ("n N", "", "", "loop the next or previous section"),
            ("", "dd", "", "delete the section in play"),
        ],
    },
    Group {
        title: "Practice",
        modes: &[Practice],
        lines: &[
            ("Esc Tab ⌘1", "p", "", "back to the library"),
            ("", "h l", "", "back or forward 5 seconds, a beat on the grid"),
            ("0 ⌥←", "", "", "back to the start"),
            ("m", "", "", "marker here, again to clear it"),
            ("o", "", "", "play from the marker, or the loop start"),
            ("g", "", "", "beat grid: loops and steps follow the beats"),
            (", .", "", "", "slower or faster by 5%, 25 to 110%, pitch kept"),
            ("", "k j", "↑ ↓", "pitch up or down a semitone, speed kept"),
            ("", "K J", "⇧↑ ⇧↓", "pitch up or down 5 cents"),
            ("s", "", "", "speed and pitch as recorded, again back"),
            ("t", "", "", "speed trainer: the loop speeds up as it repeats"),
            ("T", "", "", "the trainer's settings"),
            ("- =", "", "", "zoom out or in"),
            ("⌘0", "", "", "zoom to fit the loop, or the whole song"),
            ("⌘L", "", "", "follow the playhead again"),
            ("v", "", "", "next recording: the song, its variations"),
            ("V ⌘O", "", "", "add a variation (or drop files on the waveform)"),
            ("X", "", "", "remove the playing variation (twice)"),
            ("+ ⌘N", "", "", "add the playing song to a playlist"),
            ("1 2 3", "", "", "mute stem 1, 2, 3 (up to 9)"),
            ("⌥1 ⌥2", "", "", "solo stem 1, 2 (again to end it)"),
            ("⌘? ?", "", "", "these keys"),
        ],
    },
    Group {
        title: "Songs",
        modes: &[Search, Practice],
        lines: &[
            ("⌘← ⌘→", "", "", "previous or next song"),
            ("⌘S", "", "", "shuffle on or off"),
            ("⌘D", "", "", "favourite or not"),
            ("⌘A", "", "", "only this album"),
            ("⇧⌘A", "", "", "only this artist"),
        ],
    },
    Group {
        title: "Sidebar",
        modes: &[Library, Practice],
        lines: &[("`", "", "", "into the queue"), ("⌘B ⌃⌘S", "", "", "sidebar open or collapsed")],
    },
    Group {
        title: "Queue",
        modes: &[Queue],
        lines: &[
            ("↑ ↓", "j k", "", "move"),
            ("Enter", "", "", "play it now"),
            ("t", "", "", "to the top"),
            ("x ⌫", "dd", "", "remove it"),
            ("Esc Tab `", "", "", "back"),
            ("Space", "", "", "pause or resume"),
            ("⌘B ⌃⌘S", "", "", "sidebar open or collapsed"),
            ("⌘? ?", "", "", "these keys"),
        ],
    },
    Group {
        title: "App",
        modes: &[Library, Search, Practice, Queue],
        lines: &[
            ("⌘,", "", "", "settings"),
            ("⌘E ⌥⌘E", "", "", "the EQ"),
            ("⌘H", "", "", "hide riff"),
            ("⌘M", "", "", "minimise"),
            ("⌃⌘F", "", "", "full screen"),
            ("⌘Q", "", "", "quit"),
        ],
    },
];

#[cfg(test)]
mod tests {
    use super::*;
    use Keyboard::{Mac, Vim};

    const MODES: [Mode; 4] = [Library, Search, Practice, Queue];
    const BOTH: [Keyboard; 2] = [Mac, Vim];

    fn press_key(key: &str) -> Press<'_> {
        Press { key, ..Press::default() }
    }

    #[test]
    fn every_key_on_the_sheet_does_something_in_its_modes() {
        for keyboard in BOTH {
            for group in SHEET {
                for &mode in group.modes {
                    for &line in group.lines {
                        for cap in caps(line, keyboard) {
                            let what = line.3;
                            assert!(
                                resolve(cap, mode, keyboard).is_some(),
                                "{cap} ({what}) does nothing in {mode:?} with {keyboard:?}"
                            );
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn no_key_is_on_a_modes_sheet_twice() {
        for mode in MODES {
            for keyboard in BOTH {
                let mut seen = std::collections::HashSet::new();
                for cap in sheet(mode).flat_map(|g| g.lines.iter()).flat_map(|&l| caps(l, keyboard)) {
                    assert!(seen.insert(cap), "{cap} is listed twice in {mode:?} with {keyboard:?}");
                }
            }
        }
    }

    #[test]
    fn shared_keys_do_the_same_with_either_keyboard() {
        for group in SHEET {
            for &mode in group.modes {
                for (shared, ..) in group.lines {
                    for cap in shared.split_whitespace() {
                        assert_eq!(resolve(cap, mode, Mac), resolve(cap, mode, Vim), "{cap} in {mode:?}");
                    }
                }
            }
        }
    }

    #[test]
    fn tooltips_find_every_key_for_an_action_in_the_mode() {
        assert_eq!(keys_for(Action::Previous, Library, Vim), ["⌘←", "H"]);
        assert_eq!(keys_for(Action::Previous, Library, Mac), ["⌘←"]);
        assert_eq!(keys_for(Action::Previous, Practice, Vim), ["⌘←"]);
        assert_eq!(keys_for(Action::Toggle, Library, Mac), ["Space"]);
        assert_eq!(keys_for(Action::FullScreen, Search, Mac), ["⌃⌘F"]);
        assert_eq!(keys_for(Action::Practice, Library, Mac), ["Tab"]);
        assert_eq!(keys_for(Action::Practice, Library, Vim), ["Tab", "p"]);
        assert_eq!(keys_for(Action::Practice, Practice, Vim), ["Esc", "Tab", "p"]);
        assert_eq!(keys_for(Action::RemoveRegion, Practice, Vim), ["dd"]);
        assert!(keys_for(Action::RemoveRegion, Practice, Mac).is_empty());
        assert_eq!(keys_for(Action::LoopSwitch, Practice, Mac), ["\\", "⌥⌘L", "l"]);
        assert_eq!(keys_for(Action::Search, Library, Mac), ["⌘P"]);
        assert_eq!(keys_for(Action::Find, Library, Vim), ["⌘F"]);
        assert!(keys_for(Action::LoopStart, Library, Vim).is_empty());
        assert!(keys_for(Action::Quit, Practice, Mac).contains(&"⌘Q"));
    }

    #[test]
    fn vim_practice_keys_stay_on_the_base_layer() {
        let base = |c: char| c.is_ascii_alphanumeric() || "`-=\\;',./<>_+:?|".contains(c);
        for group in sheet(Practice) {
            for &line in group.lines {
                for cap in caps(line, Vim).filter(|c| !c.contains(['⌘', '⌥', '⌃', '⇧'])) {
                    let named = ["Esc", "Tab", "Space", "←", "→", "⌫"].contains(&cap);
                    assert!(named || cap.chars().all(base), "{cap} ({}) is off the base layer", line.3);
                }
            }
        }
    }

    #[test]
    fn practice_keys_work_with_either_keyboard() {
        for keyboard in BOTH {
            assert_eq!(resolve("a", Practice, keyboard), Some(Action::LoopStart));
            assert_eq!(resolve("b", Practice, keyboard), Some(Action::LoopEnd));
            assert_eq!(resolve("\\", Practice, keyboard), Some(Action::LoopSwitch));
            assert_eq!(resolve("|", Practice, keyboard), Some(Action::LoopClear));
            assert_eq!(resolve("⌫", Practice, keyboard), Some(Action::LoopClear));
            assert_eq!(resolve("⌥⌘L", Practice, keyboard), Some(Action::LoopSwitch));
            assert_eq!(resolve("g", Practice, keyboard), Some(Action::Grid));
            assert_eq!(resolve("s", Practice, keyboard), Some(Action::Slow));
            assert_eq!(resolve("m", Practice, keyboard), Some(Action::Marker));
            assert_eq!(resolve("0", Practice, keyboard), Some(Action::ToStart));
            assert_eq!(resolve("⌥←", Practice, keyboard), Some(Action::ToStart));
            // Quiet aliases, off the sheet.
            assert_eq!(resolve("[", Practice, keyboard), Some(Action::LoopStart));
            assert_eq!(resolve("]", Practice, keyboard), Some(Action::LoopEnd));
        }
    }

    #[test]
    fn pitch_is_on_the_arrows_with_mac_keys_and_hjkl_with_vim() {
        assert_eq!(resolve("↑", Practice, Mac), Some(Action::Pitch(100)));
        assert_eq!(resolve("⇧↓", Practice, Mac), Some(Action::Pitch(-5)));
        assert_eq!(resolve("k", Practice, Mac), None);
        assert_eq!(resolve("l", Practice, Mac), Some(Action::LoopSwitch));
        assert_eq!(resolve("k", Practice, Vim), Some(Action::Pitch(100)));
        assert_eq!(resolve("J", Practice, Vim), Some(Action::Pitch(-5)));
        assert_eq!(resolve("↑", Practice, Vim), None);
        assert_eq!(resolve("l", Practice, Vim), Some(Action::Seek(SEEK)));
    }

    #[test]
    fn the_same_key_means_what_its_mode_says() {
        assert_eq!(resolve("a", Library, Vim), Some(Action::Album));
        assert_eq!(resolve("a", Practice, Vim), Some(Action::LoopStart));
        assert_eq!(resolve("s", Library, Vim), Some(Action::Shuffle));
        assert_eq!(resolve("s", Practice, Vim), Some(Action::Slow));
        assert_eq!(resolve("Esc", Library, Mac), Some(Action::Clear));
        assert_eq!(resolve("Esc", Search, Mac), Some(Action::Search));
        assert_eq!(resolve("Esc", Practice, Mac), Some(Action::Practice));
        assert_eq!(resolve("⌫", Library, Mac), Some(Action::PlaylistRemove));
        assert_eq!(resolve("⌘L", Library, Mac), Some(Action::ToPlaying));
        assert_eq!(resolve("⌘L", Search, Vim), Some(Action::ToPlaying));
        assert_eq!(resolve("⌘L", Practice, Vim), Some(Action::Follow));
        // Loop keys are practice keys.
        assert_eq!(resolve("[", Library, Vim), None);
        assert_eq!(resolve("b", Library, Vim), None);
        // While searching, bare keys type.
        assert_eq!(resolve("m", Search, Vim), None);
        assert_eq!(resolve("Space", Search, Vim), None);
        assert_eq!(resolve("⌫", Search, Mac), None);
        assert_eq!(resolve("m", Library, Vim), Some(Action::Marker));
        assert_eq!(resolve("m", Library, Mac), None);
        // ⌥↑ is the volume everywhere; plain ↑ moves in the list.
        assert_eq!(resolve("⌥↑", Search, Mac), Some(Action::Volume(true)));
        assert_eq!(resolve("⌥↓", Practice, Mac), Some(Action::Volume(false)));
        assert_eq!(resolve("↑", Library, Mac), Some(Action::Move(-1)));
        assert_eq!(resolve("⌥←", Search, Mac), Some(Action::ToStart));
    }

    #[test]
    fn the_cmd_keys_are_the_same_with_either_keyboard() {
        for keyboard in BOTH {
            for mode in MODES {
                assert_eq!(resolve("⌘F", mode, keyboard), Some(Action::Find), "{mode:?}");
                assert_eq!(resolve("⌘D", mode, keyboard), Some(Action::Favorite), "{mode:?}");
                assert_eq!(resolve("⌘A", mode, keyboard), Some(Action::Album), "{mode:?}");
                assert_eq!(resolve("⇧⌘A", mode, keyboard), Some(Action::Artist), "{mode:?}");
                assert_eq!(resolve("⌘H", mode, keyboard), Some(Action::Hide), "{mode:?}");
                assert_eq!(resolve("⌘P", mode, keyboard), Some(Action::Search), "{mode:?}");
                assert_eq!(resolve("⌘S", mode, keyboard), Some(Action::Shuffle), "{mode:?}");
                assert_eq!(resolve("⌃⌘S", mode, keyboard), Some(Action::Sidebar), "{mode:?}");
                assert_eq!(resolve("⌥⌘E", mode, keyboard), Some(Action::EqSheet), "{mode:?}");
                assert_eq!(resolve("⌘O", mode, keyboard), Some(Action::AddVariation), "{mode:?}");
                assert_eq!(resolve("⌘1", mode, keyboard), Some(Action::SetPractice(false)), "{mode:?}");
                assert_eq!(resolve("⌘2", mode, keyboard), Some(Action::SetPractice(true)), "{mode:?}");
                assert_eq!(resolve("⌘→", mode, keyboard), Some(Action::Next), "{mode:?}");
            }
            assert_eq!(resolve("⌘0", Practice, keyboard), Some(Action::ZoomFit));
            assert_eq!(resolve("⌘N", Library, keyboard), Some(Action::AddToPlaylist));
            assert_eq!(resolve("⌘N", Practice, keyboard), Some(Action::AddToPlaylist));
            for mode in [Library, Search] {
                assert_eq!(resolve("⌥Enter", mode, keyboard), Some(Action::Queue(false)));
                assert_eq!(resolve("⇧⌥Enter", mode, keyboard), Some(Action::Queue(true)));
                assert_eq!(resolve("Enter", mode, keyboard), Some(Action::Play));
                assert_eq!(resolve("Home", mode, keyboard), Some(Action::Top));
                assert_eq!(resolve("⌘↓", mode, keyboard), Some(Action::Bottom));
            }
        }
        // ⌘H and ⌘L no longer go back and on.
        assert_eq!(resolve("⌘L", Library, Vim), Some(Action::ToPlaying));
    }

    #[test]
    fn mac_letters_start_a_search_and_symbols_keep_their_commands() {
        let shifted = Press { key: "j", shift: true, ..Press::default() };
        for key in ["j", "a", "7", "0", "m"] {
            assert!(starts_search(press_key(key), Library, Mac), "{key}");
            assert!(!starts_search(press_key(key), Library, Vim), "{key} with vim");
            assert!(!starts_search(press_key(key), Practice, Mac), "{key} in practice");
            assert_eq!(resolve(key, Library, Mac), None, "{key}");
        }
        assert!(starts_search(shifted, Library, Mac));
        for key in ["space", "+", "`", "?", "enter", "backspace", "left"] {
            assert!(!starts_search(press_key(key), Library, Mac), "{key}");
        }
        assert_eq!(resolve("?", Library, Mac), Some(Action::Help));
        assert_eq!(resolve("+", Library, Mac), Some(Action::AddToPlaylist));
        let cmd_j = Press { key: "j", cmd: true, ..Press::default() };
        assert!(!starts_search(cmd_j, Library, Mac));
    }

    #[test]
    fn cmd_v_pastes_one_line() {
        let paste = Press { key: "v", cmd: true, ..Press::default() };
        assert!(pastes(paste));
        assert!(!pastes(press_key("v")));
        for keyboard in BOTH {
            assert!(starts_search(paste, Library, keyboard));
            // While typing it isn't a key, so it types.
            assert_eq!(press(Search, paste, keyboard, &mut None), None);
        }
        assert_eq!(one_line("Neon Knights\n"), "Neon Knights");
        assert_eq!(one_line("Black Sabbath\r\n  Neon Knights "), "Black Sabbath Neon Knights");
        assert_eq!(one_line("\n\n"), "");
    }

    #[test]
    fn nudge_and_slide_have_no_keys() {
        for keyboard in BOTH {
            for mode in MODES {
                for cap in ["{", "}", "(", ")", "<", ">"] {
                    assert_eq!(resolve(cap, mode, keyboard), None, "{cap} in {mode:?}");
                }
            }
        }
    }

    #[test]
    fn vim_and_cmd_keys_agree() {
        assert_eq!(resolve("H", Library, Vim), resolve("⌘←", Library, Vim));
        assert_eq!(resolve("/", Library, Vim), resolve("⌘P", Library, Vim));
        assert_eq!(resolve("gg", Library, Vim), Some(Action::Top));
        assert_eq!(resolve("G", Library, Vim), Some(Action::Bottom));
        assert_eq!(resolve("A", Library, Vim), resolve("⇧⌘A", Library, Vim));
        assert_eq!(resolve("f", Library, Vim), resolve("⌘D", Library, Vim));
        assert_eq!(resolve("zz", Library, Vim), resolve("⌘L", Library, Vim));
        assert_eq!(resolve("i", Library, Vim), Some(Action::Search));
    }

    #[test]
    fn a_lone_g_waits_and_anything_else_cancels_it() {
        let mut pending = None;
        let (g, j) = (press_key("g"), press_key("j"));
        assert_eq!(press(Library, g, Vim, &mut pending), None);
        assert_eq!(press(Library, j, Vim, &mut pending), Some(Action::Move(1)));
        assert_eq!(press(Library, g, Vim, &mut pending), None);
        assert_eq!(press(Library, g, Vim, &mut pending), Some(Action::Top));
    }

    #[test]
    fn a_lone_d_in_practice_waits_for_the_second() {
        let mut pending = None;
        let (d, g) = (press_key("d"), press_key("g"));
        assert_eq!(press(Practice, d, Vim, &mut pending), None);
        assert_eq!(press(Practice, g, Vim, &mut pending), Some(Action::Grid));
        assert_eq!(press(Practice, d, Vim, &mut pending), None);
        assert_eq!(press(Practice, d, Vim, &mut pending), Some(Action::RemoveRegion));
        assert_eq!(resolve("dd", Practice, Mac), None);
    }

    #[test]
    fn queueing_and_the_queue_has_its_own_keys() {
        assert_eq!(resolve("q", Library, Vim), Some(Action::Queue(false)));
        assert_eq!(resolve("Q", Library, Vim), Some(Action::Queue(true)));
        assert_eq!(resolve("q", Library, Mac), None);
        assert_eq!(resolve("dd", Queue, Vim), Some(Action::QueueRemove));
        assert_eq!(resolve("dd", Queue, Mac), None);
        for keyboard in BOTH {
            assert_eq!(resolve("t", Queue, keyboard), Some(Action::QueueTop));
            assert_eq!(resolve("⌫", Queue, keyboard), Some(Action::QueueRemove));
            assert_eq!(resolve("`", Practice, keyboard), Some(Action::QueueFocus));
            assert_eq!(resolve("`", Queue, keyboard), Some(Action::QueueFocus));
            assert_eq!(resolve("⌘B", Search, keyboard), Some(Action::Sidebar));
            assert_eq!(resolve("q", Practice, keyboard), None);
            assert_eq!(resolve("⌘E", Practice, keyboard), Some(Action::EqSheet));
            assert_eq!(resolve("+", Library, keyboard), Some(Action::AddToPlaylist));
            assert_eq!(resolve("+", Practice, keyboard), Some(Action::AddToPlaylist));
            assert_eq!(resolve("⇧↑", Library, keyboard), Some(Action::PlaylistMove(true)));
            assert_eq!(resolve("↑", Library, keyboard), Some(Action::Move(-1)));
            // Top and bottom are the song list's, not the queue's.
            assert_eq!(resolve("Home", Queue, keyboard), None);
        }
        assert_eq!(resolve("J", Library, Vim), Some(Action::PlaylistMove(false)));
    }

    #[test]
    fn the_sheet_finds_lines_by_what_they_do_their_group_or_their_keys() {
        let line: Line = ("⌥Enter", "q Q", "", "queue it, or at the top");
        assert!(found("Songs", line, Mac, "queue"));
        assert!(found("Songs", line, Mac, "QUEUE top"));
        assert!(found("Songs", line, Mac, "songs"));
        assert!(found("Songs", line, Vim, "q"));
        assert!(found("Songs", line, Mac, "enter"));
        assert!(found("Songs", line, Mac, ""));
        assert!(!found("Songs", line, Mac, "queue loop"));
        // Only the keys shown are found.
        let vim_only: Line = ("", "zz", "", "to the playing song");
        assert!(!found("Finding", vim_only, Mac, "zz"));
    }
}
