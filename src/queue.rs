//! Songs to play next, ahead of the list the playing song came from. `q` adds to the
//! end, `Q` to the top; a song is queued once at most. Kept in `session.json`.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Queue(Vec<PathBuf>);

impl Queue {
    pub fn songs(&self) -> &[PathBuf] {
        &self.0
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Queues `song` at the end and returns its place (0 first), or none when it was
    /// queued already, which leaves it where it is.
    pub fn add(&mut self, song: &Path) -> Option<usize> {
        if self.0.iter().any(|s| s == song) {
            return None;
        }
        self.0.push(song.to_path_buf());
        Some(self.0.len() - 1)
    }

    /// Queues `song` first, moving it there when it was queued already.
    pub fn add_top(&mut self, song: &Path) {
        self.0.retain(|s| s != song);
        self.0.insert(0, song.to_path_buf());
    }

    pub fn remove(&mut self, i: usize) -> Option<PathBuf> {
        (i < self.0.len()).then(|| self.0.remove(i))
    }

    pub fn to_top(&mut self, i: usize) {
        if let Some(song) = self.remove(i) {
            self.0.insert(0, song);
        }
    }

    /// The `i`-th song, to play now: it leaves the queue.
    pub fn take(&mut self, i: usize) -> Option<PathBuf> {
        self.remove(i)
    }

    /// Drops the songs `keep` says are gone, after a rescan or on restoring.
    pub fn retain(&mut self, keep: impl Fn(&Path) -> bool) {
        self.0.retain(|s| keep(s));
    }
}

/// What queuing a song says: where it landed, or that it was there already.
pub fn said(place: Option<usize>) -> String {
    match place {
        None => "already queued".into(),
        Some(0) => "queued next".into(),
        Some(i) => {
            let n = i + 1;
            let suffix = match (n % 10, n % 100) {
                (_, 11..=13) => "th",
                (1, _) => "st",
                (2, _) => "nd",
                (3, _) => "rd",
                _ => "th",
            };
            format!("queued, {n}{suffix}")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn queue(names: &[&str]) -> Queue {
        Queue(names.iter().map(PathBuf::from).collect())
    }

    #[test]
    fn adding_goes_to_the_end_once() {
        let mut q = queue(&["a"]);
        assert_eq!(q.add(Path::new("b")), Some(1));
        assert_eq!(q.add(Path::new("a")), None);
        assert_eq!(q, queue(&["a", "b"]));
    }

    #[test]
    fn adding_to_the_top_moves_a_song_already_queued() {
        let mut q = queue(&["a", "b", "c"]);
        q.add_top(Path::new("c"));
        assert_eq!(q, queue(&["c", "a", "b"]));
        q.add_top(Path::new("d"));
        assert_eq!(q, queue(&["d", "c", "a", "b"]));
    }

    #[test]
    fn songs_move_to_the_top_and_leave() {
        let mut q = queue(&["a", "b", "c"]);
        q.to_top(2);
        assert_eq!(q, queue(&["c", "a", "b"]));
        assert_eq!(q.remove(1), Some("a".into()));
        assert_eq!(q.take(0), Some("c".into()));
        assert_eq!(q, queue(&["b"]));
        assert_eq!(q.remove(5), None);
        q.to_top(5);
        assert_eq!(q, queue(&["b"]));
    }

    #[test]
    fn queuing_says_where_the_song_landed() {
        assert_eq!(said(Some(0)), "queued next");
        assert_eq!(said(Some(1)), "queued, 2nd");
        assert_eq!(said(Some(2)), "queued, 3rd");
        assert_eq!(said(Some(10)), "queued, 11th");
        assert_eq!(said(Some(20)), "queued, 21st");
        assert_eq!(said(None), "already queued");
    }

    #[test]
    fn gone_songs_are_dropped() {
        let mut q = queue(&["a", "gone", "b"]);
        q.retain(|s| s != Path::new("gone"));
        assert_eq!(q, queue(&["a", "b"]));
    }

    #[test]
    fn it_is_stored_as_a_plain_list() {
        let q = queue(&["/m/a.mp3"]);
        assert_eq!(serde_json::to_string(&q).unwrap(), r#"["/m/a.mp3"]"#);
    }
}
