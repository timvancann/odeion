# Working in this repo

Riff is Tim's practice player: find a song in his music folder, play it, loop a bit, and
hand it to Transcribe! for real work. It exists because Transcribe!'s own file finding is
painful. **Transcribe! is the practice tool; riff is the way in.** Speed and pitch
control, waveforms and markers belong to Transcribe!. When a feature request drifts
there, say so before building it. The A/B loop is the deliberate exception.

A single GPUI 0.2.2 app (pinned, like jasmijn's desktop app, whose patterns this copies),
macOS only, no network, no accounts.

## Layout and where logic lives

Pure, tested modules hold the decisions; `view.rs` only maps keys and clicks onto them
and draws:

- `library.rs`: what a track is, reading one from its path, sort order, search, albums.
- `scan.rs`: walking the folder, merging with the cache, reading tags and covers on
  background threads.
- `audio.rs`: playback via rodio, and the A/B `Loop` rules.
- `playlist.rs`: next, previous and shuffle over the list a song was started from (the
  album, favourites, search results or library; the run of rows between headers).
- `keys.rs`: the keymap and the ⌘? sheet, see Keyboard.
- `wave.rs`: the practice view's waveform: read on 8 threads when a song starts (each
  placed by packet timestamps, with a second of decoder warm-up), summarised to one
  column per pixel, and which stretch the detail view shows.
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
  to the start, `o` play from the loop start, `f`/`m` favourite, `a` album, `t`
  Transcribe!, `s` shuffle, `?` keys, `dd` removes a folder in settings. No counts,
  by choice.
- **The ⌘ shortcuts always work**: ⌘P, ⌘F, ⌘A, ⌘T (the *highlighted* song), ⌘S,
  ⌘← ⌘→ or ⌘H ⌘L previous and next, ⌘, settings, ⌘? keys, ⌘Q. ⌘H is free because
  there is no menu bar to hide the app with. ⌃J ⌃K and arrows move; plain ← → seek.
- **Loop keys always work**: `[ ]` set the edges at the playhead, `\` clears, `{ }`
  widen the loop by 50 ms at the start or end and `( )` narrow it, `< >` slide the loop by its own length
  (phrase by phrase), `_ +` halve and double it. Every change plays around the edge it
  moved, to be judged by ear. On the progress bar, drag an edge or shift-click for the
  end. The rules live on `audio::Loop`.
- **`P` toggles the practice view**: the list gives way to the song's loudness contour
  (with the shown stretch boxed) over a large RMS waveform that frames the loop, or
  follows the playhead without one. `- =` zoom (⌘- ⌘= too). It is redrawn every
  frame while playing, the playhead smoothed between the player's position updates. Both take clicks and drags like the
  progress bar. The waveform is read for every song that starts, shown or not, and kept
  in memory only. Peaks are not drawn: on loud masters they all reach the top and hide
  the transients. A completed loop is framed once; after that the zoom is Tim's and
  only grows if the loop stops fitting. Loops have their own theme colour, `loops`: the
  first terminal colour (cyan, magenta, green, blue) that stands apart from the accent
  and text, since some themes' accent *is* their cyan.
- **⌘, opens settings**: folders (Enter on "Add folder…" opens the macOS picker), the
  theme picker, which works like Jasmijn's (type to filter, ↑↓ previews, Enter keeps,
  Esc restores), and vim keys on or off.
- GPUI apps get no menu bar, so ⌘Q and every other ⌘ shortcut are handled by hand, and
  none collide with system menu items.
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
his keyboard focus.

Before committing: `cargo test && cargo build --release`, no warnings. Work on `main`;
commit signing is off for this repo.

## Possible features

Ideas Tim liked but hasn't asked for yet. Check with him before building one.

- **Beats in the practice view**, next up: onset strength (spectral flux, or SuperFlux
  for guitar) plus Ellis's dynamic-programming beat tracker, computed with the waveform,
  then loop edges snapping to beats. Prototype outside riff first and judge the ticks
  by eye on real songs; neural trackers (Beat This!) are the heavier fallback.
- **A beat grid**, after beats: `b` toggles it. With it on, `[ ]` snap to the nearest
  beat, `{ } ( )` move an edge a whole beat, dragging snaps (⌥ drags freely), and
  `h l` / `← →` step by beats instead of 5 s. Back works like previous song: more than
  ~150 ms into a beat goes to its start, at its start goes to the beat before. Bars
  (shift-step, bar-sized loops) only if the tracker finds downbeats.
- **Read Transcribe!'s loops** from the `.xsc` next to the song, read-only, so loops made
  there appear in riff. The format still needs looking at.
- **Loops aligned to beats**, as Moises does with beat detection. This is Transcribe!
  territory, so only if it stays small.
