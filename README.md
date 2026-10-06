# riff

A fast, keyboard-first practice player for musicians on macOS. Find a song in your
library in a keystroke, loop the bar you're working on, slow it down, mute the guitar,
and play along.

<!-- Screenshot: the library, a few songs and the now-playing card -->
![The library](docs/screenshots/library.png)

## Why riff

I wanted a modern player for a music library that lives on disk, not in a streaming
service, that I could drive entirely from the keyboard. Every action has a key, with vim
keys if you want them, and everything can still be clicked when your hand is on the
mouse.

The practice view grew out of [Transcribe!](https://www.seventhstring.com), the tool I
learn songs with. Riff borrows its loops, waveform and slow-down, and adds what I kept
missing there: the other recordings of a song in one place. Backing tracks and stems sit
next to the original, line up with it, and are a key away. Stems play together, every
one in view and each a key away from muted.

## What it does

- **Finds songs instantly.** Search a library of thousands as you type. Tags, covers and
  lengths are cached, so riff opens straight away.
- **Loops to the sample.** Set a loop with `[` and `]`, nudge its edges by ear, slide it
  phrase by phrase. Name the parts of a song as sections and loop any of them in a
  keystroke, or drop a marker to play from with `o`.
- **Shows the music.** Press `P` for the practice view: a waveform that scrolls with the
  song, with every beat marked.
- **Follows the beat.** Turn on the beat grid and loops snap to beats, even on old
  recordings that drift.
- **Slows down without changing pitch.** 50 to 90% speed at full quality.
- **Plays your other recordings.** Drop a backing track or a folder of stems onto a song.
  Riff lines it up with the original, and you switch between them with `v`. Stems play
  together, each one muted or soloed with a single key.
- **Remembers where you were.** Each song comes back with its recording, loop and speed.
- **Hands off to Transcribe!** One key opens the song, or its saved Transcribe! file, for
  deeper work.

<!-- Screenshot: the practice view with a loop, beat markers and stem lanes -->
![The practice view](docs/screenshots/practice.png)

<!-- Screenshot: the album view -->
![An album](docs/screenshots/album.png)

## Getting started

You need macOS, [Rust](https://rustup.rs), [just](https://github.com/casey/just) and
[resvg](https://github.com/linebender/resvg) (for the app icon).

```sh
just run       # run from source
just install   # build Riff.app into ~/Applications
```

Riff scans your music folder on first start. Point it at your folders with `⌘,`, where
you can add as many as you like and pick a colour theme. Any theme installed in
[Zed](https://zed.dev) works.

## A few keys

| Key | Does |
|---|---|
| `/` or `⌘P` | search |
| `Space` | play or pause |
| `[` `]` | set the loop's start and end |
| `{ } ( )` | widen or narrow the loop |
| `m`, `o` | drop a marker, play from it |
| `r`, `n` `N` | save a section, loop the next or previous |
| `P` | practice view |
| `S`, `,` `.` | slow-down on or off, slower or faster |
| `v` | next recording: the song, a backing track, stems |
| `1`–`9` | mute a stem |
| `⌘T` | open in Transcribe! |
| `?` | every key |

Vim keys (`j k`, `gg G`, `h l` and more) are on by default and can be turned off in
settings.

## Where things live

Your music stays where it is. Riff keeps its cache, sections, variations and settings in
`~/Library/Application Support/riff/`, and never writes next to your songs.
