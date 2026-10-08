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
  window every 40 ms let the music run up to 40 ms past the loop's end. Also the
  volume, the stems' levels (`stem_gains`) and the output device (`wanted`, followed
  by `Audio::follow_output`).
- `playlist.rs`: next, previous and shuffle over the list a song was started from (the
  album, favourites, search results or library; the run of rows between headers),
  with the queue first: while a queued song plays the list stays where it was.
- `queue.rs`: the queue, songs to play next (`q` to the end, `Q` to the top, each song
  once), kept in `session.json`.
- `keys.rs`: the keymap per mode and the ⌘? sheet, see Keyboard.
- `menu.rs`: the right-click menus: their entries and moving through them by key.
- `speed.rs`: the speed in whole percent, 25 to 110, and the pitch in cents, up to an
  octave either way; both kept by a song while it plays as recorded.
- `trainer.rs`: the speed trainer: from, to, step, every N repeats, turned into a
  speed from how often the loop has come round.
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
  variation), speed, playhead and each recording's loop, marker and stem levels. Old
  entries' slow-down preset (an index) is read as its percentage.
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

Keyboard first, every control also clickable. The keymap is Tim's, settled by trying it
(plan 01 has the reasoning). `keys.rs` holds it: a key becomes an `Action` through the
current mode's map, `view.rs` performs it, and `SHEET` beside the map is the ⌘? sheet,
each group listing the modes it shows in. Tests check every key on the sheet does
something in its modes and no key is listed twice in a mode, so add a key to both or
they fail.

- **Modes, like vim**: the library (finding and playing songs; riff opens here unless
  it was left in practice), search inside it, and practice (the practice view; the mode
  *is* the view). `p` or Tab enters practice, Esc, `p` or Tab leaves; the title bar's
  Library / Practice switch does the same. The ⌘? sheet and every tooltip show the
  current mode's keys. Every primary key sits on the base layer of Tim's Glove80 (shift
  counts); brackets, braces and parentheses take two hands there, so they're off the
  sheet. A test checks the practice sheet.
- **Everywhere outside typing**: Space pauses, `← →` seek, `0` back to the start, `m`
  and `o` the marker.
- **Search is a mode** (⌘P, or `/` `i`). Typing goes to the query only while
  searching; outside it bare keys are commands, even with results showing. This
  replaced "type anywhere to search", where Space could not pause during a search.
- **Vim keys in the library, on by default** (a settings row turns them off): `j k`,
  `gg G`, `⌃U ⌃D`, `zz` to the playing song, `h l` seek, `H L` previous and next song,
  `f` favourite, `a` album, `A` artist, `i` search, `t` Transcribe!, `s` shuffle, `?`
  keys, `dd` removes a folder in settings. No counts, by choice. Practice keys are the
  mode's own and work either way.
- **The ⌘ shortcuts always work**: ⌘P, ⌘F, ⌘A, ⇧⌘A (artist view), ⌘T (the *highlighted* song), ⌘S,
  ⌘← ⌘→ or ⌘H ⌘L previous and next, ⌘, settings, ⌘? keys, ⌘Q. ⌘H is free because
  there is no menu bar to hide the app with. In the library ⌃J ⌃K and arrows move.
- **Loop keys are practice keys**: `a` `b` set the edges at the playhead (`[ ]` quietly
  too), `\` switches looping off and on keeping the edges (the loop button too, once
  both are set; placing an edge switches it back on), `|` clears. Widening or narrowing
  by 50 ms and sliding the loop by its own length (phrase by phrase) are buttons on
  the loop bar only, by Tim's choice: `h l` move the playhead and `a` or `b` again puts
  the edge there. Halving and doubling were tried and removed: Tim never used them.
  No change moves the playhead (Tim, 2026-10-07: jumping to the edge was jarring); it
  carries on, wrapping at the loop's end as usual, until `o` plays from the loop's
  start. Playing a section still starts it. On the progress
  bar, drag an edge or shift-click for the end. The rules live on `audio::Loop`.
- **Right-click menus** (`menu.rs`): on a waveform (loop starts or ends here, marker
  here, all at the moment clicked), a section (loop it, rename, delete) and a song row
  (favourite, album, artist, Transcribe!, add a variation; the row is highlighted).
  Each entry is an `Action` showing its keys, done on what was clicked rather than at
  the playhead. They open at the pointer, take `j k` / ↑ ↓ and Enter, and close on Esc
  or a click elsewhere, which does nothing else.
- **The practice view**: the list gives way to the song's loudness contour
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
- **Speed**, per song, in whole percent from 25 to 110: `,` `.` in practice step 5%
  (onto the 5% grid), `s` goes to full speed and again back to the kept speed. The
  now-playing card has − value + (a click on the value is `s`), the practice toolbar a
  slider for any percent (the only way to 1% steps, by Tim's choice), and a right-click
  on either gives presets 50 to 110% and the trainer's settings. Pitch is kept: Signalsmith
  Stretch at its full preset (Tim compared it with SoundTouch, "night and day"; the
  cheaper preset saves nothing worth it at 0.5% of a core). It sits after the loop and
  the stem mix (`audio::Stretched`), so loops, stems and beats stay in song time; at
  exactly 100% and no pitch shift samples pass straight through.
- **Pitch**, per song, apart from the speed: `k j` a semitone, `K J` 5 cents, up to an
  octave either way, through the same stretcher (`set_transpose_factor_semitones`). The
  practice toolbar has − value + by semitones; the wheel over it moves 5 cents and a
  right-click gives cents and "as recorded". `s` puts speed and pitch back to as
  recorded and again brings both back. A pitch change doesn't stop the trainer. It reports what it holds back (~0.1 s) so the playhead
  shows what's heard, and a soft limiter keeps its rare peaks (up to 1.8) under full
  scale.
- **The speed trainer** (`t` or the toolbar button; `T` or the speed menu for its
  settings sheet): with a loop on, it starts at "from" and adds "step" percent every
  N repeats up to "to", then stays. `Looped` counts its jumps back in `Shared::wraps`;
  `poll` asks `Trainer::percent` for the speed. Any hand on the speed stops it.
  Settings are global (in `settings.json`), default 70 to 100%, +5% every 2 repeats.
- **`g` toggles the beat grid** (the practice view marks beats with small chevrons at
  its top and bottom edges, faint until the grid is on, and shows the local tempo). On the grid `a b` and shift-click snap to the nearest beat, the widen and narrow buttons
  move an edge a whole beat, the slide buttons slide in beats, `h l` / `← →` step by
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
  the loop is exactly a section it renames that one). `dd` deletes the section in play:
  the loop's, else the shortest one around the playhead, which is also the one lit.
- **The marker**: `m` drops it at the playhead (on a beat with the grid), `m` again
  clears it from anywhere, and it drags like a loop edge (snapping, ⌥ freely; on a loop
  edge, grab it by its flag at the strip's top). Dropping it plays nothing. `o` plays from the marker, or from the loop start without one. One
  per recording, remembered like the loop, drawn as a gold flag on the waveforms and
  the progress bar. `m` and `o` work in both modes, with vim keys off too.
- **Variations**: drop files or a folder on a song (or the practice view), or `V` to
  pick; `v` cycles the song and its variations, keeping the position where they line
  up, `X` removes the playing one. Linked files leave the song list; songs with
  variations show `+N`. Each recording has sections and a marker of its own (keyed by its first
  file) and remembers its own loop: switching puts this one's away and brings the
  next one's back. Tim's choice, over sharing them across aligned recordings.
  Everything plays through one `audio::Mix`, a single file being a mix of one, so a
  stem set's `1`–`9` mute and `⌥1`–`⌥9` solo are instant; the practice view adds a
  lane per stem. A metronome stem starts muted. Each stem has a level: drag across its
  number and name (filled to the level) or scroll over them, or its right-click menu
  (100, 50, 25%, solo, mute). Levels are kept per stem set with the song's resume spot;
  mute and solo aren't kept.
- **Volume**: ⌥↑ ⌥↓ in 5% steps in every mode, and the slider beside the speed buttons
  (drag or scroll), from silent to full, no boost. Kept in settings.
- **The output follows the system**: every second riff asks which device should play
  (the one chosen in settings while it is connected, otherwise the system's) and, when
  that changed, reopens the output and carries on from the same moment, playing or
  paused. Opening the stems again takes a moment, so a switch isn't seamless. That
  check compares device ids only (about 35 µs). Never list devices with
  `output_devices()` on a timer: cpal asks every device which formats it plays, about
  half a second here, and doing that every second made playback drop out.
- **⌘, opens settings**: folders (Enter on "Add folder…" opens the macOS picker), the
  theme picker, which works like Jasmijn's (type to filter, ↑↓ previews, Enter keeps,
  Esc restores), vim keys on or off, and the output: Enter or a click opens a picker
  (the system's output, then each device, listed on a thread when it opens), ↑↓ or
  `j k` move, Enter or a click plays through it, Esc closes.
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
- **The queue and the sidebar** (plan 02): `q` queues the highlighted song, `Q` at the
  top (vim keys on or off; also the row's hover button and its right-click menu). Next,
  by any means, plays the queue first; previous from a queued song goes back to the
  list's song before it. The sidebar (⌘B or the title bar button, open unless Tim hid
  it) lists places, Library (the mouse's way out of an album or artist view, which
  show under it with an ×), Favourites and Playing, and the queue below: click to play
  now, hover for to-the-top and remove. `` ` `` puts the keyboard in the queue
  (`Mode::Queue`): `j k`, Enter, `t`, `dd` `x` ⌫, Esc / Tab / `` ` `` back.

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

## Plans

`docs/plans/` holds agreed plans; build from them step by step. **Plan 01** (two modes,
library and practice, with a keymap that stays on Tim's Glove80 base layer, then
volume, speed, pitch, undo, loop extras, bars and notes) is the current direction.
**Plan 02** (an EQ, band filters, the queue and a sidebar) runs beside it: steps 1 and
2 (the queue and sidebar) are built, 3 (band filters) and 4 (EQ) remain. Steps 1
(modes, keymap, right-click menus), 2 (volume, stem levels, output), 3 (speed and the
trainer) and 4 (pitch) are built; step 7 (bars) was dropped by Tim's choice; 5 (undo),
6 (lead-in and channel) and 8 (notes) remain.

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
- **Gapless playback, and blending** (Tim, 2026-10-08, backburner): no gap between
  songs that run into each other, and a toggle to fade subtly from the end of one song
  into the next. Something to experiment with, not a request yet.
