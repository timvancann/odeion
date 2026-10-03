//! The window: a search line, the library, and what's playing. Keyboard first; every
//! control can be clicked too.

use crate::audio::Audio;
use crate::keys::{self, Action};
use crate::library::{self, Track};
use crate::media::Media;
use crate::playlist::Playlist;
use crate::scan;
use crate::state::{self, Favorites, Settings};
use crate::theme::{self, Theme};
use crate::ui::{self, *};
use crate::wave::{self, Wave};
use gpui::{
    Bounds, Context, Div, FocusHandle, KeyDownEvent, MouseButton, MouseDownEvent, MouseMoveEvent,
    ObjectFit,
    PathPromptOptions, Pixels, ScrollStrategy, Stateful, UniformListScrollHandle, Window, canvas,
    div, fill, img, point, prelude::*, px, relative, rgb, size, uniform_list,
};
use std::cell::Cell;
use std::collections::HashMap;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::mpsc::Receiver;
use std::time::Duration;

const POLL: Duration = Duration::from_millis(40);
const ROW: f32 = 60.;
/// The transparent title bar's height, where the traffic lights sit.
const TITLEBAR: f32 = 32.;
const SEEK: f64 = 5.0;
/// Placing a loop's end plays this much before it.
const LEAD_IN: Duration = Duration::from_millis(1500);
/// How much the practice view shows around the playhead before any zooming.
const SPAN: Duration = Duration::from_secs(8);
/// A click this close to a loop edge grabs it.
const GRAB: f32 = 8.;
/// How long a scan's result stays in the search bar.
const RESULT_FOR: Duration = Duration::from_secs(8);
/// Theme names the picker shows at once.
const PICKER_ROWS: usize = 8;
/// Save the cache now and then while tags come in, so a quit mid-scan keeps most of it.
const SAVE_EVERY: usize = 250;


/// The ⌘, sheet: the highlighted row (each folder, then "add folder", the theme, vim
/// keys), and the theme picker when it is open.
#[derive(Default)]
struct Prefs {
    row: usize,
    picker: Option<Picker>,
    /// The first `d` of `dd` was pressed.
    delete: bool,
}

/// Browsing themes, as in Jasmijn: typing filters, moving previews, Enter keeps, Esc
/// restores the saved one.
struct Picker {
    themes: Vec<Theme>,
    query: String,
    selected: usize,
}

impl Picker {
    fn shown(&self) -> Vec<&Theme> {
        theme::filter(&self.themes, &self.query)
    }

    fn highlighted(&self) -> Option<&Theme> {
        self.shown().get(self.selected).copied()
    }
}

/// Something drawn along the song: the progress bar, and the practice view's overview
/// and detail. Painting records where it is and which stretch of the song it shows, so
/// clicks and drags can turn a position into a moment.
#[derive(Clone, Default)]
struct Strip(Rc<Cell<(Bounds<Pixels>, Duration, Duration)>>);

impl Strip {
    /// The moment under `x`, and how many seconds a pixel there is.
    fn time_at(&self, x: Pixels) -> Option<(Duration, f64)> {
        let (bounds, from, to) = self.0.get();
        let width = f32::from(bounds.size.width);
        if width <= 0. || to <= from {
            return None;
        }
        let fraction = (f32::from(x - bounds.origin.x) / width).clamp(0., 1.) as f64;
        let span = (to - from).as_secs_f64();
        Some((from + Duration::from_secs_f64(span * fraction), span / width as f64))
    }

    fn set(&self, bounds: Bounds<Pixels>, from: Duration, to: Duration) {
        self.0.set((bounds, from, to));
    }
}

/// A loop edge, being placed or dragged.
#[derive(Clone, Copy, PartialEq)]
enum Edge {
    Start,
    End,
}

enum Row {
    Header(String),
    Track(usize),
}

pub struct Riff {
    focus: FocusHandle,
    tracks: Vec<Track>,
    by_path: HashMap<PathBuf, usize>,
    scanning: Option<Receiver<(Vec<Track>, Vec<Track>)>>,
    tagging: Option<Receiver<Track>>,
    tags_left: usize,
    tags_total: usize,
    /// What the last scan changed, and since when it shows.
    scan_note: Option<(String, std::time::Instant)>,
    unsaved: usize,
    query: String,
    /// Whether keys type into the search (⌘P). Otherwise they control playback.
    searching: bool,
    /// The ⌘? sheet is open.
    help: bool,
    settings: Settings,
    /// The ⌘, sheet, when open.
    prefs: Option<Prefs>,
    /// The first key of a two-key vim command (`gg`, `zz`, `dd`).
    pending: Option<char>,
    /// The loop edge being dragged, and on which strip.
    dragging: Option<(Edge, Strip)>,
    /// The practice view (`P`) instead of the list.
    practice: bool,
    /// The playing song's waveform, once read; read for every song, shown or not.
    wave: Option<(PathBuf, Rc<Wave>)>,
    reading: Option<(PathBuf, Receiver<Result<Wave, String>>)>,
    wave_error: Option<String>,
    /// How much of the song the detail view shows.
    span: Duration,
    overview: Strip,
    detail: Strip,
    /// When the window last moved or resized and that isn't saved yet.
    moved: Option<std::time::Instant>,
    rows: Vec<Row>,
    selected: usize,
    scroll: UniformListScrollHandle,
    audio: Audio,
    now: Option<Track>,
    favorites: Favorites,
    /// The album filter (⌘A), by one of its tracks.
    album: Option<Track>,
    /// What plays next and before: the list the song was started from.
    playlist: Playlist,
    shuffle: bool,
    /// The media keys and Now Playing, when macOS lets riff have them.
    media: Option<Media>,
    message: Option<(String, bool)>,
    bar: Strip,
}

fn cache_file() -> PathBuf {
    state::data_dir().join("library.json")
}

fn favorites_file() -> PathBuf {
    state::data_dir().join("favorites.json")
}

pub fn settings_file() -> PathBuf {
    state::data_dir().join("settings.json")
}

impl Riff {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let focus = cx.focus_handle();
        focus.focus(window);
        cx.spawn_in(window, async move |this, cx| {
            loop {
                cx.background_executor().timer(POLL).await;
                if this.update(cx, |riff, cx| riff.poll(cx)).is_err() {
                    break;
                }
            }
        })
        .detach();

        cx.observe_window_bounds(window, |riff, window, cx| {
            let bounds = window.window_bounds().get_bounds();
            let Some(display) = window.display(cx).and_then(|d| d.uuid().ok()) else {
                return;
            };
            riff.settings.window = Some(state::Placement {
                display: display.to_string(),
                x: bounds.origin.x.into(),
                y: bounds.origin.y.into(),
                width: bounds.size.width.into(),
                height: bounds.size.height.into(),
            });
            riff.moved = Some(std::time::Instant::now());
        })
        .detach();

        let mut tracks = scan::load_cache(&cache_file());
        library::sort(&mut tracks);
        let settings = Settings::load(&settings_file());
        if settings.theme != theme::DEFAULT_NAME
            && let Some(t) = theme::available(&theme::zed_dirs())
                .into_iter()
                .find(|t| t.name == settings.theme)
        {
            theme::set_active(t);
        }

        let mut riff = Riff {
            focus,
            tracks,
            by_path: HashMap::new(),
            scanning: None,
            tagging: None,
            tags_left: 0,
            tags_total: 0,
            scan_note: None,
            unsaved: 0,
            query: String::new(),
            searching: false,
            help: false,
            settings,
            prefs: None,
            moved: None,
            pending: None,
            dragging: None,
            practice: false,
            wave: None,
            reading: None,
            wave_error: None,
            span: SPAN,
            overview: Strip::default(),
            detail: Strip::default(),
            rows: Vec::new(),
            selected: 0,
            scroll: UniformListScrollHandle::new(),
            audio: Audio::new(),
            now: None,
            favorites: Favorites::load(&favorites_file()),
            album: None,
            playlist: Playlist::default(),
            shuffle: false,
            media: Media::new(),
            message: None,
            bar: Strip::default(),
        };
        riff.message = riff.audio.error.clone().map(|e| (e, true));
        riff.reindex(None);
        riff.rescan();
        riff
    }

    /// Walks the folders again in the background. The list stays as it was meanwhile.
    fn rescan(&mut self) {
        let roots = self.settings.folders.clone();
        let cached = self.tracks.clone();
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let found = scan::walk_all(&roots);
            let _ = tx.send(scan::merge(&roots, found, cached));
        });
        self.scanning = Some(rx);
        // Tags still being read belong to the old list; the new merge queues them again.
        self.tagging = None;
    }

    fn save_settings(&mut self) {
        if let Err(e) = self.settings.save(&settings_file()) {
            self.message = Some((format!("couldn't save settings: {e}"), true));
        }
    }

    /// Asks macOS for folders to add, then scans them.
    fn pick_folders(&mut self, cx: &mut Context<Self>) {
        let picked = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: true,
            prompt: Some("Add".into()),
        });
        cx.spawn(async move |this, cx| {
            if let Ok(Ok(Some(folders))) = picked.await {
                let _ = this.update(cx, |riff, cx| {
                    if riff.settings.add_folders(folders) {
                        riff.save_settings();
                        riff.rescan();
                    }
                    cx.notify();
                });
            }
        })
        .detach();
    }

    fn remove_folder(&mut self, i: usize) {
        if i < self.settings.folders.len() {
            self.settings.folders.remove(i);
            self.save_settings();
            self.rescan();
        }
    }

    fn open_picker(&mut self) {
        let themes = theme::available(&theme::zed_dirs());
        let selected = themes
            .iter()
            .position(|t| t.name == self.settings.theme)
            .unwrap_or(0);
        if let Some(prefs) = &mut self.prefs {
            prefs.picker = Some(Picker { themes, query: String::new(), selected });
        }
    }

    /// Keeps a theme: draws with it and saves it.
    fn keep_theme(&mut self, t: Theme) {
        self.settings.theme = t.name.clone();
        theme::set_active(t);
        self.save_settings();
        if let Some(prefs) = &mut self.prefs {
            prefs.picker = None;
        }
    }

    /// Closes the ⌘, sheet, dropping a theme that was only being previewed.
    fn close_prefs(&mut self) {
        if let Some(Prefs { picker: Some(picker), .. }) = self.prefs.take()
            && let Some(t) = picker.themes.into_iter().find(|t| t.name == self.settings.theme)
        {
            theme::set_active(t);
        }
    }

    fn toggle_vim(&mut self) {
        self.settings.vim = !self.settings.vim;
        self.pending = None;
        self.save_settings();
    }

    /// Keys while the ⌘, sheet is open.
    fn prefs_key(&mut self, k: &gpui::Keystroke) {
        let (cmd, ctrl) = (k.modifiers.platform, k.modifiers.control);
        let Some(prefs) = &mut self.prefs else { return };
        let vim = self.settings.vim && prefs.picker.is_none() && !cmd && !ctrl;
        let up = k.key == "up" || (ctrl && k.key == "k") || (vim && k.key == "k");
        let down = k.key == "down" || (ctrl && k.key == "j") || (vim && k.key == "j");
        let dd = vim && k.key == "d" && std::mem::take(&mut prefs.delete);
        prefs.delete = vim && k.key == "d" && !dd;
        if let Some(picker) = &mut prefs.picker {
            let n = picker.shown().len();
            match k.key.as_str() {
                "escape" => {
                    // Back to the saved theme, from before browsing.
                    if let Some(t) = picker.themes.iter().find(|t| t.name == self.settings.theme) {
                        theme::set_active(t.clone());
                    }
                    prefs.picker = None;
                    return;
                }
                "enter" => {
                    if let Some(t) = picker.highlighted().cloned() {
                        self.keep_theme(t);
                    }
                    return;
                }
                _ if up && n > 0 => picker.selected = (picker.selected + n - 1) % n,
                _ if down && n > 0 => picker.selected = (picker.selected + 1) % n,
                "backspace" => {
                    picker.query.pop();
                    picker.selected = 0;
                }
                _ => {
                    let Some(text) = k.key_char.as_ref().filter(|_| !cmd && !ctrl) else {
                        return;
                    };
                    picker.query.push_str(text);
                    picker.selected = 0;
                }
            }
            if let Some(t) = picker.highlighted() {
                theme::set_active(t.clone());
            }
            return;
        }
        let folders = self.settings.folders.len();
        let row = prefs.row;
        match k.key.as_str() {
            "escape" => self.close_prefs(),
            _ if up => prefs.row = row.saturating_sub(1),
            _ if down => prefs.row = (row + 1).min(folders + 2),
            _ if dd && row < folders => {
                self.remove_folder(row);
                if let Some(prefs) = &mut self.prefs {
                    prefs.row = row.min(self.settings.folders.len());
                }
            }
            "backspace" if row < folders => {
                self.remove_folder(row);
                if let Some(prefs) = &mut self.prefs {
                    prefs.row = row.min(self.settings.folders.len());
                }
            }
            "enter" | "space" if row == folders + 1 => self.open_picker(),
            "enter" | "space" if row == folders + 2 => self.toggle_vim(),
            _ => {}
        }
    }

    fn poll(&mut self, cx: &mut Context<Self>) {
        let mut changed = self.audio.tick();
        if self.scan_note.as_ref().is_some_and(|(_, since)| since.elapsed() > RESULT_FOR)
            && self.tagging.is_none()
        {
            self.scan_note = None;
            changed = true;
        }
        // Saved once a drag or resize settles, not on every step of it.
        if self.moved.is_some_and(|m| m.elapsed() > Duration::from_millis(500)) {
            self.moved = None;
            self.save_settings();
        }
        if let Some((path, rx)) = &self.reading
            && let Ok(result) = rx.try_recv()
        {
            match result {
                Ok(w) => self.wave = Some((path.clone(), Rc::new(w))),
                Err(e) => self.wave_error = Some(e),
            }
            self.reading = None;
            changed = true;
        }
        if self.audio.ended() && self.next_song() {
            changed = true;
        }
        let events: Vec<_> = self.media.iter().flat_map(|m| m.events.try_iter()).collect();
        for event in events {
            self.media_event(event);
            changed = true;
        }
        if let (Some(media), Some(now)) = (&mut self.media, &self.now) {
            media.show(now, self.audio.duration);
            media.status(self.audio.playing(), self.audio.position());
        }
        if let Some((tracks, todo)) = self.scanning.as_ref().and_then(|rx| rx.try_recv().ok()) {
            self.scanning = None;
            let keep = self.selected_path();
            let (new, gone) = scan::changes(&self.tracks, &tracks);
            self.scan_note = ui::scan_result(new, gone).map(|n| (n, std::time::Instant::now()));
            self.tracks = tracks;
            self.tags_left = todo.len();
            self.tags_total = todo.len();
            if !todo.is_empty() {
                self.tagging = Some(scan::tag_in_background(
                    todo,
                    state::data_dir().join("covers"),
                ));
            }
            self.reindex(keep);
            scan::save_cache(&cache_file(), &self.tracks);
            changed = true;
        }
        if let Some(rx) = &self.tagging {
            let mut done = false;
            let mut tagged = Vec::new();
            loop {
                match rx.try_recv() {
                    Ok(track) => tagged.push(track),
                    Err(std::sync::mpsc::TryRecvError::Empty) => break,
                    Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                        done = true;
                        break;
                    }
                }
            }
            if !tagged.is_empty() {
                self.tags_left = self.tags_left.saturating_sub(tagged.len());
                self.unsaved += tagged.len();
                for track in tagged {
                    if let Some(&i) = self.by_path.get(&track.path) {
                        if self.now.as_ref().is_some_and(|n| n.path == track.path) {
                            self.now = Some(track.clone());
                        }
                        self.tracks[i] = track;
                    }
                }
                self.reindex(self.selected_path());
                changed = true;
            }
            if done || self.unsaved >= SAVE_EVERY {
                scan::save_cache(&cache_file(), &self.tracks);
                self.unsaved = 0;
            }
            if done {
                self.tagging = None;
                // The result shows once everything is read, not while the tags hide it.
                if let Some((_, since)) = &mut self.scan_note {
                    *since = std::time::Instant::now();
                }
                changed = true;
            }
        }
        if changed {
            cx.notify();
        }
    }

    fn selected_path(&self) -> Option<PathBuf> {
        self.selected_track().map(|t| t.path.clone())
    }

    /// Re-sorts, rebuilds the lookup and the rows, keeping `keep` selected. Take it before
    /// replacing the tracks: the selection is a row, which means nothing in a new list.
    fn reindex(&mut self, keep: Option<PathBuf>) {
        library::sort(&mut self.tracks);
        self.by_path = self
            .tracks
            .iter()
            .enumerate()
            .map(|(i, t)| (t.path.clone(), i))
            .collect();
        let was = self.selected;
        self.rebuild_rows();
        // A favourite is listed twice; stay on the copy nearest the old cursor.
        if let Some(path) = keep
            && let Some(&i) = self.by_path.get(&path)
            && let Some(row) = (0..self.rows.len())
                .filter(|&r| matches!(self.rows[r], Row::Track(t) if t == i))
                .min_by_key(|&r| r.abs_diff(was))
        {
            self.selected = row;
        }
    }

    fn rebuild_rows(&mut self) {
        self.rows.clear();
        if let Some(of) = &self.album {
            // The album's header sits above the list, see `render_album`.
            self.rows.extend(
                library::album(&self.tracks, of)
                    .into_iter()
                    .map(Row::Track),
            );
        } else if self.query.trim().is_empty() {
            // Library order, since the tracks are sorted.
            let mut favorites: Vec<usize> = self
                .favorites
                .0
                .iter()
                .filter_map(|p| self.by_path.get(p).copied())
                .collect();
            favorites.sort();
            if !favorites.is_empty() {
                self.rows.push(Row::Header("Favourites".into()));
                self.rows.extend(favorites.into_iter().map(Row::Track));
                self.rows.push(Row::Header("Library".into()));
            }
            self.rows.extend((0..self.tracks.len()).map(Row::Track));
        } else {
            self.rows.extend(
                library::search(&self.tracks, &self.query)
                    .into_iter()
                    .map(Row::Track),
            );
        }
        self.selected = self.first_track_row(0, 1).unwrap_or(0);
    }

    /// The first track row from `from`, stepping by `step`.
    fn first_track_row(&self, from: usize, step: isize) -> Option<usize> {
        let mut i = from as isize;
        while i >= 0 && (i as usize) < self.rows.len() {
            if matches!(self.rows[i as usize], Row::Track(_)) {
                return Some(i as usize);
            }
            i += step;
        }
        None
    }

    fn selected_track(&self) -> Option<&Track> {
        match self.rows.get(self.selected)? {
            Row::Track(i) => self.tracks.get(*i),
            Row::Header(_) => None,
        }
    }

    fn move_selection(&mut self, by: isize) {
        let target = (self.selected as isize + by).clamp(0, self.rows.len() as isize - 1);
        let step = if by < 0 { -1 } else { 1 };
        if let Some(row) = self
            .first_track_row(target.max(0) as usize, step)
            .or_else(|| self.first_track_row(target.max(0) as usize, -step))
        {
            self.selected = row;
            self.scroll.scroll_to_item(row, ScrollStrategy::Top);
        }
    }

    fn play(&mut self, track: Track) {
        self.audio.play(&track.path);
        if self.wave.as_ref().is_none_or(|(p, _)| *p != track.path) {
            let (tx, rx) = std::sync::mpsc::channel();
            let path = track.path.clone();
            std::thread::spawn(move || {
                let _ = tx.send(wave::read(&path));
            });
            self.reading = Some((track.path.clone(), rx));
            self.wave = None;
            self.wave_error = None;
        }
        self.now = Some(track);
        self.message = None;
    }

    /// Plays the selected song. The rest of its list follows: the album, the favourites,
    /// the search results or the library, whichever it was picked from.
    fn play_selected(&mut self) {
        let Some(track) = self.selected_track().cloned() else {
            return;
        };
        // The list is the run of songs between the headers around the selection.
        let is_track = |r: &usize| matches!(self.rows[*r], Row::Track(_));
        let start = (0..self.selected).rev().take_while(is_track).last().unwrap_or(self.selected);
        let list = (start..self.rows.len())
            .take_while(is_track)
            .filter_map(|r| match self.rows[r] {
                Row::Track(i) => Some(self.tracks[i].path.clone()),
                Row::Header(_) => None,
            })
            .collect();
        self.playlist = Playlist::new(list, &track.path, self.seed());
        self.play(track);
    }

    /// A fresh shuffle when shuffling, none otherwise.
    fn seed(&self) -> Option<u64> {
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH);
        self.shuffle.then(|| now.map_or(1, |d| d.as_nanos() as u64))
    }

    fn track_at(&self, path: &PathBuf) -> Option<Track> {
        self.by_path.get(path).map(|&i| self.tracks[i].clone())
    }

    /// Plays the next song of the list. Returns whether there was one.
    fn next_song(&mut self) -> bool {
        // A song gone since a rescan is skipped.
        while let Some(path) = self.playlist.next().cloned() {
            if let Some(track) = self.track_at(&path) {
                self.play(track);
                return true;
            }
        }
        false
    }

    /// The song before, or this one from the start when it is well underway.
    fn previous_song(&mut self) {
        let Some(path) = self.playlist.previous(self.audio.position()).cloned() else {
            return;
        };
        let current = self.now.as_ref().is_some_and(|n| n.path == path);
        if current && !self.audio.ended() {
            // Seeking keeps the A/B loop, which starting the song again would clear.
            self.audio.seek(Duration::ZERO);
        } else if let Some(track) = self.track_at(&path) {
            self.play(track);
        }
    }

    fn toggle_shuffle(&mut self) {
        self.shuffle = !self.shuffle;
        if let Some(now) = self.now.clone() {
            let seed = self.seed();
            self.playlist.reorder(&now.path, seed);
        }
    }

    fn media_event(&mut self, event: souvlaki::MediaControlEvent) {
        use souvlaki::{MediaControlEvent as E, SeekDirection};
        match event {
            E::Toggle => self.audio.toggle(),
            E::Play if !self.audio.playing() => self.audio.toggle(),
            E::Pause | E::Stop => self.audio.pause(),
            E::Next => {
                self.next_song();
            }
            E::Previous => self.previous_song(),
            E::SetPosition(at) => self.audio.seek(at.0),
            E::Seek(SeekDirection::Forward) => self.audio.seek_by(SEEK),
            E::Seek(SeekDirection::Backward) => self.audio.seek_by(-SEEK),
            E::SeekBy(SeekDirection::Forward, by) => self.audio.seek_by(by.as_secs_f64()),
            E::SeekBy(SeekDirection::Backward, by) => self.audio.seek_by(-by.as_secs_f64()),
            _ => {}
        }
    }

    fn toggle_favorite(&mut self) {
        let Some(path) = self.selected_path() else { return };
        self.favorites.toggle(&path);
        self.favorites.save(&favorites_file());
        self.reindex(Some(path));
        self.scroll.scroll_to_item(self.selected, ScrollStrategy::Center);
    }

    /// ⌘A: show only the selected song's album, or go back to what was shown before.
    fn toggle_album(&mut self) {
        let keep = self.selected_path();
        self.album = match self.album {
            Some(_) => None,
            None => self.selected_track().cloned(),
        };
        self.reindex(keep);
        self.scroll.scroll_to_item(self.selected, ScrollStrategy::Center);
    }

    /// Opens a song in Transcribe!, pausing riff.
    fn transcribe(&mut self, track: Option<Track>) {
        let Some(track) = track else { return };
        self.audio.pause();
        self.message = Some(match state::open_in_transcribe(&track) {
            Ok(()) => (format!("opened {} in Transcribe!", track.title), false),
            Err(e) => (e, true),
        });
    }

    fn key(&mut self, event: &KeyDownEvent, _window: &mut Window, cx: &mut Context<Self>) {
        let k = &event.keystroke;
        let press = keys::Press {
            key: &k.key,
            cmd: k.modifiers.platform,
            ctrl: k.modifiers.control,
            shift: k.modifiers.shift,
        };
        let global = keys::anywhere(press);
        // The sheets swallow every key but their own toggles and ⌘Q.
        match global {
            // GPUI apps get no menu bar, so ⌘Q has to be handled here.
            Some(Action::Quit) => return cx.quit(),
            Some(Action::Help) => {
                self.help = !self.help;
                cx.notify();
                return;
            }
            Some(Action::Settings) if !self.help => {
                if self.prefs.is_some() {
                    self.close_prefs();
                } else {
                    self.prefs = Some(Prefs::default());
                }
                cx.notify();
                return;
            }
            _ => {}
        }
        if self.prefs.is_some() {
            // Opening the folder dialog needs the context, so Enter on "add folder" is here.
            let add = self.settings.folders.len();
            if matches!(k.key.as_str(), "enter" | "space")
                && self.prefs.as_ref().is_some_and(|p| p.picker.is_none() && p.row == add)
            {
                self.pick_folders(cx);
            } else {
                self.prefs_key(k);
            }
            cx.notify();
            return;
        }
        if self.help {
            let mut pending = None;
            if k.key == "escape"
                || keys::normal(press, self.settings.vim, &mut pending) == Some(Action::Help)
            {
                self.help = false;
                cx.notify();
            }
            return;
        }
        let action = if self.searching {
            match k.key.as_str() {
                // Esc stops typing first, keeping the results; a second Esc clears them.
                "escape" => {
                    self.searching = false;
                    cx.notify();
                    return;
                }
                "enter" => Some(Action::Play),
                "backspace" if !press.cmd => {
                    let mut q = self.query.clone();
                    q.pop();
                    self.set_query(q);
                    cx.notify();
                    return;
                }
                _ if global.is_some() => global,
                _ => {
                    let Some(text) = k.key_char.as_ref().filter(|_| !press.cmd && !press.ctrl)
                    else {
                        return;
                    };
                    let q = format!("{}{text}", self.query);
                    self.set_query(q);
                    cx.notify();
                    return;
                }
            }
        } else {
            keys::normal(press, self.settings.vim, &mut self.pending)
        };
        if let Some(action) = action {
            self.perform(action);
            cx.notify();
        }
    }

    fn perform(&mut self, action: Action) {
        let song = self.audio.duration;
        match action {
            Action::Move(by) => self.move_selection(by),
            Action::Top => self.move_selection(-(self.rows.len() as isize)),
            Action::Bottom => self.move_selection(self.rows.len() as isize),
            Action::ToPlaying => self.to_playing(),
            Action::Search => self.searching = !self.searching,
            Action::Clear | Action::ClearSearch => self.set_query(String::new()),
            Action::Play => {
                self.searching = false;
                self.play_selected();
            }
            Action::Toggle => self.audio.toggle(),
            Action::Seek(by) => self.audio.seek_by(by),
            Action::ToStart => self.audio.seek(Duration::ZERO),
            Action::FromLoopStart => {
                let start = self.audio.looping.start.unwrap_or_default();
                self.audio.play_from(start);
            }
            Action::Previous => self.previous_song(),
            Action::Next => {
                self.next_song();
            }
            Action::Shuffle => self.toggle_shuffle(),
            Action::Favorite => self.toggle_favorite(),
            Action::Album => self.toggle_album(),
            Action::Transcribe => {
                let track = self.selected_track().cloned().or_else(|| self.now.clone());
                self.transcribe(track);
            }
            // Handled before anything else in `key`.
            Action::Help | Action::Settings | Action::Quit => {}
            Action::LoopStart => {
                let at = self.audio.position();
                self.audio.looping.set_start(at);
            }
            Action::LoopEnd => {
                let at = self.audio.position();
                self.audio.looping.set_end(at);
                if let Some(start) = self.audio.looping.start.filter(|_| self.audio.looping.active()) {
                    self.audio.seek(start);
                }
            }
            Action::LoopClear => self.audio.looping = Default::default(),
            Action::NudgeStart(by) => {
                self.audio.looping.nudge_start(by);
                self.audition(Edge::Start);
            }
            Action::NudgeEnd(by) => {
                self.audio.looping.nudge_end(by, song);
                self.audition(Edge::End);
            }
            Action::Slide(forward) => {
                self.audio.looping.slide(forward, song);
                self.audition(Edge::Start);
            }
            Action::Practice => self.practice = !self.practice,
            Action::Zoom(factor) => {
                let most = song.unwrap_or(SPAN).max(Duration::from_secs(1));
                self.span = self.span.mul_f64(factor).clamp(Duration::from_millis(500), most);
            }
            Action::Resize(factor) => {
                self.audio.looping.resize(factor, song);
                if factor < 1.0 {
                    self.audition(Edge::Start);
                }
            }
        }
    }

    /// Plays around a loop edge just placed, so it can be judged by ear: from the start,
    /// or the last stretch up to the end.
    fn audition(&mut self, edge: Edge) {
        let looping = self.audio.looping;
        let start = looping.start.unwrap_or_default();
        let at = match (edge, looping.end) {
            (Edge::End, Some(end)) => end.saturating_sub(LEAD_IN).max(start),
            _ => start,
        };
        self.audio.play_from(at);
    }

    /// Highlights the playing song, nearest the cursor when it is listed twice.
    fn to_playing(&mut self) {
        let Some(path) = self.now.as_ref().map(|n| n.path.clone()) else { return };
        let Some(&i) = self.by_path.get(&path) else { return };
        let was = self.selected;
        if let Some(row) = (0..self.rows.len())
            .filter(|&r| matches!(self.rows[r], Row::Track(t) if t == i))
            .min_by_key(|&r| r.abs_diff(was))
        {
            self.selected = row;
            self.scroll.scroll_to_item(row, ScrollStrategy::Center);
        }
    }

    fn set_query(&mut self, query: String) {
        if query != self.query || self.album.is_some() {
            self.query = query;
            self.album = None;
            self.rebuild_rows();
            self.scroll.scroll_to_item(0, ScrollStrategy::Top);
        }
    }

    /// A click on a strip: grabs a loop edge near it, sets the loop's end with shift,
    /// and otherwise seeks.
    fn strip_down(&mut self, strip: &Strip, event: &MouseDownEvent, cx: &mut Context<Self>) {
        let Some((at, per_px)) = strip.time_at(event.position.x) else { return };
        let looping = self.audio.looping;
        let near = |edge: Option<Duration>| {
            edge.is_some_and(|e| (e.as_secs_f64() - at.as_secs_f64()).abs() <= GRAB as f64 * per_px)
        };
        if near(looping.end) {
            self.dragging = Some((Edge::End, strip.clone()));
        } else if near(looping.start) {
            self.dragging = Some((Edge::Start, strip.clone()));
        } else if event.modifiers.shift {
            self.audio.looping.set_end(at);
            if self.audio.looping.active() {
                self.audition(Edge::Start);
            }
        } else {
            self.audio.seek(at);
        }
        cx.notify();
    }

    fn drag(&mut self, event: &MouseMoveEvent, _w: &mut Window, cx: &mut Context<Self>) {
        let Some((edge, strip)) = &self.dragging else { return };
        if event.pressed_button != Some(MouseButton::Left) {
            return self.drop_edge(cx);
        }
        let Some((at, _)) = strip.time_at(event.position.x) else { return };
        let song = self.audio.duration;
        match edge {
            Edge::Start => self.audio.looping.move_start(at),
            Edge::End => self.audio.looping.move_end(at, song),
        }
        cx.notify();
    }

    /// Lets go of a dragged edge, and plays around where it landed.
    fn drop_edge(&mut self, cx: &mut Context<Self>) {
        if let Some((edge, _)) = self.dragging.take() {
            self.audition(edge);
            cx.notify();
        }
    }

    fn render_row(&self, ix: usize, cx: &mut Context<Self>) -> gpui::AnyElement {
        match &self.rows[ix] {
            Row::Header(label) => div()
                .h(px(ROW))
                .px_3()
                .flex()
                .items_end()
                .pb_2()
                .gap_2()
                .font_family(MONO)
                .text_xs()
                .text_color(rgb(t().dim))
                .child(label.clone())
                .into_any_element(),
            Row::Track(i) => {
                let track = &self.tracks[*i];
                let selected = ix == self.selected;
                let playing = self.now.as_ref().is_some_and(|n| n.path == track.path);
                let favorite = self.favorites.contains(&track.path);
                // The album view's header already names the album.
                let album = (self.album.is_none()).then(|| track.album.clone());
                div()
                    .id(("row", ix))
                    .w_full()
                    .h(px(ROW))
                    .px_3()
                    .flex()
                    .items_center()
                    .gap_3()
                    .rounded_md()
                    .border_l_2()
                    .border_color(rgb(if selected { t().accent } else { t().bg }))
                    .when(selected, |d| d.bg(rgb(t().raised)))
                    .hover(|d| d.bg(rgb(t().surface)))
                    .on_click(cx.listener(move |riff, _, _, cx| {
                        riff.selected = ix;
                        riff.play_selected();
                        cx.notify();
                    }))
                    // In the album view the header shows the cover; rows show the number.
                    .child(match album {
                        Some(_) => cover(track.cover.clone(), 44.),
                        None => div()
                            .w(px(28.))
                            .flex_none()
                            .flex()
                            .justify_end()
                            .font_family(MONO)
                            .text_sm()
                            .text_color(rgb(if playing { t().accent } else { t().dim }))
                            .children(track.number.map(|n| n.to_string()))
                            .into_any_element(),
                    })
                    .child(
                        // Title over artist, set tight so each row reads as one song.
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .line_height(relative(1.3))
                            .child(
                                div()
                                    .truncate()
                                    .text_color(rgb(if selected || playing { t().accent } else { t().text }))
                                    .child(track.title.clone()),
                            )
                            .child(
                                div()
                                    .truncate()
                                    .text_sm()
                                    .text_color(rgb(t().dim))
                                    .child(track.artist.clone()),
                            ),
                    )
                    .children(album.map(|album| {
                        div()
                            .w(relative(0.32))
                            .flex_none()
                            .truncate()
                            .text_sm()
                            .text_color(rgb(t().dim))
                            .child(album)
                    }))
                    .child(
                        div()
                            .w(px(52.))
                            .flex_none()
                            .flex()
                            .justify_end()
                            .font_family(MONO)
                            .text_sm()
                            .text_color(rgb(t().dim))
                            .children(track.seconds.map(|s| ui::clock(Duration::from_secs(s)))),
                    )
                    // A fixed width, so the columns line up with or without icons.
                    .child(
                        div()
                            .w(px(36.))
                            .flex_none()
                            .flex()
                            .justify_end()
                            .gap_2()
                            .when(playing, |d| d.child(ui::icon("music", 14., t().accent)))
                            .when(favorite, |d| {
                                d.child(ui::icon("star", 14., if selected { t().accent } else { t().dim }))
                            }),
                    )
                    .into_any_element()
            }
        }
    }

    fn render_search(&self) -> impl IntoElement {
        let songs = format!("{} songs", ui::count(self.tracks.len()));
        let read = self.tags_total.saturating_sub(self.tags_left);
        let status = if self.scanning.is_some() {
            "looking for music…".to_string()
        } else if self.tagging.is_some() {
            format!("reading tags {} / {}", ui::count(read), ui::count(self.tags_total))
        } else if let Some((note, _)) = &self.scan_note {
            format!("{songs}, {note}")
        } else {
            songs
        };
        let progress = (self.tagging.is_some() && self.tags_total > 0)
            .then(|| read as f32 / self.tags_total as f32);
        div()
            .relative()
            .flex()
            .items_center()
            .gap_3()
            .h(px(48.))
            .px_4()
            .rounded_lg()
            .bg(rgb(t().surface))
            .when(self.searching, |d| d.border_1().border_color(rgb(t().accent_soft)))
            .child(ui::icon("search", 16., if self.searching { t().accent } else { t().dim }))
            .child(
                div()
                    .flex_1()
                    .flex()
                    .items_center()
                    .font_family(MONO)
                    .text_color(rgb(t().text))
                    .child(self.query.clone())
                    .when(self.searching, |d| {
                        d.child(div().ml(px(1.)).w(px(2.)).h(px(20.)).bg(rgb(t().accent)))
                    })
                    .when(self.query.is_empty(), |d| {
                        d.child(
                            div()
                                .when(self.searching, |d| d.ml_2())
                                .font_family(UI)
                                .text_color(rgb(t().faint))
                                .child(if self.searching {
                                    "type to find a song"
                                } else {
                                    "⌘P to search"
                                }),
                        )
                    }),
            )
            .child(
                div()
                    .font_family(MONO)
                    .text_xs()
                    .text_color(rgb(t().dim))
                    .child(status),
            )
            // How far the tags are read: a thin line along the bottom, inside the corners.
            .children(progress.map(|done| {
                div()
                    .absolute()
                    .bottom_0()
                    .left(px(12.))
                    .right(px(12.))
                    .h(px(2.))
                    .child(div().h_full().w(relative(done)).rounded_full().bg(rgb(t().accent)))
            }))
    }

    fn render_now(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let position = self.audio.position();
        let duration = self.audio.duration;
        let fraction = |d: Duration| match duration {
            Some(total) if !total.is_zero() => (d.as_secs_f32() / total.as_secs_f32()).min(1.),
            _ => 0.,
        };
        let looping = self.audio.looping;
        let (title, artist) = match &self.now {
            Some(t) => (t.title.clone(), format!("{}, {}", t.artist, t.album)),
            None => ("Nothing playing".to_string(), "Enter plays the selected song".to_string()),
        };
        let bar = self.bar.clone();
        let time = if self.audio.loading() {
            "opening…".to_string()
        } else {
            format!(
                "{} / {}",
                ui::clock(position),
                duration.map_or("–".to_string(), ui::clock)
            )
        };
        let loop_label = match (looping.start, looping.end) {
            (_, Some(end)) => format!(
                "{} to {}",
                ui::clock(looping.start.unwrap_or_default()),
                ui::clock(end)
            ),
            (Some(start), None) => format!("from {}, ] sets the end", ui::clock(start)),
            (None, None) => "[ ] sets a loop".to_string(),
        };

        let progress = div()
            .id("progress")
            .relative()
            .h(px(16.))
            .flex()
            .items_center()
            .cursor_pointer()
            .on_mouse_down(MouseButton::Left, {
                let bar = self.bar.clone();
                cx.listener(move |riff, e, _, cx| riff.strip_down(&bar, e, cx))
            })
            .child(
                canvas(
                    move |bounds, _, _| bar.set(bounds, Duration::ZERO, duration.unwrap_or_default()),
                    |_, _, _, _| {},
                )
                    .absolute()
                    .size_full(),
            )
            .child(
                div()
                    .relative()
                    .w_full()
                    .h(px(4.))
                    .rounded_full()
                    .bg(rgb(t().faint))
                    .children(looping.end.map(|end| {
                        let start = fraction(looping.start.unwrap_or_default());
                        div()
                            .absolute()
                            .top(px(-2.))
                            .h(px(8.))
                            .left(relative(start))
                            .w(relative(fraction(end) - start))
                            .rounded_sm()
                            .bg(rgb(t().on))
                            .opacity(0.35)
                    }))
                    .child(
                        div()
                            .absolute()
                            .h_full()
                            .rounded_full()
                            .bg(rgb(t().accent))
                            .w(relative(fraction(position))),
                    )
                    .children(looping.start.map(|start| {
                        div()
                            .absolute()
                            .top(px(-4.))
                            .left(relative(fraction(start)))
                            .w(px(2.))
                            .h(px(12.))
                            .bg(rgb(t().on))
                    })),
            );

        let button = |id: &'static str, icon: &'static str, label: Option<&'static str>, lit: bool| {
            div()
                .id(id)
                .flex()
                .items_center()
                .gap_2()
                .h(px(36.))
                .px_3()
                .rounded_md()
                .bg(rgb(if lit { t().accent_soft } else { t().raised }))
                .hover(|d| d.bg(rgb(t().faint)))
                .cursor_pointer()
                .child(ui::icon(icon, 16., if lit { t().accent } else { t().text }))
                .children(label.map(|l| div().text_sm().text_color(rgb(t().text)).child(l)))
        };

        div()
            .flex()
            .flex_col()
            .gap_3()
            .p_4()
            .rounded_lg()
            .bg(rgb(t().surface))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_4()
                    .child(cover(self.now.as_ref().and_then(|t| t.cover.clone()), 64.))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .child(
                                div()
                                    .truncate()
                                    .text_lg()
                                    .font_weight(gpui::FontWeight::MEDIUM)
                                    .text_color(rgb(t().text))
                                    .child(title),
                            )
                            .child(div().truncate().text_sm().text_color(rgb(t().dim)).child(artist)),
                    )
                    .child(button("previous", "skip-back", None, false).on_click(cx.listener(
                        |riff, _, _, cx| {
                            riff.previous_song();
                            cx.notify();
                        },
                    )))
                    .child(
                        button(
                            "toggle",
                            if self.audio.playing() { "pause" } else { "play" },
                            None,
                            false,
                        )
                        .on_click(cx.listener(|riff, _, _, cx| {
                            riff.audio.toggle();
                            cx.notify();
                        })),
                    )
                    .child(button("next", "skip-forward", None, false).on_click(cx.listener(
                        |riff, _, _, cx| {
                            riff.next_song();
                            cx.notify();
                        },
                    )))
                    .child(button("shuffle", "shuffle", None, self.shuffle).on_click(cx.listener(
                        |riff, _, _, cx| {
                            riff.toggle_shuffle();
                            cx.notify();
                        },
                    )))
                    .child(
                        button("loop", "repeat", None, looping.active()).on_click(cx.listener(
                            |riff, _, _, cx| {
                                let at = riff.audio.position();
                                let looping = &mut riff.audio.looping;
                                if looping.active() {
                                    *looping = Default::default();
                                } else if looping.start.is_none() {
                                    looping.set_start(at);
                                } else {
                                    looping.set_end(at);
                                }
                                cx.notify();
                            },
                        )),
                    )
                    .child(
                        button("transcribe", "external-link", Some("Transcribe!"), false).on_click(
                            cx.listener(|riff, _, _, cx| {
                                // The button sits on the playing card, so it means that song.
                                let track = riff.now.clone().or_else(|| riff.selected_track().cloned());
                                riff.transcribe(track);
                                cx.notify();
                            }),
                        ),
                    ),
            )
            .child(progress)
            .child(
                div()
                    .flex()
                    .justify_between()
                    .font_family(MONO)
                    .text_xs()
                    .text_color(rgb(t().dim))
                    .child(time)
                    .child(
                        div()
                            .text_color(rgb(if looping.active() { t().on } else { t().dim }))
                            .child(loop_label),
                    ),
            )
    }
}

/// A cover thumbnail, or a quiet placeholder.
fn cover(path: Option<PathBuf>, size: f32) -> gpui::AnyElement {
    let placeholder = move || {
        div()
            .size(px(size))
            .flex_none()
            .rounded_md()
            .bg(rgb(t().raised))
            .flex()
            .items_center()
            .justify_center()
            .child(ui::icon("music", size * 0.4, t().faint))
            .into_any_element()
    };
    match path {
        Some(path) => img(path)
            .size(px(size))
            .flex_none()
            .rounded_md()
            .object_fit(ObjectFit::Cover)
            .with_fallback(placeholder)
            .into_any_element(),
        None => placeholder(),
    }
}

impl Riff {
    /// The ⌘? sheet: every key, over a dimmed window. Esc, ⌘? or a click closes it.
    fn render_help(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let vim = self.settings.vim;
        let group = |(title, lines): &(&str, &[keys::Line])| {
            div()
                .flex()
                .flex_col()
                .gap_1()
                .child(
                    div()
                        .pb_1()
                        .font_family(MONO)
                        .text_xs()
                        .text_color(rgb(t().dim))
                        .child(title.to_string()),
                )
                // Without vim keys, lines that only have vim keys go.
                .children(lines.iter().filter_map(|(keys, vim_keys, what)| {
                    let caps: Vec<&str> = keys
                        .split_whitespace()
                        .chain(vim_keys.split_whitespace().filter(|_| vim))
                        .collect();
                    (!caps.is_empty()).then_some((caps, what))
                }).map(|(caps, what)| {
                    div()
                        .flex()
                        .items_center()
                        .gap_3()
                        .child(
                            div()
                                .w(px(190.))
                                .flex_none()
                                .flex()
                                .gap_1()
                                .font_family(MONO)
                                .text_sm()
                                .text_color(rgb(t().accent))
                                .children(caps.into_iter().map(|cap| {
                                    div()
                                        .px_1p5()
                                        .rounded_sm()
                                        .bg(rgb(t().raised))
                                        .border_1()
                                        .border_color(rgb(t().faint))
                                        .child(cap.to_string())
                                })),
                        )
                        .child(div().text_sm().text_color(rgb(t().text)).child(what.to_string()))
                }))
        };
        let (left, right) = keys::SHEET.split_at(2);
        overlay("help")
            .on_click(cx.listener(|riff, _, _, cx| {
                riff.help = false;
                cx.notify();
            }))
            .child(
                div()
                    .flex()
                    .gap_6()
                    .p_5()
                    .rounded_lg()
                    .bg(rgb(t().surface))
                    .border_1()
                    .border_color(rgb(t().faint))
                    .child(div().flex().flex_col().gap_5().children(left.iter().map(group)))
                    .child(div().flex().flex_col().gap_5().children(right.iter().map(group))),
            )
    }

    /// The practice view: the whole song as a loudness contour with the shown stretch
    /// boxed, and below it that stretch in detail, the loop on it. Both take clicks and
    /// drags like the progress bar.
    fn render_practice(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let t = t();
        let at = self.audio.position();
        let looping = self.audio.looping;
        let wave = self
            .wave
            .as_ref()
            .filter(|(p, _)| self.now.as_ref().is_some_and(|n| n.path == *p))
            .map(|(_, w)| w.clone());
        let song = self.audio.duration.or(wave.as_ref().map(|w| w.length()));
        let detail_dragged = self
            .dragging
            .as_ref()
            .is_some_and(|(_, strip)| Rc::ptr_eq(&strip.0, &self.detail.0));
        // While an edge is dragged here, the view holds still so it stays under the pointer.
        let (from, to) = match song {
            _ if detail_dragged => {
                let (_, from, to) = self.detail.0.get();
                (from, to)
            }
            Some(song) => wave::window(at, looping, self.span, song),
            None => (Duration::ZERO, self.span),
        };
        let status = match (&wave, &self.wave_error, &self.now) {
            (Some(_), ..) => None,
            (None, Some(e), _) => Some(format!("couldn't read the waveform: {e}")),
            (None, None, Some(_)) => Some("reading the waveform…".to_string()),
            (None, None, None) => Some("play a song to see its waveform".to_string()),
        };
        let strip = |id: &'static str, strip: &Strip, view: Painting| {
            let down = strip.clone();
            div()
                .id(id)
                .relative()
                .rounded_lg()
                .bg(rgb(t.surface))
                .overflow_hidden()
                .cursor_pointer()
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |riff, e, _, cx| riff.strip_down(&down, e, cx)),
                )
                .child(paint_wave(strip.clone(), view).absolute().size_full())
        };
        let common = Painting {
            wave: wave.clone(),
            from: Duration::ZERO,
            to: song.unwrap_or(self.span),
            at,
            looping,
            boxed: None,
            contour: true,
        };
        div()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .gap_2()
            .child(
                div()
                    .flex()
                    .justify_between()
                    .px_1()
                    .font_family(MONO)
                    .text_xs()
                    .text_color(rgb(t.dim))
                    .child(format!("Practice, {:.1} s shown", (to - from).as_secs_f64()))
                    .child("P back   ⌘= ⌘- zoom   drag the loop edges"),
            )
            .child(strip("overview", &self.overview, Painting { boxed: Some((from, to)), ..common.clone() }).h(px(40.)))
            .child(
                strip("detail", &self.detail, Painting { from, to, contour: false, ..common })
                    .flex_1()
                    .children(status.map(|s| {
                        div()
                            .absolute()
                            .size_full()
                            .flex()
                            .items_center()
                            .justify_center()
                            .text_sm()
                            .text_color(rgb(t.dim))
                            .child(s)
                    })),
            )
    }

    /// The album view's header: the cover large, the album's name and what's on it, so
    /// it's plain that only this album is listed.
    fn render_album(&self, of: &Track) -> impl IntoElement {
        let t = t();
        let songs = library::album(&self.tracks, of);
        let seconds = songs
            .iter()
            .map(|&i| self.tracks[i].seconds)
            .sum::<Option<u64>>();
        let mut about = vec![];
        if !of.artist.is_empty() {
            about.push(of.artist.clone());
        }
        if let Some(year) = songs.iter().filter_map(|&i| self.tracks[i].year).min() {
            about.push(year.to_string());
        }
        about.push(ui::album_stats(songs.len(), seconds));
        div()
            .flex()
            .items_end()
            .gap_5()
            .p_4()
            .rounded_lg()
            .bg(rgb(t.surface))
            .child(cover(of.cover.clone(), 140.))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(
                        div()
                            .font_family(MONO)
                            .text_xs()
                            .text_color(rgb(t.dim))
                            .child("Album"),
                    )
                    .child(
                        div()
                            .truncate()
                            .text_3xl()
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .text_color(rgb(t.text))
                            .child(if of.album.is_empty() { "Unknown album".into() } else { of.album.clone() }),
                    )
                    .child(div().truncate().text_sm().text_color(rgb(t.dim)).child(about.join(", "))),
            )
            .child(
                div()
                    .flex_none()
                    .font_family(MONO)
                    .text_xs()
                    .text_color(rgb(t.dim))
                    .child("⌘A back"),
            )
    }

    /// The ⌘, sheet: the folders to scan and the theme. Esc, ⌘, or a click outside closes it.
    fn render_prefs(&self, prefs: &Prefs, cx: &mut Context<Self>) -> impl IntoElement {
        let t = t();
        let folders = &self.settings.folders;
        let title = |text: &str| {
            div()
                .pt_2()
                .pb_1()
                .font_family(MONO)
                .text_xs()
                .text_color(rgb(t.dim))
                .child(text.to_string())
        };
        let row = |id: (&'static str, usize), lit: bool, icon: &str, label: String| {
            div()
                .id(id)
                .h(px(34.))
                .px_2()
                .flex()
                .items_center()
                .gap_3()
                .rounded_md()
                .border_l_2()
                .border_color(rgb(if lit { t.accent } else { t.surface }))
                .when(lit, |d| d.bg(rgb(t.raised)))
                .hover(|d| d.bg(rgb(t.raised)))
                .cursor_pointer()
                .child(ui::icon(icon, 16., if lit { t.accent } else { t.dim }))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .truncate()
                        .text_sm()
                        .text_color(rgb(if lit { t.accent } else { t.text }))
                        .child(label),
                )
        };
        let lit = |r: usize| prefs.picker.is_none() && prefs.row == r;

        let counts = scan::per_root(folders, &self.tracks);
        let mut body = div().flex().flex_col().child(title("Folders"));
        for (i, folder) in folders.iter().enumerate() {
            body = body.child(
                row(("folder", i), lit(i), "folder", tilde(folder))
                    .child(
                        div()
                            .flex_none()
                            .font_family(MONO)
                            .text_xs()
                            .text_color(rgb(t.dim))
                            .child(format!("{} songs", ui::count(counts[i]))),
                    )
                    .on_click(cx.listener(move |riff, _, _, cx| {
                        if let Some(prefs) = &mut riff.prefs {
                            prefs.row = i;
                        }
                        cx.notify();
                    }))
                    .child(
                        div()
                            .id(("remove", i))
                            .p_1()
                            .rounded_sm()
                            .hover(|d| d.bg(rgb(t.faint)))
                            .child(ui::icon("x", 14., t.dim))
                            .on_click(cx.listener(move |riff, _, _, cx| {
                                cx.stop_propagation();
                                riff.remove_folder(i);
                                cx.notify();
                            })),
                    ),
            );
        }
        body = body
            .child(
                row(("add", 0), lit(folders.len()), "plus", "Add folder…".into()).on_click(
                    cx.listener(|riff, _, _, cx| {
                        riff.pick_folders(cx);
                        cx.notify();
                    }),
                ),
            )
            .child(title("Theme"));
        body = match &prefs.picker {
            None => body.child(
                row(("theme", 0), lit(folders.len() + 1), "palette", self.settings.theme.clone())
                    .on_click(cx.listener(|riff, _, _, cx| {
                        riff.open_picker();
                        cx.notify();
                    })),
            ),
            Some(picker) => {
                let shown = picker.shown();
                let first = picker.selected.saturating_sub(PICKER_ROWS - 1);
                body.child(
                    div()
                        .h(px(34.))
                        .px_2()
                        .flex()
                        .items_center()
                        .gap_3()
                        .rounded_md()
                        .bg(rgb(t.bg))
                        .child(ui::icon("search", 14., t.accent))
                        .child(if picker.query.is_empty() {
                            div().text_sm().text_color(rgb(t.faint)).child("type to filter")
                        } else {
                            div()
                                .font_family(MONO)
                                .text_sm()
                                .text_color(rgb(t.text))
                                .child(picker.query.clone())
                        }),
                )
                .children(shown.into_iter().enumerate().skip(first).take(PICKER_ROWS).map(
                    |(i, theme)| {
                        let chosen = theme.clone();
                        row(("pick", i), i == picker.selected, "palette", theme.name.clone())
                            .children(
                                (theme.name == self.settings.theme)
                                    .then(|| ui::icon("check", 14., t.dim)),
                            )
                            .on_click(cx.listener(move |riff, _, _, cx| {
                                riff.keep_theme(chosen.clone());
                                cx.notify();
                            }))
                    },
                ))
            }
        };
        let vim_label = if self.settings.vim { "Vim keys on" } else { "Vim keys off" };
        body = body.child(title("Keys")).child(
            row(("vim", 0), lit(folders.len() + 2), "keyboard", vim_label.into()).on_click(
                cx.listener(|riff, _, _, cx| {
                    riff.toggle_vim();
                    cx.notify();
                }),
            ),
        );
        let hint = match (prefs.picker.is_some(), self.settings.vim) {
            (true, _) => "↑↓ preview   Enter keeps   Esc restores",
            (false, true) => "j k move   Enter choose   dd remove folder   Esc close",
            (false, false) => "↑↓ move   Enter choose   ⌫ remove folder   Esc close",
        };

        overlay("prefs")
            .on_click(cx.listener(|riff, _, _, cx| {
                riff.close_prefs();
                cx.notify();
            }))
            .child(
                div()
                    .id("prefs-sheet")
                    .w(px(520.))
                    .flex()
                    .flex_col()
                    .gap_3()
                    .p_5()
                    .rounded_lg()
                    .bg(rgb(t.surface))
                    .border_1()
                    .border_color(rgb(t.faint))
                    // A click inside the sheet is not a click outside it.
                    .on_click(|_, _, cx| cx.stop_propagation())
                    .child(body)
                    .child(
                        div()
                            .pt_2()
                            .font_family(MONO)
                            .text_xs()
                            .text_color(rgb(t.dim))
                            .child(hint),
                    ),
            )
    }
}

/// What a waveform strip draws: `from` to `to` of the song as RMS, the playhead and
/// loop on it, and in the overview the stretch the detail shows. The detail is scaled to
/// the song's loudest moment, so loudness compares across zooms; a contour (the
/// overview) to its own loudest column, so sections and breaks stand out.
#[derive(Clone)]
struct Painting {
    wave: Option<Rc<Wave>>,
    from: Duration,
    to: Duration,
    at: Duration,
    looping: crate::audio::Loop,
    boxed: Option<(Duration, Duration)>,
    contour: bool,
}

fn tint(color: u32, alpha: u8) -> gpui::Rgba {
    gpui::rgba((color << 8) | alpha as u32)
}

/// One bar per pixel: cheap enough to redraw every frame at any zoom.
fn paint_wave(strip: Strip, p: Painting) -> gpui::Canvas<()> {
    canvas(
        move |bounds, _, _| strip.set(bounds, p.from, p.to),
        move |bounds, _, window, _| {
            let t = t();
            let (w, h) = (f32::from(bounds.size.width), f32::from(bounds.size.height));
            let span = (p.to - p.from).as_secs_f64();
            if span <= 0. || w <= 0. {
                return;
            }
            let x_of = |d: Duration| {
                let x = ((d.as_secs_f64() - p.from.as_secs_f64()) / span) as f32 * w;
                x.clamp(0., w)
            };
            let rect = |x: f32, y: f32, wd: f32, ht: f32| {
                Bounds::new(
                    point(bounds.origin.x + px(x), bounds.origin.y + px(y)),
                    size(px(wd), px(ht)),
                )
            };
            if let Some(end) = p.looping.end {
                let (a, b) = (x_of(p.looping.start.unwrap_or_default()), x_of(end));
                window.paint_quad(fill(rect(a, 0., b - a, h), tint(t.on, 0x26)));
            }
            if let Some((from, to)) = p.boxed {
                let (a, b) = (x_of(from), x_of(to));
                window.paint_quad(fill(rect(a, 0., (b - a).max(2.), h), tint(t.text, 0x14)));
            }
            if let Some(wave) = &p.wave {
                let columns = wave.columns(p.from, p.to, w as usize);
                let scale = if p.contour {
                    columns.iter().map(|c| c.rms()).fold(0., f32::max)
                } else {
                    wave.loudest()
                };
                let mid = h / 2.;
                let bar = |x: usize, v: f32| {
                    let half = (v / scale.max(1e-6)).min(1.) * (mid - 2.);
                    rect(x as f32, mid - half, 1., (half * 2.).max(1.))
                };
                let played = x_of(p.at) as usize;
                for (x, c) in columns.iter().enumerate() {
                    let color = if x < played { t.accent } else { t.dim };
                    window.paint_quad(fill(bar(x, c.rms()), tint(color, 0xff)));
                }
            }
            if let Some(end) = p.looping.end {
                for edge in [p.looping.start.unwrap_or_default(), end] {
                    window.paint_quad(fill(rect(x_of(edge) - 1., 0., 2., h), tint(t.on, 0xff)));
                }
            }
            window.paint_quad(fill(rect(x_of(p.at), 0., 1., h), tint(t.text, 0xff)));
        },
    )
}

/// A dimmed layer over the whole window, holding a sheet in its middle.
fn overlay(id: &'static str) -> Stateful<Div> {
    div()
        .id(id)
        .absolute()
        .top_0()
        .left_0()
        .size_full()
        .flex()
        .items_center()
        .justify_center()
        .bg(gpui::rgba(t().bg << 8 | 0xcc))
        .occlude()
}

/// A folder as people write it: `~/Music` rather than `/Users/tim/Music`.
fn tilde(path: &std::path::Path) -> String {
    let home = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_default();
    match path.strip_prefix(&home) {
        Ok(rest) if !home.as_os_str().is_empty() => format!("~/{}", rest.display()),
        _ => path.display().to_string(),
    }
}

impl Render for Riff {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // The detail view keeps the loop in sight, refitting only when it must.
        let looping = self.audio.looping;
        if let Some(end) = looping.end.filter(|_| self.dragging.is_none()) {
            self.span = wave::fit(self.span, end - looping.start.unwrap_or_default());
        }
        let hint = match &self.message {
            Some((text, error)) => div()
                .text_color(rgb(if *error { t().error } else { t().dim }))
                .child(text.clone()),
            None if self.searching => div().child(
                "Enter plays and stops searching   Esc stops searching   ⌘⌫ clears",
            ),
            None if self.settings.vim => div().child(
                "/ search   Space pause   [ ] loop   { } ( ) nudge   < > slide   ? all keys",
            ),
            None => div().child("⌘P search   Space pause   [ ] loop   ⌘T Transcribe!   ⌘? all keys"),
        };
        div()
            .track_focus(&self.focus)
            .on_key_down(cx.listener(Self::key))
            .on_mouse_move(cx.listener(Self::drag))
            .on_mouse_up(MouseButton::Left, cx.listener(|riff, _, _, cx| riff.drop_edge(cx)))
            .relative()
            .size_full()
            .flex()
            .flex_col()
            .gap_3()
            .p_3()
            .pt(px(TITLEBAR))
            .bg(rgb(t().bg))
            .child(
                // The strip the traffic lights sit in: drags the window, and a double click
                // zooms it like any title bar.
                div()
                    .id("titlebar")
                    .absolute()
                    .top_0()
                    .left_0()
                    .w_full()
                    .h(px(TITLEBAR))
                    .on_click(|event, window, _| {
                        if event.click_count() == 2 {
                            window.titlebar_double_click();
                        }
                    }),
            )
            .font_family(UI)
            .text_color(rgb(t().text))
            .child(self.render_search())
            .when(self.practice, |d| d.child(self.render_practice(cx)))
            .when(!self.practice, |d| {
                d.children(self.album.as_ref().map(|of| self.render_album(of))).child(
                    uniform_list(
                        "library",
                        self.rows.len(),
                        cx.processor(|riff, range: std::ops::Range<usize>, _window, cx| {
                            range.map(|ix| riff.render_row(ix, cx)).collect::<Vec<_>>()
                        }),
                    )
                    .track_scroll(self.scroll.clone())
                    .flex_1(),
                )
            })
            .child(self.render_now(cx))
            .child(
                div()
                    .px_1()
                    .font_family(MONO)
                    .text_xs()
                    .text_color(rgb(t().dim))
                    .child(hint),
            )
            .when(self.help, |d| d.child(self.render_help(cx)))
            .children(self.prefs.as_ref().map(|p| self.render_prefs(p, cx)))
    }
}
