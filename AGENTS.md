# Working in this repo

Riff is Tim's practice player: find a song in his music folder, play it, and practise
it: loops to the sample, a marker, a waveform with beats, named sections, slow-down, and other
recordings of the song (backing tracks, stem sets) to switch between. It began as a way
into Transcribe! (whose file finding is painful) and grew into the practice tool, by
Tim's choice; Transcribe! is still a button away for deeper work. Practice features
are in scope; ask before adding anything that turns riff into an editor or a DAW.

A single GPUI 0.2.2 app (pinned, like jasmijn's desktop app, whose patterns this copies),
macOS only, no network, no accounts.

## Layout and where logic lives

Pure, tested modules hold the decisions; `view.rs` only maps keys and clicks onto them
and draws:

- `library.rs`: what a track is, reading one from its path, sort order, search, albums.
- `scan.rs`: walking the folder, merging with the cache, reading tags and covers on
  background threads.
- `audio.rs`: playback via rodio, and the A/B `Loop` rules. The loop is kept inside
  the audio stream (`Looped`), to the sample, and that wrapper also keeps the playing
  position, since rodio's player can't see a jump made inside it. Checking from the
  window every 40 ms let the music run up to 40 ms past the loop's end.
- `playlist.rs`: next, previous and shuffle over the list a song was started from (the
  album, favourites, search results or library; the run of rows between headers).
- `keys.rs`: the keymap and the ⌘? sheet, see Keyboard.
- `wave.rs`: the practice view's waveform: read on 8 threads when a song starts (each
  placed by packet timestamps, with a second of decoder warm-up), summarised to one
  column per pixel, and which stretch the detail view shows.
- `beats.rs`: where the beats fall (SuperFlux onsets, Ellis tempo and tracker at
  tightness 300, each beat moved onto its attack to within ~1 ms) and the grid rules on
  them. Found in the background when a song starts, about 0.3 s, and cached in
  `beats/` in the data folder (re-found when the file or `DETECTOR` changes). Old rock drifts, so
  the grid is the detected beats, never "BPM times n".
- `variations.rs`: other recordings of a song (a backing track, a stem set), stored in
  `variations.json`. Built in the background when files are dropped or picked: a much
  shorter file (a count-in) is left out, stems are named by what sets their file names
  apart, and the set is aligned to the song by its onsets (10 ms to find, 1 ms rises to
  place). One that doesn't clearly match gets a timeline of its own. Tim's backing track
  aligned at +1.907 s, his Moises stems at +0.047 s (encoder delay).
- `resume.rs`: where each song was left (`resume.json`): its recording (song or
  variation), slow-down, playhead and each recording's loop.
- `session.rs`: what riff showed when it closed (`session.json`): the playing song,
  the highlighted one, practice, album or artist view, grid and zoom. Riff opens that
  way, the song paused where it was (its recording, loops and playhead come from
  `resume.json`), never playing on its own. Saved within a second of a change and on
  ⌘Q, since closing the window skips the quit path.
- `regions.rs`: sections (still "regions" in the code and `regions.json`): named
  stretches per recording, in order; renaming the same stretch, dragging an edge,
  which one is current, next and previous, removal.
- `media.rs`: the media keys and Now Playing via souvlaki. macOS sends media keys to the
  app that last reported a song, not as key presses, so this is the only way in.
- `state.rs`: paths, settings, favourites, opening a song in Transcribe!.
- `theme.rs`: Jasmijn's themes, copied: built-in gold plus every installed Zed theme.

New behaviour gets its rule and tests in one of these, not in `view.rs`.

## The music library

Riff scans the folders listed in ⌘, settings (`settings.json` in the data folder below),
by default Google Drive (`~/Library/CloudStorage/.../Mijn Drive/Music`, or `RIFF_MUSIC`
until settings are saved). A track's Artist/Album path is read from the deepest listed
folder holding it. The Drive folder is laid out `Artist/Album/NN Title.ext`, about
2,700 tracks of mp3, flac, wav and aiff. Tags and embedded covers are correct; they win over the path.

- **Files may be cloud-only.** Drive streams on demand: reading a cloud-only file's
  header took 1.6s and pulled about 4 MB. A full tag scan is then slow, and that is
  expected, not a bug. Tim has made the folder available offline, but new music arrives
  cloud-only.
- **The cache makes startup instant.** `~/Library/Application Support/riff/` holds
  `library.json` (tracks, re-read only when size or mtime changes), `covers/` (256px
  JPEG thumbnails named by content hash) and `favorites.json`. Deleting `library.json`
  forces a full rescan.
- **Transcribe! keeps its `.xsc` next to the song**, same name, `.xsc` extension. It
  holds loops and markers, so riff opens the `.xsc` when one exists, otherwise the song.
- AIFF needs rodio's `symphonia-aiff` feature; the default features leave it out.

## Keyboard

Keyboard first, every control also clickable. The keymap is Tim's, settled by trying it.
`keys.rs` holds it: a key becomes an `Action`, `view.rs` performs it, and `SHEET` beside
the map is the ⌘? sheet. A test checks every key on the sheet does something, so add a
key to both or the test fails.

- **Search is a mode** (⌘P or `/`). Typing goes to the query only while searching;
  outside it bare keys are commands, even with results showing. This replaced "type
  anywhere to search", where Space could not pause during a search.
- **Vim keys are on by default** (a settings row turns them off): `j k`, `gg G`,
  `⌃U ⌃D`, `zz` to the playing song, `h l` seek, `H L` previous and next song, `^` back
  to the start, `f` favourite, `a` album, `A` artist, `i` search, `t`
  Transcribe!, `s` shuffle, `?` keys, `dd` removes a folder in settings. No counts,
  by choice.
- **The ⌘ shortcuts always work**: ⌘P, ⌘F, ⌘A, ⇧⌘A (artist view), ⌘T (the *highlighted* song), ⌘S,
  ⌘← ⌘→ or ⌘H ⌘L previous and next, ⌘, settings, ⌘? keys, ⌘Q. ⌘H is free because
  there is no menu bar to hide the app with. ⌃J ⌃K and arrows move; plain ← → seek.
- **Loop keys always work**: `[ ]` set the edges at the playhead, `\` clears, `|` switches
  looping off and on keeping the edges (the loop button too, once both are set; placing
  an edge switches it back on), `{ }`
  widen the loop by 50 ms at the start or end and `( )` narrow it, `< >` slide the loop by its own length
  (phrase by phrase). Halving and doubling were tried and removed: Tim never used them.
  Every change plays around the edge it
  moved, to be judged by ear. On the progress bar, drag an edge or shift-click for the
  end. The rules live on `audio::Loop`.
- **`P` toggles the practice view**: the list gives way to the song's loudness contour
  (with the shown stretch boxed) over a large RMS waveform that frames the loop, or
  follows the playhead without one. `- =` zoom (⌘- ⌘= too, and the
  wheel over either waveform). Over a loop edge the cursor becomes a resize arrow
  (`edge_grips`, which only set the cursor; the strip under them takes the press). It is redrawn every
  frame while playing, the playhead smoothed between the player's position updates. Both take clicks and drags like the
  progress bar. The waveform is read for every song that starts, shown or not, and kept
  in memory only. Peaks are not drawn: on loud masters they all reach the top and hide
  the transients. A completed loop is framed once; after that the zoom is Tim's and
  only grows if the loop stops fitting. Loops have their own theme colour, `loops`: the
  first terminal colour (cyan, magenta, green, blue) that stands apart from the accent
  and text, since some themes' accent *is* their cyan.
  Sections take the next one that also stands apart from the loop's (`sections`; soft
  blue in Gold), so a loop sitting on a section reads as a different thing.
- **Songs resume where they were left**: recording, slow-down and loops always; the
  playhead only when Tim picks the song himself (a song following on in a list starts
  at the top), and not when it was left within 5 s of the end. Slow-down is per song.
  Kept on leaving a song, switching recording or speed, every 5 s of playing, and ⌘Q.
- **Slow-down**: `S` on and off, `,` `.` step through 50, 60, 70, 80, 90% and full speed
  (`.` at 90% turns slow-down off, `,` at full speed goes to 90%; `S` jumps to the preset); also the 50 to 90 buttons on the now-playing card (the lit one again turns it
  off). Pitch is kept: Signalsmith
  Stretch at its full preset (Tim compared it with SoundTouch, "night and day"; the
  cheaper preset saves nothing worth it at 0.5% of a core). It sits after the loop and
  the stem mix (`audio::Stretched`), so loops, stems and beats stay in song time; off,
  samples pass straight through. It reports what it holds back (~0.1 s) so the playhead
  shows what's heard, and a soft limiter keeps its rare peaks (up to 1.8) under full
  scale.
- **`b` toggles the beat grid** (the practice view marks beats with small chevrons at
  its top and bottom edges, faint until the grid is on, and shows the local tempo). On the grid `[ ]` and shift-click snap to the nearest beat, `{ } ( )`
  move an edge a whole beat, `< >` slide in beats, `h l` / `← →` step by
  beats (back restarts the beat you're in first), dragging snaps (⌥ drags freely), and so does a click that moves the playhead (⌥-click
  doesn't).
  Off, everything is as before; songs without a steady beat should stay off.
- **Sections and the loop are separate things** (Tim, 2026-10-05: they were too tied
  together). A section is a named, saved stretch of the song; the loop is the one
  unnamed stretch that repeats. Playing a section (a click, `n` `N`) sets the loop to
  it, and from then on they're apart: the loop can shrink inside the section or move
  off it without changing it. Sections change only by dragging their edges in the
  section lane (snapping on the grid, ⌥ freely). `r` saves the loop as a section in a
  dialog (Enter saves, an empty name saves one saying where it is, Esc cancels; when
  the loop is exactly a section it renames that one). `x` deletes the section in play:
  the loop's, else the shortest one around the playhead, which is also the one lit.
- **The marker**: `m` drops it at the playhead (on a beat with the grid), `m` again
  clears it from anywhere, and it drags like a loop edge (snapping, ⌥ freely; on a loop
  edge, grab it by its flag at the strip's top). Dropping it plays nothing. `o` plays from the marker, or from the loop start without one. One
  per recording, remembered like the loop, drawn as a gold flag on the waveforms and
  the progress bar. `m` and `o` work with vim keys off too; favourite lost `m`.
- **Variations**: drop files or a folder on a song (or the practice view), or `V` to
  pick; `v` cycles the song and its variations, keeping the position where they line
  up, `X` removes the playing one. Linked files leave the song list; songs with
  variations show `+N`. Each recording has sections and a marker of its own (keyed by its first
  file) and remembers its own loop: switching puts this one's away and brings the
  next one's back. Tim's choice, over sharing them across aligned recordings.
  Everything plays through one `audio::Mix`, a single file being a mix of one, so a
  stem set's `1`–`9` mute and `⌥1`–`⌥9` solo are instant; the practice view adds a
  lane per stem. A metronome stem starts muted.
- **⌘, opens settings**: folders (Enter on "Add folder…" opens the macOS picker), the
  theme picker, which works like Jasmijn's (type to filter, ↑↓ previews, Enter keeps,
  Esc restores), and vim keys on or off.
- **Every action has a button too**, built with `Riff::tool` (an icon, a tooltip and
  the action, done through `Riff::act` exactly as its key would). Tooltips list keys
  from `keys::keys_for`, which reads the ⌘? sheet through the real keymap, so they stay
  right if keys are ever remapped; never write a key into a tooltip by hand. Icons live
  in `assets/icons/`: Lucide's where one fits, riff's own (loop edges, widen, narrow,
  slide, beats, sections) drawn in the same stroke style from the loop's brackets. The
  design is the "Riff controls and icons" canvas.
- GPUI apps get no menu bar, so ⌘Q, ⌘M (minimise), ⌃⌘F (full screen) and every other
  ⌘ shortcut are handled by hand, and none collide with system menu items. A real menu
  bar would take ⌘H for hiding riff, which is previous song here.
- **The letter rail** beside the full library (not in search, album or artist views):
  click or drag along `# A–Z` and the list glides (220 ms, eased, by setting the scroll
  offset each frame) to that letter's first artist below the favourites, highlighting
  it. Letters follow the sort (`library::initial`), so a jump never lands out of order.
- **The list has a scrollbar like Zed's**, drawn by riff (GPUI 0.2.2 has none): a thin
  thumb shown while the list moves and for a second after, or with the pointer on it.
  Dragging the thumb scrolls directly; a click elsewhere on the track glides there.
- The Transcribe! button sits on the now-playing card, so it opens the *playing* song.

## GPUI traps hit here

- **The selection is a row index**, which means nothing in a new list. Take the
  selected path *before* replacing or re-sorting tracks and pass it to `reindex`.
  Getting this wrong made the highlight jump to a random song after the scan.
- Rows in a `uniform_list` need `w_full()` or they shrink to their text.

## Running and checking

`just run` runs from source; `just install` builds `Riff.app` (icon rendered from
`bundle/icon.svg` with resvg) into `~/Applications`.

Tim uses riff while you work on it. **Leave his running instance alone**: restarting it
cuts off whatever he is practising. Build, then tell him to restart. Playing audio
makes noise in his room, so verify decoding without an output device (rodio's
`Decoder` plus `try_seek` in a throwaway test) and leave listening to him. To see the
UI, screenshot the window with `screencapture -l<window id>`; driving keys would steal
his keyboard focus. So does merely opening a second riff window: whatever Tim is typing
elsewhere then lands in it as riff keys (a stray `1` muted a stem, a `v` switched
recordings). Only open one when he isn't typing, or ask first.

Before committing: `cargo test && cargo build --release`, no warnings. Work on `main`;
commit signing is off for this repo.

## Possible features

Ideas Tim liked but hasn't asked for yet. Check with him before building one.

- **Grid corrections**, later (Tim skipped them for now, 2026-10-03): shift the grid by half a beat (funk read on the
  offbeats) and halve or double the tempo (Neon Knights read at 95, Wildest Dreams at
  200), applied from the playhead on. Then perhaps tap tempo. Neural trackers (Beat
  This!) only if corrections are needed too often; they would also bring bars.
- **Read Transcribe!'s loops** from the `.xsc` next to the song, read-only, so loops made
  there appear in riff. The format still needs looking at.
- **Loops aligned to beats**, as Moises does with beat detection. This is Transcribe!
  territory, so only if it stays small.
