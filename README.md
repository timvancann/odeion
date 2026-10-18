# riff

A keyboard-first practice player for musicians on macOS. Find a song in your own music
library in a keystroke, loop the bar you're working on, slow it down, mute the guitar
and play along.

![The library](docs/screenshots/library.png)

## What it does

**A player for the music you own.** Riff scans the folders you point it at (mp3, flac,
wav, aiff, m4a, ogg) and opens instantly, even on a library of thousands of songs,
because tags, covers and lengths are cached. Search as you type, browse by album or
artist, mark favourites, queue songs up and build playlists. The list shows songs in
columns or grouped under each artist, and a sidebar holds the queue and your playlists.

**Loops to the sample.** Set a loop while the song plays and it repeats exactly, with
no gap. Nudge its edges, slide it phrase by phrase, or drag them on the waveform.
Name the parts of a song as sections (verse, solo, outro) and jump between them, or drop
a marker to play from.

**A waveform you can work in.** The practice view shows the whole song's shape over a
large waveform that frames your loop or follows the playhead. Every beat is marked,
found from the recording itself, so the grid follows old records that drift in tempo.
Turn the grid on and loops, the marker and seeking all snap to beats.

**Slower, or in another key.** Change the speed from 25 to 110% without changing the
pitch, or the pitch by semitones and cents without changing the speed, using Signalsmith
Stretch at full quality. A speed trainer starts a loop slow and speeds it up a step
every few repeats, up to the speed you set.

**Your other recordings, lined up.** Drop a backing track or a folder of stems (from
Moises or any other stem splitter) onto a song. Riff lines them up with the original
on its own, and they're a key away while it plays. Stems get a lane each, with their
own level, and mute or solo with a single key. Each recording keeps its own loops, sections and
marker.

![A song with its stems](docs/screenshots/stems.png)

**Remembers everything.** Each song comes back with the recording, speed, pitch, loops
and stem levels you left it with, and riff reopens on the song you were playing.

**Keyboard first, mouse welcome.** Every action has a key, with vim keys if you want
them, and every action is also a button whose tooltip tells you its keys. `⌘?` lists
them all and finds them as you type.

**And the rest.** A 10-band EQ with presets you can save, media keys and Now Playing,
any output device (riff follows the system when you switch), and every colour theme
installed in [Zed](https://zed.dev).

## Getting started

You need macOS, [Rust](https://rustup.rs), [just](https://github.com/casey/just) and
[resvg](https://github.com/linebender/resvg) (for the app icon).

```sh
just run       # run from source
just install   # build Riff.app into ~/Applications
```

On first start riff asks for the folder your music is in. Add more in settings (`⌘,`).

Your music stays where it is: riff never writes next to your songs. Its cache, loops,
sections, playlists and settings live in `~/Library/Application Support/riff/`, or in
another folder you choose in settings.

## Inspiration

Riff started as a way to find songs for [Transcribe!](https://www.seventhstring.com),
the tool I learned songs with for years, because finding a file in it is painful. It
grew until it did everything I used Transcribe! for: loops, the waveform, slowing down,
sections and markers. What it adds is the other recordings of a song in one place, an
idea from [Moises](https://moises.ai), where backing tracks and stems sit next to the
original.

The player side looks back to Winamp, with its queue, playlists and the 10-band EQ.
The look and the keyboard come from [Zed](https://zed.dev), and the collapsing sidebar
from VS Code.

## Acknowledgements

Riff stands on these projects:

- [GPUI](https://www.gpui.rs), Zed's UI framework, through
  [gpui-pre](https://crates.io/crates/gpui-pre), its maintained snapshot on crates.io.
- [rodio](https://github.com/RustAudio/rodio) and
  [cpal](https://github.com/RustAudio/cpal) for playback,
  [Symphonia](https://github.com/pdeljanov/Symphonia) for decoding.
- [Signalsmith Stretch](https://signalsmith-audio.co.uk/code/stretch/) for changing
  speed and pitch.
- [Lofty](https://github.com/Serial-ATA/lofty-rs) for tags and covers.
- [RustFFT](https://github.com/ejmahler/RustFFT) for the spectra behind beat finding.
- [souvlaki](https://github.com/Sinono3/souvlaki) for media keys and Now Playing.
- Beat finding follows Böck and Widmer's SuperFlux onset detector and Dan Ellis's
  dynamic-programming beat tracker.
- The EQ's filters use Robert Bristow-Johnson's Audio EQ Cookbook.
- Icons from [Lucide](https://lucide.dev), fonts
  [JetBrains Mono](https://www.jetbrains.com/lp/mono/) and
  [Space Grotesk](https://fonts.google.com/specimen/Space+Grotesk).

## Licence

GPL-3.0. See [LICENSE](LICENSE).
