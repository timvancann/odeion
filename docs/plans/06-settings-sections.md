# Plan 06: Settings in sections

Asked for by Tim on 2026-11-28, after the sort order's row didn't fit the old sheet:
"at what point does it make sense to start having a more streamlined settings? Like T3
Code has." Built from a mockup he approved ("de mockup ziet er goed uit").

## Why now

- Rows that need more than a label: the folder lists and the sort order's chips.
- Settings that need a line saying what they do (what backing tracks are, what the data
  folder is for), which the one-column sheet had no room for.
- Rows were counted by hand in the view (`prefs_rest + n`); adding Sort by meant
  renumbering every row after it in four places.

## The page

- First built as an 820 by 600 px sheet over the list. Tim asked for what was behind
  it to be blurred; GPUI has no backdrop blur (only shadows blur), and fading what's
  behind looks the same as the scrim already there, so settings became a page, as in
  T3 Code: in place of the sidebar and the list, the title bar and the now-playing card
  staying. Esc, ⌘,, the ×, the title bar's settings button or its Library / Practice
  switch go back. The groups are at most 760 px wide, centred.
- A rail on the left with four sections,
  each with its key: Library `⌘1`, Playback `⌘2`, Look `⌘3`, Keys & data `⌘4`; Tab and
  ⇧Tab go round them, a click picks one.
- The open section's page: "Settings / <section>" and the ×, then a heading per group
  (with a line about it where it needs one) over a card of rows. A row is its name and
  what it does on the left, its value on the right; a folder row is the folder, its
  count and a ×. The page scrolls when it's long; the key hint stays under it.
- Library: music folders, backing tracks, Sort by. Playback: the output. Look:
  appearance and theme. Keys & data: the keyboard and the data folder.
- The theme and output pickers open under their rows, inside the card.
- The EQ and the speed trainer keep sheets of their own: they're used while practising,
  not while setting up.

## Where it lives

`prefs.rs` holds the sections, groups and rows and each row's words, tested; the view
asks it which row is highlighted (`Odeion::pref_at`) and highlights one by what it is
(`pref_go`), never by counting.

Status (2026-11-28): built.
