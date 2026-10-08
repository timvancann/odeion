//! What each key does, and the ⌘? sheet that lists them, side by side so they can't
//! disagree. Pure: `view.rs` turns a keystroke into a `Press` and performs the `Action`.
//!
//! Riff has modes, like vim. The library is for finding and playing songs, search is
//! its sub-mode where keys type, and practice is the waveform with the loop, sections
//! and stems. A key means what the current mode's map says, after the few keys that
//! work everywhere. Vim keys (letters, `/`, `?`) in the library can be turned off in
//! settings; practice keys are the mode's own and always work.

/// The loop bar's widen and narrow buttons move an edge this far.
pub const NUDGE: f64 = 0.05;
/// Each zoom step shows this many times more or less.
pub const ZOOM: f64 = 1.5;
/// ⌃D and ⌃U move this many rows.
pub const HALF_PAGE: isize = 6;
const SEEK: f64 = 5.0;

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
    Transcribe,
    Help,
    Settings,
    Quit,
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
    /// Zoom the practice view: below 1 closer in, above 1 further out.
    Zoom(f64),
    /// The beat grid on or off: loop edges and steps follow the beats.
    Grid,
    /// Save the loop as a named region (or rename the region it is).
    SaveRegion,
    /// The next (true) or previous region, as the loop.
    Region(bool),
    /// The section in play as the loop; from a section's menu, that section. No key.
    LoopRegion,
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
pub fn press(mode: Mode, p: Press, vim: bool, pending: &mut Option<char>) -> Option<Action> {
    let first = pending.take();
    if let Some(action) = anywhere(p) {
        return Some(action);
    }
    match mode {
        Mode::Search => search(p),
        Mode::Library => outside(p).or_else(|| library(p, vim, first, pending)),
        Mode::Practice => outside(p).or_else(|| practice(p, first, pending)),
        Mode::Queue => outside(p).or_else(|| queue(p, first, pending)),
    }
}

/// Keys that work in every mode, even while typing a search: ⌘ shortcuts, ← → and the
/// volume.
pub fn anywhere(p: Press) -> Option<Action> {
    use Action::*;
    Some(match (p.key, p.cmd, p.ctrl) {
        ("p", true, _) => Search,
        ("m", true, false) => Minimize,
        ("f", true, true) => FullScreen,
        ("f", true, _) => Favorite,
        ("a", true, _) if p.shift => Artist,
        ("a", true, _) => Album,
        ("t", true, _) => Transcribe,
        ("s", true, _) => Shuffle,
        ("left" | "h", true, _) => Previous,
        ("right" | "l", true, _) => Next,
        ("up", false, false) if p.alt => Volume(true),
        ("down", false, false) if p.alt => Volume(false),
        ("left", false, false) => Seek(-SEEK),
        ("right", false, false) => Seek(SEEK),
        ("?", true, _) => Help,
        ("/", true, _) if p.shift => Help,
        (",", true, _) => Settings,
        ("q", true, _) => Quit,
        ("b", true, false) if !p.shift => Sidebar,
        _ => return None,
    })
}

/// Bare keys that work in the library and in practice, though not while typing.
fn outside(p: Press) -> Option<Action> {
    if p.cmd || p.ctrl || p.alt {
        return None;
    }
    Some(match (p.key, p.shift) {
        ("space", _) => Action::Toggle,
        ("0", false) => Action::ToStart,
        ("m", false) => Action::Marker,
        ("o", false) => Action::FromLoopStart,
        ("`", false) => Action::QueueFocus,
        _ => return None,
    })
}

/// Moving through the list, in the library and while searching.
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

fn search(p: Press) -> Option<Action> {
    if let Some(action) = moving(p) {
        return Some(action);
    }
    Some(match (p.key, p.cmd) {
        ("enter", false) => Action::Play,
        // Stops typing, keeping the results; Esc again in the library clears them.
        ("escape", _) => Action::Search,
        ("backspace", true) => Action::ClearSearch,
        _ => return None,
    })
}

fn library(p: Press, vim: bool, first: Option<char>, pending: &mut Option<char>) -> Option<Action> {
    use Action::*;
    if let Some(action) = moving(p) {
        return Some(action);
    }
    if p.cmd {
        return (p.key == "backspace").then_some(ClearSearch);
    }
    let always = match (p.key, p.shift, p.ctrl) {
        ("enter", false, false) => Some(Play),
        ("escape", ..) => Some(Clear),
        ("tab", false, false) | ("p", false, false) => Some(Practice),
        ("q", shift, false) => Some(Queue(shift)),
        _ => None,
    };
    if always.is_some() || !vim {
        return always;
    }
    Some(match (p.key, p.shift, p.ctrl) {
        ("g", false, false) if first == Some('g') => Top,
        ("z", false, false) if first == Some('z') => ToPlaying,
        ("g" | "z", false, false) => {
            *pending = p.key.chars().next();
            return None;
        }
        ("g", true, _) => Bottom,
        ("d", _, true) => Move(HALF_PAGE),
        ("u", _, true) => Move(-HALF_PAGE),
        ("j", false, false) => Move(1),
        ("k", false, false) => Move(-1),
        ("h", false, false) => Seek(-SEEK),
        ("l", false, false) => Seek(SEEK),
        ("h", true, _) => Previous,
        ("l", true, _) => Next,
        ("/", ..) => Search,
        ("?", ..) => Help,
        ("f", false, false) => Favorite,
        ("a", false, false) => Album,
        ("a", true, _) => Artist,
        ("i", false, false) => Search,
        ("t", false, false) => Transcribe,
        ("s", false, false) => Shuffle,
        _ => return None,
    })
}

/// In the sidebar's queue. Its keys are its own, with vim keys on or off.
fn queue(p: Press, first: Option<char>, pending: &mut Option<char>) -> Option<Action> {
    use Action::*;
    if let Some(action) = moving(p) {
        return Some(action);
    }
    if p.cmd || p.ctrl || p.alt {
        return None;
    }
    Some(match (p.key, p.shift) {
        ("j", false) => Move(1),
        ("k", false) => Move(-1),
        ("enter", _) => QueuePlay,
        ("t", false) => QueueTop,
        ("x", false) | ("backspace", _) => QueueRemove,
        ("d", false) if first == Some('d') => QueueRemove,
        ("d", false) => {
            *pending = Some('d');
            return None;
        }
        ("escape" | "tab", _) => QueueFocus,
        ("?", _) => Help,
        _ => return None,
    })
}

/// Practice keys sit on the Glove80's base layer (shift counts as base). Brackets are
/// kept as quiet aliases for the loop edges, off the sheet.
fn practice(p: Press, first: Option<char>, pending: &mut Option<char>) -> Option<Action> {
    use Action::*;
    if p.cmd {
        return match p.key {
            "=" | "+" => Some(Zoom(1.0 / ZOOM)),
            "-" => Some(Zoom(ZOOM)),
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
    Some(match (p.key, p.shift) {
        ("escape" | "tab", _) | ("p", false) => Practice,
        ("a", false) | ("[", _) => LoopStart,
        ("b", false) | ("]", _) => LoopEnd,
        ("\\", _) => LoopSwitch,
        ("|", _) => LoopClear,
        ("r", false) => SaveRegion,
        ("n", false) => Region(true),
        ("n", true) => Region(false),
        ("d", false) if first == Some('d') => RemoveRegion,
        ("d", false) => {
            *pending = Some('d');
            return None;
        }
        ("h", false) => Seek(-SEEK),
        ("l", false) => Seek(SEEK),
        ("g", false) => Grid,
        ("s", false) => Slow,
        ("t", false) => Trainer,
        ("k", false) => Pitch(100),
        ("j", false) => Pitch(-100),
        ("k", true) => Pitch(5),
        ("j", true) => Pitch(-5),
        ("t", true) => TrainerSettings,
        (",", _) => Speed(false),
        (".", _) => Speed(true),
        ("-", _) => Zoom(ZOOM),
        ("=", _) => Zoom(1.0 / ZOOM),
        ("v", false) => Variation,
        ("v", true) => AddVariation,
        ("x", true) => RemoveVariation,
        ("?", _) => Help,
        _ => return None,
    })
}

/// What a keycap on the sheet does in `mode` ("⌘←", "G", "⌃⌘F", "gg"), read through the
/// same keymap as a real key press, so the sheet and tooltips can't drift from it.
pub fn resolve(cap: &str, mode: Mode, vim: bool) -> Option<Action> {
    // Two of the same letter, like `gg` or `dd`: pressed twice.
    let mut letters = cap.chars();
    if let (Some(a), Some(b), None) = (letters.next(), letters.next(), letters.next())
        && a == b
        && a.is_ascii_lowercase()
    {
        let mut pending = None;
        let press = Press { key: &cap[..1], ..Press::default() };
        self::press(mode, press, vim, &mut pending);
        return self::press(mode, press, vim, &mut pending);
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
        other => other,
    };
    let upper = named.len() == 1 && named.chars().all(|c| c.is_ascii_uppercase());
    let key = if upper || ctrl || cmd { named.to_lowercase() } else { named.to_string() };
    let press = Press { key: &key, cmd, ctrl, shift: shift || (upper && !ctrl && !cmd), alt };
    self::press(mode, press, vim, &mut None)
}

/// The keys for an action in `mode`, as the sheet writes them: for tooltips and menus.
/// Vim keys only while vim keys are on.
pub fn keys_for(action: Action, mode: Mode, vim: bool) -> Vec<&'static str> {
    sheet(mode)
        .flat_map(|g| g.lines.iter())
        .flat_map(|(keys, vim_keys, _)| {
            keys.split_whitespace().chain(vim_keys.split_whitespace().filter(move |_| vim))
        })
        .filter(|cap| resolve(cap, mode, vim) == Some(action))
        .collect()
}

/// One line of the ⌘? sheet: the keys that always work, the vim keys, and what they do.
pub type Line = (&'static str, &'static str, &'static str);

/// A group of lines on the ⌘? sheet, and the modes whose sheet shows it.
pub struct Group {
    pub title: &'static str,
    pub modes: &'static [Mode],
    pub lines: &'static [Line],
}

/// Whether a sheet line is found by `query`, typed on the ⌘? sheet: every word of it
/// somewhere in what the line does, its group's title or its keys, case aside.
pub fn found(title: &str, (keys, vim_keys, what): &Line, query: &str) -> bool {
    let hay = format!("{title} {keys} {vim_keys} {what}").to_lowercase();
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
            ("Space", "", "pause or resume"),
            ("← →", "", "back or forward 5 seconds, a beat on the grid"),
            ("0", "", "back to the start"),
            ("m", "", "marker here, again to clear it"),
            ("o", "", "play from the marker, or the loop start"),
            ("⌥↑ ⌥↓", "", "volume up or down"),
        ],
    },
    Group {
        title: "Library",
        modes: &[Library],
        lines: &[
            ("Enter", "", "play the highlighted song"),
            ("", "h l", "back or forward 5 seconds"),
            ("⌘← ⌘→", "H L", "previous or next song"),
            ("⌘S", "s", "shuffle on or off"),
            ("p Tab", "", "practice mode"),
            ("⌘?", "?", "these keys"),
        ],
    },
    Group {
        title: "Songs",
        modes: &[Library],
        lines: &[
            ("⌘F", "f", "favourite or not"),
            ("⌘A", "a", "only this album, again to leave"),
            ("⇧⌘A", "A", "only this artist, again to leave"),
            ("⌘T", "t", "highlighted song to Transcribe!"),
            ("q Q", "", "queue it, Q at the top"),
        ],
    },
    Group {
        title: "Finding",
        modes: &[Library],
        lines: &[
            ("⌘P", "/ i", "search"),
            ("↑ ↓ ⌃J ⌃K", "j k", "move"),
            ("", "gg G", "top or bottom"),
            ("", "⌃U ⌃D", "half a page"),
            ("PgUp PgDn", "", "move 10"),
            ("", "zz", "to the playing song"),
            ("Esc", "", "clear the search, leave the album or artist"),
            ("⌘⌫", "", "clear the search"),
        ],
    },
    Group {
        title: "Searching",
        modes: &[Search],
        lines: &[
            ("Enter", "", "play the highlighted song"),
            ("↑ ↓ ⌃J ⌃K", "", "move"),
            ("PgUp PgDn", "", "move 10"),
            ("Esc ⌘P", "", "stop typing, keep the results"),
            ("⌥↑ ⌥↓", "", "volume up or down"),
            ("⌘⌫", "", "clear the search"),
            ("⌘?", "", "these keys"),
        ],
    },
    Group {
        title: "Loop",
        modes: &[Practice],
        lines: &[
            ("a b", "", "loop starts or ends here"),
            ("\\", "", "loop on or off, keeping its edges"),
            ("|", "", "clear the loop"),
            ("r", "", "save it as a section, or rename its section"),
            ("n N", "", "loop the next or previous section"),
            ("dd", "", "delete the section in play"),
        ],
    },
    Group {
        title: "Practice",
        modes: &[Practice],
        lines: &[
            ("Esc p Tab", "", "back to the library"),
            ("h l", "", "back or forward 5 seconds, a beat on the grid"),
            ("g", "", "beat grid: loops and steps follow the beats"),
            (", .", "", "slower or faster by 5%, 25 to 110%, pitch kept"),
            ("k j", "", "pitch up or down a semitone, speed kept"),
            ("K J", "", "pitch up or down 5 cents"),
            ("s", "", "speed and pitch as recorded, again back"),
            ("t", "", "speed trainer: the loop speeds up as it repeats"),
            ("T", "", "the trainer's settings"),
            ("- =", "", "zoom out or in"),
            ("v", "", "next recording: the song, its variations"),
            ("V", "", "add a variation (or drop files on the waveform)"),
            ("X", "", "remove the playing variation"),
            ("1 2 3", "", "mute stem 1, 2, 3 (up to 9)"),
            ("⌥1 ⌥2", "", "solo stem 1, 2 (again to end it)"),
            ("⌘? ?", "", "these keys"),
        ],
    },
    Group {
        title: "Songs",
        modes: &[Search, Practice],
        lines: &[
            ("⌘← ⌘→", "", "previous or next song"),
            ("⌘S", "", "shuffle on or off"),
            ("⌘F", "", "favourite or not"),
            ("⌘A", "", "only this album"),
            ("⇧⌘A", "", "only this artist"),
            ("⌘T", "", "highlighted song to Transcribe!"),
        ],
    },
    Group {
        title: "Sidebar",
        modes: &[Library, Practice],
        lines: &[("`", "", "into the queue"), ("⌘B", "", "sidebar open or collapsed")],
    },
    Group {
        title: "Queue",
        modes: &[Queue],
        lines: &[
            ("j k ↑ ↓", "", "move"),
            ("Enter", "", "play it now"),
            ("t", "", "to the top"),
            ("dd x ⌫", "", "remove it"),
            ("Esc Tab `", "", "back"),
            ("Space", "", "pause or resume"),
            ("⌘B", "", "sidebar open or collapsed"),
            ("⌘? ?", "", "these keys"),
        ],
    },
    Group {
        title: "App",
        modes: &[Library, Search, Practice, Queue],
        lines: &[
            ("⌘,", "", "settings"),
            ("⌘M", "", "minimise"),
            ("⌃⌘F", "", "full screen"),
            ("⌘Q", "", "quit"),
        ],
    },
];

#[cfg(test)]
mod tests {
    use super::*;

    const MODES: [Mode; 4] = [Library, Search, Practice, Queue];

    fn caps(mode: Mode, vim: bool) -> Vec<&'static str> {
        sheet(mode)
            .flat_map(|g| g.lines.iter())
            .flat_map(|(keys, vim_keys, _)| {
                keys.split_whitespace().chain(vim_keys.split_whitespace().filter(move |_| vim))
            })
            .collect()
    }

    #[test]
    fn every_key_on_the_sheet_does_something_in_its_modes() {
        for group in SHEET {
            for &mode in group.modes {
                for (keys, vim_keys, what) in group.lines {
                    for cap in keys.split_whitespace() {
                        assert!(resolve(cap, mode, false).is_some(), "{cap} ({what}) does nothing in {mode:?}");
                        assert!(resolve(cap, mode, true).is_some(), "{cap} ({what}) does nothing in {mode:?} with vim");
                    }
                    for cap in vim_keys.split_whitespace() {
                        assert!(resolve(cap, mode, true).is_some(), "{cap} ({what}) does nothing in {mode:?}");
                    }
                }
            }
        }
    }

    #[test]
    fn no_key_is_on_a_modes_sheet_twice() {
        for mode in MODES {
            for vim in [false, true] {
                let mut seen = std::collections::HashSet::new();
                for cap in caps(mode, vim) {
                    assert!(seen.insert(cap), "{cap} is listed twice in {mode:?}");
                }
            }
        }
    }

    #[test]
    fn tooltips_find_every_key_for_an_action_in_the_mode() {
        assert_eq!(keys_for(Action::Previous, Library, true), ["⌘←", "H"]);
        assert_eq!(keys_for(Action::Previous, Library, false), ["⌘←"]);
        assert_eq!(keys_for(Action::Previous, Practice, true), ["⌘←"]);
        assert_eq!(keys_for(Action::Toggle, Library, false), ["Space"]);
        assert_eq!(keys_for(Action::FullScreen, Search, false), ["⌃⌘F"]);
        assert_eq!(keys_for(Action::Practice, Library, false), ["p", "Tab"]);
        assert_eq!(keys_for(Action::Practice, Practice, false), ["Esc", "p", "Tab"]);
        assert_eq!(keys_for(Action::RemoveRegion, Practice, false), ["dd"]);
        assert!(keys_for(Action::LoopStart, Library, true).is_empty());
        assert!(keys_for(Action::Quit, Practice, false).contains(&"⌘Q"));
    }

    #[test]
    fn practice_keys_stay_on_the_base_layer() {
        let base = |c: char| c.is_ascii_alphanumeric() || "`-=\\;',./<>_+:?|".contains(c);
        for group in sheet(Practice) {
            for (keys, _, what) in group.lines {
                for cap in keys.split_whitespace().filter(|c| !c.contains(['⌘', '⌥', '⌃', '⇧'])) {
                    let named = ["Esc", "Tab", "Space", "←", "→"].contains(&cap);
                    assert!(named || cap.chars().all(base), "{cap} ({what}) is off the base layer");
                }
            }
        }
    }

    #[test]
    fn practice_keys_work_with_vim_off() {
        assert_eq!(resolve("a", Practice, false), Some(Action::LoopStart));
        assert_eq!(resolve("b", Practice, false), Some(Action::LoopEnd));
        assert_eq!(resolve("\\", Practice, false), Some(Action::LoopSwitch));
        assert_eq!(resolve("|", Practice, false), Some(Action::LoopClear));
        assert_eq!(resolve("g", Practice, false), Some(Action::Grid));
        assert_eq!(resolve("s", Practice, false), Some(Action::Slow));
        assert_eq!(resolve("dd", Practice, false), Some(Action::RemoveRegion));
        // Quiet aliases, off the sheet.
        assert_eq!(resolve("[", Practice, false), Some(Action::LoopStart));
        assert_eq!(resolve("]", Practice, false), Some(Action::LoopEnd));
    }

    #[test]
    fn the_same_key_means_what_its_mode_says() {
        assert_eq!(resolve("a", Library, true), Some(Action::Album));
        assert_eq!(resolve("a", Practice, true), Some(Action::LoopStart));
        assert_eq!(resolve("s", Library, true), Some(Action::Shuffle));
        assert_eq!(resolve("s", Practice, true), Some(Action::Slow));
        assert_eq!(resolve("Esc", Library, true), Some(Action::Clear));
        assert_eq!(resolve("Esc", Search, true), Some(Action::Search));
        assert_eq!(resolve("Esc", Practice, true), Some(Action::Practice));
        // Loop keys are practice keys.
        assert_eq!(resolve("[", Library, true), None);
        assert_eq!(resolve("b", Library, true), None);
        // While searching, bare keys type.
        assert_eq!(resolve("m", Search, true), None);
        assert_eq!(resolve("Space", Search, true), None);
        assert_eq!(resolve("m", Library, false), Some(Action::Marker));
        assert_eq!(resolve("0", Practice, false), Some(Action::ToStart));
        // ⌥↑ is the volume everywhere; plain ↑ moves in the list.
        assert_eq!(resolve("⌥↑", Search, false), Some(Action::Volume(true)));
        assert_eq!(resolve("⌥↓", Practice, false), Some(Action::Volume(false)));
        assert_eq!(resolve("↑", Library, false), Some(Action::Move(-1)));
    }

    #[test]
    fn nudge_and_slide_have_no_keys() {
        for mode in MODES {
            for cap in ["{", "}", "(", ")", "<", ">"] {
                assert_eq!(resolve(cap, mode, true), None, "{cap} in {mode:?}");
            }
        }
    }

    #[test]
    fn vim_and_cmd_keys_agree() {
        assert_eq!(resolve("H", Library, true), resolve("⌘←", Library, true));
        assert_eq!(resolve("/", Library, true), resolve("⌘P", Library, true));
        assert_eq!(resolve("gg", Library, true), Some(Action::Top));
        assert_eq!(resolve("G", Library, true), Some(Action::Bottom));
        assert_eq!(resolve("A", Library, true), resolve("⇧⌘A", Library, true));
        assert_eq!(resolve("i", Library, true), Some(Action::Search));
    }

    #[test]
    fn without_vim_library_letters_do_nothing() {
        assert_eq!(resolve("j", Library, false), None);
        assert_eq!(resolve("gg", Library, false), None);
        assert_eq!(resolve("⌘F", Library, false), Some(Action::Favorite));
        assert_eq!(resolve("p", Library, false), Some(Action::Practice));
    }

    #[test]
    fn a_lone_g_waits_and_anything_else_cancels_it() {
        let mut pending = None;
        let g = Press { key: "g", ..Press::default() };
        let j = Press { key: "j", ..Press::default() };
        assert_eq!(press(Library, g, true, &mut pending), None);
        assert_eq!(press(Library, j, true, &mut pending), Some(Action::Move(1)));
        assert_eq!(press(Library, g, true, &mut pending), None);
        assert_eq!(press(Library, g, true, &mut pending), Some(Action::Top));
    }

    #[test]
    fn a_lone_d_in_practice_waits_for_the_second() {
        let mut pending = None;
        let d = Press { key: "d", ..Press::default() };
        let g = Press { key: "g", ..Press::default() };
        assert_eq!(press(Practice, d, false, &mut pending), None);
        assert_eq!(press(Practice, g, false, &mut pending), Some(Action::Grid));
        assert_eq!(press(Practice, d, false, &mut pending), None);
        assert_eq!(press(Practice, d, false, &mut pending), Some(Action::RemoveRegion));
    }

    #[test]
    fn q_queues_with_or_without_vim_and_the_queue_has_its_own_keys() {
        for vim in [false, true] {
            assert_eq!(resolve("q", Library, vim), Some(Action::Queue(false)));
            assert_eq!(resolve("Q", Library, vim), Some(Action::Queue(true)));
            assert_eq!(resolve("t", Queue, vim), Some(Action::QueueTop));
            assert_eq!(resolve("dd", Queue, vim), Some(Action::QueueRemove));
        }
        assert_eq!(resolve("`", Practice, false), Some(Action::QueueFocus));
        assert_eq!(resolve("`", Queue, false), Some(Action::QueueFocus));
        assert_eq!(resolve("⌘B", Search, false), Some(Action::Sidebar));
        assert_eq!(resolve("q", Practice, false), None);
    }

    #[test]
    fn the_sheet_finds_lines_by_what_they_do_their_group_or_their_keys() {
        let line: Line = ("q Q", "", "queue it, Q at the top");
        assert!(found("Songs", &line, "queue"));
        assert!(found("Songs", &line, "QUEUE top"));
        assert!(found("Songs", &line, "songs"));
        assert!(found("Songs", &line, "q"));
        assert!(found("Songs", &line, ""));
        assert!(!found("Songs", &line, "queue loop"));
    }
}
