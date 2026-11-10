# Changelog

What changed in each release of Odeion, newest first. Each section becomes that
release's notes on GitHub.

## Unreleased

## 0.3.0 (2026-11-10)

**New**

- Drag songs to put a playlist in a new order.
- Sort a playlist by artist or by title: right-click the playlist or its header.
- In an artist's view, the grouped list puts the songs under each album.
- The mouse's back button works like Esc: back out of an album, artist, playlist or
  practice.

**Fixed**

- On MP3s the beat grid and waveform no longer run 25 ms behind the music, so a loop
  snapped to a beat starts on the attack. Stems or a backing track added to an MP3
  before this keep their old alignment: remove them and add them again to line them up
  anew.
- Muting, soloing or changing the level of a stem no longer clicks.
- The queue is no longer lost when the library cache is deleted or damaged.
- Favourites are saved the way playlists are, so Odeion crashing while it saves no
  longer wipes them.
- Tooltips wait a moment before they show.
- Windows: the shortcut sheet no longer lists keys that can't work there, like
  "Ctrl+Ctrl+F".

## 0.2.1 (2026-11-08)

- The practice view's Follow button lights up while the view follows the playhead, and
  switches following off and on.
- Scrolling sideways over the waveform moves the view along the song.
- A view held in place zooms about its middle instead of its start.
- The overview shows the loop and the marker without letting them be dragged there.
- A paused playhead past the loop's end no longer jumps back by the loop's length.

## 0.2.0 (2026-11-08)

- Riff is now called Odeion. Your data moves to the new folder by itself.
- An Odeion menu in the menu bar, with Check for Updates.
- A first Windows build, as a trial: Ctrl does what ⌘ does, keys are spelled out
  (Ctrl+F, Alt+↑), and a crash leaves `crash.log` in the data folder.

## 0.1.0 (2026-11-08)

The first public release: a music player for your own library and a practice tool in
one, with loops, slow-down, pitch, sections, a marker, beats, stems and backing tracks.
