# Plan 01: two modes, and the practice features that need them

Agreed with Tim on 2026-10-06. Riff has grown enough practice features that one flat
keymap no longer fits: too many keys across too many parts of the player, and the
loop keys (`[ ] { } ( )`) live on layers of his Glove80, so they take two hands. This
plan splits riff into two modes, gives each its own keys, and then adds the practice
features from the comparison with Transcribe!, Amazing Slow Downer and Moises.

Every action stays reachable with the keyboard alone and with the mouse alone.
Right-clicks are allowed where they make things contextually easier.

## The keyboard it is designed for

Tim's Glove80 (TailorKey, QWERTY) has these on its base layer: letters, the number
row, Tab, Esc, and the symbols `` ` `` `-` `=` `\` `;` `'` `,` `.` `/`. Shift sits on a
thumb, so `< > _ + : ? |` are a thumb plus one key. Brackets, braces and parentheses
are on other layers. **No primary key in this plan is off the base layer** (with
shift counting as base). Brackets may stay as quiet aliases but are not on the sheet.

## Modes

Like vim: library mode is normal, practice mode is entered and left.

- **Library mode**: finding and playing songs. Riff opens here, unless the session
  says it was left in practice mode.
- **Search**: a sub-mode of the library (`/` or `i`), as now.
- **Practice mode**: the practice view. `p` (or Tab) enters it, Esc (or `p`, Tab)
  leaves it. The mode *is* the view: there is no practice view outside practice mode.

The vim setting keeps its meaning for the library (`j k`, `gg G` and so on versus
arrows only). Practice keys are the mode's own and work either way.

### Everywhere

| Keys | Does |
|---|---|
| Space | play or pause |
| `← →` | back or forward 5 s (a beat on the grid) |
| `0` | back to the start |
| `m`, `o` | marker on or off, play from the marker (or the loop start) |
| ⌥↑ ⌥↓ | volume up or down |
| ⌘← ⌘→ | previous or next song |
| ⌘P ⌘F ⌘A ⇧⌘A ⌘T ⌘S | search, favourite, album, artist, Transcribe!, shuffle |
| ⌘, ⌘? ⌘M ⌃⌘F ⌘Q | settings, keys, minimise, full screen, quit |

### Library mode

Today's list keys, minus everything that moves to practice:

| Keys | Does |
|---|---|
| Enter | play the highlighted song |
| `j k`, ↑ ↓, ⌃J ⌃K | move |
| `gg G`, ⌃U ⌃D, PgUp PgDn, `zz` | top, bottom, half a page, ten, to the playing song |
| `/` `i` | search |
| `h l` | back or forward 5 s |
| `H L` | previous or next song |
| `f`, `a`, `A`, `t`, `s` | favourite, album, artist, Transcribe!, shuffle |
| `p`, Tab | practice mode |
| `?` | keys |

### Practice mode

| Keys | Does |
|---|---|
| Esc, `p`, Tab | back to the library |
| `a` `b` | loop starts / ends here |
| `\`, `\|` | loop on or off keeping its edges, clear the loop |
| `r`, `n N`, `dd` | save the loop as a section (or rename its section), loop the next / previous section, delete the section in play |
| `h l` | step back / forward 5 s, a beat on the grid |
| `H L` | step a bar (once bars exist, step 7) |
| `g` | beat grid on or off |
| `,` `.` | slower / faster by 5%, 25% to 110% |
| `s` | speed and pitch back to normal, and again back to how they were |
| `t` | speed trainer on or off |
| `k` `j` | pitch up / down a semitone |
| `K` `J` | pitch up / down 5 cents |
| `-` `=` | zoom out / in (the wheel too) |
| `v`, `V`, `X` | next recording, add a variation, remove it |
| `1`-`9`, ⌥`1`-`9` | mute, solo a stem |
| `u` ⌘Z, ⇧⌘Z ⌃R | undo, redo (loop, marker, sections) |
| `i` | a note on the song or the section in play |
| `w`, `e` | lead-in (off, a beat, a bar), which channel (both, left, right) |
| `?` | keys |

**Moving the loop has no keys, by Tim's choice** (2026-10-06): `h l` move the
playhead, and `a` or `b` again puts the edge there. `\` switches the loop off and on;
`|` clears it. The waveform still takes drags,
and the loop bar keeps its widen, narrow and slide buttons for the mouse.

**Fine speed is mouse-only, by Tim's choice**: a slider for any percentage between 25
and 110. 1% steps only matter to the trainer, which sets them itself.

`w` and `e` are the least settled; they are cheap to move once the features exist.

## The mouse

Everything above has a button or a direct manipulation, as now, with a tooltip whose
keys come from the keymap (`keys::keys_for`, now per mode). New:

- **A mode switch** in the title bar (the practice view button becomes it).
- **Right-click menus**, each entry an `Action`, showing its keys:
  - waveform: loop starts here, loop ends here, marker here, bar 1 starts here;
  - section: loop it, rename, delete, note;
  - stem: level 100 / 50 / 25%, solo, mute;
  - speed: presets (50 to 110%), trainer settings;
  - song row: favourite, album, artist, Transcribe!, add a variation.
  Menus open at the pointer, close on Esc or a click outside, and take `j k` / ↑ ↓
  and Enter.
- **A volume slider** on the now-playing card, and a **level per stem** (drag across
  the stem's name, or the wheel over it).
- **Speed and pitch controls** in the practice toolbar: the value, step buttons, and
  a speed slider (the only way to 1% speeds).

## Build order

Each step lands on its own, with its rules and tests in the pure modules and
`view.rs` only drawing and dispatching, as now. Each ends with `cargo test` and
`cargo build --release` clean, and AGENTS.md updated.

1. **Modes and the keymap.** `keys::Mode { Library, Search, Practice }`; the map is
   global keys, then the mode's. `SHEET` groups carry the modes they belong to; the ⌘?
   sheet shows the current mode's keys and the global ones. `resolve` and `keys_for`
   take the mode. Tests: every sheet key does something in its mode, and no key is
   bound twice within a mode. The practice view becomes the mode. Nudge and slide lose
   their keys (their buttons stay); `s` toggles speed and pitch together. `\` and `|`
   swap: `\` is now loop on or off (the frequent one, unshifted), `|` clears.
   The right-click menu element, with the waveform, section and song row menus.
   *Done when* every action from today is reachable in its mode, by key and by mouse.
2. **Volume and output.** A master volume (`Player::set_volume`), kept in settings.
   Stem levels: `Mix` already holds live gains; levels per stem set are kept with the
   song's resume spot. Following the system's output device: check the default device
   every second or so and, when it changes, reopen the output and carry on at the same
   position. A device picker in settings.
3. **Speed.** Speed becomes a value from 25% to 110% instead of a preset index
   (resume entries migrate: preset n becomes its percentage). `Stretched` passes samples
   straight through only at exactly 100%. Then the **trainer** (`trainer.rs`, pure):
   from, to, step, every N repeats. `Looped` counts its wraps in a shared atomic; the
   view reads the count and asks the trainer for the speed.
4. **Pitch.** Semitones and cents, per song, kept in the resume spot. Signalsmith
   Stretch transposes as well as stretches; the bypass becomes "speed 100% and pitch
   0". The practice toolbar shows it.
5. **Undo.** `history.rs`: before every change to the loop, the marker or the
   recording's sections, a snapshot of the three; `u` and ⌃R walk it, 50 deep, per
   session, not saved.
6. **Loop extras.** A **lead-in**: every repeat starts a beat or a bar before the
   loop, so the music itself counts in, while the loop still marks exactly the part
   (sections, the trainer and bars stay right). Experimental: try it and adjust. Done inside `Looped`: the jump at the
   loop's end lands on start minus the lead-in. Drawn faintly before the loop. This
   replaces a silent gap or clicks, which Tim does by starting loops early. Channel
   choice inside `Mix`: both, left in both ears, right in both ears.
7. **Bars.** No downbeat detector: Tim marks "bar 1 starts here" on a beat (right-click
   or a key) and picks the meter (4/4 by default). `bars.rs` counts bars from the
   detected beats, so bars follow tempo drift. Numbered bars on the waveform, `H L` step
   bars, loops can be set in whole bars. Kept per recording.
8. **Notes.** A note on the song and on each section (sections get an optional note).
   `i` opens the note for the section in play, else the song.

## Open, to settle while building

- Whether `w` and `e` are the right keys for gap and channel.
- Where the trainer's settings live: a right-click menu on speed, or a small sheet.
- Whether the mode switch should also be a visible toggle on the now-playing card.
