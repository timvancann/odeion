# Working in this repo

Odeion (called riff until 2026-11-08, when Tim renamed it: "riff" was already the name
of many repos and apps) is Tim's practice player: find a song in his music folder, play it, and practise
it: loops to the sample, a marker, a waveform with beats, named sections, slow-down, and other
recordings of the song (backing tracks, stem sets) to switch between. It began as a way
into Transcribe! (whose file finding is painful) and grew into the practice tool, by
Tim's choice. The Transcribe! button was removed on 2026-10-08: Odeion now does
everything Tim used it for. Practice features
are in scope; ask before adding anything that turns Odeion into an editor or a DAW.

A single GPUI app on `gpui-pre` 0.3.9, the maintained snapshot of Zed's GPUI (pinned
exactly, the version rsdisk and `gpui-component` 0.7.3 use; renamed to `gpui` in
`Cargo.toml` so code says `gpui::`). Jasmijn's desktop app, whose patterns this copies,
moves to the same version. Odeion is made on and for macOS, with a Windows build in
trial (CI builds `odeion.exe`; Ctrl stands in for ⌘), no accounts, and no network except
one request when the user chooses Check for Updates (`update.rs`, Tim 2026-11-08):
never on its own, nothing about the user sent.

**Odeion is going public** (Tim, 2026-10-09), source-available under the PolyForm
Noncommercial 1.0.0 licence (Tim, 2026-11-07, replacing the GPL-3.0: free for
noncommercial use, commercial use needs his permission). Tim is still its
main user and the keymap and design stay his, but the code is written for other people
too:

- Nothing personal in the code: no paths, names or accounts of Tim's. Defaults suit
  anyone (Odeion starts with no music folder and asks for one); Tim's setup lives in his
  `settings.json`, never in the source.
- `README.md` is for people who find the repo: what Odeion does, its inspiration and
  acknowledgements, no keymap (the ⌘? sheet is the reference). A feature that changes
  what Odeion does updates the README in the same commit, and a new dependency or
  borrowed algorithm gets a line in its acknowledgements.
- Screenshots in `docs/screenshots/` (`library.png`, `stems.png`) are Tim's to take.
- Bundled assets need a licence that allows it, with the licence file beside them
  (the fonts' OFL; Lucide's ISC for the icons).

## Layout and where logic lives

Pure, tested modules hold the decisions; `view.rs` only maps keys and clicks onto them
and draws:

- `library.rs`: what a track is, reading one from its path, sort order, search, albums,
  and the grouped list view: grouping under artists (`grouped`) and which artist
  stays pinned on top (`pinned`).
- `scan.rs`: walking the folder, merging with the cache, reading tags and covers on
  background threads. A listed folder that can't be opened (gone, an unplugged drive,
  macOS privacy) is reported with why (`scan::why`), never silently skipped; folders
  and files below it that can't be opened are passed over without a message. Either
  way the songs known there stay listed as they were (`Walked::missed`), never taken
  for deleted: a Drive folder that didn't answer once would otherwise lose its songs'
  tags and covers to a slow re-read.
- `empty.rs`: what Odeion says where there is nothing to show, each case in its own
  words: the empty list (no folder, first scan with a live count, unreadable folders,
  no songs with the formats Odeion plays, a search finding nothing, an empty playlist;
  `empty::why` picks which), the practice view before a waveform, no loop, the
  now-playing card before the first song, and the can't-read and can't-play messages.
  Keys come in from `keys::keys_for`, never written there.
- `audio.rs`: playback via rodio, and the A/B `Loop` rules. The loop is kept inside
  the audio stream (`Looped`), to the sample, and that wrapper also keeps the playing
  position, since rodio's player can't see a jump made inside it. Checking from the
  window every 40 ms let the music run up to 40 ms past the loop's end. Also the
  volume, the stems' levels (`stem_gains`) and the output device (`wanted`, followed
  by `Audio::follow_output`).
- `playlist.rs`: next, previous and shuffle over the list a song was started from (the
  album, favourites, search results or library; the run of rows between headers),
  with the queue first: while a queued song plays the list stays where it was.
- `playlists.rs`: playlists, named lists filled song by song, kept in
  `playlists.json` (gone songs stay in the file, unlisted).
- `queue.rs`: the queue, songs to play next (`q` to the end, `Q` to the top, each song
  once), kept in `session.json`.
- `keys.rs`: the keymap per mode and the ⌘? sheet, see Keyboard.
- `menu.rs`: the right-click menus: their entries and moving through them by key.
- `eq.rs`: the global EQ (ten Winamp bands and a preamp, ±12 dB), its presets
  (built in and saved, in `settings.json`) and the peaking filters' maths. It plays
  as `audio::Equalized`, last in the sound after the stretcher.
- `speed.rs`: the speed in whole percent, 25 to 110, and the pitch in cents, up to an
  octave either way; both kept by a song while it plays as recorded.
- `trainer.rs`: the speed trainer: from, to, step, every N repeats, turned into a
  speed from how often the loop has come round.
- `wave.rs`: the practice view's waveform: read on 8 threads when a song starts (each
  placed by packet timestamps, with a second of decoder warm-up), summarised to one
  column per pixel, and which stretch the detail view shows. At most `MOST` points
  (64 MB): a file claiming a longer length is read by its packets and stops there.
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
- `session.rs`: what Odeion showed when it closed (`session.json`): the playing song,
  the highlighted one, practice, album or artist view, grid and zoom. Odeion opens that
  way, the song paused where it was (its recording, loops and playhead come from
  `resume.json`), never playing on its own. Saved within a second of a change, on ⌘Q
  and when the window closes. Closing the window quits Odeion (`QuitMode::LastWindowClosed`
  in `main.rs`): GPUI's macOS default left a windowless process with no way back.
- `regions.rs`: sections (still "regions" in the code and `regions.json`): named
  stretches per recording, in order; renaming the same stretch, dragging an edge,
  which one is current, next and previous, removal.
- `media.rs`: the media keys and Now Playing via souvlaki. macOS sends media keys to the
  app that last reported a song, not as key presses, so this is the only way in.
- `state.rs`: paths, settings, favourites, and the data folder: the default, or one
  chosen in settings, named by a `data-folder` note in the default folder and read once
  per run. The default is each system's usual place (`odeion` in Application Support,
  `%APPDATA%` or `~/.local/share`); the old `riff` folder beside it moves there once
  (`move_from_riff`), and covers are found again beside a moved cache
  (`scan::load_cache`). Every JSON file of Odeion's loads through `state::load_json`: one that can't
  be read is renamed `<name>.broken` before Odeion starts it afresh, and Odeion says so
  when it opens, so a save never overwrites what it held. Quitting (⌘Q or closing the
  window) is held back when saving fails, the error showing; another quit within 10 s
  (`QUIT_AGAIN`) goes ahead. A session that failed to save is tried again.
- `theme.rs`: colours by role (`bg`, `selection`, `accent_text`, `loops`, `marker`...).
  Two schemes of Odeion's own, each a Light and a Dark picked by Appearance: Auto
  (follows macOS live), Light or Dark; Auto is the default. **T3**, the default for new
  installs (Tim, 2026-11-08): T3 Code's zinc greys and primary blue, read from its
  stylesheet; the played wave is the blue, so its sections are amber. **Gold** (riff's
  own scheme before the rename, called Gold in the oldest settings too): Jasmijn's gold (Tim approved these palettes, the red marker and the cyan loop on
  2026-11-07, from screenshots); settings files from before T3 keep it. Saved as "T3"
  or "" (`Theme::saved`, `is_own`). Installed Zed themes are editor themes
  (experimental): only their surfaces, text and an accent that shows and stands apart
  from the practice marks; loop, sections, marker, playhead and waveform always come
  from Gold's palette of the same lightness. Tests hold every palette of Odeion's own to
  the contrast minimums (body text 7:1, dim 4.5:1, marks 3:1) and its marks apart with
  red-green colour blindness. `view.rs` only reads roles.

New behaviour gets its rule and tests in one of these, not in `view.rs`.

## The music library

Odeion scans the folders listed in ⌘, settings (`settings.json` in the data folder below),
none at first: the empty list asks for one (a button, or Enter). A track's Artist/Album
path is read from the deepest listed folder holding it. Tags and embedded covers win
over the path. A listed folder a scan can't open shows an error ("can't read
~/Music: ...") and "can't read" beside it in settings.

Tim's library, the one Odeion is tested on, is a Google Drive folder laid out
`Artist/Album/NN Title.ext`, about 2,700 tracks of mp3, flac, wav and aiff, with
correct tags and covers.

- **Files may be cloud-only.** Drive streams on demand: reading a cloud-only file's
  header took 1.6s and pulled about 4 MB. A full tag scan is then slow, and that is
  expected, not a bug. Tim has made the folder available offline, but new music arrives
  cloud-only.
- **The cache makes startup instant.** `~/Library/Application Support/odeion/` holds
  `library.json` (tracks, re-read only when size or mtime changes), `covers/` (256px
  JPEG thumbnails named by content hash) and `favorites.json`. Deleting `library.json`
  forces a full rescan.
- AIFF needs rodio's `symphonia-aiff` feature; the default features leave it out.

## Keyboard

Keyboard first, every control also clickable. The keymap is Tim's, settled by trying it
(plan 01 has the reasoning). `keys.rs` holds it: a key becomes an `Action` through the
current mode's map, `view.rs` performs it, and `SHEET` beside the map is the ⌘? sheet,
each group listing the modes it shows in. Tests check, for each keyboard, that every
key on the sheet does something in its modes and no key is listed twice in a mode, so
add a key to both or they fail.

- **Two keyboards** (Tim, 2026-11-07, for open sourcing), a settings row ("Keyboard:
  Mac / Vim", `settings.json`'s `keyboard`): **Mac**, the default, and **Vim**. Tim's own
  file says Vim (an old file's `vim: true` reads as Vim, `false` as Mac). Each mode's map
  is the keys both share plus the keyboard's own, and the sheet line is (shared, Vim,
  Mac, what), so the sheet and tooltips show the current keyboard's keys. The ⌘ layer
  is the same in both; they differ only in bare letters and plain arrows. With the Mac
  keys a letter or digit typed in the library starts a search with it
  (`keys::starts_search`; Space and symbols like `+` `` ` `` `?` keep their commands),
  and the vim letters below don't exist. A key added to both goes in the shared column;
  plain arrows never replace Vim's hjkl.
- **Modes, like vim**: the library (finding and playing songs; Odeion opens here unless
  it was left in practice), search inside it, and practice (the practice view; the mode
  *is* the view). Tab or ⌘2 enters practice, Esc, Tab or ⌘1 leaves (Vim's `p` both); the title bar's
  Library / Practice switch does the same. Typing on the ⌘? sheet finds keys
  (`keys::found`). The ⌘? sheet and every tooltip show the
  current mode's keys. There is no hint bar at the bottom (removed 2026-10-08, the
  buttons' tooltips say the keys); messages ("queued, 3rd") show for a few seconds on
  the now-playing card in the loop's place, errors for ten (a song that won't open:
  "couldn't play <title>: <why>"). Every primary key sits on the base layer of Tim's Glove80 (shift
  counts); brackets, braces and parentheses take two hands there, so they're off the
  sheet. A test checks Vim's practice sheet.
- **Everywhere outside typing**: Space pauses, `← →` seek, ⌥← back to the start; in
  practice (and Vim's library) `0` too, and `m` and `o` the marker.
- **Search is a mode** (⌘F or ⌘P; with the Mac keys just typing, with Vim `/` `i`).
  ⌘F only goes into the search (already there, nothing changes); ⌘P and Esc leave it.
  Typing goes to the query only while searching; outside it bare keys are commands
  (or, with the Mac keys, a letter starts a new search), even with results showing.
  Space never types outside search, so it always pauses: the problem with the old
  "type anywhere to search". ⌘V pastes the clipboard as one line (`keys::one_line`)
  into the search, starting one from the library, and into every name being typed.
  The search is contextual (Tim, 2026-10-09): with an album, artist or playlist open
  it finds only among its songs, in their order, the bar saying "in <name>"; its × or
  ⌫ on an empty search widens it to everything, and Esc clears it before leaving the
  view. A × in the field clears what's typed, as Esc does (so it stays in the album). Opening an album or artist from results puts the search aside and closing
  brings it back. Over everything, playlists whose names match are listed first
  (`library::named`), Enter or a click opening one.
- **Vim's library letters**: `j k`, `gg G`, `⌃U ⌃D`, `zz` to the playing song, `h l`
  seek, `H L` previous and next song, `f` favourite, `a` album, `A` artist, `/ i`
  search, `s` shuffle, `p` practice, `q Q` queue, `x` out of the playlist, `dd`
  removes a folder in settings. No counts, by choice. Practice keys are the mode's own
  and mostly shared; Vim keeps `h l` seek, `k j K J` pitch, `p` and `dd` there, the Mac
  keys have ↑ ↓ ⇧↑ ⇧↓ pitch and `l` for the loop.
- **The ⌘ shortcuts always work, the same in both**: ⌘F search, ⌘P search on or off,
  ⌘D favourite, ⌘A album, ⇧⌘A artist, ⌘S shuffle, ⌘L to the playing song (in
  practice, Follow), ⌘← ⌘→ previous and next, ⌘H hides Odeion (`cx.hide()`), ⌘1 ⌘2
  library or practice, ⌘O add a variation, ⌘N add to a playlist, ⌘E or ⌥⌘E the EQ,
  ⌘B or ⌃⌘S the sidebar, ⌘, settings, ⌘? keys, ⌘Q. In the library and search, arrows
  and ⌃J ⌃K move, Home End or ⌘↑ ⌘↓ go to the top or bottom, ⌥Enter queues and
  ⇧⌥Enter queues at the top.
- **Loop keys are practice keys**: `a` `b` set the edges at the playhead (`[ ]` quietly
  too), `\` or ⌥⌘L (Mac keys: `l` too) switches looping off and on keeping the edges (the loop button too, once
  both are set; placing an edge switches it back on; with no loop either says how to
  make one instead), `|` or ⌫ clears. Widening or narrowing
  by 50 ms and sliding the loop by its own length (phrase by phrase) are buttons on
  the loop bar only, by Tim's choice: `← →` (Vim `h l`) move the playhead and `a` or `b` again puts
  the edge there. Halving and doubling were tried and removed: Tim never used them.
  No change moves the playhead (Tim, 2026-10-07: jumping to the edge was jarring); it
  carries on, wrapping at the loop's end as usual, until `o` plays from the loop's
  start. Playing a section still starts it. On the progress
  bar, drag an edge or shift-click for the end. On the detail waveform, a drag across
  empty waveform draws the loop, as in Transcribe! and Moises: both ends at once,
  either direction, snapped to beats on the grid (⌥ freely), looping, the playhead
  left alone and the zoom too. It shows live and replaces the loop on release; Esc
  lets go. Under 4 px of movement (`audio::SWEEP`) or 100 ms of loop (`MIN_LOOP`) it
  is a click and seeks. The rules live on `audio::Loop` (`Loop::swept`).
- **Right-click menus** (`menu.rs`) wherever a Mac user would look for one: a
  waveform or the progress bar (loop starts or ends here, marker here, all at the
  moment clicked; inside the loop also save it as a section and clear it; on the
  marker's flag, play from it and clear it), a section (loop it, rename, delete), a
  song row (favourite, album, artist, queue, add to a playlist, add a variation, and
  in a playlist remove and move; the row is highlighted), a queued song (play now, to
  the top, remove, album, artist, add to a playlist), the now-playing card (the
  playing song's entries, then its loop and marker while it has them), the loop bar
  (on or off, replay, save, clear; without a loop, its edges at the playhead), a
  playlist in the sidebar or the search results (open, play, shuffle, queue all,
  rename, delete), Favourites in the sidebar, an artist's row in the grouped list, and
  the album, artist and playlist headers (play, shuffle, queue all, add all to a
  playlist, back, or rename and delete). Rename and delete open the playlist, whose
  header asks for the name or "for good". Thin lines set groups apart (`Menu::then`).
  Each entry is an `Action` showing its keys, done on what was clicked rather than at
  the playhead. They open at the pointer, take `j k` / ↑ ↓ and Enter, and close on Esc
  or a click elsewhere, which does nothing else. A ctrl-click opens them too, with no
  code of Odeion's: GPUI's macOS layer turns a ctrl-left press into a right press. A
  right-click on something inside another surface with a menu stops there
  (`opens_menu`), so the speed control and progress bar don't also open the card's.
- **The practice view**: the list gives way to the song's loudness contour
  (with the shown stretch boxed) over a large RMS waveform that frames the loop while
  the playhead is in it, and otherwise follows the playhead (Tim, 2026-10-29: Follow
  showed a loop the playhead had left, so it seemed not to work). `- =` zoom (⌘- ⌘= too, and the
  wheel over either waveform); ⌘0 zooms to fit the loop, or the whole song without one
  (`wave::zoom_fit`). Dragging the box on the contour moves the detail view
  without moving the playhead (Tim, 2026-10-29: the view was tied to the playhead); it
  stays there until the toolbar's Follow, a section, `o` or another song, and a click
  in the box that doesn't move still seeks (`wave::pan`, `Odeion::panned`).
  Under the waveform, the controls Tim reaches for with a guitar on his lap are large
  buttons with names (`Odeion::big`, 44 px): previous section, section start (the
  section in play's start, else the loop's, never the marker: `Regions::restart`),
  play/pause, next section, loop, and replay (named for where `o` goes); the finer loop
  tools stay icons beside them. Section start has no key yet; Follow is ⌘L. Over the
  detail waveform the cursor is an I-beam, for drawing a loop. Over a loop edge it becomes a resize arrow
  (`edge_grips`, which only set the cursor; the strip under them takes the press), 24 px
  across (`GRAB`), as is every press area (`HIT`), though what's drawn may be thinner. It is redrawn every
  frame while playing, the playhead smoothed between the player's position updates. Both take clicks like the
  progress bar; a drag on the contour's box moves the view, one on the detail draws a loop. The waveform is read for every song that starts, shown or not, and kept
  in memory only. Peaks are not drawn: on loud masters they all reach the top and hide
  the transients. A completed loop is framed once; after that the zoom is Tim's and
  only grows if the loop stops fitting. The loop is sky cyan (`loops`), sections a
  deeper blue (`section`), the marker red, chosen to stay apart from each other and the
  played gold even with red-green colour blindness; shape tells them apart too: loop
  edges are brackets (ticks pointing into the loop), the marker a flag, the playhead a
  plain line. Gold on the waveform means only played; the overview's box is neutral.
- **Songs resume where they were left**: recording, slow-down and loops always; the
  playhead only when Tim picks the song himself (a song following on in a list starts
  at the top), and not when it was left within 5 s of the end. Slow-down is per song.
  Kept on leaving a song, switching recording or speed, every 5 s of playing, ⌘Q and
  closing the window.
- **Speed**, per song, in whole percent from 25 to 110: `,` `.` in practice step 5%
  (onto the 5% grid), `s` goes to full speed and again back to the kept speed. The
  now-playing card has − value + (a click on the value is `s`), the practice toolbar a
  slider for any percent (the only way to 1% steps, by Tim's choice), and a right-click
  on either, or the card's chevron, gives presets 50 to 110% and the trainer's settings. Pitch is kept: Signalsmith
  Stretch at its full preset (Tim compared it with SoundTouch, "night and day"; the
  cheaper preset saves nothing worth it at 0.5% of a core). It sits after the loop and
  the stem mix (`audio::Stretched`), so loops, stems and beats stay in song time; at
  exactly 100% and no pitch shift samples pass straight through.
- **Pitch**, per song, apart from the speed: a semitone on ↑ ↓ (Vim `k j`), 5 cents on
  ⇧↑ ⇧↓ (Vim `K J`), up to an
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
  move an edge a whole beat, the slide buttons slide in beats, `← →` (Vim `h l`) step by
  beats (back restarts the beat you're in first), dragging snaps (⌥ drags freely), and so does a click that moves the playhead (⌥-click
  doesn't).
  Off, everything is as before; songs without a steady beat should stay off.
- **Sections and the loop are separate things** (Tim, 2026-10-05: they were too tied
  together). A section is a named, saved stretch of the song; the loop is the one
  unnamed stretch that repeats. Playing a section (a click, `n` `N`) sets the loop to
  it, and from then on they're apart: the loop can shrink inside the section or move
  off it without changing it. Sections change only by dragging their edges in the
  section lane (snapping on the grid, ⌥ freely), by 24 px grips inside each section; a
  press on a grip that doesn't move plays the section, so a narrow one still plays on
  a click. `r` saves the loop as a section in a
  dialog (Enter saves, an empty name saves one saying where it is, Esc cancels; when
  the loop is exactly a section it renames that one). Vim's `dd` deletes the section in play:
  the loop's, else the shortest one around the playhead, which is also the one lit.
  The Mac keys have no key for it (Tim's choice): the delete button and the section
  menu's Delete, which ask "for good" beside the lane first; `dd` is two presses already.
- **The marker**: `m` drops it at the playhead (on a beat with the grid), `m` again
  clears it from anywhere, and it drags like a loop edge (snapping, ⌥ freely; on a loop
  edge, grab it by its flag at the strip's top). Dropping it plays nothing. `o` plays from the marker, or from the loop start without one. One
  per recording, remembered like the loop, drawn as a red flag on the waveforms and
  the progress bar. `m` and `o` work in practice with either keyboard, and in Vim's
  library (the Mac library types them).
- **Variations**: drop files or a folder on a song (or the practice view), or `V` (⌘O
  anywhere) to pick; `v` cycles the song and its variations, keeping the position where they line
  up, `X` removes the playing one (asking first: a second `X` within 4 s, Tim lost a stem set to one press; from the toolbar's ×, a second click, and the question says so). The files stay on disk. Linked files leave the song list; songs with
  variations show `+N`. Each recording has sections and a marker of its own (keyed by its first
  file) and remembers its own loop: switching puts this one's away and brings the
  next one's back. Tim's choice, over sharing them across aligned recordings.
  Everything plays through one `audio::Mix`, a single file being a mix of one, so a
  stem set's `1`–`9` mute and `⌥1`–`⌥9` solo are instant; the practice view adds a
  lane per stem. A metronome stem starts muted. Each stem has a level: drag across its
  number and name (filled to the level; the level moves with the drag, so a click
  alone leaves it) or scroll over them, or its right-click menu
  (100, 50, 25%, solo, mute). Levels are kept per stem set with the song's resume spot;
  mute and solo aren't kept.
- **Playlists** (plan 03): `+` or ⌘N adds the highlighted song (the playing one in
  practice) through a picker (each playlist, a check where the song is already, and
  "New playlist…" named in place); the song menu has "Add to playlist…" too, and the
  now-playing card a button doing as `+` does. The
  sidebar lists them; a click opens one in the list under a header (rename in place,
  delete asks "for good"). Names typed in place (a new playlist, a rename, an EQ
  preset) have Save and Cancel buttons beside them, and a click elsewhere cancels. In its view ⌫ (Vim `x` too) removes a song, ⇧↑ ⇧↓ (Vim `K J` too) move it,
  also from the row's menu and from its up, down and × buttons (shown like the queue
  buttons, on the highlighted row or under the pointer). The picker also takes a whole
  list from a menu's "Add all to playlist…". Enter plays from that song through the playlist. The
  session keeps an open playlist by name.
- **The EQ** (⌘E, ⌥⌘E or the title bar's sliders button, lit while it does something):
  a sheet with on/off, the preset by name (‹ › to step, save, remove) and a vertical
  slider per column, the preamp first. Drag, wheel or double-click to 0; ← → or `h l` pick
  a column, ↑ ↓ or `k j` a dB, `0`, `n N`, `e`, ⌘S or `w` (type a name, Enter or ⌘S), `x`. Global, for
  the speakers and headphones; flat or off, samples pass straight through.
- **Volume**: ⌥↑ ⌥↓ in 5% steps in every mode, and the slider beside the speed buttons
  (drag or scroll), from silent to full, no boost. Kept in settings.
- **The output follows the system**: every second Odeion asks which device should play
  (the one chosen in settings while it is connected, otherwise the system's) and, when
  that changed, reopens the output and carries on from the same moment, playing or
  paused. Opening the stems again takes a moment, so a switch isn't seamless. That
  check compares device ids only (about 35 µs). Never list devices with
  `output_devices()` on a timer: cpal asks every device which formats it plays, about
  half a second here, and doing that every second made playback drop out.
- **⌘, opens settings**: folders (Enter on "Add folder…" opens the macOS picker),
  Appearance (Enter or a click cycles Auto, Light, Dark; it means Odeion's own theme, so
  from an editor theme the first press goes back to Odeion's own), the
  theme picker (T3 and Gold first, then "Editor themes (experimental)"), which works like Jasmijn's (type to filter, ↑↓ previews, Enter keeps,
  Esc restores; it shows 8 themes, so the wheel over them moves the highlight too, and
  a click keeps one), the keyboard (Mac or Vim; Enter, Space or a click switches), and
  the output: Enter or a click opens a picker
  (the system's output, then each device, listed on a thread when it opens), ↑↓ or
  `j k` move, Enter or a click plays through it, Esc closes.
  Last, the data folder (for Tim's demos, kept apart from his own data): pick another
  folder or go back to the default, and Odeion saves where everything is, then reopens
  from the new folder (`open -n` on the bundle, or the binary again from source).
- **Every action has a button too**, built with `Odeion::tool` (an icon, a tooltip and
  the action, done through `Odeion::act` exactly as its key would). Tooltips list keys
  from `keys::keys_for`, which reads the ⌘? sheet through the real keymap, so they stay
  right if keys are ever remapped; never write a key into a tooltip by hand. A control
  that isn't an action (a sheet's ×, a menu opened by a click, a delete that asks
  first) is `Odeion::button`, its tooltip still naming the keys that do the same. Every
  sheet has a × in its header. Icons live
  in `assets/icons/`: Lucide's where one fits, Odeion's own (loop edges, widen, narrow,
  slide, beats, sections) drawn in the same stroke style from the loop's brackets. The
  design is the "Riff controls and icons" canvas (named before the rename).
- The menu bar has one menu, Odeion (`main.rs`, `menu_bar`, Tim 2026-11-08): the
  version, Check for Updates… (asks GitHub's API for the latest release on a
  background thread, then a native dialog says "You're on the latest version" or offers
  the Mac zip with Download; `update.rs` compares versions, `view.rs` only asks and
  shows), Settings…, Keyboard Shortcuts, Hide and Quit. Its entries
  aren't bound to keys, so they show no shortcuts: ⌘Q, ⌘M (minimise), ⌃⌘F (full
  screen) and every other ⌘ shortcut are handled by hand, as before, and none collide
  with system menu items. ⌘H hides Odeion through `cx.hide()`.
- **The letter rail** beside the full library (not in search, album or artist views):
  click or drag along `# A–Z` and the list glides (220 ms, eased, by setting the scroll
  offset each frame) to that letter's first artist below the favourites, highlighting
  it. Letters follow the sort (`library::initial`), so a jump never lands out of order.
- **Two list views**, switched in the library's title bar and kept in settings (no
  key yet): *columns* (cover, title, artist, album on one 44 px line, artist and
  album alike in the dim small type, by Tim's choice) and
  *grouped* (the full list and favourites under a row per artist with its song count,
  songs indented with their covers, without the artist; the top artist stays pinned until the next one
  pushes it up, Tim's condition for this view). Search, album and artist views aren't
  grouped; album and artist views drop the columns their header already names. Artist
  rows are part of a section's run, so playing a song still queues up the whole list.
- **The list has a scrollbar like Zed's**, drawn by Odeion (GPUI has none): a thin
  thumb shown while the list moves and for a second after, or with the pointer on it.
  Dragging the thumb scrolls directly; a click elsewhere on the track glides there.
- **The queue and the sidebar** (plan 02): ⌥Enter queues the highlighted song,
  ⇧⌥Enter at the top (Vim's `q` `Q` too; also the row's hover buttons, shown on the highlighted row,
  and its right-click menu). Next,
  by any means, plays the queue first; previous from a queued song goes back to the
  list's song before it. The sidebar (open unless Tim collapsed it; ⌘B, ⌃⌘S or its own
  top-left button) lists places, Library (the mouse's way out of an album or artist
  view, which show under it with an ×), Favourites and Playing, and the queue below
  with covers: click to play now, hover for to-the-top and remove (drawn over the
  title's end, so titles keep the width; the row's tooltip says they're there). Its
  clear asks "clear all" once more. Collapsed, as in VS Code, only the icons
  stay, the queue's with its count. `` ` `` puts the keyboard in the queue
  (`Mode::Queue`): ↑ ↓ (Vim `j k`), Enter, `t`, `x` ⌫ (Vim `dd`), Esc / Tab / `` ` `` back.

## GPUI traps hit here

- **The selection is a row index**, which means nothing in a new list. Take the
  selected path *before* replacing or re-sorting tracks and pass it to `reindex`.
  Getting this wrong made the highlight jump to a random song after the scan.
- Rows in a `uniform_list` need `w_full()` or they shrink to their text.
- **The app comes from `gpui-pre-platform`** (`gpui_platform::application()`):
  `gpui::Application` has no `new()` since 0.3. That crate needs its `font-kit`
  feature, or macOS draws no text at all. It also has `runtime_shaders`, which
  compiles the Metal shaders when Odeion starts instead of at build time: the build-time
  path needs the Metal toolchain, and `metal` failed here ("missing Metal Toolchain")
  even with the component installed. Drop the feature once `xcrun metal --version`
  works.

## Running and checking

`just run` runs from source; `just install` builds `Odeion.app` (icon rendered from
`bundle/icon.svg` with resvg) into `~/Applications`. A pushed `v*` tag matching
`Cargo.toml`'s version publishes a universal `Odeion-<version>.zip` as a GitHub release
(`.github/workflows/release.yml`), signed and notarized once the Apple secrets are set;
`docs/releasing.md` has the steps.

Tim uses Odeion while you work on it. **Leave his running instance alone**: restarting it
cuts off whatever he is practising. Build, then tell him to restart. Playing audio
makes noise in his room, so verify decoding without an output device (rodio's
`Decoder` plus `try_seek` in a throwaway test) and leave listening to him. To see the
UI, screenshot the window with `screencapture -l<window id>`; driving keys would steal
his keyboard focus. So does merely opening a second Odeion window: whatever Tim is typing
elsewhere then lands in it as Odeion keys (a stray `1` muted a stem, a `v` switched
recordings). Only open one when he isn't typing, or ask first.

Before committing: `cargo test && cargo build --release`, no warnings (the
future-incompatibility note about the `block` crate comes from a dependency). Work on
`main`; commit signing is off for this repo.

## Plans

`docs/plans/` holds agreed plans; build from them step by step. **Plan 01** (two modes,
library and practice, with a keymap that stays on Tim's Glove80 base layer, then
volume, speed, pitch, undo, loop extras, bars and notes) is the current direction.
**Plan 02** (an EQ, band filters, the queue and a sidebar) runs beside it: steps 1 and
2 (the queue and sidebar) and 4 (the EQ) are built; 3 (band filters) remains. Steps 1
(modes, keymap, right-click menus), 2 (volume, stem levels, output), 3 (speed and the
trainer) and 4 (pitch) are built; step 7 (bars) was dropped by Tim's choice; 5 (undo),
6 (lead-in and channel) and 8 (notes) remain.

## Possible features

Ideas Tim liked but hasn't asked for yet. Check with him before building one.

- **Grid corrections**, later (Tim skipped them for now, 2026-10-03): shift the grid by half a beat (funk read on the
  offbeats) and halve or double the tempo (Neon Knights read at 95, Wildest Dreams at
  200), applied from the playhead on. Then perhaps tap tempo. Neural trackers (Beat
  This!) only if corrections are needed too often; they would also bring bars.
- **Loops aligned to beats**, as Moises does with beat detection, only if it stays
  small.
- **Gapless playback, and blending** (Tim, 2026-10-08, backburner): no gap between
  songs that run into each other, and a toggle to fade subtly from the end of one song
  into the next. Something to experiment with, not a request yet.
- **Tags and covers from MusicBrainz** (Tim, 2026-11-08, on the README's roadmap): look
  up songs or an album with missing tags or no cover on MusicBrainz and the Cover Art
  Archive (by the MusicBrainz IDs already in the tags when there are some), show the
  match, and write it into the files only after the user confirms. It breaks the
  README's "Odeion never writes next to your songs" and is network access, so both lines
  change with it. Tags in FLAC are Vorbis comments with a PICTURE block, not ID3; the
  audio must stay byte-identical (FLAC's stored MD5 checks that). MusicBrainz asks for a
  descriptive User-Agent and at most one request a second.
- **Stems from Moises, in Odeion** (Tim, 2026-11-07, waiting on API access): split a
  song into stems through the Moises developer API (now Music.AI) and add them as a
  variation. Bring your own key: each user's key in the macOS Keychain, never in
  `settings.json` or the source, and the feature off until a key is set. It would be
  Odeion's first network access beyond the update check, so the network lines here and
  in the README change with it; nothing uploads without a deliberate action on one song, after saying the
  file goes to Moises. Downloaded stems are untrusted files: Odeion names them, caps
  their size and checks they decode before linking them.
