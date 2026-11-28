# Plan 05: The sort order

Asked for by Tim on 2026-11-28, with Edit info's year: Winamp's custom sort orders,
`%artist - %year - %album - %title`, "but we're in Rust, and it's 2026, we can make this
a LOT better".

## Decisions (Tim's answers)

- **Only a sort order.** "It won't change the display, but it will be a sort order."
- **Dragged, not typed.** A first version took a typed line of fields with typo
  suggestions and Tab completion; Tim, seeing it: "really bad UI... when I said it's
  2026 means I don't want a free text field. I'd rather have the user drag a different
  order". The settings sheet widened with it (520 to 680 px).
- **The grouped view groups by the first field.** Sorted year first, the list sits
  under a row per year; under an artist row per run it would repeat over almost every
  song. Rejected: keeping artist rows, and always grouping by artist.
- **Sort names and the genre are read from the tags.** Rejected for now: album artist
  and date added.
- **One order**, in settings. Rejected: saved orders to switch between.

## The order (`order.rs`)

- Fields: artist, album, title, year, disc, track, length, genre, file name, folder.
  The default is artist, year, album, disc, track, title: the order before this.
- Each field compares as what it is (numbers as numbers, text without case or
  accents), an empty value goes last either way round, each field can be turned
  around (A–Z or Z–A, 1–9 or 9–1), `year` is the album's (an album never splits), and
  artist, album and title sort by the file's sort-name tags ("Beatles, The"). Ties go
  by path.
- Kept in settings as a line, `artist -year album`.

## The row in settings

- The order's fields as chips: drag one onto another to take its place, click its
  A–Z to turn it around, × to take it out (never the last). Below, under Add, the
  fields not used: a click or a drag into place brings one in, a used one dragged
  there goes out. Default puts the order back.
- Keys on the row: ← → pick, ⇧← ⇧→ move, Enter or Space turns around or adds, ⌫ takes
  out (Vim: `h l`, `H L`, `dd`).
- Every change is kept and the list re-sorts at once: the full list, favourites, an
  artist's view, what plays next. An album stays in track order, a playlist in its own,
  search best match first. The letter rail files by the first field (none for a
  number).

Status (2026-11-28): built. Checked on Tim's library cache: the default order puts all
2,608 songs with an artist where they were, except duplicate files (an MP3 beside its
FLAC), which now go by path; the 11 without an artist move from the top to the end.
