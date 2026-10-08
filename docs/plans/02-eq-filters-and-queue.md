# Plan 02: an EQ, band filters, a queue and a sidebar

Drafted with Tim on 2026-10-08, from comparing riff with Winamp and other good players.
Two independent parts: **sound** (a global 10-band EQ and per-song band filters) and
**the queue** (play these next, shown in a collapsible sidebar that also gets you back
to the full list with the mouse). Either can be built first.

Out of scope: saved playlists, dragging to reorder the queue, a parametric EQ, a
spectrum display, an EQ per song, the waveform showing the filtered sound, gapless
playback and blending between songs (both noted under "Possible features" in AGENTS.md).

## Decisions

**EQ shape.** A 10-band graphic EQ like Winamp's (31 Hz to 16 kHz in octaves, ±12 dB
per band in 1 dB steps, plus a preamp), with built-in presets and Tim's own saved
ones. Global: it's there to make the speakers and headphones sound right, not to
practise with. Rejected: 5 bands (too coarse) and a parametric EQ (fiddly, DAW
territory).

**Band filters.** Instrument presets that one key cycles through (bass, guitar,
vocals, no bass, off), each a low cut and a high cut, plus a range slider in the
practice toolbar to set both cuts freely. Kept per song with the speed, since
whether a song needs one depends on the song. Rejected: resetting on every song, and
sliders only.

**Where they sit in the sound.** Mix → Looped → **filter** → Stretched → **EQ** →
output. The filter comes before the pitch shift, so "bass" keeps following the bass
when the song is transposed. The EQ comes last, because it colours what you hear.

**The queue.** A list of songs to play next, ahead of the list the playing song came
from. `q` adds the highlighted song to the end, `Q` puts it at the top. The queue
can be edited by keyboard and mouse: kick a song out, move it to the top, play it
now. It's kept across restarts. No duplicates: `q` on a song already queued says so
and changes nothing; `Q` on one moves it to the top.
Rejected for now: drag to reorder (moving to the top covers the need).

**The sidebar.** A collapsible panel on the left, in both modes, toggled with ⌘B
(Zed's key, free here) and a title-bar button. One open/closed setting, kept in the
session, open the first time (Tim, 2026-10-08, to get a feel for it). It holds the navigation (so album and artist views can be left with the
mouse) and the queue below it.

## 1. The EQ

### 1.1 Rules (`eq.rs`, pure)

- Bands at 31, 62, 125, 250, 500, 1k, 2k, 4k, 8k and 16k Hz, each a peaking biquad
  (RBJ cookbook) with Q = 1.41 (about an octave wide). Gains are whole dB from −12 to
  +12; the preamp too.
- `Eq { on: bool, preamp: i8, bands: [i8; 10] }`. It's **flat** when it's off, or
  when the preamp and every band are 0. Flat passes samples through untouched, like
  `Stretched` at 100%.
- Built-in presets: Flat, More bass, Less bass, More treble, Less treble, Voice,
  Loudness (a smile: lows and highs up, mids flat). The exact curves are set by ear
  when the step is built. As built (preamp, then 31 Hz to 16 kHz): More bass -3,
  6 5 4 2 0…; Less bass 0, -6 -5 -4 -2 0…; More treble -3, …0 2 4 5 6; Less treble
  0, …0 -2 -4 -5 -6; Voice -2, -3 -2 -1 0 2 4 4 2 0 -1; Loudness -3, 5 4 2 0 -1 -1 0 2
  4 5. Tim tunes them by ear.
- Saved presets: a name and the gains. Saving under an existing name overwrites it;
  built-in presets can't be overwritten or deleted. Which preset is current is shown
  by name; touching a band makes it "Custom".
- Kept in `settings.json` (`eq`, `eq_presets`). An old file without them loads flat
  and off.

### 1.2 Sound (`audio.rs`)

- An `Equalized` source after `Stretched`. The EQ travels in `Shared` packed into two
  atomic words (`Eq::pack`); the stage reads them once per block of 512 frames and
  recomputes its coefficients when they changed. The audio thread never locks. Bands
  at 0 dB stay in the chain (they pass samples unchanged), so moving one band never
  resets the others' memory.
- Filter state per channel. After a seek, the state resets.
- Boosts can push peaks over full scale: when the EQ isn't flat, its output goes
  through the same `soft_limit` as the stretcher's.

### 1.3 Screen and keys

- ⌘E opens the **EQ sheet** in every mode, as does a button in the title bar, lit
  while the EQ does something (built that way instead of a settings row and a practice
  toolbar button, so it's reachable from both modes). The sheet is shaped like the trainer settings sheet:
  on/off, the preset name, the preamp and ten vertical sliders (drag or wheel; a
  double-click resets one to 0).
- Sheet keys: `h l` / ← → pick a band (the preamp is the first), `k j` / ↑ ↓ ±1 dB,
  `0` resets the band, `n N` next and previous preset, `e` on or off, `w` saves as a
  preset (the name is typed in the sheet's preset row), `x` removes the saved preset
  showing, Esc or Enter closes. Saving under an existing name takes the new spelling.
  Touching a band switches the EQ on. Like the trainer
  sheet, these keys only exist while the sheet is open.

## 2. Band filters

### 2.1 Rules (`filter.rs`, pure)

- `Band { low: Option<u32>, high: Option<u32> }` in Hz: a high-pass at `low` and a
  low-pass at `high`, each two Butterworth biquads in a row (24 dB per octave, so
  what's cut is clearly gone). With neither, the band is off and passes samples
  through untouched.
- Presets, starting points to be tuned by ear: **bass** 40 to 250 Hz, **guitar** 120
  to 6000 Hz, **vocals** 300 to 3400 Hz, **no bass** from 200 Hz. `c` (cut) cycles off →
  bass → guitar → vocals → no bass → off.
- Free cuts snap to a log scale between 20 Hz and 20 kHz, and the low one stays below
  the high one. A band that matches no preset is shown as "custom 150–2400 Hz".
- Kept in the resume spot per song (`Spot::band`), like the pace.

### 2.2 Sound (`audio.rs`)

- A `Filtered` source between `Looped` and `Stretched`, fed through `Shared` like the
  EQ (atomics plus a version number, read per block).
- **Switching doesn't click.** On a change the old and new filters both run for 20 ms
  and the output crossfades between them. The test: a sine through a switch never
  jumps between samples by more than it does without one.
- After a seek, or a loop wrapping back, the filter state resets. Its warm-up is a
  few samples, so a wrap doesn't make an audible gap.

### 2.3 Screen and keys

- Practice mode: `c` cycles the presets, `C` back to off. The practice toolbar gets
  a filter button (lit when a band is on, showing its name; right-click for the
  presets) and a range slider with two handles over a log frequency axis. Dragging
  either handle makes the band custom.
- The ⌘? sheet and tooltips through `keys.rs`, as always.

## 3. The queue

### 3.1 Rules (`queue.rs`, pure; next and previous in `playlist.rs`)

- `Queue(Vec<PathBuf>)`: `add` (refusing a duplicate), `add_top` (moving a
  duplicate to the top), `remove(i)`, `to_top(i)`,
  `take(i)` (play now) and `retain` (drop songs gone after a rescan).
- **Next**, by key, button, media key or the song ending, plays the queue's first
  song and removes it from the queue. With an empty queue, the list carries on as now.
- **While a queued song plays, the list stays where it was.** When the queue runs
  out, the list continues after the song that was playing before the queue started.
- **Previous** during a queued song restarts it when it's past 3 s (`RESTART_AFTER`),
  and otherwise goes back to the list's song that played before it. In the list,
  previous works as now. Queued songs that have played aren't kept anywhere.
- Playing a song yourself (Enter, a click) starts its list as now and leaves the
  queue alone. Shuffle only shuffles the list, never the queue.
- A song with a loop never ends, so the queue waits, as the list does now.
- Kept in `session.json` (`queue`). Songs that have gone aren't restored.

### 3.2 Keys

- Library mode: `q` queues the highlighted song at the end, `Q` at the top, with or
  without vim keys. A message says "queued, 3rd", "queued next" or "already queued".
- `` ` `` (backtick, in library and practice) puts the keyboard in the queue,
  opening the sidebar if it's closed. This is a new mode, `Mode::Queue`, like search
  is: `j k` / ↑ ↓ move, Enter plays now, `t` moves to the top, `dd`, `x` or Backspace
  removes, Esc, Tab or `` ` `` goes back to the mode it came from.
- ⌘B toggles the sidebar everywhere.
- The song row's right-click menu gets "Queue" (`q`) and "Queue next" (`Q`). The queue rows' menu has play
  now, to the top, and remove.

## 4. The sidebar

- Left of the list or the practice view, 240 px, the theme's panel colour. Hidden,
  the window looks as now.
- **Navigation**: *Library* (the full list: leaves album, artist and search views,
  the mouse version of Esc), *Favourites* (the full list, scrolled to its
  favourites), *Playing* (`zz`). While an album or artist view is open it's listed
  under Library, lit, with an × that closes it. In practice mode, a click on any of
  these also goes back to library mode.
- **Queue**: a header with the count and a clear button, then a row per song (title,
  artist below it). Hovering a row shows to-top and remove buttons; a click plays it
  now. Empty, it says "q queues the highlighted song", its keys read from the keymap
  like every tooltip.
- In queue mode the highlighted queue row is lit as the list's selection is.
- Icons: Lucide's `panel-left`, `arrow-up-to-line`, `list-plus`; `x` is already there.

## Build order

Each step ends with `cargo test && cargo build --release` clean and AGENTS.md updated.

1. **Queue rules**: `queue.rs` and next/previous in `playlist.rs`, with tests for
   §3.1, the session round trip and an old `session.json` loading.
2. **Queue in the app**: `q`, the song menu entry, `Mode::Queue` and its keys on the
   sheet, ⌘B, and the sidebar from §4.
3. **Band filters**: `filter.rs` with tests (a sine in the band passes within 1 dB,
   one two octaves out is down by more than 20 dB, off is bit-exact, switching doesn't
   jump), `Filtered` in the chain, `c C`, the toolbar button and range slider, the
   band in the resume spot. Tim tunes the presets by ear.
4. **EQ**: `eq.rs` with tests (each band's gain at its own frequency within 0.5 dB,
   flat and off are bit-exact, presets save, overwrite and load, old settings load
   flat), `Equalized` in the chain, the sheet and ⌘E. Tim tunes the built-in presets.

## Keys settled with Tim

`c C` for filters, `q Q` to queue, `` ` `` into the queue, ⌘E for the EQ, ⌘B for the
sidebar.
