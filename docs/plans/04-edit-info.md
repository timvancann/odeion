# Plan 04: Edit info

Asked for by Tim on 2026-11-27: titles from the tags or the file name aren't always
the best description, especially for backing tracks. A limited tag editor from a
song's right-click menu, editing only what Odeion uses.

Out of scope: genre, composer, BPM, key, lyrics, comments, album artist (Odeion reads
none of them), renaming files, editing several unrelated songs at once.

## Decisions (Tim's answers)

- **Written into the files.** "Into the files of course": other players (Plex, VLC)
  see the fix, and nothing is lost when Odeion's data folder goes. Rejected: keeping
  the edits in Odeion's data folder. The README's "never writes next to your songs"
  changes to say Odeion writes only tags, and only when asked.
- **The fields Odeion uses.** Title, artist, album, track number, disc number (the
  album order sorts by disc first), year (album order and the album header) and the
  front cover.

## 1. Rules and writing (`tags.rs`)

- A sheet: a row per field and the cover last. One song shows every field; several
  (an album's or artist's) only artist, album, disc and year, since title and track
  are each song's own. Where the songs differ the field is empty, "mixed", and stays
  as each song has it unless something is typed.
- Typing without a cursor: a field highlighted by Tab or a click is selected whole, so
  typing replaces it and ⌫ clears it; after that typing adds and ⌫ takes the last
  character. Number fields take digits only (3 for track and disc, 4 for the year);
  an empty field or 0 removes the tag.
- Each format through its own tag, never lofty's generic one (which drops what it
  can't hold): ID3v2.4 for MP3, AIFF and WAV, plus an MP3's ID3v1 if it has one;
  Vorbis comments and the picture blocks for FLAC; Vorbis comments for Ogg; MP4 atoms
  for M4A. A new cover replaces the front cover (in ID3 also an "other" picture, which
  many files use as their cover; in MP4 all, which have no kinds), at most 1000 px
  across, scaled into a JPEG when larger.
- Safety: the tags go into a hidden copy beside the file, which is read back (same
  length, rate and channels; every changed field as asked; every other tag and picture
  still there) and only then renamed over the original. A failure leaves the song
  untouched and says which field or tag stood in the way.

## 2. The sheet

- "Edit info…" in the song menus (row, queue, now-playing card) and `⌘I` (the
  highlighted song, in practice the playing one); "Edit info of all…" on the album
  and artist headers and their rows in the grouped list.
- Tab and ↑ ↓ move, Enter or ⌘S saves (on the cover row Enter picks an image), Esc or
  a click outside cancels. An image dropped anywhere on the sheet becomes the cover.
- Saving runs on a thread; each song is read afresh into the library as it's written,
  then a message says what was saved or why not.

Status (2026-11-27): built, both steps. Checked on copies of Tim's FLAC, Ogg, AIFF,
WAV and three MP3 songs, every field edited: the audio bytes and every other tag
stayed. That check found lofty's ID3 year bug (the old year kept winning), now worked
around. M4A goes through the same path but is untested on a real file.
