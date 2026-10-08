//! What plays next: the queue first, then the list a song was started from (an album,
//! the favourites, search results or the library), in order or shuffled.

use crate::queue::Queue;
use std::path::{Path, PathBuf};
use std::time::Duration;

/// Previous restarts the song once it has played this long, like any player.
pub const RESTART_AFTER: Duration = Duration::from_secs(3);

#[derive(Debug, Default, Clone, PartialEq)]
pub struct Playlist {
    /// The list as it was shown.
    list: Vec<PathBuf>,
    /// The play order: `list`, or a shuffle of it.
    order: Vec<PathBuf>,
    at: usize,
    /// The queued song playing, if one is. The list stays where it was meanwhile.
    queued: Option<PathBuf>,
}

impl Playlist {
    /// `list` playing from `current`. With a seed the rest of the list is shuffled.
    pub fn new(list: Vec<PathBuf>, current: &Path, seed: Option<u64>) -> Playlist {
        let mut p = Playlist { order: list.clone(), list, at: 0, queued: None };
        p.reorder(current, seed);
        p
    }

    /// Shuffles (with a seed) or unshuffles, keeping `current` playing. Shuffled, the
    /// current song comes first and everything else follows in random order. While a
    /// queued song plays, the list's own song counts as the current one.
    pub fn reorder(&mut self, current: &Path, seed: Option<u64>) {
        let current = match self.queued {
            Some(_) => self.order.get(self.at).cloned().unwrap_or_default(),
            None => current.to_path_buf(),
        };
        let current = current.as_path();
        self.order = self.list.clone();
        match seed {
            Some(seed) => {
                self.order.retain(|p| p != current);
                shuffle(&mut self.order, seed);
                self.order.insert(0, current.to_path_buf());
                self.at = 0;
            }
            None => self.at = self.order.iter().position(|p| p == current).unwrap_or(0),
        }
    }

    /// The queue's first song, which leaves the queue, or else the list's next.
    pub fn next(&mut self, queue: &mut Queue) -> Option<PathBuf> {
        if let Some(song) = queue.take(0) {
            self.queued = Some(song.clone());
            return Some(song);
        }
        self.queued = None;
        let next = self.order.get(self.at + 1)?;
        self.at += 1;
        Some(next.clone())
    }

    /// A queued song played now, out of turn: the list stays where it was.
    pub fn play_queued(&mut self, song: &Path) {
        self.queued = Some(song.to_path_buf());
    }

    /// The song before, or this one again when it is past `RESTART_AFTER` or first.
    /// Before a queued song comes the list's song that played before it.
    pub fn previous(&mut self, position: Duration) -> Option<&PathBuf> {
        if self.queued.is_some() {
            if position >= RESTART_AFTER {
                return self.queued.as_ref();
            }
            self.queued = None;
            return self.order.get(self.at);
        }
        if position < RESTART_AFTER && self.at > 0 {
            self.at -= 1;
        }
        self.order.get(self.at)
    }
}

/// Fisher-Yates with xorshift64: no dependency for a shuffle that only has to look random.
fn shuffle(items: &mut [PathBuf], seed: u64) {
    let mut state = seed | 1;
    for i in (1..items.len()).rev() {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        items.swap(i, (state % (i as u64 + 1)) as usize);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn list(names: &[&str]) -> Vec<PathBuf> {
        names.iter().map(PathBuf::from).collect()
    }

    #[test]
    fn walks_the_list_from_the_song_started() {
        let mut p = Playlist::new(list(&["a", "b", "c"]), Path::new("b"), None);
        let none = &mut Queue::default();
        assert_eq!(p.next(none), Some("c".into()));
        assert_eq!(p.next(none), None);
        assert_eq!(p.previous(Duration::ZERO), Some(&"b".into()));
        assert_eq!(p.previous(Duration::ZERO), Some(&"a".into()));
        assert_eq!(p.previous(Duration::ZERO), Some(&"a".into()));
    }

    #[test]
    fn previous_restarts_a_song_well_underway() {
        let mut p = Playlist::new(list(&["a", "b"]), Path::new("b"), None);
        assert_eq!(p.previous(Duration::from_secs(10)), Some(&"b".into()));
        assert_eq!(p.previous(Duration::from_secs(1)), Some(&"a".into()));
    }

    #[test]
    fn shuffled_plays_every_song_once_starting_with_the_current() {
        let songs = list(&["a", "b", "c", "d", "e", "f"]);
        let mut p = Playlist::new(songs.clone(), Path::new("c"), Some(42));
        let mut played = vec![p.order[0].clone()];
        while let Some(next) = p.next(&mut Queue::default()) {
            played.push(next);
        }
        assert_eq!(played[0], PathBuf::from("c"));
        assert_ne!(played, songs);
        played.sort();
        assert_eq!(played, songs);
    }

    #[test]
    fn unshuffling_continues_after_the_current_song_in_list_order() {
        let mut p = Playlist::new(list(&["a", "b", "c", "d"]), Path::new("a"), Some(7));
        let current = p.next(&mut Queue::default()).unwrap();
        p.reorder(&current, None);
        let expected = list(&["a", "b", "c", "d"])
            .into_iter()
            .skip_while(|s| *s != current)
            .nth(1);
        assert_eq!(p.next(&mut Queue::default()), expected);
    }

    fn queued(names: &[&str]) -> Queue {
        let mut q = Queue::default();
        for n in names {
            q.add(Path::new(n));
        }
        q
    }

    #[test]
    fn the_queue_plays_first_and_the_list_carries_on_where_it_was() {
        let mut p = Playlist::new(list(&["a", "b", "c"]), Path::new("a"), None);
        let mut q = queued(&["x", "y"]);
        assert_eq!(p.next(&mut q), Some("x".into()));
        assert_eq!(p.next(&mut q), Some("y".into()));
        assert!(q.is_empty());
        assert_eq!(p.next(&mut q), Some("b".into()));
    }

    #[test]
    fn previous_from_a_queued_song_restarts_it_or_goes_back_to_the_list() {
        let mut p = Playlist::new(list(&["a", "b", "c"]), Path::new("b"), None);
        let mut q = queued(&["x"]);
        p.next(&mut q);
        assert_eq!(p.previous(Duration::from_secs(10)), Some(&"x".into()));
        assert_eq!(p.previous(Duration::from_secs(1)), Some(&"b".into()));
        assert_eq!(p.previous(Duration::from_secs(1)), Some(&"a".into()));
    }

    #[test]
    fn a_song_played_from_the_queue_out_of_turn_keeps_the_list_in_place() {
        let mut p = Playlist::new(list(&["a", "b", "c"]), Path::new("a"), None);
        p.play_queued(Path::new("x"));
        assert_eq!(p.previous(Duration::ZERO), Some(&"a".into()));
        p.play_queued(Path::new("x"));
        assert_eq!(p.next(&mut Queue::default()), Some("b".into()));
    }

    #[test]
    fn shuffling_during_a_queued_song_keeps_the_lists_place() {
        let mut p = Playlist::new(list(&["a", "b", "c", "d"]), Path::new("b"), None);
        p.next(&mut queued(&["x"]));
        p.reorder(Path::new("x"), None);
        assert_eq!(p.next(&mut Queue::default()), Some("c".into()));
    }
}
