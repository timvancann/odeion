<h1 align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="bundle/wordmark-on-dark.svg">
    <img src="bundle/wordmark-on-light.svg" width="320" alt="Odeion">
  </picture>
</h1>

<p align="center">
  A music player and a practice tool in one.<br>
  Listen to your own library, or loop the solo, slow it down, mute the guitar and play along.
</p>

<p align="center">
  <a href="https://github.com/timvancann/odeion/releases/latest/download/Odeion-macOS.zip"><img src="https://img.shields.io/badge/Download_for_macOS-2350d6?style=for-the-badge&logo=apple&logoColor=white" alt="Download for macOS"></a>
  <a href="https://github.com/timvancann/odeion/releases/latest/download/Odeion-Windows.zip"><img src="https://img.shields.io/badge/Download_for_Windows-2350d6?style=for-the-badge&logo=windows&logoColor=white" alt="Download for Windows"></a>
</p>

<p align="center">
  <a href="https://github.com/timvancann/odeion/releases/latest"><img src="https://img.shields.io/github/v/release/timvancann/odeion?label=release&color=2350d6" alt="Latest release"></a>
  <a href="https://github.com/timvancann/odeion/releases"><img src="https://img.shields.io/github/downloads/timvancann/odeion/total?color=2350d6" alt="Downloads"></a>
  <a href="https://github.com/timvancann/odeion/actions/workflows/ci.yml"><img src="https://github.com/timvancann/odeion/actions/workflows/ci.yml/badge.svg" alt="CI"></a>
  <a href="LICENSE"><img src="https://img.shields.io/badge/licence-PolyForm_Noncommercial-18181b" alt="Licence: PolyForm Noncommercial"></a>
</p>

<a href="https://youtu.be/_kaOeKP2ras"><img src="docs/demo.gif" alt="Searching the library, opening a song in practice mode, a loop playing, then the song's stems with the lead guitar muted"></a>

<p align="center"><a href="https://youtu.be/_kaOeKP2ras"><b>Watch the five-minute demo</b></a></p>

<p align="center"><sub>Named after the <a href="https://en.wikipedia.org/wiki/Odeon_(building)">odeion</a>,
the small concert hall of ancient Greece, where people came to hear music and musicians
came to play it.</sub></p>

## What it does

**Listen**

- Your own music folder, opened instantly, even with thousands of songs.
- Search as you type, albums, artists, favourites, a queue and playlists.

![The library](docs/screenshots/library.png)

**Practise**

- Drag across the waveform to loop a part, gapless.
- Slow down or change key without the other changing, plus a speed trainer.
- Drop in stems ([Moises](https://moises.ai) or any splitter) or a backing track; Odeion
  lines them up. Mute or solo with one key.
- Beats found from the recording, so loops snap to them even on records that drift.
- Name sections and drop a marker. Every song comes back as you left it.

![A song with its stems](docs/screenshots/stems.png)

**Everywhere**

- Keyboard first (Mac or Vim keys), every key also a button. `⌘?` lists them all.
- Right-click menus, a 10-band EQ, media keys, light and dark.
- Two colour schemes: T3 (cool greys, clear blue) and Gold (warm paper, gold).
- Plays mp3, flac, wav, aiff, m4a (AAC and ALAC) and ogg.

## Getting started

**macOS** (13 or later, Apple silicon and Intel):
[download the latest Odeion](https://github.com/timvancann/odeion/releases/latest/download/Odeion-macOS.zip),
unzip it and drag Odeion into Applications. If macOS says it can't check the app, open
it once, then click Open Anyway in System Settings, Privacy & Security.

**Windows** (new, still being tried out):
[download the latest Odeion](https://github.com/timvancann/odeion/releases/latest/download/Odeion-Windows.zip),
unzip it and run `odeion.exe`. Windows warns about an unknown publisher the first time:
click More info, then Run anyway. Ctrl does what ⌘ does on a Mac.

Older versions are on the [Releases](https://github.com/timvancann/odeion/releases) page.

Or build it yourself, with [Rust](https://rustup.rs),
[just](https://github.com/casey/just) and
[resvg](https://github.com/linebender/resvg) (for the app icon):

```sh
just run       # run from source
just install   # build Odeion.app into ~/Applications
```

On first start Odeion asks for the folder your music is in. Add more in settings (`⌘,`).

Your music stays where it is: Odeion never writes next to your songs. Its cache, loops,
sections, playlists and settings live in `~/Library/Application Support/odeion/`, or in
another folder you choose in settings. Odeion only goes online when you choose Check for
Updates in its menu, to ask GitHub for the latest release, and when you then choose
Install, to download it. It installs only a release signed with its own key.

## Inspiration

Odeion started as a way to find songs for [Transcribe!](https://www.seventhstring.com),
the tool I learned songs with for years, because finding a file in it is painful. It
grew until it did everything I used Transcribe! for: loops, the waveform, slowing down,
sections and markers. What it adds is the other recordings of a song in one place, an
idea from [Moises](https://moises.ai), where backing tracks and stems sit next to the
original.

The player side looks back to Winamp, with its queue, playlists and the 10-band EQ.
The look and the keyboard come from [Zed](https://zed.dev), and the collapsing sidebar
from VS Code.

## Acknowledgements

Odeion stands on these projects:

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
- [ureq](https://github.com/algesten/ureq) for checking for updates, and
  [minisign-verify](https://github.com/jedisct1/rust-minisign-verify) for checking
  they're genuine. Installing them in place follows Zed's updater.
- Beat finding follows Böck and Widmer's SuperFlux onset detector and Dan Ellis's
  dynamic-programming beat tracker.
- The EQ's filters use Robert Bristow-Johnson's Audio EQ Cookbook.
- The default T3 colour scheme takes its greys and blue from
  [T3 Code](https://github.com/pingdotgg/t3code), whose clean look it follows.
- Icons from [Lucide](https://lucide.dev), fonts
  [JetBrains Mono](https://www.jetbrains.com/lp/mono/) and
  [Space Grotesk](https://fonts.google.com/specimen/Space+Grotesk); the wordmark is set in
  [Poppins](https://fonts.google.com/specimen/Poppins).

## Roadmap

- **Tags and covers from MusicBrainz.** Look up a song or album with missing tags or no
  cover on [MusicBrainz](https://musicbrainz.org) and the Cover Art Archive, check the
  match, and fill them in. Only when you ask, since it writes into your files.
- **Stems from Moises, in Odeion.** Split a song into stems through the
  [Moises API](https://music.ai) with
  your own key, and get them back lined up on the song, ready to mute and solo.

## Licence

[PolyForm Noncommercial 1.0.0](LICENSE): free to use, change and share for any
noncommercial purpose (personal use, study, teaching, hobby projects, charities). Selling
Odeion or using it in a commercial product needs the author's permission. The bundled
fonts and icons keep their own licences, beside them in `assets/`.
