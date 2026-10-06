//! Right-click menus: a few actions at the pointer, each showing its keys. Pure: which
//! entries each menu has and moving through them by key. `view.rs` opens one where the
//! right-click was and performs the chosen action on what was clicked, not at the
//! playhead or on the highlighted song.

use crate::keys::Action;
use std::time::Duration;

/// What a menu was opened on.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Target {
    /// A moment on a waveform.
    Moment(Duration),
    /// A section, by its place in the playing recording's list.
    Section(usize),
    /// A song, by its row in the list.
    Song(usize),
    /// A stem of the playing stem set.
    Stem(usize),
    /// The speed controls.
    Speed,
    /// The pitch controls.
    Pitch,
}

pub struct Menu {
    pub target: Target,
    pub entries: Vec<(&'static str, Action)>,
    /// The highlighted entry.
    pub row: usize,
}

#[derive(Debug, PartialEq)]
pub enum Outcome {
    Close,
    Choose(Action),
}

impl Menu {
    fn new(target: Target, entries: Vec<(&'static str, Action)>) -> Menu {
        Menu { target, entries, row: 0 }
    }

    /// On a waveform, at the moment under the pointer.
    pub fn waveform(at: Duration) -> Menu {
        Menu::new(
            Target::Moment(at),
            vec![
                ("Loop starts here", Action::LoopStart),
                ("Loop ends here", Action::LoopEnd),
                ("Marker here", Action::Marker),
            ],
        )
    }

    /// On section `i` in the section lane.
    pub fn section(i: usize) -> Menu {
        Menu::new(
            Target::Section(i),
            vec![
                ("Loop it", Action::LoopRegion),
                ("Rename", Action::SaveRegion),
                ("Delete", Action::RemoveRegion),
            ],
        )
    }

    /// On the song in `row`. Album and artist say "back" when that view is already open.
    pub fn song(row: usize, favorite: bool, album: bool, artist: bool) -> Menu {
        Menu::new(
            Target::Song(row),
            vec![
                (if favorite { "Not a favourite" } else { "Favourite" }, Action::Favorite),
                (if album { "Back from the album" } else { "Only this album" }, Action::Album),
                (if artist { "Back from the artist" } else { "Only this artist" }, Action::Artist),
                ("Open in Transcribe!", Action::Transcribe),
                ("Add a variation…", Action::AddVariation),
            ],
        )
    }

    /// On the speed controls: presets, and the trainer's settings.
    pub fn speed() -> Menu {
        Menu::new(
            Target::Speed,
            vec![
                ("50%", Action::SetSpeed(50)),
                ("60%", Action::SetSpeed(60)),
                ("70%", Action::SetSpeed(70)),
                ("80%", Action::SetSpeed(80)),
                ("90%", Action::SetSpeed(90)),
                ("100%", Action::SetSpeed(100)),
                ("110%", Action::SetSpeed(110)),
                ("Trainer settings…", Action::TrainerSettings),
            ],
        )
    }

    /// On the pitch controls: fine steps, and back to as recorded.
    pub fn pitch() -> Menu {
        Menu::new(
            Target::Pitch,
            vec![
                ("Up a semitone", Action::Pitch(100)),
                ("Down a semitone", Action::Pitch(-100)),
                ("Up 5 cents", Action::Pitch(5)),
                ("Down 5 cents", Action::Pitch(-5)),
                ("As recorded", Action::SetPitch(0)),
            ],
        )
    }

    /// On stem `i`: its level, solo and mute, saying what a toggle will do.
    pub fn stem(i: usize, muted: bool, soloed: bool) -> Menu {
        Menu::new(
            Target::Stem(i),
            vec![
                ("Level 100%", Action::Level(i, 100)),
                ("Level 50%", Action::Level(i, 50)),
                ("Level 25%", Action::Level(i, 25)),
                (if soloed { "End the solo" } else { "Solo" }, Action::Solo(i)),
                (if muted { "Unmute" } else { "Mute" }, Action::Mute(i)),
            ],
        )
    }

    /// A key while the menu is open: `j k` and the arrows move, Enter or Space chooses,
    /// Esc closes. Every other key is swallowed.
    pub fn key(&mut self, key: &str) -> Option<Outcome> {
        match key {
            "j" | "down" => self.row = (self.row + 1).min(self.entries.len().saturating_sub(1)),
            "k" | "up" => self.row = self.row.saturating_sub(1),
            "enter" | "space" => return self.entries.get(self.row).map(|e| Outcome::Choose(e.1)),
            "escape" => return Some(Outcome::Close),
            _ => {}
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_move_within_the_menu_and_enter_chooses() {
        let mut menu = Menu::waveform(Duration::from_secs(3));
        assert_eq!(menu.key("k"), None);
        assert_eq!(menu.row, 0);
        menu.key("j");
        menu.key("down");
        menu.key("j");
        assert_eq!(menu.row, 2);
        assert_eq!(menu.key("enter"), Some(Outcome::Choose(Action::Marker)));
        menu.key("up");
        assert_eq!(menu.key("space"), Some(Outcome::Choose(Action::LoopEnd)));
        assert_eq!(menu.key("escape"), Some(Outcome::Close));
        assert_eq!(menu.key("x"), None);
    }

    #[test]
    fn menus_act_on_what_was_clicked() {
        assert_eq!(Menu::waveform(Duration::from_secs(3)).target, Target::Moment(Duration::from_secs(3)));
        assert_eq!(Menu::section(2).target, Target::Section(2));
        assert_eq!(Menu::song(7, false, false, false).target, Target::Song(7));
        // A stem's entries name the stem, so they work from anywhere.
        let stem = Menu::stem(3, false, false);
        assert_eq!(stem.target, Target::Stem(3));
        assert!(stem.entries.iter().all(|(_, a)| matches!(a, Action::Level(3, _) | Action::Solo(3) | Action::Mute(3))));
    }

    #[test]
    fn song_menu_says_what_a_toggle_will_do() {
        let labels = |m: Menu| m.entries.iter().map(|e| e.0).collect::<Vec<_>>();
        assert!(labels(Menu::song(0, false, false, false)).contains(&"Favourite"));
        let open = labels(Menu::song(0, true, true, true));
        assert!(open.contains(&"Not a favourite"));
        assert!(open.contains(&"Back from the album"));
        assert!(open.contains(&"Back from the artist"));
    }
}
