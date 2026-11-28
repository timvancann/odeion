//! Editing a song's info: its title, artist, album, track and disc number, year and
//! cover, written into the file's own tags so every other player sees them too (Tim,
//! 2026-11-27). The sheet's rules are pure; `write` does the file work.
//!
//! Each format is written through its own tag (ID3v2 for MP3, AIFF and WAV, Vorbis
//! comments for FLAC and Ogg, MP4 atoms for M4A), never through lofty's generic tag,
//! which drops what it can't represent. Only the fields asked for change. The new tags
//! go into a copy beside the file, which is read back and checked before it replaces
//! the file, so a failed write leaves the song as it was, and a song playing from the
//! old file plays on.

use crate::library::Track;
use lofty::config::{ParseOptions, WriteOptions};
use lofty::file::{AudioFile, FileType};
use lofty::id3::v2::Id3v2Tag;
use lofty::ogg::OggPictureStorage;
use lofty::picture::{MimeType, Picture, PictureType};
use lofty::prelude::*;
use lofty::probe::Probe;
use lofty::tag::ItemKey;
use lofty::tag::items::Timestamp;
use std::fs::OpenOptions;
use std::io::{Seek, SeekFrom};
use std::path::{Path, PathBuf};

/// An embedded cover is at most this many pixels across; a larger image is scaled down.
pub const COVER_SIZE: u32 = 1000;

/// The pictures a new cover replaces. Many files keep their cover as "other": it goes
/// too, or players could still show it.
const COVERS: [PictureType; 2] = [PictureType::CoverFront, PictureType::Other];

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Field {
    Title,
    Artist,
    Album,
    Track,
    Disc,
    Year,
}

impl Field {
    pub fn label(self) -> &'static str {
        match self {
            Field::Title => "Title",
            Field::Artist => "Artist",
            Field::Album => "Album",
            Field::Track => "Track",
            Field::Disc => "Disc",
            Field::Year => "Year",
        }
    }

    /// How many digits a number field takes; none for text.
    pub fn digits(self) -> Option<usize> {
        match self {
            Field::Track | Field::Disc => Some(3),
            Field::Year => Some(4),
            _ => None,
        }
    }

    fn of(self, t: &Track) -> String {
        let number = |n: Option<u32>| n.map(|n| n.to_string()).unwrap_or_default();
        match self {
            Field::Title => t.title.clone(),
            Field::Artist => t.artist.clone(),
            Field::Album => t.album.clone(),
            Field::Track => number(t.number),
            Field::Disc => number(t.disc),
            Field::Year => number(t.year),
        }
    }
}

/// One field on the sheet: what's typed, and what the songs had (none when they differ).
#[derive(Debug, Clone, PartialEq)]
pub struct Entry {
    pub field: Field,
    pub value: String,
    pub was: Option<String>,
}

impl Entry {
    /// The songs differ here, so nothing changes unless something is typed.
    pub fn mixed(&self) -> bool {
        self.was.is_none()
    }

    pub fn changed(&self) -> bool {
        match &self.was {
            Some(was) => self.value.trim() != was,
            None => !self.value.trim().is_empty(),
        }
    }
}

/// The Edit info sheet: the songs it edits, a row per field and the cover last.
#[derive(Debug, Clone)]
pub struct Sheet {
    pub songs: Vec<PathBuf>,
    pub rows: Vec<Entry>,
    /// The highlighted row: a field, or `rows.len()` for the cover.
    pub at: usize,
    /// While the highlighted field is being edited, what it held before, for Esc. Not
    /// editing, the field is selected whole, as a spreadsheet's cell: typing replaces
    /// it, ⌫ clears it, Enter edits it from its end (Tim, 2026-11-28: Enter saving the
    /// sheet was confusing, "Enter usually means start editing").
    pub editing: Option<String>,
    /// An image chosen to become the cover.
    pub cover: Option<PathBuf>,
}

impl Sheet {
    /// A sheet for `tracks`: every field for one song; for several, those an album or
    /// artist shares (a title and track number are each song's own).
    pub fn new(tracks: &[&Track]) -> Option<Sheet> {
        let first = tracks.first()?;
        let fields: &[Field] = match tracks.len() {
            1 => &[Field::Title, Field::Artist, Field::Album, Field::Track, Field::Disc, Field::Year],
            _ => &[Field::Artist, Field::Album, Field::Disc, Field::Year],
        };
        let rows = fields
            .iter()
            .map(|&field| {
                let value = field.of(first);
                let same = tracks.iter().all(|t| field.of(t) == value);
                Entry { field, value: if same { value.clone() } else { String::new() }, was: same.then_some(value) }
            })
            .collect();
        Some(Sheet {
            songs: tracks.iter().map(|t| t.path.clone()).collect(),
            rows,
            at: 0,
            editing: None,
            cover: None,
        })
    }

    pub fn on_cover(&self) -> bool {
        self.at == self.rows.len()
    }

    /// Highlights row `at` (the cover is the last), its field selected whole; a field
    /// being edited keeps what was typed.
    pub fn select(&mut self, at: usize) {
        self.at = at.min(self.rows.len());
        self.editing = None;
    }

    /// Enter: edits the highlighted field from its end. Not on the cover.
    pub fn start(&mut self) {
        if let Some(entry) = self.rows.get(self.at) {
            self.editing = Some(entry.value.clone());
        }
    }

    /// Enter while editing: keeps what was typed. Whether a field was being edited.
    pub fn finish(&mut self) -> bool {
        self.editing.take().is_some()
    }

    /// Esc while editing: puts back what the field held. Whether a field was being edited.
    pub fn cancel(&mut self) -> bool {
        let Some(was) = self.editing.take() else { return false };
        if let Some(entry) = self.rows.get_mut(self.at) {
            entry.value = was;
        }
        true
    }

    /// The next row, or the previous; Tab and ⇧Tab, ↓ and ↑.
    pub fn step(&mut self, forward: bool) {
        let at = if forward { self.at + 1 } else { self.at.saturating_sub(1) };
        self.select(at);
    }

    /// Typed into the highlighted field: replacing it while it is selected whole. A
    /// number field takes digits only, up to its length.
    /// A key a field doesn't take (a letter in a number) leaves it as it was.
    pub fn type_text(&mut self, text: &str) {
        let Some(entry) = self.rows.get_mut(self.at) else { return };
        let text: String = match entry.field.digits() {
            Some(_) => text.chars().filter(char::is_ascii_digit).collect(),
            None => text.to_string(),
        };
        if text.is_empty() {
            return;
        }
        if self.editing.is_none() {
            self.editing = Some(std::mem::take(&mut entry.value));
        }
        match entry.field.digits() {
            Some(most) => {
                let room = most.saturating_sub(entry.value.len());
                entry.value.extend(text.chars().take(room));
            }
            None => entry.value.push_str(&text),
        }
    }

    /// ⌫: clears a field selected whole (editing it, so Esc brings it back), otherwise
    /// takes its last character.
    pub fn backspace(&mut self) {
        let Some(entry) = self.rows.get_mut(self.at) else { return };
        match self.editing {
            None => self.editing = Some(std::mem::take(&mut entry.value)),
            Some(_) => {
                entry.value.pop();
            }
        }
    }

    /// Whether saving would change anything.
    pub fn changes(&self) -> bool {
        self.cover.is_some() || self.rows.iter().any(Entry::changed)
    }

    /// What saving writes into each file, with `cover` the chosen image's bytes as they
    /// are to be embedded.
    pub fn edit(&self, cover: Option<Vec<u8>>) -> Edit {
        let mut edit = Edit { cover, ..Edit::default() };
        for entry in self.rows.iter().filter(|e| e.changed()) {
            let text = entry.value.trim().to_string();
            let number = text.parse::<u32>().ok().filter(|&n| n > 0);
            match entry.field {
                Field::Title => edit.title = Some(text),
                Field::Artist => edit.artist = Some(text),
                Field::Album => edit.album = Some(text),
                Field::Track => edit.track = Some(number),
                Field::Disc => edit.disc = Some(number),
                Field::Year => edit.year = Some(number),
            }
        }
        edit
    }
}

/// What to change in a file's tags. `None` leaves a field as it is; an empty text or a
/// `Some(None)` number removes it.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Edit {
    pub title: Option<String>,
    pub artist: Option<String>,
    pub album: Option<String>,
    pub track: Option<Option<u32>>,
    pub disc: Option<Option<u32>>,
    pub year: Option<Option<u32>>,
    /// A JPEG or PNG to become the front cover.
    pub cover: Option<Vec<u8>>,
}

/// An image file as a cover to embed: as it is when it's a JPEG or PNG of at most
/// `COVER_SIZE` pixels across, otherwise scaled down into a JPEG.
pub fn cover(image: &Path) -> Result<Vec<u8>, String> {
    let data = std::fs::read(image).map_err(|e| e.to_string())?;
    let format = image::guess_format(&data).map_err(|_| "not a JPEG or PNG image".to_string())?;
    let picture = image::load_from_memory(&data).map_err(|_| "not a JPEG or PNG image".to_string())?;
    let small = picture.width().max(picture.height()) <= COVER_SIZE;
    if small && matches!(format, image::ImageFormat::Jpeg | image::ImageFormat::Png) {
        return Ok(data);
    }
    let mut out = Vec::new();
    let scaled = picture.resize(COVER_SIZE, COVER_SIZE, image::imageops::FilterType::Lanczos3).to_rgb8();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, 90).encode_image(&scaled).map_err(|e| e.to_string())?;
    Ok(out)
}

/// Writes `edit` into the tags of the file at `path`, through a checked copy that then
/// replaces it.
pub fn write(path: &Path, edit: &Edit) -> Result<(), String> {
    let kind = Probe::open(path)
        .map_err(|e| e.to_string())?
        .guess_file_type()
        .map_err(|e| e.to_string())?
        .file_type()
        .ok_or("not a format Odeion can write tags into")?;
    let name = path.file_name().ok_or("no file name")?.to_string_lossy();
    // No audio extension, so a scan meanwhile doesn't take it for a song.
    let copy = path.with_file_name(format!(".{name}.odeion"));
    let done = std::fs::copy(path, &copy)
        .map_err(|e| e.to_string())
        .and_then(|_| write_into(&copy, kind, edit))
        .and_then(|()| check(path, &copy, kind, edit))
        // Fails on Windows while the song plays: the file is open there.
        .and_then(|()| std::fs::rename(&copy, path).map_err(|e| e.to_string()));
    if done.is_err() {
        let _ = std::fs::remove_file(&copy);
    }
    done
}

fn write_into(path: &Path, kind: FileType, edit: &Edit) -> Result<(), String> {
    let mut file = OpenOptions::new().read(true).write(true).open(path).map_err(|e| e.to_string())?;
    let parse = ParseOptions::new();
    fn err(e: impl std::fmt::Display) -> String {
        e.to_string()
    }
    macro_rules! with_id3 {
        ($kind:ty, $also:expr) => {{
            let mut audio = <$kind>::read_from(&mut file, parse).map_err(err)?;
            ($also)(&mut audio);
            let mut tag = audio.id3v2_mut().map(std::mem::take).unwrap_or_default();
            set_id3(&mut tag, edit);
            audio.set_id3v2(tag);
            file.seek(SeekFrom::Start(0)).map_err(|e| e.to_string())?;
            // Always ID3v2.4: written as 2.3, frames only 2.4 defines (the sort orders,
            // which taggers put in 2.3 tags too) would be dropped.
            audio.save_to(&mut file, WriteOptions::default()).map_err(err)?;
        }};
    }
    match kind {
        // An old ID3v1 tag beside it changes too, or players that read it show the old.
        FileType::Mpeg => with_id3!(lofty::mpeg::MpegFile, |audio: &mut lofty::mpeg::MpegFile| {
            if let Some(v1) = audio.id3v1_mut() {
                apply(v1, edit);
            }
        }),
        // So do the text chunks AIFF and WAV files may carry beside their ID3.
        FileType::Aiff => with_id3!(lofty::iff::aiff::AiffFile, |audio: &mut lofty::iff::aiff::AiffFile| {
            if let Some(text) = audio.text_chunks_mut() {
                apply(text, edit);
            }
        }),
        FileType::Wav => with_id3!(lofty::iff::wav::WavFile, |audio: &mut lofty::iff::wav::WavFile| {
            if let Some(info) = audio.riff_info_mut() {
                apply(info, edit);
            }
        }),
        FileType::Flac => {
            let mut audio = lofty::flac::FlacFile::read_from(&mut file, parse).map_err(err)?;
            let mut tag = audio.vorbis_comments_mut().map(std::mem::take).unwrap_or_default();
            apply(&mut tag, edit);
            if let Some(data) = &edit.cover {
                // A FLAC file keeps its pictures in blocks of its own, but a cover inside
                // the comments would still be read first by some players.
                for kind in COVERS {
                    tag.remove_picture_type(kind);
                    audio.remove_picture_type(kind);
                }
                audio.insert_picture(front(data), None).map_err(|e| e.to_string())?;
            }
            audio.set_vorbis_comments(tag);
            file.seek(SeekFrom::Start(0)).map_err(|e| e.to_string())?;
            audio.save_to(&mut file, WriteOptions::default()).map_err(err)?;
        }
        FileType::Vorbis => {
            let mut audio = lofty::ogg::VorbisFile::read_from(&mut file, parse).map_err(err)?;
            let tag = audio.vorbis_comments_mut();
            apply(tag, edit);
            if let Some(data) = &edit.cover {
                for kind in COVERS {
                    tag.remove_picture_type(kind);
                }
                tag.insert_picture(front(data), None).map_err(|e| e.to_string())?;
            }
            file.seek(SeekFrom::Start(0)).map_err(|e| e.to_string())?;
            audio.save_to(&mut file, WriteOptions::default()).map_err(err)?;
        }
        FileType::Mp4 => {
            let mut audio = lofty::mp4::Mp4File::read_from(&mut file, parse).map_err(err)?;
            let mut tag = audio.ilst_mut().map(std::mem::take).unwrap_or_default();
            apply(&mut tag, edit);
            if let Some(data) = &edit.cover {
                // MP4 covers have no kinds: the new one replaces them all.
                tag.remove_pictures();
                tag.insert_picture(front(data));
            }
            audio.set_ilst(tag);
            file.seek(SeekFrom::Start(0)).map_err(|e| e.to_string())?;
            audio.save_to(&mut file, WriteOptions::default()).map_err(err)?;
        }
        _ => return Err("not a format Odeion can write tags into".into()),
    }
    Ok(())
}

fn set_id3(tag: &mut Id3v2Tag, edit: &Edit) {
    // lofty's `set_date` adds a date frame beside one read from the file instead of
    // replacing it, and the old one still reads first: the year never changed.
    if edit.year.is_some()
        && let Ok(id) = lofty::id3::v2::FrameId::new("TDRC")
    {
        tag.remove(&id).for_each(drop);
    }
    apply(tag, edit);
    if let Some(data) = &edit.cover {
        for kind in COVERS {
            tag.remove_picture_type(kind);
        }
        tag.insert_picture(front(data));
    }
}

/// The fields of `edit` set or removed in any format's own tag.
fn apply(tag: &mut impl Accessor, edit: &Edit) {
    if let Some(v) = &edit.title {
        if v.is_empty() { tag.remove_title() } else { tag.set_title(v.clone()) }
    }
    if let Some(v) = &edit.artist {
        if v.is_empty() { tag.remove_artist() } else { tag.set_artist(v.clone()) }
    }
    if let Some(v) = &edit.album {
        if v.is_empty() { tag.remove_album() } else { tag.set_album(v.clone()) }
    }
    match edit.track {
        Some(Some(n)) => tag.set_track(n),
        Some(None) => tag.remove_track(),
        None => {}
    }
    match edit.disc {
        Some(Some(n)) => tag.set_disk(n),
        Some(None) => tag.remove_disk(),
        None => {}
    }
    match edit.year {
        Some(Some(year)) => tag.set_date(Timestamp { year: year.min(u16::MAX as u32) as u16, ..Timestamp::default() }),
        Some(None) => tag.remove_date(),
        None => {}
    }
}

fn front(data: &[u8]) -> Picture {
    let mime = if data.starts_with(b"\x89PNG") { MimeType::Png } else { MimeType::Jpeg };
    Picture::unchecked(data.to_vec()).pic_type(PictureType::CoverFront).mime_type(mime).build()
}

/// The tag items an edit may change.
fn edited(edit: &Edit) -> Vec<ItemKey> {
    let mut keys = Vec::new();
    for (asked, these) in [
        (edit.title.is_some(), &[ItemKey::TrackTitle][..]),
        (edit.artist.is_some(), &[ItemKey::TrackArtist]),
        (edit.album.is_some(), &[ItemKey::AlbumTitle]),
        (edit.track.is_some(), &[ItemKey::TrackNumber]),
        (edit.disc.is_some(), &[ItemKey::DiscNumber]),
        (edit.year.is_some(), &[ItemKey::RecordingDate, ItemKey::Year]),
    ] {
        if asked {
            keys.extend_from_slice(these);
        }
    }
    keys
}

/// Everything in a file's main tag an edit leaves alone, to compare before and after.
fn untouched(tag: Option<&lofty::tag::Tag>, edit: &Edit) -> (Vec<String>, Vec<(PictureType, Vec<u8>)>) {
    let Some(tag) = tag else { return Default::default() };
    let keys = edited(edit);
    let mut items: Vec<String> = tag
        .items()
        .filter(|i| !keys.contains(&i.key()))
        .map(|i| format!("{:?}={:?}", i.key(), i.value()))
        .collect();
    items.sort();
    let pictures = tag
        .pictures()
        .iter()
        .filter(|p| edit.cover.is_none() || !COVERS.contains(&p.pic_type()))
        .map(|p| (p.pic_type(), p.data().to_vec()))
        .collect();
    (items, pictures)
}

/// Reads the written copy back: the same audio (length, rate, channels) as the
/// original, every field changed as asked, and every other tag and picture as it was.
fn check(original: &Path, copy: &Path, kind: FileType, edit: &Edit) -> Result<(), String> {
    let read = |path: &Path| {
        Probe::open(path).map(|p| p.set_file_type(kind)).and_then(|p| p.read()).map_err(|e| e.to_string())
    };
    let (before, after) = (read(original)?, read(copy)?);
    let audio = |f: &lofty::file::TaggedFile| {
        let p = f.properties();
        (p.duration(), p.sample_rate(), p.channels())
    };
    if audio(&before) != audio(&after) {
        return Err("writing the tags would have changed the audio".into());
    }
    let (was, now) = (untouched(before.primary_tag(), edit), untouched(after.primary_tag(), edit));
    if was.1 != now.1 {
        return Err("writing would have changed its other pictures".into());
    }
    // Something new may appear (lofty names itself in a FLAC comment block it creates);
    // nothing may go or change.
    if let Some(item) = was.0.iter().find(|i| !now.0.contains(i)) {
        let what = item.split('=').next().unwrap_or_default();
        return Err(format!("writing would have lost or changed a tag Odeion doesn't edit ({what})"));
    }
    // An edit that only takes things out of a file without other tags leaves none at
    // all: every field then reads back empty, as asked.
    let empty = lofty::tag::Tag::new(after.primary_tag_type());
    let tag = after.primary_tag().unwrap_or(&empty);
    let text = |asked: &Option<String>, got: Option<std::borrow::Cow<str>>| match asked {
        Some(v) => got.map(|g| g.trim().to_string()).unwrap_or_default() == *v,
        None => true,
    };
    let number = |asked: Option<Option<u32>>, got: Option<u32>| asked.is_none_or(|n| n == got);
    let cover = edit.cover.as_ref().is_none_or(|data| tag.pictures().iter().any(|p| p.data() == data.as_slice()));
    let wrong = [
        (text(&edit.title, tag.title()), "title"),
        (text(&edit.artist, tag.artist()), "artist"),
        (text(&edit.album, tag.album()), "album"),
        (number(edit.track, tag.track()), "track"),
        (number(edit.disc, tag.disk()), "disc"),
        (number(edit.year, tag.date().map(|d| u32::from(d.year))), "year"),
        (cover, "cover"),
    ]
    .into_iter()
    .find(|(ok, _)| !ok);
    match wrong {
        Some((_, what)) => Err(format!("the {what} didn't read back as written")),
        None => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lofty::tag::Tag;

    fn track(title: &str, album: &str, number: u32) -> Track {
        let mut t = Track::from_path(Path::new("/m"), Path::new(&format!("/m/A/{album}/{title}.mp3")), 0, 0);
        (t.title, t.artist, t.album, t.number, t.year) = (title.into(), "Dio".into(), album.into(), Some(number), Some(1983));
        t
    }

    #[test]
    fn one_song_has_every_field_and_an_album_those_it_shares() {
        let (a, b) = (track("Holy Diver", "Holy Diver", 2), track("Rainbow in the Dark", "Holy Diver", 9));
        let one = Sheet::new(&[&a]).unwrap();
        assert_eq!(one.rows.len(), 6);
        assert_eq!(one.rows[0].value, "Holy Diver");
        assert_eq!(one.rows[3].value, "2");
        let both = Sheet::new(&[&a, &b]).unwrap();
        let fields: Vec<Field> = both.rows.iter().map(|e| e.field).collect();
        assert_eq!(fields, [Field::Artist, Field::Album, Field::Disc, Field::Year]);
        assert!(Sheet::new(&[]).is_none());
    }

    #[test]
    fn songs_that_differ_change_only_when_something_is_typed() {
        let (a, mut b) = (track("Stand Up", "Holy Diver", 1), track("Egypt", "The Last in Line", 9));
        b.year = Some(1984);
        let mut sheet = Sheet::new(&[&a, &b]).unwrap();
        let album = &sheet.rows[1];
        assert!(album.mixed() && album.value.is_empty() && !album.changed());
        assert!(!sheet.changes());
        sheet.select(3);
        sheet.type_text("1985");
        assert_eq!(sheet.edit(None), Edit { year: Some(Some(1985)), ..Edit::default() });
    }

    #[test]
    fn typing_replaces_a_field_selected_whole_then_adds_to_it() {
        let a = track("Holy Diver", "Holy Diver", 2);
        let mut sheet = Sheet::new(&[&a]).unwrap();
        sheet.type_text("Gypsy");
        sheet.type_text("!");
        assert_eq!(sheet.rows[0].value, "Gypsy!");
        sheet.backspace();
        assert_eq!(sheet.rows[0].value, "Gypsy");
        sheet.step(true);
        sheet.backspace();
        assert_eq!(sheet.rows[1].value, "", "⌫ clears a field selected whole");
        assert_eq!(sheet.edit(None), Edit { title: Some("Gypsy".into()), artist: Some(String::new()), ..Edit::default() });
    }

    #[test]
    fn enter_edits_a_field_from_its_end_and_esc_puts_it_back() {
        let a = track("Holy Diver", "Holy Diver", 2);
        let mut sheet = Sheet::new(&[&a]).unwrap();
        sheet.start();
        sheet.type_text(" (live)");
        assert_eq!(sheet.rows[0].value, "Holy Diver (live)", "Enter keeps the field, typing adds to it");
        assert!(sheet.finish());
        assert!(!sheet.finish(), "done editing");
        // Typing replaces again once done; Esc brings back what was there.
        sheet.type_text("Gypsy");
        assert!(sheet.cancel());
        assert_eq!(sheet.rows[0].value, "Holy Diver (live)");
        assert!(!sheet.cancel(), "Esc with nothing being edited is the sheet's");
        // A cleared field comes back with Esc too.
        sheet.backspace();
        assert_eq!(sheet.rows[0].value, "");
        sheet.cancel();
        assert_eq!(sheet.rows[0].value, "Holy Diver (live)");
        // Moving on keeps what was typed, as Tab in a spreadsheet does.
        sheet.start();
        sheet.type_text("!");
        sheet.step(true);
        assert_eq!(sheet.rows[0].value, "Holy Diver (live)!");
        assert!(!sheet.cancel(), "nothing being edited on the next row");
        // On the cover row there's nothing to edit.
        sheet.select(sheet.rows.len());
        sheet.start();
        assert!(sheet.editing.is_none());
    }

    #[test]
    fn number_fields_take_digits_up_to_their_length() {
        let a = track("Holy Diver", "Holy Diver", 2);
        let mut sheet = Sheet::new(&[&a]).unwrap();
        sheet.select(3);
        sheet.type_text("x");
        sheet.type_text(" ");
        assert_eq!(sheet.rows[3].value, "2", "a letter or space leaves the number as it was");
        sheet.type_text("1a2-3 45");
        assert_eq!(sheet.rows[3].value, "123");
        sheet.select(5);
        sheet.type_text("19834");
        assert_eq!(sheet.rows[5].value, "1983");
        assert!(!sheet.rows[5].changed(), "typed back as it was");
        sheet.select(4);
        sheet.type_text("0");
        assert_eq!(sheet.edit(None).disc, Some(None), "no disc 0: the disc goes");
    }

    #[test]
    fn the_cover_is_the_row_after_the_fields() {
        let a = track("Holy Diver", "Holy Diver", 2);
        let mut sheet = Sheet::new(&[&a]).unwrap();
        for _ in 0..10 {
            sheet.step(true);
        }
        assert!(sheet.on_cover());
        sheet.type_text("x");
        assert!(!sheet.changes(), "typing on the cover row changes nothing");
        sheet.cover = Some("/c.jpg".into());
        assert!(sheet.changes());
    }

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("odeion-tags-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// Bytes that look like nothing, standing in for audio.
    fn noise(n: usize) -> Vec<u8> {
        let mut x: u32 = 0x1234_5678;
        (0..n)
            .map(|_| {
                x ^= x << 13;
                x ^= x >> 17;
                x ^= x << 5;
                x as u8
            })
            .collect()
    }

    /// A second of 8 kHz 16-bit mono PCM.
    fn wav(audio: &[u8]) -> Vec<u8> {
        let mut f = b"RIFF".to_vec();
        f.extend((36 + audio.len() as u32).to_le_bytes());
        f.extend(b"WAVEfmt ");
        f.extend(16u32.to_le_bytes());
        f.extend(1u16.to_le_bytes());
        f.extend(1u16.to_le_bytes());
        f.extend(8000u32.to_le_bytes());
        f.extend(16000u32.to_le_bytes());
        f.extend(2u16.to_le_bytes());
        f.extend(16u16.to_le_bytes());
        f.extend(b"data");
        f.extend((audio.len() as u32).to_le_bytes());
        f.extend(audio);
        f
    }

    /// A FLAC file's stream info for a second at 8 kHz, then `audio` as its frames.
    fn flac(audio: &[u8]) -> Vec<u8> {
        let mut f = b"fLaC".to_vec();
        f.extend([0x80, 0, 0, 34]);
        f.extend(4096u16.to_be_bytes());
        f.extend(4096u16.to_be_bytes());
        f.extend([0; 6]);
        f.extend(((8000u64 << 44) | (15 << 36) | 8000).to_be_bytes());
        f.extend([0; 16]);
        f.extend([0xFF, 0xF8]);
        f.extend(audio);
        f
    }

    /// An AIFF file of `audio` as 16-bit mono at 44.1 kHz (an even number of bytes).
    fn aiff(audio: &[u8]) -> Vec<u8> {
        let mut comm = 1u16.to_be_bytes().to_vec();
        comm.extend((audio.len() as u32 / 2).to_be_bytes());
        comm.extend(16u16.to_be_bytes());
        // 44,100 as an 80-bit extended float.
        comm.extend([0x40, 0x0E, 0xAC, 0x44, 0, 0, 0, 0, 0, 0]);
        let mut body = b"AIFF".to_vec();
        body.extend(b"COMM");
        body.extend((comm.len() as u32).to_be_bytes());
        body.extend(comm);
        body.extend(b"SSND");
        body.extend((audio.len() as u32 + 8).to_be_bytes());
        body.extend([0; 8]);
        body.extend(audio);
        let mut f = b"FORM".to_vec();
        f.extend((body.len() as u32).to_be_bytes());
        f.extend(body);
        f
    }

    fn encoded(format: crate::export::Format) -> Vec<u8> {
        let tone = (0..44_100).map(|i| (i as f32 * 0.05).sin() * 0.3).collect();
        crate::export::encode(&crate::export::Rendered { channels: 1, rate: 44_100, samples: tone }, format).unwrap()
    }

    fn png(width: u32, height: u32) -> Vec<u8> {
        let mut out = std::io::Cursor::new(Vec::new());
        image::RgbImage::from_pixel(width, height, image::Rgb([200, 40, 40]))
            .write_to(&mut out, image::ImageFormat::Png)
            .unwrap();
        out.into_inner()
    }

    fn every_field() -> Edit {
        Edit {
            title: Some("Rainbow in the Dark".into()),
            artist: Some("Dio".into()),
            album: Some("Holy Diver".into()),
            track: Some(Some(9)),
            disc: Some(Some(1)),
            year: Some(Some(1983)),
            cover: Some(png(8, 8)),
        }
    }

    /// Writes every field into `bytes` saved as `name`, reads it back as the scan does
    /// and checks the audio stayed: `audio`, if given, still in the file in one piece.
    fn round_trip(name: &str, bytes: Vec<u8>, audio: Option<&[u8]>) {
        let dir = scratch(name);
        let path = dir.join(name);
        std::fs::write(&path, bytes).unwrap();
        let edit = every_field();
        write(&path, &edit).unwrap();
        let file = Probe::open(&path).unwrap().read().unwrap();
        let tag = file.primary_tag().unwrap();
        assert_eq!(tag.title().as_deref(), Some("Rainbow in the Dark"), "{name}");
        assert_eq!(tag.artist().as_deref(), Some("Dio"), "{name}");
        assert_eq!(tag.album().as_deref(), Some("Holy Diver"), "{name}");
        assert_eq!((tag.track(), tag.disk()), (Some(9), Some(1)), "{name}");
        assert_eq!(tag.date().map(|d| d.year), Some(1983), "{name}");
        let front = tag.pictures().iter().find(|p| p.pic_type() == PictureType::CoverFront);
        assert_eq!(front.map(|p| p.data()), edit.cover.as_deref(), "{name}");
        if let Some(audio) = audio {
            let now = std::fs::read(&path).unwrap();
            assert!(now.windows(audio.len()).any(|w| w == audio), "{name}: the audio changed");
        }
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 1, "{name}: the copy is gone");
        // Taking a field away again.
        write(&path, &Edit { album: Some(String::new()), year: Some(None), ..Edit::default() }).unwrap();
        let file = Probe::open(&path).unwrap().read().unwrap();
        let tag = file.primary_tag().unwrap();
        assert_eq!((tag.album(), tag.date()), (None, None), "{name}");
        assert_eq!(tag.title().as_deref(), Some("Rainbow in the Dark"), "{name}: other fields stay");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn writes_mp3() {
        let mp3 = encoded(crate::export::Format::Mp3);
        round_trip("song.mp3", mp3.clone(), Some(&mp3));
    }

    #[test]
    fn writes_ogg() {
        round_trip("song.ogg", encoded(crate::export::Format::Ogg), None);
    }

    #[test]
    fn writes_wav_and_flac_leaving_their_audio_alone() {
        let audio = noise(16_000);
        round_trip("song.wav", wav(&audio), Some(&audio));
        round_trip("song.flac", flac(&audio), Some(&audio));
    }

    #[test]
    fn a_field_cleared_stays_cleared() {
        let dir = scratch("cleared");
        let song = dir.join("Dio").join("Holy Diver");
        std::fs::create_dir_all(&song).unwrap();
        let path = song.join("1. Rainbow in the Dark.wav");
        std::fs::write(&path, wav(&noise(16_000))).unwrap();
        let read = || crate::scan::reread(std::slice::from_ref(&dir), &path, &dir.join("covers"));
        // Without tags, the folders and the file name say what they can.
        let t = read();
        assert_eq!((t.title.as_str(), t.artist.as_str(), t.album.as_str(), t.number), ("Rainbow in the Dark", "Dio", "Holy Diver", Some(1)));
        // Taking out what isn't there, from a file with no tags: it reads back empty.
        write(&path, &Edit { track: Some(None), ..Edit::default() }).unwrap();
        // With a tag, the folders and the file name no longer fill in what was cleared.
        write(&path, &Edit { title: Some("Rainbow".into()), artist: Some(String::new()), track: Some(None), ..Edit::default() }).unwrap();
        let t = read();
        assert_eq!((t.title.as_str(), t.artist.as_str(), t.album.as_str(), t.number), ("Rainbow", "", "", None));
        // A title taken out still has a name to show: the file's.
        write(&path, &Edit { title: Some(String::new()), ..Edit::default() }).unwrap();
        assert_eq!(read().title, "Rainbow in the Dark");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn writes_aiff_leaving_its_audio_alone() {
        let audio = noise(16_000);
        round_trip("song.aiff", aiff(&audio), Some(&audio));
    }

    #[test]
    fn tags_odeion_doesnt_edit_stay() {
        for (name, bytes) in [("kept.mp3", encoded(crate::export::Format::Mp3)), ("kept.flac", flac(&noise(4000)))] {
            let dir = scratch(name);
            let path = dir.join(name);
            std::fs::write(&path, bytes).unwrap();
            let mut file = Probe::open(&path).unwrap().read().unwrap();
            let mut tag = Tag::new(file.primary_tag_type());
            tag.set_genre("Heavy metal".into());
            tag.insert_text(ItemKey::MusicBrainzRecordingId, "0b0f1a1e".into());
            (tag.set_track(2), tag.set_track_total(9), tag.set_disk(1), tag.set_disk_total(2));
            // A year already there, which lofty's ID3 kept beside the new one.
            tag.set_date(Timestamp { year: 1980, ..Timestamp::default() });
            file.insert_tag(tag);
            file.save_to_path(&path, WriteOptions::default()).unwrap();
            let edit = Edit {
                title: Some("Holy Diver".into()),
                track: Some(Some(3)),
                disc: Some(Some(2)),
                year: Some(Some(1983)),
                ..Edit::default()
            };
            write(&path, &edit).unwrap();
            let file = Probe::open(&path).unwrap().read().unwrap();
            let tag = file.primary_tag().unwrap();
            assert_eq!(tag.title().as_deref(), Some("Holy Diver"), "{name}");
            assert_eq!(tag.genre().as_deref(), Some("Heavy metal"), "{name}");
            assert_eq!(tag.get_string(ItemKey::MusicBrainzRecordingId), Some("0b0f1a1e"), "{name}");
            assert_eq!((tag.track(), tag.track_total(), tag.disk(), tag.disk_total()), (Some(3), Some(9), Some(2), Some(2)), "{name}");
            assert_eq!(tag.date().map(|d| d.year), Some(1983), "{name}");
            let _ = std::fs::remove_dir_all(dir);
        }
    }

    #[test]
    fn a_file_that_cant_take_tags_is_left_as_it_was() {
        let dir = scratch("broken");
        // Not audio, and a FLAC that breaks off after its marker: the second is copied
        // before it fails, and the copy must go.
        for (name, bytes) in [("notes.mp3", &b"not audio at all"[..]), ("cut.flac", &b"fLaC\x80\0\0\x22broken"[..])] {
            let path = dir.join(name);
            std::fs::write(&path, bytes).unwrap();
            assert!(write(&path, &every_field()).is_err(), "{name}");
            assert_eq!(std::fs::read(&path).unwrap(), bytes, "{name}");
            assert!(!dir.join(format!(".{name}.odeion")).exists(), "{name}: no copy left behind");
        }
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_large_cover_is_scaled_down_and_a_small_one_kept() {
        let dir = scratch("cover");
        let (big, small) = (dir.join("big.png"), dir.join("small.png"));
        std::fs::write(&big, png(1500, 300)).unwrap();
        std::fs::write(&small, png(600, 600)).unwrap();
        let scaled = image::load_from_memory(&cover(&big).unwrap()).unwrap();
        assert_eq!((scaled.width(), scaled.height()), (1000, 200));
        assert_eq!(cover(&small).unwrap(), png(600, 600));
        std::fs::write(&small, b"text").unwrap();
        assert!(cover(&small).is_err());
        let _ = std::fs::remove_dir_all(dir);
    }
}
