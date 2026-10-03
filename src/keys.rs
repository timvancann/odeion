//! What each key does, and the ⌘? sheet that lists them, side by side so they can't
//! disagree. Pure: `view.rs` turns a keystroke into a `Press` and performs the `Action`.
//!
//! Outside search, bare keys are commands. Vim keys (letters, `/`, `^`, `?`) can be
//! turned off in settings; the ⌘ shortcuts, arrows and loop keys always work.

/// Nudging a loop edge moves it this far.
pub const NUDGE: f64 = 0.05;
/// Each zoom step shows this many times more or less.
pub const ZOOM: f64 = 1.5;
/// ⌃D and ⌃U move this many rows.
pub const HALF_PAGE: isize = 6;
const SEEK: f64 = 5.0;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Action {
    Move(isize),
    Top,
    Bottom,
    /// Highlight the playing song.
    ToPlaying,
    Search,
    /// Esc outside search: clear the search and leave the album.
    Clear,
    ClearSearch,
    Play,
    Toggle,
    Seek(f64),
    ToStart,
    FromLoopStart,
    Previous,
    Next,
    Shuffle,
    Favorite,
    Album,
    Transcribe,
    Help,
    Settings,
    Quit,
    LoopStart,
    LoopEnd,
    LoopClear,
    /// Looping on or off, keeping the edges.
    LoopSwitch,
    NudgeStart(f64),
    NudgeEnd(f64),
    Slide(bool),
    Resize(f64),
    /// The practice view with the waveform, on or off.
    Practice,
    /// Zoom the practice view: below 1 closer in, above 1 further out.
    Zoom(f64),
    /// The beat grid on or off: loop edges and steps follow the beats.
    Grid,
    /// Save the loop as a named region (or rename the region it is).
    SaveRegion,
    /// The next (true) or previous region, as the loop.
    Region(bool),
    /// Delete the region the loop is.
    RemoveRegion,
}

/// A key as GPUI reports it: `key` is the character or a name like "left", with shift
/// already applied to symbols ("{", "?") but not to letters ("g" plus `shift`).
#[derive(Debug, Clone, Copy, Default)]
pub struct Press<'a> {
    pub key: &'a str,
    pub cmd: bool,
    pub ctrl: bool,
    pub shift: bool,
}

/// Keys that work everywhere, searching or not: ⌘ and ⌃ shortcuts, arrows, paging.
pub fn anywhere(p: Press) -> Option<Action> {
    use Action::*;
    Some(match (p.key, p.cmd, p.ctrl) {
        ("j", false, true) | ("down", false, false) => Move(1),
        ("k", false, true) | ("up", false, false) => Move(-1),
        ("pagedown", ..) => Move(10),
        ("pageup", ..) => Move(-10),
        ("p", true, _) => Search,
        ("f", true, _) => Favorite,
        ("a", true, _) => Album,
        ("t", true, _) => Transcribe,
        ("s", true, _) => Shuffle,
        ("left" | "h", true, _) => Previous,
        ("right" | "l", true, _) => Next,
        ("left", false, false) => Seek(-SEEK),
        ("right", false, false) => Seek(SEEK),
        ("?", true, _) => Help,
        ("/", true, _) if p.shift => Help,
        (",", true, _) => Settings,
        ("=" | "+", true, _) => Zoom(1.0 / ZOOM),
        ("-", true, _) => Zoom(ZOOM),
        ("q", true, _) => Quit,
        ("backspace", true, _) => ClearSearch,
        _ => return None,
    })
}

/// Keys outside search. `pending` holds the first key of `gg` and `zz`.
pub fn normal(p: Press, vim: bool, pending: &mut Option<char>) -> Option<Action> {
    use Action::*;
    let first = pending.take();
    if let Some(action) = anywhere(p) {
        return Some(action);
    }
    if p.cmd {
        return None;
    }
    if p.key == "p" && p.shift && !p.ctrl {
        return Some(Practice);
    }
    if !p.ctrl {
        match (p.key, p.shift) {
            ("b", false) => return Some(Grid),
            ("r", false) => return Some(SaveRegion),
            ("n", false) => return Some(Region(true)),
            ("n", true) => return Some(Region(false)),
            ("x", false) => return Some(RemoveRegion),
            _ => {}
        }
    }
    let always = match p.key {
        "enter" => Some(Play),
        "escape" => Some(Clear),
        "space" => Some(Toggle),
        "[" => Some(LoopStart),
        "]" => Some(LoopEnd),
        "\\" => Some(LoopClear),
        "|" => Some(LoopSwitch),
        // Braces widen the loop, parentheses narrow it, each at the edge on its side.
        "{" => Some(NudgeStart(-NUDGE)),
        "}" => Some(NudgeEnd(NUDGE)),
        "(" => Some(NudgeStart(NUDGE)),
        ")" => Some(NudgeEnd(-NUDGE)),
        "<" => Some(Slide(false)),
        ">" => Some(Slide(true)),
        "_" => Some(Resize(0.5)),
        "+" => Some(Resize(2.0)),
        "-" => Some(Zoom(ZOOM)),
        "=" => Some(Zoom(1.0 / ZOOM)),
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
        ("^", ..) => ToStart,
        ("o", false, false) => FromLoopStart,
        ("/", ..) => Search,
        ("?", ..) => Help,
        ("f" | "m", false, false) => Favorite,
        ("a", false, false) => Album,
        ("t", false, false) => Transcribe,
        ("s", false, false) => Shuffle,
        _ => return None,
    })
}

/// One line of the ⌘? sheet: the keys that always work, the vim keys, and what they do.
pub type Line = (&'static str, &'static str, &'static str);

/// Every shortcut, as the ⌘? sheet lists them, one keycap per space-separated key.
pub const SHEET: &[(&str, &[Line])] = &[
    (
        "Playing",
        &[
            ("Enter", "", "play the highlighted song"),
            ("Space", "", "pause or resume"),
            ("← →", "h l", "back or forward 5 seconds"),
            ("", "^", "back to the start"),
            ("", "o", "play from the loop start"),
            ("⌘← ⌘→", "H L", "previous or next song"),
            ("⌘S", "s", "shuffle on or off"),
        ],
    ),
    (
        "Loop",
        &[
            ("[ ]", "", "loop starts or ends here"),
            ("\\", "", "clear the loop"),
            ("|", "", "loop on or off, keeping its edges"),
            ("{ }", "", "widen at the start or end"),
            ("( )", "", "narrow at the start or end"),
            ("< >", "", "slide back or on by its length"),
            ("_ +", "", "halve or double it"),
            ("r", "", "save it as a region, named"),
            ("n N", "", "next or previous region"),
            ("x", "", "delete its region"),
        ],
    ),
    (
        "Practice",
        &[
            ("P", "", "the waveform, on or off"),
            ("- =", "", "zoom out or in"),
            ("b", "", "beat grid: loops and steps follow the beats"),
        ],
    ),
    (
        "Songs",
        &[
            ("⌘F", "f m", "favourite or not"),
            ("⌘A", "a", "only this album, again to leave"),
            ("⌘T", "t", "highlighted song to Transcribe!"),
        ],
    ),
    (
        "Finding",
        &[
            ("⌘P", "/", "search on or off"),
            ("↑ ↓ ⌃J ⌃K", "j k", "move"),
            ("", "gg G", "top or bottom"),
            ("", "⌃U ⌃D", "half a page"),
            ("PgUp PgDn", "", "move 10"),
            ("", "zz", "to the playing song"),
            ("Esc", "", "stop searching, again to clear"),
            ("⌘⌫", "", "clear the search"),
        ],
    ),
    ("App", &[("⌘,", "", "settings"), ("⌘?", "?", "these keys"), ("⌘Q", "", "quit")]),
];

#[cfg(test)]
mod tests {
    use super::*;

    /// Reads a keycap as the sheet writes it ("⌘←", "G", "⌃D", "gg") into presses.
    fn presses(cap: &str) -> Vec<Press<'static>> {
        let cap: &'static str = Box::leak(cap.to_string().into_boxed_str());
        if cap == "gg" || cap == "zz" {
            let key = &cap[..1];
            return vec![Press { key, ..Press::default() }; 2];
        }
        let (cmd, rest) = match cap.strip_prefix('⌘') {
            Some(rest) => (true, rest),
            None => (false, cap),
        };
        let (ctrl, rest) = match rest.strip_prefix('⌃') {
            Some(rest) => (true, rest),
            None => (false, rest),
        };
        let named = match rest {
            "←" => "left",
            "→" => "right",
            "↑" => "up",
            "↓" => "down",
            "⌫" => "backspace",
            "Enter" => "enter",
            "Space" => "space",
            "Esc" => "escape",
            "PgUp" => "pageup",
            "PgDn" => "pagedown",
            other => other,
        };
        let upper = named.len() == 1 && named.chars().all(|c| c.is_ascii_uppercase());
        let key: &'static str = if upper || ctrl || cmd {
            Box::leak(named.to_lowercase().into_boxed_str())
        } else {
            named
        };
        vec![Press { key, cmd, ctrl, shift: upper && !ctrl && !cmd }]
    }

    fn resolve(cap: &str, vim: bool) -> Option<Action> {
        let mut pending = None;
        presses(cap).into_iter().fold(None, |_, p| normal(p, vim, &mut pending))
    }

    #[test]
    fn every_key_on_the_sheet_does_something() {
        for (_, lines) in SHEET {
            for (keys, vim_keys, what) in *lines {
                for cap in keys.split_whitespace() {
                    assert!(resolve(cap, false).is_some(), "{cap} ({what}) does nothing");
                }
                for cap in vim_keys.split_whitespace() {
                    assert!(resolve(cap, true).is_some(), "{cap} ({what}) does nothing in vim");
                }
            }
        }
    }

    #[test]
    fn vim_and_cmd_keys_agree() {
        assert_eq!(resolve("H", true), resolve("⌘←", true));
        assert_eq!(resolve("f", true), resolve("m", true));
        assert_eq!(resolve("/", true), resolve("⌘P", true));
        assert_eq!(resolve("gg", true), Some(Action::Top));
        assert_eq!(resolve("G", true), Some(Action::Bottom));
    }

    #[test]
    fn without_vim_letters_do_nothing_but_loop_keys_still_work() {
        assert_eq!(resolve("j", false), None);
        assert_eq!(resolve("gg", false), None);
        assert_eq!(resolve("{", false), Some(Action::NudgeStart(-NUDGE)));
        assert_eq!(resolve("}", false), Some(Action::NudgeEnd(NUDGE)));
        assert_eq!(resolve("(", false), Some(Action::NudgeStart(NUDGE)));
        assert_eq!(resolve(")", false), Some(Action::NudgeEnd(-NUDGE)));
        assert_eq!(resolve("⌘F", false), Some(Action::Favorite));
    }

    #[test]
    fn a_lone_g_waits_and_anything_else_cancels_it() {
        let mut pending = None;
        let g = Press { key: "g", ..Press::default() };
        let j = Press { key: "j", ..Press::default() };
        assert_eq!(normal(g, true, &mut pending), None);
        assert_eq!(normal(j, true, &mut pending), Some(Action::Move(1)));
        assert_eq!(normal(g, true, &mut pending), None);
        assert_eq!(normal(g, true, &mut pending), Some(Action::Top));
    }
}
