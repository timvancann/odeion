# Changelog

What changed in each release of Odeion, newest first. Each section becomes that
release's notes on GitHub.

## Unreleased

## 0.5.0 (2026-11-11)

**New**

- Check for Updates installs a new version itself: it downloads it in the background,
  checks it's genuine, puts it in place, and the title bar offers Restart to update.
  Getting this version is a download by hand one last time; the ones after it install
  themselves.
- The big waveform shows the sections above it, with their edges through the waveform,
  so a loop is easy to set inside one. Section edges are dragged there.
- A list of the song's sections beside the practice view to jump to them. It folds
  away to a button.

**Changed**

- A click on a section jumps to it, and `n` `N` and the previous and next buttons do the
  same, showing the section centred in the waveform, about two thirds of it, until the
  playhead leaves it; a double-click loops it. A loop you made stays: when the section is outside
  it, looping switches off and the loop button brings it back.
- Sections can no longer be resized in the lane over the whole song, where small ones
  were too narrow to grab.
- Tooltips over the waveforms and sections are shorter and wait longer.

## 0.4.0 (2026-11-10)

**New**

- Column headers over a playlist's songs sort it by title, artist, album or length.
  Click again to reverse, a third time for the playlist's own order. The songs play in
  the order shown; the playlist itself stays as it was.
- The gauge beside the speed is a button: full speed, and again back to the slowed
  speed.

**Changed**

- `s` and the gauge change only the speed. A pitch shift stays until you change it.
  A song you left at full speed with `s` while it had a pitch shift now opens with that
  shift; the pitch control's right-click menu puts it back to as recorded.

**Fixed**

- Left and right no longer swap after playing from the loop's start or the marker, or
  clicking around, in a slowed-down or pitch-shifted song, nor when the speed changes.
- The song Odeion reopens with goes on to the next one when it ends, through the list
  it was started from.
- Switching to a backing track or stems that start later than the song no longer
  crashes Odeion.
- Windows: the cover shows in the media controls when its path has a space.

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
