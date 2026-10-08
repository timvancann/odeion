# Plan 03: playlists

Asked for by Tim on 2026-10-08, right after the queue. Named lists of songs, filled
song by song, each opening in the list like an album does.

Out of scope: saving the queue as a playlist, dragging to reorder, a song more than
once in a playlist, smart or folder playlists, importing or exporting (m3u).

## Decisions (Tim's answers)

- **Filled song by song.** From a song's right-click menu ("Add to playlist…") or a
  key, never from the queue. Rejected: saving the queue, both.
- **A playlist view.** Playlists are listed in the sidebar; a click shows one in the
  list with its own header, like an album. Enter plays from that song on through the
  rest of the playlist. Rejected: loading it into the queue.
- **Edited by key and menu.** In its view a song can be removed or moved up and down;
  the header renames or deletes the playlist. Rejected: remove only, drag to reorder.

## 1. Rules (`playlists.rs`, pure)

- `Playlists(Vec<Named { name, songs: Vec<PathBuf> }>)`, kept in order of creation in
  `playlists.json` in the data folder (saved through a temp file and a rename, like
  the session).
- `create(name)`: a trimmed, non-empty name not already used (case aside); returns
  its place. `rename(i, name)` under the same rule. `delete(i)`.
- `add(i, song)`: at the end; a song already in it is refused and left where it is.
  `remove(i, at)`, `up(i, at)`, `down(i, at)` (no-ops at the ends).
- Songs that have gone (a rescan) stay in the file but aren't listed, so a song
  coming back from a cloud-only state isn't lost.

## 2. Adding

- `+` adds a song to a playlist: the highlighted one in the library, the playing one
  in practice. The song row's menu has "Add to playlist…" too.
- Both open the **playlist picker**: a small sheet listing every playlist (a check on
  those that have the song already) and "New playlist…" last. `j k` / ↑ ↓ and Enter,
  or a click; on "New playlist…" a name is typed and Enter creates it with the song.
  Esc closes. A message says "added to <name>" or "already in <name>".

## 3. The playlist view

- The sidebar gets a **Playlists** section between the places and the queue: one
  entry per playlist with its song count, lit while open. Collapsed, one icon stands
  for them and opens the sidebar.
- A click opens the view: a header like the album's (name, song count and length,
  rename and delete buttons) over the songs in the playlist's order, in the columns
  layout. Esc or the sidebar's Library leave it, as they leave an album. ⌘A / ⇧⌘A
  open the song's album or artist over it, and leaving those comes back to the
  playlist (as built).
- Keys in the view (library mode): `x` removes the highlighted song from the playlist,
  `K J` (vim keys) or ⇧↑ ⇧↓ move it up or down, the highlight following. The row's
  menu gets "Remove from playlist", "Move up" and "Move down" while the view is open.
- Rename: the header's name becomes a text field (Enter keeps, Esc cancels). Delete:
  the button asks once more ("delete for good?") before it deletes.
- The session remembers an open playlist view by name.

Status (2026-10-08): built, all three steps.

## Build order

1. `playlists.rs` with tests for §1, the file round trip.
2. The picker, `+`, the menu entry.
3. The sidebar section, the view and its header, the edit keys and menu entries, the
   session. Keys on the ⌘? sheet; AGENTS.md updated.
