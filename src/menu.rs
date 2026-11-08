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
    /// A song in the sidebar's queue, by its place there.
    Queued(usize),
    /// The playing song, on the now-playing card.
    Playing,
    /// The loop, on the bar under the waveform.
    Loop,
    /// The marker's flag on a waveform.
    Marker,
    /// A playlist, by its place: in the sidebar or among search results.
    Playlist(usize),
    /// The favourites, in the sidebar.
    Favourites,
    /// The open album, artist or playlist, on its header: the songs listed.
    Shown,
    /// An artist's row in the grouped list, by its row.
    Artist(usize),
}

pub struct Menu {
    pub target: Target,
    pub entries: Vec<(&'static str, Action)>,
    /// The entries a thin line is drawn above, setting groups apart.
    pub breaks: Vec<usize>,
    /// The highlighted entry.
    pub row: usize,
}

/// What the open album, artist or playlist header's menu is for.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Shown {
    Album,
    Artist,
    Playlist,
}

#[derive(Debug, PartialEq)]
pub enum Outcome {
    Close,
    Choose(Action),
}

impl Menu {
    fn new(target: Target, entries: Vec<(&'static str, Action)>) -> Menu {
        Menu { target, entries, breaks: Vec::new(), row: 0 }
    }

    /// More entries, set apart from those before by a line. None, no line.
    fn then(mut self, entries: impl IntoIterator<Item = (&'static str, Action)>) -> Menu {
        let at = self.entries.len();
        self.entries.extend(entries);
        if at > 0 && self.entries.len() > at {
            self.breaks.push(at);
        }
        self
    }

    /// On a waveform, at the moment under the pointer; inside the loop, the loop's own
    /// entries too.
    pub fn waveform(at: Duration, in_loop: bool) -> Menu {
        let looped = [("Save the loop as a section…", Action::SaveRegion), ("Clear the loop", Action::LoopClear)];
        Menu::new(
            Target::Moment(at),
            vec![
                ("Loop starts here", Action::LoopStart),
                ("Loop ends here", Action::LoopEnd),
                ("Marker here", Action::Marker),
            ],
        )
        .then(looped.into_iter().filter(|_| in_loop))
    }

    /// On the marker's flag.
    pub fn marker() -> Menu {
        Menu::new(Target::Marker, vec![("Play from the marker", Action::FromLoopStart), ("Clear the marker", Action::Marker)])
    }

    /// On the loop bar, saying what a toggle will do. `active`: it has an end, so on or
    /// off, replay, save and clear; otherwise its edges are placed at the playhead.
    /// `edges`: any edge is set; `marker`: replay goes to the marker.
    pub fn looping(active: bool, on: bool, edges: bool, marker: bool) -> Menu {
        let replay = if marker { "Play from the marker" } else { "Play from the loop start" };
        let entries = match active {
            true => vec![
                (if on { "Loop off" } else { "Loop on" }, Action::LoopSwitch),
                (replay, Action::FromLoopStart),
                ("Save the loop as a section…", Action::SaveRegion),
                ("Clear the loop", Action::LoopClear),
            ],
            false => {
                let mut entries = vec![("Loop starts here", Action::LoopStart), ("Loop ends here", Action::LoopEnd)];
                if edges {
                    entries.push(("Clear the loop", Action::LoopClear));
                }
                entries
            }
        };
        Menu::new(Target::Loop, entries)
    }

    /// On the now-playing card: the playing song's entries, then the loop's and the
    /// marker's while there are any.
    pub fn playing(favorite: bool, active: bool, on: bool, marker: bool) -> Menu {
        let mut timing = Vec::new();
        if active {
            timing.push((if on { "Loop off" } else { "Loop on" }, Action::LoopSwitch));
        }
        if active || marker {
            timing.push((if marker { "Play from the marker" } else { "Play from the loop start" }, Action::FromLoopStart));
        }
        if active {
            timing.push(("Clear the loop", Action::LoopClear));
        }
        if marker {
            timing.push(("Clear the marker", Action::Marker));
        }
        Menu::new(
            Target::Playing,
            vec![
                (if favorite { "Not a favourite" } else { "Favourite" }, Action::Favorite),
                ("Only this album", Action::Album),
                ("Only this artist", Action::Artist),
                ("Add to playlist…", Action::AddToPlaylist),
                ("Add a variation…", Action::AddVariation),
            ],
        )
        .then(timing)
    }

    /// On playlist `i`, in the sidebar or the search results. Rename and delete open
    /// it, where its header asks for the name or "for good".
    pub fn playlist(i: usize) -> Menu {
        Menu::new(
            Target::Playlist(i),
            vec![
                ("Open", Action::OpenPlaylist),
                ("Play", Action::PlayAll),
                ("Shuffle", Action::ShuffleAll),
                ("Queue all", Action::QueueAll),
            ],
        )
        .then([("Rename…", Action::RenamePlaylist), ("Delete…", Action::DeletePlaylist)])
    }

    /// On the favourites in the sidebar.
    pub fn favourites() -> Menu {
        Menu::new(
            Target::Favourites,
            vec![("Play", Action::PlayAll), ("Shuffle", Action::ShuffleAll), ("Queue all", Action::QueueAll)],
        )
    }

    /// On the open album, artist or playlist's header: its songs, then going back (or,
    /// for a playlist, renaming and deleting it).
    pub fn shown(of: Shown) -> Menu {
        let menu = Menu::new(
            Target::Shown,
            vec![
                ("Play", Action::PlayAll),
                ("Shuffle", Action::ShuffleAll),
                ("Queue all", Action::QueueAll),
                ("Add all to playlist…", Action::AddAllToPlaylist),
            ],
        );
        match of {
            Shown::Album => menu.then([("Back from the album", Action::Album)]),
            Shown::Artist => menu.then([("Back from the artist", Action::Artist)]),
            Shown::Playlist => menu.then([("Rename…", Action::RenamePlaylist), ("Delete…", Action::DeletePlaylist)]),
        }
    }

    /// On an artist's row in the grouped list, `row`.
    pub fn artist(row: usize) -> Menu {
        Menu::new(
            Target::Artist(row),
            vec![
                ("Only this artist", Action::Artist),
                ("Play", Action::PlayAll),
                ("Shuffle", Action::ShuffleAll),
                ("Queue all", Action::QueueAll),
                ("Add all to playlist…", Action::AddAllToPlaylist),
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

    /// On the song in `row`. Album and artist say "back" when that view is already open;
    /// in a playlist's view the song can leave it or move.
    pub fn song(row: usize, favorite: bool, album: bool, artist: bool, in_playlist: bool) -> Menu {
        let listed = [
            ("Remove from playlist", Action::PlaylistRemove),
            ("Move up", Action::PlaylistMove(true)),
            ("Move down", Action::PlaylistMove(false)),
        ];
        Menu::new(
            Target::Song(row),
            vec![
                (if favorite { "Not a favourite" } else { "Favourite" }, Action::Favorite),
                (if album { "Back from the album" } else { "Only this album" }, Action::Album),
                (if artist { "Back from the artist" } else { "Only this artist" }, Action::Artist),
            ],
        )
        .then([
            ("Queue", Action::Queue(false)),
            ("Queue next", Action::Queue(true)),
            ("Add to playlist…", Action::AddToPlaylist),
            ("Add a variation…", Action::AddVariation),
        ])
        .then(listed.into_iter().filter(|_| in_playlist))
    }

    /// On the `i`-th song in the queue.
    pub fn queued(i: usize) -> Menu {
        Menu::new(
            Target::Queued(i),
            vec![("Play now", Action::QueuePlay), ("To the top", Action::QueueTop), ("Remove", Action::QueueRemove)],
        )
        .then([
            ("Only this album", Action::Album),
            ("Only this artist", Action::Artist),
            ("Add to playlist…", Action::AddToPlaylist),
        ])
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
        let mut menu = Menu::waveform(Duration::from_secs(3), false);
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
        assert_eq!(Menu::waveform(Duration::from_secs(3), false).target, Target::Moment(Duration::from_secs(3)));
        assert_eq!(Menu::playlist(4).target, Target::Playlist(4));
        assert_eq!(Menu::artist(9).target, Target::Artist(9));
        assert_eq!(Menu::section(2).target, Target::Section(2));
        assert_eq!(Menu::song(7, false, false, false, false).target, Target::Song(7));
        assert_eq!(Menu::queued(1).target, Target::Queued(1));
        // A stem's entries name the stem, so they work from anywhere.
        let stem = Menu::stem(3, false, false);
        assert_eq!(stem.target, Target::Stem(3));
        assert!(stem.entries.iter().all(|(_, a)| matches!(a, Action::Level(3, _) | Action::Solo(3) | Action::Mute(3))));
    }

    #[test]
    fn song_menu_says_what_a_toggle_will_do() {
        let labels = |m: Menu| m.entries.iter().map(|e| e.0).collect::<Vec<_>>();
        assert!(labels(Menu::song(0, false, false, false, false)).contains(&"Favourite"));
        let open = labels(Menu::song(0, true, true, true, true));
        assert!(open.contains(&"Remove from playlist"));
        assert!(open.contains(&"Not a favourite"));
        assert!(open.contains(&"Back from the album"));
        assert!(open.contains(&"Back from the artist"));
    }

    fn labels(m: &Menu) -> Vec<&'static str> {
        m.entries.iter().map(|e| e.0).collect()
    }

    #[test]
    fn groups_are_set_apart_and_empty_ones_leave_no_line() {
        // Favourite, album and artist; the queue and playlists; the playlist's own.
        assert_eq!(Menu::song(0, false, false, false, true).breaks, [3, 7]);
        assert_eq!(Menu::song(0, false, false, false, false).breaks, [3]);
        assert!(Menu::waveform(Duration::ZERO, false).breaks.is_empty());
        let inside = Menu::waveform(Duration::ZERO, true);
        assert_eq!(inside.breaks, [3]);
        assert!(labels(&inside).contains(&"Clear the loop"));
        // Nothing to say about the loop or marker: no line under the song's entries.
        assert!(Menu::playing(false, false, false, false).breaks.is_empty());
        // Moving by key goes across a line as if it weren't there.
        let mut menu = Menu::queued(0);
        (0..3).for_each(|_| _ = menu.key("j"));
        assert_eq!(menu.key("enter"), Some(Outcome::Choose(Action::Album)));
    }

    #[test]
    fn the_loop_menu_offers_what_the_loop_can_do() {
        // Without a loop: its edges, at the playhead.
        assert_eq!(labels(&Menu::looping(false, false, false, false)), ["Loop starts here", "Loop ends here"]);
        assert!(labels(&Menu::looping(false, false, true, false)).contains(&"Clear the loop"));
        let on = Menu::looping(true, true, true, false);
        assert_eq!(on.entries[0], ("Loop off", Action::LoopSwitch));
        assert!(labels(&on).contains(&"Play from the loop start"));
        // Replay says where it goes: the marker wins, as it does for o.
        assert!(labels(&Menu::looping(true, false, true, true)).contains(&"Play from the marker"));
        assert_eq!(Menu::looping(true, false, true, true).entries[0].0, "Loop on");
    }

    #[test]
    fn the_now_playing_card_adds_the_loop_and_marker_only_when_there_are_any() {
        let bare = Menu::playing(true, false, false, false);
        assert_eq!(bare.target, Target::Playing);
        assert_eq!(bare.entries[0].0, "Not a favourite");
        assert_eq!(bare.entries.len(), 5);
        let marked = labels(&Menu::playing(false, false, false, true));
        assert!(marked.contains(&"Play from the marker") && marked.contains(&"Clear the marker"));
        assert!(!marked.contains(&"Clear the loop"));
        let looped = labels(&Menu::playing(false, true, true, false));
        assert!(looped.contains(&"Loop off") && looped.contains(&"Clear the loop"));
    }

    #[test]
    fn list_menus_play_shuffle_and_queue_their_songs() {
        for menu in [Menu::playlist(0), Menu::favourites(), Menu::shown(Shown::Album), Menu::artist(0)] {
            let actions: Vec<Action> = menu.entries.iter().map(|e| e.1).collect();
            for a in [Action::PlayAll, Action::ShuffleAll, Action::QueueAll] {
                assert!(actions.contains(&a), "{a:?} missing");
            }
        }
        // Deleting a playlist asks first: the entry says so.
        assert!(labels(&Menu::playlist(0)).contains(&"Delete…"));
        assert!(labels(&Menu::shown(Shown::Playlist)).contains(&"Delete…"));
        assert!(labels(&Menu::shown(Shown::Artist)).contains(&"Back from the artist"));
        assert_eq!(Menu::marker().entries[1], ("Clear the marker", Action::Marker));
    }
}
