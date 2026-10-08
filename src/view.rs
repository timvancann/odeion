//! The window: a search line, the library, and what's playing. Keyboard first; every
//! control can be clicked too.

use crate::audio::Audio;
use crate::beats::{self, Beats};
use crate::keys::{self, Action};
use crate::library::{self, Grouped, Kind, Track};
use crate::media::Media;
use crate::menu::{Menu, Outcome, Target};
use crate::playlist::Playlist;
use crate::queue::Queue;
use crate::regions::{Region, Regions};
use crate::session::Session;
use crate::resume::{Resume, Spot};
use crate::variations::{self, Variation, Variations};
use crate::scan;
use crate::state::{self, Favorites, ListView, Settings};
use crate::theme::{self, Theme};
use crate::ui::{self, *};
use crate::wave::{self, Wave};
use gpui::{
    Bounds, Context, Div, FocusHandle, KeyDownEvent, MouseButton, MouseDownEvent, MouseMoveEvent,
    ExternalPaths, ObjectFit,
    PathPromptOptions, Pixels, ScrollStrategy, Stateful, UniformListScrollHandle, Window, canvas,
    anchored, deferred, div, fill, img, point, prelude::*, px, relative, rgb, size, uniform_list,
};
use std::cell::Cell;
use std::collections::HashMap;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::mpsc::Receiver;
use std::time::Duration;

const POLL: Duration = Duration::from_millis(40);
/// How often the session is checked and, when it changed, saved.
const SESSION_EVERY: Duration = Duration::from_secs(1);
/// The scrollbar stays this long after the list stops moving, as in Zed.
const SCROLLBAR_LINGERS: Duration = Duration::from_secs(1);
const ROW: f32 = 44.;
/// The transparent title bar's height, where the traffic lights sit.
const TITLEBAR: f32 = 32.;
const SEEK: f64 = 5.0;
/// How much the practice view shows around the playhead before any zooming.
const SPAN: Duration = Duration::from_secs(10);
/// A press this close to a strip's top grabs the marker by its flag.
const FLAG: f32 = 10.;
/// A click this close to a loop edge grabs it.
const GRAB: f32 = 8.;
/// How long a scan's result stays in the search bar.
const RESULT_FOR: Duration = Duration::from_secs(8);
/// Theme names the picker shows at once.
const PICKER_ROWS: usize = 8;
/// Save the cache now and then while tags come in, so a quit mid-scan keeps most of it.
const SAVE_EVERY: usize = 250;


/// The ⌘, sheet: the highlighted row (each folder, then "add folder", the theme, vim
/// keys, the output), and the theme picker when it is open.
#[derive(Default)]
struct Prefs {
    row: usize,
    picker: Option<Picker>,
    /// The first `d` of `dd` was pressed.
    delete: bool,
    /// The system's output and the chosen device's name when the sheet opened; asking
    /// every frame would be wasted.
    system: Option<crate::audio::Output>,
    chosen: Option<String>,
    /// The output picker, when open: the highlighted entry (0 the system's, then each
    /// device), the devices once listed, and the listing under way. Listing takes half
    /// a second, so it happens on a thread.
    choosing: Option<usize>,
    outputs: Vec<crate::audio::Output>,
    listing: Option<Receiver<Vec<crate::audio::Output>>>,
}

impl Prefs {
    fn new(chosen: Option<&str>) -> Prefs {
        Prefs {
            system: crate::audio::system_output(),
            chosen: chosen.and_then(crate::audio::name_of),
            ..Prefs::default()
        }
    }
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

/// A tooltip: what a control does, and the keys that do the same, looked up from the
/// keymap so they stay right when keys change.
struct Tip {
    what: gpui::SharedString,
    keys: Vec<&'static str>,
}

impl Render for Tip {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let t = t();
        div()
            .flex()
            .items_center()
            .gap_2()
            .px_2()
            .py_1()
            .rounded_md()
            .bg(rgb(t.raised))
            .border_1()
            .border_color(rgb(t.faint))
            .font_family(UI)
            .text_xs()
            .text_color(rgb(t.text))
            .child(self.what.clone())
            .children(self.keys.iter().map(|k| {
                div()
                    .px_1()
                    .rounded_sm()
                    .border_1()
                    .border_color(rgb(t.faint))
                    .font_family(MONO)
                    .text_color(rgb(t.accent))
                    .child(k.to_string())
            }))
    }
}

/// A level that drags like a slider: the master volume, or a stem's.
#[derive(Clone, Copy, PartialEq)]
enum Slider {
    Volume,
    Stem(usize),
    /// The speed, from 25 to 110%.
    Speed,
}

/// A loop edge, being placed or dragged.
#[derive(Clone, Copy, PartialEq)]
enum Edge {
    Start,
    End,
    /// The marker, dragged like a loop edge.
    Marker,
}

/// Where a sidebar entry takes the list.
#[derive(Clone, Copy)]
enum Place {
    Library,
    Favourites,
    Playing,
}

enum Row {
    /// A section: "Favourites", "Library".
    Header(String),
    /// An artist and how many songs follow, in the list grouped by artist.
    Artist(String, usize),
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
    /// What's typed on the ⌘? sheet, to find keys by.
    help_query: String,
    settings: Settings,
    /// The ⌘, sheet, when open.
    prefs: Option<Prefs>,
    /// The first key of a two-key vim command (`gg`, `zz`, `dd`).
    pending: Option<char>,
    /// The loop edge being dragged, and on which strip.
    dragging: Option<(Edge, Strip)>,
    /// Practice mode: the practice view instead of the list, with its own keys.
    practice: bool,
    /// The right-click menu, and where it opened.
    menu: Option<(Menu, gpui::Point<Pixels>)>,
    /// The playing song's waveform, once read; read for every song, shown or not.
    wave: Option<(PathBuf, Rc<Wave>)>,
    reading: Option<(PathBuf, Receiver<Result<Vec<Wave>, String>>)>,
    /// Each stem's waveform, when a stem set plays.
    stem_waves: Vec<Rc<Wave>>,
    /// Which stems are muted, and the one soloed.
    muted: Vec<bool>,
    /// Each stem set's levels, by its first file, like `loops`.
    levels: HashMap<PathBuf, Vec<f32>>,
    /// A level being dragged, and where its slider was drawn.
    sliding: Option<(Slider, Rc<Cell<Bounds<Pixels>>>)>,
    solo: Option<usize>,
    /// The playing song's beats, found in the background like the waveform.
    beats: Option<(PathBuf, Rc<Beats>)>,
    finding: Option<(PathBuf, Receiver<Result<Beats, String>>)>,
    /// The beat grid (`g`): loop edges and steps follow the beats.
    grid: bool,
    /// Saved loops, per song.
    regions: Regions,
    /// The name being typed for the loop as a region (`r`).
    naming: Option<String>,
    /// The section being renamed from its menu; without one, `naming` names the loop.
    renaming: Option<usize>,
    /// Other recordings per song, and which of the playing song's is on: 0 the song
    /// itself, n its n-th variation.
    variations: Variations,
    variation: usize,
    /// How fast the playing song plays, and the slowed speed it keeps.
    speed: crate::speed::Speed,
    /// The speed trainer, when on: the loop's wraps when it started.
    training: Option<u64>,
    /// The trainer's settings sheet, when open: the highlighted setting.
    trainer_sheet: Option<usize>,
    /// Where each song was left, and when the playing one's was last kept.
    resume: Resume,
    kept: std::time::Instant,
    /// Each recording's loop while another plays, so switching back brings it back.
    loops: HashMap<PathBuf, crate::audio::Loop>,
    /// Each recording's marker, where `o` plays from (`m` drops it).
    marks: HashMap<PathBuf, Duration>,
    /// A section edge being dragged in the section lane: which section, and its start
    /// (or end).
    shaping: Option<(usize, bool)>,
    /// The section lane, where it was last drawn, spanning the whole song.
    lane: Strip,
    /// Files being made a variation of a song, in the background.
    adding: Option<(PathBuf, Receiver<Result<Variation, String>>)>,
    wave_error: Option<String>,
    /// How much of the song the detail view shows.
    span: Duration,
    /// The playhead as the practice view draws it, and the clock that moves it.
    smooth: crate::audio::Smooth,
    started: std::time::Instant,
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
    /// The artist filter (⇧⌘A), by one of their tracks.
    artist: Option<Track>,
    /// What plays next and before: the list the song was started from.
    playlist: Playlist,
    /// Songs to play next, ahead of the list.
    queue: Queue,
    /// The sidebar is open; collapsed, only its icons show.
    sidebar: bool,
    /// The highlighted song in the queue, while the keyboard is in it (`Mode::Queue`).
    queue_at: Option<usize>,
    shuffle: bool,
    /// The media keys and Now Playing, when macOS lets riff have them.
    media: Option<Media>,
    message: Option<(String, bool)>,
    bar: Strip,
    /// The letter rail beside the library, where it was last drawn.
    rail: Rc<Cell<Bounds<Pixels>>>,
    /// The letter under the pointer while it drags along the rail.
    scrubbing: Option<char>,
    /// A scroll in progress: from and to (list offsets) and when it began.
    glide: Option<(Pixels, Pixels, std::time::Instant)>,
    /// The list's scrollbar track, where it was last drawn.
    track: Rc<Cell<Bounds<Pixels>>>,
    /// Where the pointer holds the scrollbar's thumb, from its top, while dragging it.
    grab: Option<Pixels>,
    /// The pointer is over the scrollbar.
    over_track: bool,
    /// The list's scroll offset last drawn, and when it last changed: the scrollbar
    /// shows while the list moves and a moment after.
    seen_offset: Pixels,
    scrolled_at: Option<std::time::Instant>,
    /// The session as last saved, and when it was last checked.
    saved: Session,
    saved_at: std::time::Instant,
    /// The playhead as last kept in `resume.json`.
    kept_at: Duration,
}

fn cache_file() -> PathBuf {
    state::data_dir().join("library.json")
}

fn favorites_file() -> PathBuf {
    state::data_dir().join("favorites.json")
}

fn resume_file() -> PathBuf {
    state::data_dir().join("resume.json")
}

fn variations_file() -> PathBuf {
    state::data_dir().join("variations.json")
}

fn session_file() -> PathBuf {
    state::data_dir().join("session.json")
}

fn regions_file() -> PathBuf {
    state::data_dir().join("regions.json")
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
        let audio = Audio::new(settings.volume, settings.output.clone());
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
            help_query: String::new(),
            settings,
            prefs: None,
            moved: None,
            pending: None,
            dragging: None,
            practice: false,
            menu: None,
            wave: None,
            reading: None,
            stem_waves: Vec::new(),
            muted: Vec::new(),
            levels: HashMap::new(),
            sliding: None,
            solo: None,
            beats: None,
            finding: None,
            grid: false,
            regions: Regions::load(&regions_file()),
            naming: None,
            renaming: None,
            variations: Variations::load(&variations_file()),
            variation: 0,
            speed: Default::default(),
            training: None,
            trainer_sheet: None,
            loops: HashMap::new(),
            marks: HashMap::new(),
            shaping: None,
            lane: Strip::default(),
            resume: Resume::load(&resume_file()),
            kept: std::time::Instant::now(),
            adding: None,
            wave_error: None,
            span: SPAN,
            smooth: Default::default(),
            started: std::time::Instant::now(),
            overview: Strip::default(),
            detail: Strip::default(),
            rows: Vec::new(),
            selected: 0,
            scroll: UniformListScrollHandle::new(),
            audio,
            now: None,
            favorites: Favorites::load(&favorites_file()),
            album: None,
            artist: None,
            playlist: Playlist::default(),
            queue: Queue::default(),
            sidebar: true,
            queue_at: None,
            shuffle: false,
            media: Media::new(),
            message: None,
            bar: Strip::default(),
            rail: Rc::default(),
            scrubbing: None,
            glide: None,
            track: Rc::default(),
            grab: None,
            over_track: false,
            seen_offset: Pixels::ZERO,
            scrolled_at: None,
            saved: Session::default(),
            saved_at: std::time::Instant::now(),
            kept_at: Duration::ZERO,
        };
        riff.message = riff.audio.error.clone().map(|e| (e, true));
        riff.reindex(None);
        riff.restore(Session::load(&session_file()));
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

    /// Opens the output picker, listing the devices on a thread.
    fn open_outputs(&mut self) {
        let Some(prefs) = &mut self.prefs else { return };
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let _ = tx.send(crate::audio::outputs());
        });
        (prefs.choosing, prefs.listing) = (Some(0), Some(rx));
    }

    /// Plays through entry `i` of the picker (0 the system's), and keeps it.
    fn choose_output(&mut self, i: usize) {
        let Some(prefs) = &mut self.prefs else { return };
        let Some(chosen) = (match i {
            0 => Some(None),
            i => prefs.outputs.get(i - 1).map(|o| Some(o.clone())),
        }) else {
            return;
        };
        prefs.choosing = None;
        prefs.chosen = chosen.as_ref().map(|o| o.name.clone());
        self.settings.output = chosen.map(|o| o.id);
        self.audio.chosen = self.settings.output.clone();
        self.save_settings();
        if let Some(name) = self.audio.follow_output() {
            self.message = Some((format!("playing through {name}"), false));
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
        if let Some(at) = prefs.choosing {
            let last = prefs.outputs.len();
            match k.key.as_str() {
                "escape" => prefs.choosing = None,
                // Nothing to pick until the devices are listed.
                "enter" | "space" if prefs.listing.is_none() => self.choose_output(at),
                _ if up => prefs.choosing = Some(at.saturating_sub(1)),
                _ if down => prefs.choosing = Some((at + 1).min(last)),
                _ => {}
            }
            return;
        }
        let folders = self.settings.folders.len();
        let row = prefs.row;
        match k.key.as_str() {
            "escape" => self.close_prefs(),
            _ if up => prefs.row = row.saturating_sub(1),
            _ if down => prefs.row = (row + 1).min(folders + 3),
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
            "enter" | "space" if row == folders + 3 => self.open_outputs(),
            _ => {}
        }
    }

    fn poll(&mut self, cx: &mut Context<Self>) {
        let mut changed = self.audio.tick();
        // The trainer: the speed for how often the loop has come round since it began.
        if let Some(base) = self.training {
            let wraps = self.audio.wraps();
            // A recording opened anew counts from zero again.
            let base = if wraps < base { 0 } else { base };
            self.training = Some(base);
            let percent = self.settings.trainer.percent(wraps - base);
            if percent != self.speed.now() {
                self.speed.set(percent);
                self.apply_speed();
                changed = true;
            }
        }
        if let Some(prefs) = &mut self.prefs
            && let Some(outputs) = prefs.listing.as_ref().and_then(|rx| rx.try_recv().ok())
        {
            // The picker opens on what plays now.
            let now = self.settings.output.as_ref().and_then(|id| outputs.iter().position(|o| &o.id == id));
            prefs.choosing = Some(now.map_or(0, |i| i + 1));
            (prefs.outputs, prefs.listing) = (outputs, None);
            changed = true;
        }
        if self.saved_at.elapsed() > SESSION_EVERY {
            self.save_session();
            // The output device is checked as often: headphones plugged in, AirPods
            // connecting, or a device chosen in settings coming back.
            if let Some(name) = self.audio.follow_output() {
                self.message = Some((format!("playing through {name}"), false));
                changed = true;
            }
        }
        if self.scrolled_at.is_some_and(|at| at.elapsed() > SCROLLBAR_LINGERS) {
            self.scrolled_at = None;
            changed = true;
        }
        // Where the playing song is, kept now and then, so a crash loses little.
        if self.audio.playing() && self.kept.elapsed() > Duration::from_secs(5) {
            self.keep_spot();
            self.store_resume();
        }
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
                Ok(parts) => {
                    self.wave = Some((path.clone(), Rc::new(wave::mix(&parts))));
                    if parts.len() > 1 {
                        self.stem_waves = parts.into_iter().map(Rc::new).collect();
                    }
                }
                Err(e) => self.wave_error = Some(e),
            }
            self.reading = None;
            changed = true;
        }
        if let Some((song, rx)) = &self.adding
            && let Some(result) = match rx.try_recv() {
                Ok(result) => Some(result),
                Err(std::sync::mpsc::TryRecvError::Empty) => None,
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    Some(Err("the reader stopped without an answer".into()))
                }
            }
        {
            let song = song.clone();
            self.adding = None;
            match result {
                Ok(v) => {
                    let how = match v.offset {
                        Some(o) if o.abs() < 0.002 => "lined up with the song".to_string(),
                        Some(o) => format!("lined up, {:+.3} s from the song", o),
                        None => "on its own timeline".to_string(),
                    };
                    let what = if v.files.len() > 1 { format!("{} stems", v.files.len()) } else { v.label.clone() };
                    // Said first, so a failure to store replaces it rather than hides.
                    self.message = Some((format!("added {what}, {how}; v switches"), false));
                    self.variations.add(&song, v);
                    self.store_variations();
                    let keep = self.selected_path();
                    self.rebuild_rows();
                    self.reindex(keep);
                }
                Err(e) => self.message = Some((format!("couldn't add that: {e}"), true)),
            }
            changed = true;
        }
        if let Some((path, rx)) = &self.finding
            && let Ok(result) = rx.try_recv()
        {
            // Without beats the grid simply stays off for this song; the waveform still
            // reports a song that can't be read.
            if let Ok(found) = result {
                self.beats = Some((path.clone(), Rc::new(found)));
            }
            self.finding = None;
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
        // A glide aimed at the old list would drag the new one along.
        self.glide = None;
        self.rows.clear();
        if let Some(of) = &self.album {
            // The album's header sits above the list, see `render_album`.
            self.rows.extend(
                library::album(&self.tracks, of)
                    .into_iter()
                    .map(Row::Track),
            );
        } else if let Some(of) = &self.artist {
            // Likewise the artist's, see `render_artist`.
            self.rows.extend(library::artist(&self.tracks, of).into_iter().map(Row::Track));
        } else if self.query.trim().is_empty() {
            // Library order, since the tracks are sorted.
            let mut favorites: Vec<usize> = self
                .favorites
                .0
                .iter()
                .filter_map(|p| self.by_path.get(p).copied())
                .collect();
            favorites.sort();
            let songs: Vec<usize> =
                (0..self.tracks.len()).filter(|&i| !self.variations.is_variation(&self.tracks[i].path)).collect();
            if !favorites.is_empty() {
                self.rows.push(Row::Header("Favourites".into()));
                let rows = self.list_rows(&favorites);
                self.rows.extend(rows);
                self.rows.push(Row::Header("Library".into()));
            }
            let rows = self.list_rows(&songs);
            self.rows.extend(rows);
        } else {
            self.rows.extend(
                library::search(&self.tracks, &self.query)
                    .into_iter()
                    .filter(|&i| !self.variations.is_variation(&self.tracks[i].path))
                    .map(Row::Track),
            );
        }
        self.selected = self.first_track_row(0, 1).unwrap_or(0);
    }

    /// The full list's songs as rows: grouped under each artist when that's the view.
    fn list_rows(&self, songs: &[usize]) -> Vec<Row> {
        match self.settings.list {
            ListView::Columns => songs.iter().map(|&i| Row::Track(i)).collect(),
            ListView::Grouped => library::grouped(&self.tracks, songs)
                .into_iter()
                .map(|g| match g {
                    Grouped::Artist(name, n) => Row::Artist(name, n),
                    Grouped::Track(i) => Row::Track(i),
                })
                .collect(),
        }
    }

    /// Whether the rows are grouped under artists: the full list in the grouped view.
    fn grouped(&self) -> bool {
        self.settings.list == ListView::Grouped
            && self.album.is_none()
            && self.artist.is_none()
            && self.query.trim().is_empty()
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
            Row::Header(_) | Row::Artist(..) => None,
        }
    }

    fn move_selection(&mut self, by: isize) {
        self.glide = None;
        let target = (self.selected as isize + by).clamp(0, self.rows.len() as isize - 1);
        let step = if by < 0 { -1 } else { 1 };
        if let Some(row) = self
            .first_track_row(target.max(0) as usize, step)
            .or_else(|| self.first_track_row(target.max(0) as usize, -step))
        {
            self.selected = row;
            // A row off screen is brought in at the edge it comes from: one row's
            // scroll, not a page's (always `Top` threw a row below the view to the top).
            let edge = if by > 0 { ScrollStrategy::Bottom } else { ScrollStrategy::Top };
            self.scroll.scroll_to_item(row, edge);
        }
    }

    /// Plays a song where it was left: its recording, slow-down and loops, and, when
    /// picked by hand (`pick`), its playhead. A song that follows on in a list starts
    /// from the top.
    fn play(&mut self, track: Track, pick: bool) {
        self.keep_spot();
        (self.shaping, self.dragging) = (None, None);
        self.store_resume();
        self.variation = 0;
        self.muted.clear();
        self.solo = None;
        let spot = self.resume.of(&track.path).cloned().unwrap_or_default();
        self.loops.extend(spot.loops.clone());
        self.marks.extend(spot.marks.clone());
        self.levels.extend(spot.levels.clone());
        self.speed = spot.pace();
        self.training = None;
        self.audio.set_speed(self.speed.factor());
        self.audio.set_pitch(self.speed.pitch());
        let at = if pick { spot.start(track.seconds.map(Duration::from_secs)) } else { Duration::ZERO };
        let variations = self.variations.of(&track.path).len();
        self.audio.play(std::slice::from_ref(&track.path), &[1.0]);
        if self.wave.as_ref().is_none_or(|(p, _)| *p != track.path) {
            self.read_wave(vec![track.path.clone()]);
        }
        if self.beats.as_ref().is_none_or(|(p, _)| *p != track.path) {
            let (tx, rx) = std::sync::mpsc::channel();
            let (path, size, modified) = (track.path.clone(), track.size, track.modified);
            std::thread::spawn(move || {
                let dir = state::data_dir().join("beats");
                let _ = tx.send(beats::cached(&path, size, modified, &dir));
            });
            self.finding = Some((track.path.clone(), rx));
            self.beats = None;
        }
        let path = track.path.clone();
        self.now = Some(track);
        self.message = None;
        if (1..=variations).contains(&spot.variation) {
            self.open_recording(spot.variation, at);
        } else {
            self.audio.looping = self.loops.get(&path).copied().unwrap_or_default();
            self.audio.resume_at(at);
            if self.audio.looping.active() {
                self.frame_loop();
            }
        }
    }

    /// What riff shows now, as the next start should show it.
    fn session(&self) -> Session {
        Session {
            playing: self.now.as_ref().map(|n| n.path.clone()),
            selected: self.selected_path(),
            practice: self.practice,
            album: self.album.as_ref().map(|t| t.path.clone()),
            artist: self.artist.as_ref().map(|t| t.path.clone()),
            grid: self.grid,
            span: Some(self.span),
            queue: self.queue.clone(),
            sidebar_collapsed: !self.sidebar,
        }
    }

    /// Saves the session when it changed, and the paused playhead when it moved (while
    /// playing, `poll` keeps it every few seconds), so closing the window loses nothing.
    fn save_session(&mut self) {
        self.saved_at = std::time::Instant::now();
        let session = self.session();
        if session != self.saved {
            if let Err(e) = session.store(&session_file()) {
                self.message = Some((format!("couldn't save the session: {e}"), true));
            }
            self.saved = session;
        }
        if self.now.is_some()
            && !self.audio.playing()
            && !self.audio.loading()
            && self.audio.position() != self.kept_at
        {
            self.keep_spot();
            self.store_resume();
        }
    }

    /// Opens as riff was left: the song that was playing, paused where it was (in its
    /// recording, with its loops), the view, the grid and zoom, and the highlighted song.
    fn restore(&mut self, session: Session) {
        let track = |riff: &Self, path: &Option<PathBuf>| {
            path.as_ref().and_then(|p| riff.by_path.get(p)).map(|&i| riff.tracks[i].clone())
        };
        self.grid = session.grid;
        self.queue = session.queue.clone();
        self.sidebar = !session.sidebar_collapsed;
        self.queue.retain(|p| self.by_path.contains_key(p));
        if let Some(span) = session.span {
            self.span = span;
        }
        if let Some(song) = track(self, &session.playing) {
            self.play(song, true);
            self.audio.open_paused();
        }
        self.album = track(self, &session.album);
        self.artist = track(self, &session.artist).filter(|_| self.album.is_none());
        self.practice = session.practice;
        self.reindex(session.selected.clone());
        self.scroll.scroll_to_item(self.selected, ScrollStrategy::Center);
        self.saved = session;
    }

    /// Notes where the playing song is: its recording, slow-down, playhead and loops.
    fn keep_spot(&mut self) {
        let Some(now) = self.now.clone() else { return };
        self.kept_at = self.audio.position();
        if let Some(recording) = self.recording() {
            self.loops.insert(recording, self.audio.looping);
        }
        let recordings = std::iter::once(now.path.clone())
            .chain(self.variations.of(&now.path).iter().filter_map(|v| v.files.first().cloned()));
        let loops = recordings
            .filter_map(|r| self.loops.get(&r).map(|l| (r, *l)))
            .filter(|(_, l)| l.start.is_some() || l.end.is_some())
            .collect();
        let marks = std::iter::once(now.path.clone())
            .chain(self.variations.of(&now.path).iter().filter_map(|v| v.files.first().cloned()))
            .filter_map(|r| self.marks.get(&r).map(|m| (r, *m)))
            .collect();
        let levels = self
            .variations
            .of(&now.path)
            .iter()
            .filter_map(|v| v.files.first())
            .filter_map(|r| self.levels.get(r).map(|l| (r.clone(), l.clone())))
            .filter(|(_, l)| l.iter().any(|l| *l != 1.0))
            .collect();
        let spot = Spot {
            variation: self.variation,
            pace: Some(self.speed),
            at: self.audio.position(),
            loops,
            marks,
            levels,
            ..Spot::default()
        };
        self.resume.keep(&now.path, spot);
        self.kept = std::time::Instant::now();
    }

    fn store_resume(&mut self) {
        if let Err(e) = self.resume.store(&resume_file()) {
            self.message = Some((format!("couldn't save where songs were: {e}"), true));
        }
    }

    /// Reads the waveform of what plays (stems summed), keyed by its first file.
    fn read_wave(&mut self, files: Vec<PathBuf>) {
        let (tx, rx) = std::sync::mpsc::channel();
        let key = files[0].clone();
        std::thread::spawn(move || {
            let _ = tx.send(files.iter().map(|f| wave::read(f)).collect());
        });
        self.reading = Some((key, rx));
        self.wave = None;
        self.wave_error = None;
        self.stem_waves.clear();
    }

    /// The speed: slower and faster by 5%, and what plays now, lit when it isn't full
    /// speed (a click there is `s`). Right-click for presets and the trainer.
    fn speed_control(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let t = t();
        let on = self.speed.on;
        let label = if self.training.is_some() { format!("{}% trainer", self.speed.now()) } else { format!("{}%", self.speed.now()) };
        div()
            .id("speed")
            .flex()
            .items_center()
            .h(px(36.))
            .p(px(3.))
            .gap(px(2.))
            .rounded_md()
            .bg(rgb(t.raised))
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(|riff, e: &MouseDownEvent, _, cx| {
                    riff.menu = Some((Menu::speed(), e.position));
                    cx.notify();
                }),
            )
            .child(div().px_1().child(ui::icon("gauge", 16., if on { t.accent } else { t.dim })))
            .child(self.tool("slower", "minus", "5% slower, pitch kept", Action::Speed(false), false, cx))
            .child(
                div()
                    .id("speed-now")
                    .h_full()
                    .min_w(px(44.))
                    .px_2()
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded_sm()
                    .font_family(MONO)
                    .text_xs()
                    .cursor_pointer()
                    .when(on, |d| d.bg(rgb(t.accent_soft)))
                    .hover(|d| d.bg(rgb(t.faint)))
                    .text_color(rgb(if on { t.accent } else { t.dim }))
                    .tooltip(self.tip(
                        format!("Full speed, again back to {}%; right-click for presets and the trainer", self.speed.percent),
                        &[Action::Slow],
                    ))
                    .child(label)
                    .on_click(cx.listener(|riff, _, _, cx| riff.act(Action::Slow, cx))),
            )
            .child(self.tool("faster", "plus", "5% faster, pitch kept", Action::Speed(true), false, cx))
    }

    /// The pitch: down and up a semitone, and what plays now, lit when shifted. The
    /// wheel over it moves 5 cents; right-click for cents and back to as recorded.
    fn pitch_control(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let t = t();
        let cents = self.speed.pitch();
        let on = cents != 0;
        pill()
            .id("pitch")
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(|riff, e: &MouseDownEvent, _, cx| {
                    riff.menu = Some((Menu::pitch(), e.position));
                    cx.notify();
                }),
            )
            .on_scroll_wheel(cx.listener(|riff, e: &gpui::ScrollWheelEvent, _, cx| {
                let dy = f32::from(e.delta.pixel_delta(px(20.)).y);
                if dy != 0. {
                    riff.act(Action::Pitch(if dy < 0. { 5 } else { -5 }), cx);
                }
            }))
            .child(self.tool("pitch-down", "minus", "Down a semitone, speed kept", Action::Pitch(-100), false, cx))
            .child(
                div()
                    .id("pitch-now")
                    .h(px(28.))
                    .min_w(px(64.))
                    .px_2()
                    .flex()
                    .items_center()
                    .justify_center()
                    .gap_1()
                    .rounded_md()
                    .font_family(MONO)
                    .text_xs()
                    .when(on, |d| d.bg(rgb(t.accent_soft)))
                    .text_color(rgb(if on { t.accent } else { t.dim }))
                    .tooltip(self.tip("Pitch: scroll for 5 cents; right-click for more", &[Action::Pitch(5), Action::Pitch(-5)]))
                    .child(ui::icon("music", 12., if on { t.accent } else { t.dim }))
                    .child(crate::speed::pitch_label(cents)),
            )
            .child(self.tool("pitch-up", "plus", "Up a semitone, speed kept", Action::Pitch(100), false, cx))
    }

    /// The master volume: an icon and a bar filled to the level, dragged or scrolled.
    fn volume_slider(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let t = t();
        let volume = self.audio.volume();
        div()
            .flex()
            .items_center()
            .gap_2()
            .h(px(28.))
            .px_2()
            .rounded_md()
            .bg(rgb(t.raised))
            .child(ui::icon(if volume == 0.0 { "volume-x" } else { "volume-2" }, 16., t.dim))
            .child(
                self.level_area("volume", Slider::Volume, gpui::rgb(t.accent), cx)
                    .tooltip(self.tip(format!("Volume {:.0}%: drag or scroll", volume * 100.), &[Action::Volume(true), Action::Volume(false)]))
                    .w(px(72.))
                    .h(px(10.))
                    .rounded_full()
                    .bg(rgb(t.faint)),
            )
    }

    /// The title bar's switch between library and practice mode, the current one lit.
    fn mode_switch(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let t = t();
        let segment = |id: &'static str, icon: &'static str, label: &'static str, on: bool| {
            div()
                .id(id)
                .h(px(22.))
                .px_2()
                .flex()
                .items_center()
                .gap_1p5()
                .rounded_md()
                .text_xs()
                .when(on, |d| d.bg(rgb(t.accent_soft)))
                .when(!on, |d| d.cursor_pointer().hover(|d| d.bg(rgb(t.faint))))
                .text_color(rgb(if on { t.accent } else { t.dim }))
                .child(ui::icon(icon, 14., if on { t.accent } else { t.dim }))
                .child(label)
                .when(!on, |d| {
                    d.tooltip(self.tip(format!("{label} mode"), &[Action::Practice])).on_click(cx.listener(
                        |riff, _, _, cx| {
                            // Not also a click on the title bar.
                            cx.stop_propagation();
                            riff.act(Action::Practice, cx);
                        },
                    ))
                })
        };
        pill()
            .p(px(2.))
            .mr_2()
            .child(segment("mode-library", "music", "Library", !self.practice))
            .child(segment("mode-practice", "audio-waveform", "Practice", self.practice))
    }

    /// The list's view, in the library's title bar: columns, or grouped by artist.
    fn list_switch(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let t = t();
        let segment = |id: &'static str, icon: &'static str, what: &'static str, on: bool| {
            div()
                .id(id)
                .h(px(22.))
                .w(px(28.))
                .flex()
                .items_center()
                .justify_center()
                .rounded_md()
                .when(on, |d| d.bg(rgb(t.accent_soft)))
                .when(!on, |d| d.cursor_pointer().hover(|d| d.bg(rgb(t.faint))))
                .child(ui::icon(icon, 14., if on { t.accent } else { t.dim }))
                .tooltip(self.tip(what, &[Action::ListView]))
                .when(!on, |d| {
                    d.on_click(cx.listener(|riff, _, _, cx| {
                        cx.stop_propagation();
                        riff.act(Action::ListView, cx);
                    }))
                })
        };
        let grouped = self.settings.list == ListView::Grouped;
        pill()
            .p(px(2.))
            .mr_2()
            .child(segment("list-columns", "columns-3", "Columns: artist, title, album", !grouped))
            .child(segment("list-grouped", "list-tree", "Grouped by artist", grouped))
    }

    /// An action from a click, done exactly as its key would do it.
    fn act(&mut self, action: Action, cx: &mut Context<Self>) {
        match action {
            Action::Settings => {
                if self.prefs.is_some() {
                    self.close_prefs();
                } else {
                    self.prefs = Some(Prefs::new(self.settings.output.as_deref()));
                }
            }
            Action::AddVariation => {
                if let Some(song) = self.now.as_ref().map(|n| n.path.clone()) {
                    self.pick_variation(song, cx);
                }
            }
            _ => {
                self.perform(action);
            }
        }
        cx.notify();
    }

    /// An icon button doing `action`, lit while it is on; its tooltip says what it does
    /// and its keys.
    fn tool(
        &self,
        id: impl Into<gpui::ElementId>,
        icon: &'static str,
        what: &str,
        action: Action,
        lit: bool,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let t = t();
        div()
            .id(id)
            .w(px(32.))
            .h(px(28.))
            .flex_none()
            .flex()
            .items_center()
            .justify_center()
            .rounded_md()
            .cursor_pointer()
            .when(lit, |d| d.bg(rgb(t.accent_soft)))
            .hover(|d| d.bg(rgb(t.faint)))
            .child(ui::icon(icon, 16., if lit { t.accent } else { t.text }))
            .tooltip(self.tip(what.to_string(), &[action]))
            .on_click(cx.listener(move |riff, _, _, cx| {
                // Not also a click on what it sits on (a row, the title bar).
                cx.stop_propagation();
                riff.act(action, cx);
            }))
    }

    /// A tooltip for a control doing `what`, with the keys for `actions`.
    fn tip(
        &self,
        what: impl Into<gpui::SharedString>,
        actions: &[Action],
    ) -> impl Fn(&mut Window, &mut gpui::App) -> gpui::AnyView + 'static {
        let what = what.into();
        let keys: Vec<&'static str> =
            actions.iter().flat_map(|a| keys::keys_for(*a, self.mode(), self.settings.vim)).collect();
        move |_, cx| cx.new(|_| Tip { what: what.clone(), keys: keys.clone() }).into()
    }

    /// Hands the speed to the audio and says it. Kept with the song by `keep_levels`,
    /// so a slider drag doesn't write the file at every step.
    fn apply_speed(&mut self) {
        self.audio.set_speed(self.speed.factor());
        self.audio.set_pitch(self.speed.pitch());
        let pitch = crate::speed::pitch_label(self.speed.pitch());
        self.message = Some((
            match (self.speed.now(), self.speed.pitch()) {
                (100, 0) => "as recorded".to_string(),
                (p, 0) => format!("{p}% speed, pitch kept"),
                (100, _) => format!("pitch {pitch}, speed kept"),
                (p, _) => format!("{p}% speed, pitch {pitch}"),
            },
            false,
        ));
    }

    /// Each stem's gain: its level, the soloed one alone, the muted ones silent.
    fn gains(&self) -> Vec<f32> {
        crate::audio::stem_gains(&self.stem_levels(), &self.muted, self.solo)
    }

    /// The playing stem set's levels.
    fn stem_levels(&self) -> Vec<f32> {
        self.recording().and_then(|r| self.levels.get(&r).cloned()).unwrap_or_default()
    }

    /// Sets a level: the volume, or a stem's (heard at once, kept by `keep_levels`).
    fn set_level(&mut self, slider: Slider, level: f32) {
        match slider {
            Slider::Volume => {
                self.audio.set_volume(level);
                self.settings.volume = self.audio.volume();
                self.message = Some((format!("volume {:.0}%", self.audio.volume() * 100.), false));
            }
            Slider::Stem(i) if i < self.muted.len() => {
                let (Some(recording), n) = (self.recording(), self.muted.len()) else { return };
                let levels = self.levels.entry(recording).or_insert_with(|| vec![1.0; n]);
                levels.resize(n, 1.0);
                levels[i] = level.clamp(0.0, 1.0);
                self.audio.set_gains(&self.gains());
            }
            Slider::Stem(_) => {}
            Slider::Speed => {
                use crate::speed::{FASTEST, SLOWEST};
                // The trainer gives way to a hand on the speed.
                self.training = None;
                self.speed.set(SLOWEST + (level * (FASTEST - SLOWEST) as f32).round() as u16);
                self.apply_speed();
            }
        }
    }

    /// Keeps the levels: the volume in settings, a stem set's with the song.
    fn keep_levels(&mut self, slider: Slider) {
        match slider {
            Slider::Volume => self.save_settings(),
            Slider::Stem(_) | Slider::Speed => {
                self.keep_spot();
                self.store_resume();
            }
        }
    }

    /// The level a slider shows.
    fn level(&self, slider: Slider) -> f32 {
        match slider {
            Slider::Volume => self.audio.volume(),
            Slider::Stem(i) => self.stem_levels().get(i).copied().unwrap_or(1.0),
            Slider::Speed => {
                use crate::speed::{FASTEST, SLOWEST};
                (self.speed.now().clamp(SLOWEST, FASTEST) - SLOWEST) as f32 / (FASTEST - SLOWEST) as f32
            }
        }
    }

    /// A slider's level from the pointer's place along it.
    fn slide(&mut self, slider: Slider, bounds: Bounds<Pixels>, x: Pixels) {
        let width = f32::from(bounds.size.width);
        if width > 0. {
            self.set_level(slider, crate::audio::level_at(f32::from(x - bounds.origin.x) / width));
        }
    }

    /// An area that sets `slider`'s level: a drag across it, or the wheel over it. Its
    /// level shows as a fill from the left; the caller sizes it and adds what it shows.
    fn level_area(&self, id: impl Into<gpui::ElementId>, slider: Slider, fill: gpui::Rgba, cx: &mut Context<Self>) -> Stateful<Div> {
        let bounds: Rc<Cell<Bounds<Pixels>>> = Rc::default();
        let (drawn, pressed) = (bounds.clone(), bounds);
        div()
            .id(id)
            .relative()
            .overflow_hidden()
            .cursor(gpui::CursorStyle::ResizeLeftRight)
            .child(canvas(move |b, _, _| drawn.set(b), |_, _, _, _| {}).absolute().size_full())
            .child(div().absolute().top_0().left_0().h_full().w(relative(self.level(slider))).bg(fill))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |riff, e: &MouseDownEvent, _, cx| {
                    cx.stop_propagation();
                    riff.slide(slider, pressed.get(), e.position.x);
                    riff.sliding = Some((slider, pressed.clone()));
                    cx.notify();
                }),
            )
            .on_scroll_wheel(cx.listener(move |riff, e: &gpui::ScrollWheelEvent, _, cx| {
                let dy = f32::from(e.delta.pixel_delta(px(20.)).y);
                if dy != 0. {
                    // A notch is a 5% step, as ⌥↑ ⌥↓ are for the volume.
                    let level = riff.level(slider) - dy / 20. * crate::audio::VOLUME_STEP;
                    riff.set_level(slider, crate::audio::level_at(level));
                    riff.keep_levels(slider);
                    cx.notify();
                }
            }))
    }

    /// Lets go of a level being dragged, keeping it.
    fn drop_slider(&mut self) -> bool {
        let Some((slider, _)) = self.sliding.take() else { return false };
        self.keep_levels(slider);
        true
    }

    /// The playing variation, if one is on rather than the song itself.
    fn current_variation(&self) -> Option<&Variation> {
        let now = self.now.as_ref()?;
        self.variations.of(&now.path).get(self.variation.checked_sub(1)?)
    }

    /// Seconds from the song's timeline to what plays: 0 for the song, a variation's
    /// offset, or none for a variation on a timeline of its own.
    fn offset(&self) -> Option<f64> {
        match self.current_variation() {
            None => Some(0.0),
            Some(v) => v.offset,
        }
    }

    /// The next recording of the playing song: the song, then each variation. Position
    /// and loop carry over between recordings that line up; one on its own timeline
    /// starts from its beginning.
    fn next_variation(&mut self) {
        let Some(now) = self.now.clone() else { return };
        let list = self.variations.of(&now.path).to_vec();
        if list.is_empty() {
            self.message = Some(("no variations: drop files on the song, or V to pick".into(), false));
            return;
        }
        self.select_recording((self.variation + 1) % (list.len() + 1));
    }

    /// Plays recording `target` of the song (0 the song, n its n-th variation), from the
    /// same moment where the two line up.
    fn select_recording(&mut self, target: usize) {
        let Some(now) = self.now.clone() else { return };
        let list = self.variations.of(&now.path).to_vec();
        if target == self.variation || target > list.len() {
            return;
        }
        let into = if target == 0 { Some(0.0) } else { list[target - 1].offset };
        // Each recording keeps its own loop: this one's is put away, the next one's
        // comes back. The position carries over where the two line up.
        if let Some(leaving) = self.recording() {
            self.loops.insert(leaving, self.audio.looping);
        }
        let at = match (self.offset(), into) {
            (Some(from), Some(to)) => {
                Duration::from_secs_f64((self.audio.position().as_secs_f64() + to - from).max(0.0))
            }
            _ => Duration::ZERO,
        };
        self.open_recording(target, at);
        self.keep_spot();
        self.store_resume();
    }

    /// Plays recording `target` of the song (0 the song, n its n-th variation) from
    /// `at`, with that recording's own loop.
    fn open_recording(&mut self, target: usize, at: Duration) {
        let Some(now) = self.now.clone() else { return };
        let list = self.variations.of(&now.path).to_vec();
        if target > list.len() {
            return;
        }
        let into = if target == 0 { Some(0.0) } else { list[target - 1].offset };
        let (files, gains, label) = match target {
            0 => (vec![now.path.clone()], vec![1.0], "the song".to_string()),
            n => {
                let v = &list[n - 1];
                (v.files.clone(), variations::default_gains(v), v.label.clone())
            }
        };
        self.variation = target;
        // A drag belongs to the recording it started on.
        (self.shaping, self.dragging) = (None, None);
        let looping = self.recording().and_then(|r| self.loops.get(&r).copied()).unwrap_or_default();
        // A stem muted from the start (the metronome) shows as muted; one recording has
        // no stems to mute.
        self.muted = if files.len() > 1 { gains.iter().map(|g| *g == 0.0).collect() } else { Vec::new() };
        self.solo = None;
        // Each stem at the level it was left at.
        let gains = if files.len() > 1 { self.gains() } else { gains };
        self.audio.switch(&files, &gains, at, looping);
        if looping.active() {
            self.frame_loop();
        }
        if self.wave.as_ref().is_none_or(|(p, _)| *p != files[0]) {
            self.read_wave(files.clone());
        }
        if into.is_some() {
            self.song_beats();
        }
        if into.is_none() && self.beats.as_ref().is_none_or(|(p, _)| *p != files[0]) {
            // On its own timeline the song's beats don't apply; find this one's.
            let (tx, rx) = std::sync::mpsc::channel();
            let key = files[0].clone();
            std::thread::spawn(move || {
                let _ = tx.send(beats::read_mix(&files));
            });
            self.finding = Some((key, rx));
        }
        let timeline = if into.is_none() { ", its own timeline" } else { "" };
        self.message = Some((format!("playing {label}{timeline}"), false));
    }

    /// The song's own beats again, after a recording with a timeline of its own had them
    /// replaced (from the cache, so at once).
    fn song_beats(&mut self) {
        let Some(now) = self.now.clone() else { return };
        if self.beats.as_ref().is_some_and(|(p, _)| *p == now.path) {
            return;
        }
        let (tx, rx) = std::sync::mpsc::channel();
        let (path, size, modified) = (now.path.clone(), now.size, now.modified);
        std::thread::spawn(move || {
            let dir = state::data_dir().join("beats");
            let _ = tx.send(beats::cached(&path, size, modified, &dir));
        });
        self.finding = Some((now.path, rx));
    }

    /// Makes dropped or picked files a variation of `song`, in the background: reading
    /// and aligning takes a moment.
    fn add_variation(&mut self, song: PathBuf, dropped: Vec<PathBuf>) {
        let (tx, rx) = std::sync::mpsc::channel();
        let path = song.clone();
        let names: Vec<String> = dropped
            .iter()
            .filter_map(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()))
            .collect();
        std::thread::spawn(move || {
            // A panic becomes an error to show, not a reader that silently never answers.
            let built = std::panic::catch_unwind(|| variations::build(&path, &dropped))
                .unwrap_or_else(|panic| {
                    let why = panic
                        .downcast_ref::<String>()
                        .cloned()
                        .or_else(|| panic.downcast_ref::<&str>().map(|s| s.to_string()))
                        .unwrap_or_else(|| "it crashed".into());
                    Err(format!("reading failed: {why}"))
                });
            let _ = tx.send(built);
        });
        self.adding = Some((song, rx));
        self.message = Some((format!("reading {}…", names.join(", ")), false));
    }

    /// Asks for files to add as a variation of `song`.
    fn pick_variation(&mut self, song: PathBuf, cx: &mut Context<Self>) {
        let picked = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: true,
            multiple: true,
            prompt: Some("Add as variation".into()),
        });
        cx.spawn(async move |this, cx| {
            if let Ok(Ok(Some(files))) = picked.await {
                let _ = this.update(cx, |riff, cx| {
                    riff.add_variation(song, files);
                    cx.notify();
                });
            }
        })
        .detach();
    }

    fn store_variations(&mut self) {
        if let Err(e) = self.variations.store(&variations_file()) {
            self.message = Some((format!("couldn't save variations: {e}"), true));
        }
    }

    /// Plays the selected song. The rest of its list follows: the album, the favourites,
    /// the search results or the library, whichever it was picked from.
    fn play_selected(&mut self) {
        let Some(track) = self.selected_track().cloned() else {
            return;
        };
        // The list is the run of songs between the section headers around the
        // selection; artist rows are part of it.
        let is_track = |r: &usize| !matches!(self.rows[*r], Row::Header(_));
        let start = (0..self.selected).rev().take_while(is_track).last().unwrap_or(self.selected);
        let list = (start..self.rows.len())
            .take_while(is_track)
            .filter_map(|r| match self.rows[r] {
                Row::Track(i) => Some(self.tracks[i].path.clone()),
                Row::Header(_) | Row::Artist(..) => None,
            })
            .collect();
        self.playlist = Playlist::new(list, &track.path, self.seed());
        self.play(track, true);
    }

    /// A fresh shuffle when shuffling, none otherwise.
    fn seed(&self) -> Option<u64> {
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH);
        self.shuffle.then(|| now.map_or(1, |d| d.as_nanos() as u64))
    }

    fn track_at(&self, path: &PathBuf) -> Option<Track> {
        self.by_path.get(path).map(|&i| self.tracks[i].clone())
    }

    /// Plays the next song: the queue's first, else the list's. Returns whether there
    /// was one.
    fn next_song(&mut self) -> bool {
        // A song gone since a rescan is skipped.
        while let Some(path) = self.playlist.next(&mut self.queue) {
            if let Some(track) = self.track_at(&path) {
                self.play(track, false);
                return true;
            }
        }
        false
    }

    /// The highlighted queue row, kept inside the queue as it shrinks.
    fn queued_at(&self) -> Option<usize> {
        let at = self.queue_at?;
        (!self.queue.is_empty()).then(|| at.min(self.queue.len() - 1))
    }

    /// The `i`-th queued song, now: it leaves the queue and the list stays where it was.
    fn play_queued(&mut self, i: usize) {
        let Some(path) = self.queue.take(i) else { return };
        if self.queue.is_empty() {
            self.queue_at = None;
        }
        if let Some(track) = self.track_at(&path) {
            self.playlist.play_queued(&path);
            self.play(track, true);
        }
    }

    /// A sidebar entry: the full list (scrolled to its favourites, or to the playing
    /// song), leaving practice, search, and the album or artist view.
    fn go_to(&mut self, place: Place) {
        (self.practice, self.searching, self.queue_at) = (false, false, None);
        self.set_query(String::new());
        match place {
            Place::Library => {}
            Place::Favourites => {
                if let Some(row) = self.rows.iter().position(|r| matches!(r, Row::Header(h) if h == "Favourites")) {
                    self.selected = self.first_track_row(row, 1).unwrap_or(row);
                    self.scroll.scroll_to_item(row, ScrollStrategy::Top);
                }
            }
            Place::Playing => self.to_playing(),
        }
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
            self.play(track, false);
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
        self.artist = None;
        self.reindex(keep);
        self.scroll.scroll_to_item(self.selected, ScrollStrategy::Center);
    }

    /// ⇧⌘A: everything by the selected song's artist, or back to what was shown before.
    fn toggle_artist(&mut self) {
        let keep = self.selected_path();
        self.artist = match self.artist {
            Some(_) => None,
            None => self.selected_track().cloned(),
        };
        self.album = None;
        self.reindex(keep);
        self.scroll.scroll_to_item(self.selected, ScrollStrategy::Center);
    }

    /// The album or artist of `track` (the playing song, from its card), with `track`
    /// highlighted.
    fn show(&mut self, track: Track, artist: bool) {
        let keep = Some(track.path.clone());
        (self.album, self.artist) = if artist { (None, Some(track)) } else { (Some(track), None) };
        self.query.clear();
        self.searching = false;
        self.practice = false;
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

    /// Which keys apply now: practice, searching, or the library.
    fn mode(&self) -> keys::Mode {
        if self.queue_at.is_some() {
            keys::Mode::Queue
        } else if self.practice {
            keys::Mode::Practice
        } else if self.searching {
            keys::Mode::Search
        } else {
            keys::Mode::Library
        }
    }

    fn key(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let k = &event.keystroke;
        let press = keys::Press {
            key: &k.key,
            cmd: k.modifiers.platform,
            ctrl: k.modifiers.control,
            shift: k.modifiers.shift,
            alt: k.modifiers.alt,
        };
        // An open menu takes every key until it closes.
        if let Some((menu, _)) = &mut self.menu {
            match menu.key(&k.key) {
                Some(Outcome::Close) => self.menu = None,
                Some(Outcome::Choose(action)) => self.choose(action, cx),
                None => {}
            }
            return cx.notify();
        }
        let global = keys::anywhere(press);
        // The sheets swallow every key but their own toggles and ⌘Q.
        match global {
            // GPUI apps get no menu bar, so ⌘Q has to be handled here.
            // Without a menu bar macOS doesn't do these for riff.
            Some(Action::Minimize) => return window.minimize_window(),
            Some(Action::FullScreen) => return window.toggle_fullscreen(),
            Some(Action::Quit) => {
                self.keep_spot();
                self.store_resume();
                self.save_session();
                return cx.quit();
            }
            Some(Action::Help) => {
                self.help = !self.help;
                cx.notify();
                return;
            }
            Some(Action::Settings) if !self.help => {
                if self.prefs.is_some() {
                    self.close_prefs();
                } else {
                    self.prefs = Some(Prefs::new(self.settings.output.as_deref()));
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
        if let Some(at) = self.trainer_sheet {
            let field = crate::trainer::FIELDS[at];
            match k.key.as_str() {
                "escape" | "enter" => self.trainer_sheet = None,
                "up" | "k" => self.trainer_sheet = Some(at.saturating_sub(1)),
                "down" | "j" => self.trainer_sheet = Some((at + 1).min(crate::trainer::FIELDS.len() - 1)),
                "left" | "h" | "-" | "," => self.nudge_trainer(field, false),
                "right" | "l" | "=" | "." => self.nudge_trainer(field, true),
                _ => {}
            }
            return cx.notify();
        }
        if let Some(name) = &mut self.naming {
            match k.key.as_str() {
                "enter" => self.name_region(),
                // A dialog's Esc: nothing saved.
                "escape" => (self.naming, self.renaming) = (None, None),
                "backspace" => {
                    name.pop();
                }
                _ => {
                    if let Some(text) = k.key_char.as_ref().filter(|_| !press.cmd && !press.ctrl) {
                        name.push_str(text);
                    }
                }
            }
            cx.notify();
            return;
        }
        if self.help {
            // Typing finds keys; Esc clears what's typed, then closes, as ⌘? and ? do.
            let mut pending = None;
            let closes = keys::press(self.mode(), press, self.settings.vim, &mut pending) == Some(Action::Help);
            match k.key.as_str() {
                "escape" if !self.help_query.is_empty() => self.help_query.clear(),
                "escape" => self.help = false,
                _ if closes => self.help = false,
                "backspace" => {
                    self.help_query.pop();
                }
                _ => {
                    if let Some(text) = k.key_char.as_ref().filter(|_| !press.cmd && !press.ctrl) {
                        self.help_query.push_str(text);
                    }
                }
            }
            if !self.help {
                self.help_query.clear();
            }
            return cx.notify();
        }
        let action = keys::press(self.mode(), press, self.settings.vim, &mut self.pending);
        // While searching, what isn't a key types into the query.
        if self.searching && !self.practice && action.is_none() {
            if k.key == "backspace" {
                let mut q = self.query.clone();
                q.pop();
                self.set_query(q);
            } else if let Some(text) = k.key_char.as_ref().filter(|_| !press.cmd && !press.ctrl) {
                let q = format!("{}{text}", self.query);
                self.set_query(q);
            }
            return cx.notify();
        }
        if let Some(action) = action {
            self.act(action, cx);
        }
    }

    fn perform(&mut self, action: Action) {
        let song = self.audio.duration;
        match action {
            Action::Move(by) if self.queue_at.is_some() => {
                let last = self.queue.len().saturating_sub(1);
                let at = self.queued_at().unwrap_or(0) as isize + by;
                self.queue_at = Some(at.clamp(0, last as isize) as usize);
            }
            Action::Move(by) => self.move_selection(by),
            Action::Queue(top) => {
                let Some(path) = self.selected_path() else { return };
                let place = match top {
                    true => {
                        self.queue.add_top(&path);
                        Some(0)
                    }
                    false => self.queue.add(&path),
                };
                self.message = Some((crate::queue::said(place), false));
            }
            Action::QueueFocus if self.queue_at.is_some() => self.queue_at = None,
            Action::QueueFocus if self.queue.is_empty() => {
                self.message = Some(("the queue is empty, q queues the highlighted song".into(), false));
            }
            Action::QueueFocus => (self.sidebar, self.queue_at) = (true, Some(0)),
            Action::ListView => {
                self.settings.list = match self.settings.list {
                    ListView::Columns => ListView::Grouped,
                    ListView::Grouped => ListView::Columns,
                };
                self.save_settings();
                let keep = self.selected_path();
                self.reindex(keep);
                self.scroll.scroll_to_item(self.selected, ScrollStrategy::Center);
            }
            Action::Sidebar => {
                self.sidebar = !self.sidebar;
                if !self.sidebar {
                    self.queue_at = None;
                }
            }
            Action::QueuePlay => {
                if let Some(i) = self.queued_at() {
                    self.play_queued(i);
                }
            }
            Action::QueueTop => {
                if let Some(i) = self.queued_at() {
                    self.queue.to_top(i);
                    self.queue_at = Some(0);
                }
            }
            Action::QueueRemove => {
                if let Some(i) = self.queued_at() {
                    self.queue.remove(i);
                    // Kept inside the queue; an empty one hands the keyboard back.
                    self.queue_at = self.queued_at();
                }
            }
            Action::Top => self.move_selection(-(self.rows.len() as isize)),
            Action::Bottom => self.move_selection(self.rows.len() as isize),
            Action::ToPlaying => self.to_playing(),
            // From practice, search goes back to the library to type.
            Action::Search if self.practice => (self.practice, self.searching) = (false, true),
            Action::Search => self.searching = !self.searching,
            Action::Clear | Action::ClearSearch => self.set_query(String::new()),
            Action::Play => {
                self.searching = false;
                self.play_selected();
            }
            Action::Toggle => self.audio.toggle(),
            Action::Seek(by) => match self.grid() {
                // Back restarts the beat you're in first, like previous song.
                Some(b) => {
                    let at = self.audio.position();
                    let to = if by < 0. { b.previous(at) } else { b.next(at) };
                    if let Some(to) = to {
                        self.audio.seek(to);
                    }
                }
                None => self.audio.seek_by(by),
            },
            Action::ToStart => self.audio.seek(Duration::ZERO),
            Action::FromLoopStart => {
                self.audio.play_from(crate::audio::replay_from(self.marker(), self.audio.looping));
            }
            Action::Marker => {
                let at = self.snap(self.audio.position());
                self.set_marker(crate::audio::place_marker(self.marker(), at));
            }
            Action::Previous => self.previous_song(),
            Action::Next => {
                self.next_song();
            }
            Action::Shuffle => self.toggle_shuffle(),
            Action::Favorite => self.toggle_favorite(),
            Action::Album => self.toggle_album(),
            Action::Artist => self.toggle_artist(),
            Action::Transcribe => {
                let track = self.selected_track().cloned().or_else(|| self.now.clone());
                self.transcribe(track);
            }
            // Bare `?` arrives here; ⌘? is handled first in `key`, as are ⌘, and ⌘Q.
            Action::Help => self.help = !self.help,
            Action::Settings | Action::Quit | Action::Minimize | Action::FullScreen => {}
            Action::LoopStart => self.audio.looping.set_start(self.snap(self.audio.position())),
            Action::LoopEnd => self.loop_end(self.snap(self.audio.position())),
            Action::LoopClear => self.audio.looping = Default::default(),
            Action::LoopSwitch => self.audio.looping.switch(),
            // On the grid, an edge moves a whole beat and the loop slides in
            // beats, so it stays on the music as the band drifts.
            Action::NudgeStart(by) => {
                let grid = self.grid();
                let looping = &mut self.audio.looping;
                match grid {
                    Some(b) => {
                        let from = looping.start.unwrap_or_default();
                        if let Some(to) = b.step(from, if by < 0. { -1 } else { 1 }) {
                            looping.move_start(to);
                        }
                    }
                    None => looping.nudge_start(by),
                }
            }
            Action::NudgeEnd(by) => {
                let grid = self.grid();
                let looping = &mut self.audio.looping;
                match (grid, looping.end) {
                    (Some(b), Some(end)) => {
                        if let Some(to) = b.step(end, if by < 0. { -1 } else { 1 }) {
                            looping.move_end(to, song);
                        }
                    }
                    _ => looping.nudge_end(by, song),
                }
            }
            Action::Slide(forward) => {
                let grid = self.grid();
                let looping = &mut self.audio.looping;
                match (grid, looping.end) {
                    (Some(b), Some(end)) => {
                        let start = looping.start.unwrap_or_default();
                        if let Some((a, z)) = b.slide(start, end, forward) {
                            (looping.start, looping.end) = (Some(a), Some(z));
                        }
                    }
                    _ => looping.slide(forward, song),
                }
            }
            Action::Grid => self.grid = !self.grid,
            Action::SaveRegion => {
                let Some(recording) = self.recording() else { return };
                if self.audio.looping.active() {
                    let known = self.regions.matching(&recording, self.audio.looping);
                    self.naming = Some(known.map(|r| r.name.clone()).unwrap_or_default());
                }
            }
            Action::Region(forward) => {
                let Some(recording) = self.recording() else { return };
                let looping = self.audio.looping;
                let from = looping.start.filter(|_| looping.active()).unwrap_or(self.audio.position());
                if let Some(region) = self.regions.step(&recording, from, forward).cloned() {
                    self.play_region(&region);
                }
            }
            // A hand on the speed stops the trainer.
            Action::Slow | Action::Speed(_) | Action::SetSpeed(_) | Action::Pitch(_) | Action::SetPitch(_) => {
                match action {
                    Action::Slow => self.speed.toggle(),
                    Action::Speed(faster) => self.speed.step(faster),
                    Action::SetSpeed(percent) => self.speed.set(percent),
                    Action::Pitch(by) => self.speed.shift(by),
                    Action::SetPitch(cents) => self.speed.set_pitch(cents),
                    _ => {}
                }
                // The trainer keeps going through a change of pitch, not of speed.
                if !matches!(action, Action::Pitch(_) | Action::SetPitch(_)) {
                    self.training = None;
                }
                self.apply_speed();
                self.keep_levels(Slider::Speed);
            }
            Action::Trainer => match self.training {
                Some(_) => {
                    self.training = None;
                    self.message = Some((format!("trainer off, staying at {}%", self.speed.now()), false));
                }
                None if !self.audio.looping.looping() => {
                    self.message = Some(("the trainer needs a loop: set one with a and b".into(), false));
                }
                None => {
                    self.training = Some(self.audio.wraps());
                    let t = self.settings.trainer;
                    self.speed.set(t.from);
                    self.apply_speed();
                    self.message = Some((
                        format!("trainer: {}% to {}%, {}% faster every {} repeats", t.from, t.to, t.step, t.every),
                        false,
                    ));
                }
            },
            Action::TrainerSettings => self.trainer_sheet = Some(0),
            Action::Variation => self.next_variation(),
            Action::Mute(n) if n < self.muted.len() => {
                self.muted[n] = !self.muted[n];
                self.audio.set_gains(&self.gains());
            }
            Action::Solo(n) if n < self.muted.len() => {
                self.solo = if self.solo == Some(n) { None } else { Some(n) };
                self.audio.set_gains(&self.gains());
            }
            Action::Mute(_) | Action::Solo(_) => {}
            Action::Level(i, percent) => {
                self.set_level(Slider::Stem(i), percent as f32 / 100.);
                self.keep_levels(Slider::Stem(i));
            }
            Action::Volume(up) => {
                self.set_level(Slider::Volume, crate::audio::step_volume(self.audio.volume(), up));
                self.keep_levels(Slider::Volume);
            }
            // Needs the context for the file dialog, so it is handled in `act`.
            Action::AddVariation => {}
            Action::RemoveVariation => {
                let Some(now) = self.now.clone() else { return };
                let Some(i) = self.variation.checked_sub(1) else {
                    self.message = Some(("the song itself can't be removed; v to a variation".into(), false));
                    return;
                };
                if let Some(gone) = self.variations.remove(&now.path, i) {
                    // Back to the song itself, from the variation that's gone, with its loop
                    // and beats.
                    self.variation = 0;
                    self.muted.clear();
                    self.solo = None;
                    let looping = self.loops.get(&now.path).copied().unwrap_or_default();
                    self.audio.switch(std::slice::from_ref(&now.path), &[1.0], Duration::ZERO, looping);
                    self.read_wave(vec![now.path.clone()]);
                    self.song_beats();
                    self.message = Some((format!("removed {}", gone.label), false));
                    self.store_variations();
                    let keep = self.selected_path();
                    self.rebuild_rows();
                    self.reindex(keep);
                }
            }
            Action::RemoveRegion => {
                if let Some(i) = self.region_in_play() {
                    self.remove_region(i);
                }
            }
            Action::LoopRegion => {
                if let Some(i) = self.region_in_play() {
                    self.loop_region(i);
                }
            }
            Action::Practice => {
                self.practice = !self.practice;
                self.searching &= !self.practice;
            }
            Action::Zoom(factor) => {
                let most = song.unwrap_or(SPAN).max(Duration::from_secs(1));
                self.span = self.span.mul_f64(factor).clamp(Duration::from_millis(500), most);
            }
        }
    }

    /// The loop ends at `at`. The playhead carries on; o plays from the loop's start.
    fn loop_end(&mut self, at: Duration) {
        self.audio.looping.set_end(at);
        self.frame_loop();
    }

    /// The playing recording's marker at `at`, or none.
    fn set_marker(&mut self, at: Option<Duration>) {
        let Some(recording) = self.recording() else { return };
        match at {
            Some(m) => {
                self.marks.insert(recording, m);
                self.message = Some((format!("marker at {}, o plays from it", ui::clock(m)), false));
            }
            None => {
                self.marks.remove(&recording);
                self.message = Some(("marker cleared".into(), false));
            }
        }
    }

    /// The section in play, by its place in the playing recording's list.
    fn region_in_play(&self) -> Option<usize> {
        let recording = self.recording()?;
        self.regions.current(&recording, self.audio.looping, self.audio.position())
    }

    fn loop_region(&mut self, i: usize) {
        let Some(recording) = self.recording() else { return };
        if let Some(region) = self.regions.of(&recording).get(i).cloned() {
            self.play_region(&region);
        }
    }

    fn remove_region(&mut self, i: usize) {
        let Some(recording) = self.recording() else { return };
        if let Some(name) = self.regions.remove(&recording, i) {
            // Said first, so a failure to store replaces it rather than hides.
            self.message = Some((format!("removed {name}"), false));
            self.store_regions();
        }
    }

    /// Right-click on a waveform strip: its menu, for the moment under the pointer.
    fn strip_menu(&mut self, strip: &Strip, event: &MouseDownEvent, cx: &mut Context<Self>) {
        if let Some((at, _)) = strip.time_at(event.position.x) {
            self.menu = Some((Menu::waveform(at), event.position));
            cx.notify();
        }
    }

    /// A menu entry chosen: its action, done on what the menu was opened on.
    fn choose(&mut self, action: Action, cx: &mut Context<Self>) {
        let Some((menu, _)) = self.menu.take() else { return };
        match (menu.target, action) {
            (Target::Moment(at), Action::LoopStart) => self.audio.looping.set_start(self.snap(at)),
            (Target::Moment(at), Action::LoopEnd) => self.loop_end(self.snap(at)),
            // Moves the marker there; it never clears from a menu.
            (Target::Moment(at), Action::Marker) => self.set_marker(Some(self.snap(at))),
            (Target::Section(i), Action::LoopRegion) => self.loop_region(i),
            (Target::Section(i), Action::RemoveRegion) => self.remove_region(i),
            (Target::Section(i), Action::SaveRegion) => {
                let name = self.recording().and_then(|r| self.regions.of(&r).get(i).map(|s| s.name.clone()));
                if let Some(name) = name {
                    (self.naming, self.renaming) = (Some(name), Some(i));
                }
            }
            (Target::Queued(i), Action::QueuePlay) => self.play_queued(i),
            (Target::Queued(i), Action::QueueTop) => self.queue.to_top(i),
            (Target::Queued(i), Action::QueueRemove) => {
                self.queue.remove(i);
            }
            (Target::Song(row), Action::AddVariation) => {
                self.selected = row;
                if let Some(song) = self.selected_path() {
                    self.pick_variation(song, cx);
                }
            }
            (Target::Song(row), action) => {
                self.selected = row;
                self.act(action, cx);
            }
            (_, action) => self.act(action, cx),
        }
        cx.notify();
    }

    /// The loop becomes a region, under the typed name or, without one, a name saying
    /// where it is. From a section's menu, that section is renamed instead.
    fn name_region(&mut self) {
        let Some(name) = self.naming.take() else { return };
        if let Some(i) = self.renaming.take() {
            let Some(recording) = self.recording() else { return };
            if let Some(name) = self.regions.rename(&recording, i, name.trim()) {
                self.message = Some((format!("renamed to {name}"), false));
                self.store_regions();
            }
            return;
        }
        let looping = self.audio.looping;
        let (Some(recording), Some(end)) = (self.recording(), looping.end) else { return };
        let start = looping.start.unwrap_or_default();
        let name = Some(name.trim().to_string())
            .filter(|n| !n.is_empty())
            .unwrap_or_else(|| Region::default_name(start, end));
        self.message = Some((format!("saved {name}"), false));
        let region = Region { name, start, end };
        self.regions.save(&recording, region);
        self.store_regions();
    }

    fn store_regions(&mut self) {
        if let Err(e) = self.regions.store(&regions_file()) {
            self.message = Some((format!("couldn't save sections: {e}"), true));
        }
    }

    /// A section as the loop, framed and playing from its start. From then on the loop
    /// is its own: changing it leaves the section as it is.
    fn play_region(&mut self, region: &Region) {
        self.audio.looping =
            crate::audio::Loop { start: Some(region.start), end: Some(region.end), off: false };
        self.frame_loop();
        self.audio.play_from(region.start);
    }

    /// The playing recording's marker.
    fn marker(&self) -> Option<Duration> {
        self.recording().and_then(|r| self.marks.get(&r).copied())
    }

    /// The recording that plays, as regions and loops know it: the song's file, or a
    /// variation's first. Each recording has regions and a loop of its own.
    fn recording(&self) -> Option<PathBuf> {
        match self.current_variation() {
            Some(v) => v.files.first().cloned(),
            None => self.now.as_ref().map(|n| n.path.clone()),
        }
    }

    /// The beats of what plays: the song's, moved by a variation's offset, or a
    /// variation's own when it has its own timeline.
    fn beats_now(&self) -> Option<Rc<Beats>> {
        let now = self.now.as_ref()?;
        let key = match self.offset() {
            Some(_) => &now.path,
            None => self.audio.files.first()?,
        };
        let beats = self.beats.as_ref().filter(|(p, _)| p == key).map(|(_, b)| b.clone())?;
        match self.offset() {
            Some(o) if o != 0.0 => Some(Rc::new(Beats {
                times: beats
                    .times
                    .iter()
                    .map(|t| Duration::from_secs_f64(t.as_secs_f64() + o))
                    .filter(|t| !t.is_zero())
                    .collect(),
            })),
            _ => Some(beats),
        }
    }

    /// The beats of what plays while the grid is on.
    fn grid(&self) -> Option<Rc<Beats>> {
        self.beats_now().filter(|_| self.grid)
    }

    /// A moment on the grid: the nearest beat while it is on, otherwise itself.
    fn snap(&self, at: Duration) -> Duration {
        self.grid().and_then(|b| b.nearest(at)).unwrap_or(at)
    }

    /// Zooms the practice view to a loop just completed. Afterwards the zoom is the
    /// player's own (`wave::fit`).
    fn frame_loop(&mut self) {
        let looping = self.audio.looping;
        if let Some(end) = looping.end {
            self.span = wave::frame(end - looping.start.unwrap_or_default());
        }
    }

    /// The whole library is listed (no search, album or artist), so the letter rail shows.
    fn browsing(&self) -> bool {
        !self.practice && self.album.is_none() && self.artist.is_none() && self.query.trim().is_empty()
    }

    /// Where each letter's artists begin in the list, below the favourites.
    fn rail_letters(&self) -> Vec<(char, usize)> {
        let from = self.rows.iter().position(|r| matches!(r, Row::Header(h) if h == "Library")).map_or(0, |h| h + 1);
        library::letters((from..self.rows.len()).filter_map(|r| match self.rows[r] {
            Row::Track(i) => Some((r, self.tracks[i].artist.as_str())),
            Row::Header(_) | Row::Artist(..) => None,
        }))
    }

    /// The pointer at `y` on the rail: glides to that letter's first song and highlights it.
    fn scrub(&mut self, y: Pixels) {
        let bounds = self.rail.get();
        let height = f32::from(bounds.size.height);
        if height <= 0. {
            return;
        }
        let count = library::ALPHABET.len();
        let slot = ((f32::from(y - bounds.origin.y) / height * count as f32) as usize).min(count - 1);
        let letter = library::ALPHABET.as_bytes()[slot] as char;
        if self.scrubbing == Some(letter) {
            return;
        }
        self.scrubbing = Some(letter);
        if let Some(row) = library::jump(&self.rail_letters(), letter) {
            self.selected = row;
            self.glide_to(row);
        }
    }

    /// Scrolls smoothly until `row` is at the top of the list (or the list's end shows).
    fn glide_to(&mut self, row: usize) {
        let Some((content, _, _)) = self.list_extent() else { return };
        self.glide_by(content * (row as f32 / self.rows.len() as f32));
    }

    /// Scrolls smoothly until the list is `down` from its top (as far as it goes).
    fn glide_by(&mut self, down: Pixels) {
        let Some((content, view, from)) = self.list_extent() else { return };
        let to = -down.min(content - view).max(Pixels::ZERO);
        self.glide = Some((from, to, std::time::Instant::now()));
    }

    /// The list's full height, how much of it shows, and its scroll offset (zero or
    /// less), once it has been laid out.
    fn list_extent(&self) -> Option<(Pixels, Pixels, Pixels)> {
        let state = self.scroll.0.borrow();
        let size = state.last_item_size.filter(|_| !self.rows.is_empty())?;
        Some((size.contents.height, size.item.height, state.base_handle.offset().y))
    }

    /// The scrollbar's thumb on a track `track` high: its top and height. None when
    /// the whole list fits.
    fn thumb(&self, track: Pixels) -> Option<(Pixels, Pixels)> {
        let (content, view, offset) = self.list_extent()?;
        if content <= view || track <= Pixels::ZERO {
            return None;
        }
        let height = (track * (view / content)).max(px(28.)).min(track);
        let travel = (-offset / (content - view)).clamp(0., 1.);
        Some(((track - height) * travel, height))
    }

    /// The thumb dragged so its top is at `top` (window coordinates): the list follows
    /// directly, no glide.
    fn drag_thumb(&mut self, top: Pixels) {
        let bounds = self.track.get();
        let (Some((content, view, _)), Some((_, height))) =
            (self.list_extent(), self.thumb(bounds.size.height))
        else {
            return;
        };
        let room = bounds.size.height - height;
        let travel = if room > Pixels::ZERO { ((top - bounds.origin.y) / room).clamp(0., 1.) } else { 0. };
        self.glide = None;
        let base = &self.scroll.0.borrow().base_handle;
        base.set_offset(gpui::point(base.offset().x, -(content - view) * travel));
    }

    /// A press on the scrollbar: on the thumb it grabs it, elsewhere the list glides so
    /// the thumb centres there.
    fn track_down(&mut self, y: Pixels) {
        let bounds = self.track.get();
        let Some((top, height)) = self.thumb(bounds.size.height) else { return };
        let at = y - bounds.origin.y;
        if at >= top && at <= top + height {
            self.grab = Some(at - top);
        } else if let Some((content, view, _)) = self.list_extent() {
            let room = bounds.size.height - height;
            let travel = if room > Pixels::ZERO { ((at - height / 2.) / room).clamp(0., 1.) } else { 0. };
            self.glide_by((content - view) * travel);
        }
    }

    /// The scrollbar over the list's right edge: a thin thumb, shown while the list
    /// moves (and a second after), while dragged, or with the pointer on it.
    fn render_scrollbar(&self, cx: &mut Context<Self>) -> gpui::AnyElement {
        let t = t();
        let bounds = self.track.get();
        let thumb = self.thumb(bounds.size.height);
        let shown = self.grab.is_some() || self.over_track || self.scrolled_at.is_some();
        let strong = self.grab.is_some() || self.over_track;
        let track = self.track.clone();
        div()
            .id("scrollbar")
            .absolute()
            .top_0()
            .bottom_0()
            .right_0()
            .w(px(14.))
            .when(thumb.is_some(), |d| {
                d.on_hover(cx.listener(|riff, hovered: &bool, _, cx| {
                    riff.over_track = *hovered;
                    cx.notify();
                }))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|riff, e: &MouseDownEvent, _, cx| {
                        cx.stop_propagation();
                        riff.track_down(e.position.y);
                        cx.notify();
                    }),
                )
            })
            .child(canvas(move |bounds, _, _| track.set(bounds), |_, _, _, _| {}).absolute().size_full())
            .children(thumb.filter(|_| shown).map(|(top, height)| {
                div()
                    .absolute()
                    .top(top)
                    .h(height)
                    .right(px(3.))
                    .w(px(if strong { 8. } else { 6. }))
                    .rounded_full()
                    .bg(rgb(if strong { t.dim } else { t.faint }))
            }))
            .into_any_element()
    }

    /// The letter rail: `#` and A to Z, the one at the top of the list lit, letters
    /// without artists faint. Click or drag along it to glide there.
    fn render_rail(&self, cx: &mut Context<Self>) -> gpui::AnyElement {
        let t = t();
        let letters = self.rail_letters();
        let state = self.scroll.0.borrow();
        let top = state.last_item_size.filter(|_| !self.rows.is_empty()).map(|size| {
            let row_height = f32::from(size.contents.height) / self.rows.len() as f32;
            (-f32::from(state.base_handle.offset().y) / row_height.max(1.)).round() as usize
        });
        drop(state);
        let here = self
            .scrubbing
            .or_else(|| top.and_then(|top| letters.iter().filter(|&&(_, at)| at <= top).last().map(|&(l, _)| l)));
        let rail = self.rail.clone();
        div()
            .id("rail")
            .relative()
            .flex_none()
            .w(px(22.))
            .py_1()
            .flex()
            .flex_col()
            .rounded_md()
            .cursor_pointer()
            .hover(|d| d.bg(rgb(t.surface)))
            .font_family(MONO)
            .text_size(px(10.))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|riff, e: &MouseDownEvent, _, cx| {
                    riff.scrub(e.position.y);
                    cx.notify();
                }),
            )
            .child(canvas(move |bounds, _, _| rail.set(bounds), |_, _, _, _| {}).absolute().size_full())
            .children(library::ALPHABET.chars().map(|letter| {
                let has = letters.iter().any(|&(l, _)| l == letter);
                let lit = here == Some(letter);
                div()
                    .flex_1()
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_color(rgb(if lit {
                        t.accent
                    } else if has {
                        t.dim
                    } else {
                        t.faint
                    }))
                    .child(letter.to_string())
            }))
            .into_any_element()
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
        if query != self.query || self.album.is_some() || self.artist.is_some() {
            self.query = query;
            self.album = None;
            self.artist = None;
            self.rebuild_rows();
            self.scroll.scroll_to_item(0, ScrollStrategy::Top);
        }
    }

    /// A click on a strip: grabs a loop edge near it (the loop's only, never a
    /// section's), sets the loop's end with shift, and otherwise seeks.
    fn strip_down(&mut self, strip: &Strip, event: &MouseDownEvent, cx: &mut Context<Self>) {
        let Some((at, per_px)) = strip.time_at(event.position.x) else { return };
        let looping = self.audio.looping;
        let near = |edge: Option<Duration>| {
            edge.is_some_and(|e| (e.as_secs_f64() - at.as_secs_f64()).abs() <= GRAB as f64 * per_px)
        };
        // The marker by its flag (the strip's top), so it can be grabbed even on a loop
        // edge; anywhere else along it only where no loop edge is.
        let by_flag = f32::from(event.position.y - strip.0.get().0.origin.y) <= FLAG;
        let marker = self.marker();
        if near(marker) && (by_flag || !(near(looping.start) || near(looping.end))) {
            self.dragging = Some((Edge::Marker, strip.clone()));
        } else if near(looping.end) {
            self.dragging = Some((Edge::End, strip.clone()));
        } else if near(looping.start) {
            self.dragging = Some((Edge::Start, strip.clone()));
        } else if event.modifiers.shift {
            self.audio.looping.set_end(self.snap(at));
            self.frame_loop();
        } else {
            // On the grid the playhead lands on the nearest beat; ⌥ places it freely.
            self.audio.seek(if event.modifiers.alt { at } else { self.snap(at) });
        }
        cx.notify();
    }

    fn drag(&mut self, event: &MouseMoveEvent, _w: &mut Window, cx: &mut Context<Self>) {
        if let Some((slider, bounds)) = self.sliding.clone() {
            if event.pressed_button == Some(MouseButton::Left) {
                self.slide(slider, bounds.get(), event.position.x);
            } else {
                self.drop_slider();
            }
            return cx.notify();
        }
        if let Some(grab) = self.grab {
            if event.pressed_button == Some(MouseButton::Left) {
                self.drag_thumb(event.position.y - grab);
            } else {
                self.grab = None;
            }
            return cx.notify();
        }
        if self.scrubbing.is_some() {
            if event.pressed_button == Some(MouseButton::Left) {
                self.scrub(event.position.y);
            } else {
                self.scrubbing = None;
            }
            return cx.notify();
        }
        if let Some((i, start)) = self.shaping {
            if event.pressed_button != Some(MouseButton::Left) {
                return self.drop_edge(cx);
            }
            let (Some(recording), Some((at, _))) = (self.recording(), self.lane.time_at(event.position.x)) else {
                return;
            };
            let at = if event.modifiers.alt { at } else { self.snap(at) };
            self.shaping = self.regions.move_edge(&recording, i, start, at).map(|i| (i, start));
            return cx.notify();
        }
        let Some((edge, strip)) = &self.dragging else { return };
        if event.pressed_button != Some(MouseButton::Left) {
            return self.drop_edge(cx);
        }
        let Some((at, _)) = strip.time_at(event.position.x) else { return };
        // On the grid an edge snaps to beats; ⌥ drags it freely.
        let at = if event.modifiers.alt { at } else { self.snap(at) };
        let song = self.audio.duration;
        match edge {
            Edge::Start => self.audio.looping.move_start(at),
            Edge::End => self.audio.looping.move_end(at, song),
            Edge::Marker => {
                if let Some(recording) = self.recording() {
                    self.marks.insert(recording, at);
                }
            }
        }
        cx.notify();
    }

    /// Lets go of a dragged edge: a section's is saved; the loop's and the marker just
    /// stay where they landed, the playhead carrying on (o plays from them).
    fn drop_edge(&mut self, cx: &mut Context<Self>) {
        if self.shaping.take().is_some() {
            self.store_regions();
            cx.notify();
        }
        if self.dragging.take().is_some() {
            cx.notify();
        }
    }

    /// An artist over its songs in the grouped list; also pinned on top of the list.
    /// A click opens the artist view.
    fn render_artist_row(&self, ix: usize, name: &str, songs: usize, cx: &mut Context<Self>) -> gpui::AnyElement {
        div()
            .id(("artist-row", ix))
            .w_full()
            .h(px(ROW))
            .px_3()
            .flex()
            .items_end()
            .pb_1p5()
            .gap_3()
            .bg(rgb(t().bg))
            .cursor_pointer()
            .child(
                div()
                    .truncate()
                    .text_lg()
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .text_color(rgb(t().text))
                    .child(name.to_string()),
            )
            .child(
                div()
                    .flex_none()
                    .pb(px(3.))
                    .font_family(MONO)
                    .text_xs()
                    .text_color(rgb(t().dim))
                    .child(if songs == 1 { "1 song".to_string() } else { format!("{songs} songs") }),
            )
            .tooltip(self.tip("Only this artist", &[Action::Artist]))
            .on_click(cx.listener(move |riff, _, _, cx| {
                riff.selected = ix + 1;
                riff.act(Action::Artist, cx);
            }))
            .into_any_element()
    }

    /// The grouped list's artist pinned on top, until the next one pushes it off.
    fn render_pinned(&self, cx: &mut Context<Self>) -> Option<gpui::AnyElement> {
        if !self.grouped() || self.rows.is_empty() {
            return None;
        }
        let (content, _, offset) = self.list_extent()?;
        let row = content / self.rows.len() as f32;
        let kinds: Vec<Kind> = self
            .rows
            .iter()
            .map(|r| match r {
                Row::Header(_) => Kind::Section,
                Row::Artist(..) => Kind::Artist,
                Row::Track(_) => Kind::Track,
            })
            .collect();
        let (head, push) = library::pinned(&kinds, -offset / row)?;
        let Row::Artist(name, songs) = &self.rows[head] else { return None };
        Some(
            div()
                .absolute()
                .top(-row * push)
                .left_0()
                .right(px(14.))
                .child(self.render_artist_row(head, name, *songs, cx))
                .into_any_element(),
        )
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
            Row::Artist(name, songs) => self.render_artist_row(ix, name, *songs, cx),
            Row::Track(i) => {
                let track = &self.tracks[*i];
                let selected = ix == self.selected;
                let playing = self.now.as_ref().is_some_and(|n| n.path == track.path);
                let favorite = self.favorites.contains(&track.path);
                // The album view's header already names the album.
                let album = (self.album.is_none()).then(|| track.album.clone());
                let variations = self.variations.of(&track.path).len();
                let queued = self.queue.songs().contains(&track.path);
                let in_group = self.grouped();
                let show_artist = !in_group && self.album.is_none() && self.artist.is_none();
                div()
                    .id(("row", ix))
                    .group("song-row")
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
                    // Right-click highlights the song and opens its menu.
                    .on_mouse_down(MouseButton::Right, {
                        let (album, artist) = (self.album.is_some(), self.artist.is_some());
                        cx.listener(move |riff, e: &MouseDownEvent, _, cx| {
                            riff.selected = ix;
                            riff.menu = Some((Menu::song(ix, favorite, album, artist), e.position));
                            cx.notify();
                        })
                    })
                    // Files dropped on a song become a variation of it.
                    .drag_over::<ExternalPaths>(|style, _, _, _| style.bg(tint(t().loops, 0x30)))
                    .on_drop({
                        let song = track.path.clone();
                        cx.listener(move |riff, dropped: &ExternalPaths, _, cx| {
                            riff.add_variation(song.clone(), dropped.paths().to_vec());
                            cx.notify();
                        })
                    })
                    // In the album view the header shows the cover; rows show the number.
                    // Grouped rows sit indented under their artist.
                    .child(match (self.album.is_some(), in_group) {
                        (true, _) => div()
                            .w(px(28.))
                            .flex_none()
                            .flex()
                            .justify_end()
                            .font_family(MONO)
                            .text_sm()
                            .text_color(rgb(if playing { t().accent } else { t().dim }))
                            .children(track.number.map(|n| n.to_string()))
                            .into_any_element(),
                        (false, true) => div().pl_4().child(cover(track.cover.clone(), 30.)).into_any_element(),
                        (false, false) => cover(track.cover.clone(), 30.),
                    })
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(
                                div()
                                    .truncate()
                                    .text_color(rgb(if selected || playing { t().accent } else { t().text }))
                                    .child(track.title.clone()),
                            )
                            // How many other recordings the song has.
                            .children((variations > 0).then(|| {
                                div()
                                    .flex_none()
                                    .font_family(MONO)
                                    .text_xs()
                                    .text_color(rgb(t().dim))
                                    .child(format!("+{variations}"))
                            })),
                    )
                    // The artist, styled like the album beside it. It opens the artist
                    // view, as ⇧⌘A does.
                    .when(show_artist, |d| {
                        d.child(
                            div().w(relative(0.24)).flex_none().min_w_0().flex().child(
                                div()
                                    .id(("artist", ix))
                                    .truncate()
                                    .text_sm()
                                    .text_color(rgb(t().dim))
                                    .cursor_pointer()
                                    .hover(|d| d.text_color(rgb(t().accent)).underline())
                                    .tooltip(self.tip("Only this artist", &[Action::Artist]))
                                    .on_click(cx.listener(move |riff, _, _, cx| {
                                        cx.stop_propagation();
                                        riff.selected = ix;
                                        riff.act(Action::Artist, cx);
                                    }))
                                    .child(track.artist.clone()),
                            ),
                        )
                    })
                    // The album name opens the album view, as ⌘A does.
                    .children(album.map(|album| {
                        div()
                            .w(relative(0.24))
                            .flex_none()
                            .min_w_0()
                            .flex()
                            .child(
                                div()
                                    .id(("album", ix))
                                    .truncate()
                                    .text_sm()
                                    .text_color(rgb(t().dim))
                                    .cursor_pointer()
                                    .hover(|d| d.text_color(rgb(t().accent)).underline())
                                    .tooltip(self.tip("Only this album", &[Action::Album]))
                                    .on_click(cx.listener(move |riff, _, _, cx| {
                                        cx.stop_propagation();
                                        riff.selected = ix;
                                        riff.act(Action::Album, cx);
                                    }))
                                    .child(album),
                            )
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
                            .w(px(76.))
                            .flex_none()
                            .flex()
                            .items_center()
                            .justify_end()
                            .gap_1()
                            .when(playing, |d| d.child(ui::icon("music", 14., t().accent)))
                            // Queued songs show it; others when highlighted or under the
                            // pointer, to click.
                            .child(
                                div()
                                    .id(("queue", ix))
                                    .size(px(24.))
                                    .flex_none()
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .rounded_md()
                                    .cursor_pointer()
                                    .hover(|d| d.bg(rgb(t().faint)))
                                    .when(!queued && !selected, |d| {
                                        d.opacity(0.).group_hover("song-row", |s| s.opacity(1.))
                                    })
                                    .child(ui::icon("list-plus", 14., if queued { t().accent } else { t().dim }))
                                    .tooltip(self.tip(if queued { "Queued" } else { "Queue it" }, &[Action::Queue(false)]))
                                    .on_click(cx.listener(move |riff, _, _, cx| {
                                        cx.stop_propagation();
                                        riff.selected = ix;
                                        riff.act(Action::Queue(false), cx);
                                    })),
                            )
                            // A favourite's star always shows; others' when the row is
                            // highlighted or under the pointer, to click.
                            .child(
                                div()
                                    .id(("star", ix))
                                    .size(px(24.))
                                    .flex_none()
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .rounded_md()
                                    .cursor_pointer()
                                    .hover(|d| d.bg(rgb(t().faint)))
                                    .when(!favorite && !selected, |d| {
                                        d.opacity(0.).group_hover("song-row", |s| s.opacity(1.))
                                    })
                                    .child(ui::icon(
                                        "star",
                                        14.,
                                        if favorite { t().accent } else { t().dim },
                                    ))
                                    .tooltip(self.tip(
                                        if favorite { "Not a favourite" } else { "Favourite" },
                                        &[Action::Favorite],
                                    ))
                                    .on_click(cx.listener(move |riff, _, _, cx| {
                                        cx.stop_propagation();
                                        riff.selected = ix;
                                        riff.act(Action::Favorite, cx);
                                    })),
                            ),
                    )
                    .into_any_element()
            }
        }
    }

    fn render_search(&self, cx: &mut Context<Self>) -> impl IntoElement {
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
            .id("search")
            .relative()
            .flex()
            .items_center()
            .gap_3()
            .h(px(48.))
            .px_4()
            .rounded_lg()
            .bg(rgb(t().surface))
            .cursor_text()
            .when(self.searching, |d| d.border_1().border_color(rgb(t().accent_soft)))
            .when(!self.searching, |d| d.tooltip(self.tip("Search", &[Action::Search])))
            // Typing goes to the search in the library, so a click there leaves practice.
            .on_click(cx.listener(|riff, _, _, cx| {
                (riff.practice, riff.searching) = (false, true);
                cx.notify();
            }))
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
                "{} / {}{}",
                ui::clock(position),
                duration.map_or("–".to_string(), ui::clock),
                match (self.speed.now(), self.speed.pitch()) {
                    (100, 0) => String::new(),
                    (p, 0) => format!(" at {p}%"),
                    (p, c) => format!(" at {p}%, {}", crate::speed::pitch_label(c)),
                }
            )
        };
        let loop_label = match (looping.start, looping.end) {
            (_, Some(end)) => format!(
                "{} to {}{}",
                ui::clock(looping.start.unwrap_or_default()),
                ui::clock(end),
                if looping.off { ", off (| loops it)" } else { "" }
            ),
            (Some(start), None) => format!("from {}, ] sets the end", ui::clock(start)),
            (None, None) => "[ ] sets a loop".to_string(),
        };

        let progress = div()
            .id("progress")
            .tooltip(self.tip("Click to jump; drag a loop edge or the marker; shift-click ends the loop", &[]))
            .children(duration.map(|d| edge_grips(looping, self.marker(), Duration::ZERO, d)).into_iter().flatten())
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
                    // Played first, the loop over it: drawn the other way round, the
                    // played gold hid the loop region and only its start showed.
                    .child(
                        div()
                            .absolute()
                            .h_full()
                            .rounded_full()
                            .bg(rgb(t().accent))
                            .w(relative(fraction(position))),
                    )
                    .children(looping.end.map(|end| {
                        let start = fraction(looping.start.unwrap_or_default());
                        div()
                            .absolute()
                            .top(px(-2.))
                            .h(px(8.))
                            .left(relative(start))
                            .w(relative(fraction(end) - start))
                            .rounded_sm()
                            .bg(rgb(t().loops))
                            .opacity(if looping.off { 0.15 } else { 0.45 })
                    }))
                    // The marker, where o plays from.
                    .children(self.marker().map(|m| {
                        div()
                            .absolute()
                            .top(px(-5.))
                            .left(relative(fraction(m)))
                            .w(px(2.))
                            .h(px(14.))
                            .bg(rgb(t().accent))
                    }))
                    .children([looping.start, looping.end].into_iter().flatten().map(|edge| {
                        div()
                            .absolute()
                            .top(px(-4.))
                            .left(relative(fraction(edge)))
                            .w(px(2.))
                            .h(px(12.))
                            .bg(rgb(t().loops))
                            .opacity(if looping.off { 0.5 } else { 1.0 })
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
                            .child(self.now_subtitle(artist, cx)),
                    )
                    .child(
                        pill()
                            .child(self.tool("previous", "skip-back", "Previous song", Action::Previous, false, cx))
                            .child(self.tool(
                                "toggle",
                                if self.audio.playing() { "pause" } else { "play" },
                                "Play or pause",
                                Action::Toggle,
                                false,
                                cx,
                            ))
                            .child(self.tool("next", "skip-forward", "Next song", Action::Next, false, cx))
                            .child(self.tool("shuffle", "shuffle", "Shuffle", Action::Shuffle, self.shuffle, cx)),
                    )
                    .child(
                        pill()
                            .child(self.tool("loop-start", "loop-start", "Loop starts here", Action::LoopStart, false, cx))
                            .child(self.tool("loop-end", "loop-end", "Loop ends here", Action::LoopEnd, false, cx))
                            .child(self.tool(
                                "loop",
                                "repeat",
                                "Loop on or off, keeping its edges",
                                Action::LoopSwitch,
                                looping.looping(),
                                cx,
                            )),
                    )
                    .child(
                        button("transcribe", "external-link", Some("Transcribe!"), false)
                            .tooltip(self.tip("Open the playing song in Transcribe!", &[]))
                            .on_click(
                            cx.listener(|riff, _, _, cx| {
                                // The button sits on the playing card, so it means that song.
                                let track = riff.now.clone().or_else(|| riff.selected_track().cloned());
                                riff.transcribe(track);
                                cx.notify();
                            }),
                        ),
                    ),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_3()
                    .child(progress.flex_1())
                    .child(self.speed_control(cx))
                    .child(self.volume_slider(cx)),
            )
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
                            .text_color(rgb(if looping.looping() { t().loops } else { t().dim }))
                            .child(loop_label),
                    ),
            )
    }
}

/// A rounded group of tools that belong together.
fn pill() -> Div {
    div().flex().items_center().gap(px(2.)).p(px(3.)).rounded_lg().bg(rgb(t().raised))
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
        let query = self.help_query.as_str();
        let group = |keys::Group { title, lines, .. }: &keys::Group| {
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
                // Without vim keys, lines that only have vim keys go; so do lines the
                // typed words don't find.
                .children(lines.iter().filter(|l| keys::found(title, l, query)).filter_map(|(keys, vim_keys, what)| {
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
        // Each group goes to the shorter column, so the two stay even as groups grow.
        let (mut left, mut right) = (Vec::new(), Vec::new());
        let shown = |g: &keys::Group| g.lines.iter().filter(|l| keys::found(g.title, l, query)).count();
        let lines = |col: &Vec<&keys::Group>| col.iter().map(|g| shown(g) + 2).sum::<usize>();
        for g in keys::sheet(self.mode()).filter(|g| shown(g) > 0) {
            if lines(&left) <= lines(&right) { left.push(g) } else { right.push(g) }
        }
        overlay("help")
            .on_click(cx.listener(|riff, _, _, cx| {
                riff.help = false;
                cx.notify();
            }))
            .child(
                div()
                    .id("keys-sheet")
                    // Scrolls rather than cuts keys off in a short window.
                    .max_h(relative(0.94))
                    .overflow_y_scroll()
                    .flex()
                    .flex_col()
                    .gap_4()
                    .p_5()
                    .rounded_lg()
                    .bg(rgb(t().surface))
                    .border_1()
                    .border_color(rgb(t().faint))
                    // The sheet lists this mode's keys, so it says which mode that is.
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_4()
                            .child(div().text_color(rgb(t().accent)).child(match self.mode() {
                                keys::Mode::Library => "Library keys",
                                keys::Mode::Search => "Search keys",
                                keys::Mode::Practice => "Practice keys",
                                keys::Mode::Queue => "Queue keys",
                            }))
                            .child(
                                div()
                                    .flex_1()
                                    .px_2()
                                    .py_0p5()
                                    .flex()
                                    .items_center()
                                    .gap_2()
                                    .rounded_md()
                                    .bg(rgb(t().raised))
                                    .text_sm()
                                    .child(ui::icon("search", 13., t().dim))
                                    .child(if query.is_empty() {
                                        div().text_color(rgb(t().dim)).child("type to find a key")
                                    } else {
                                        div().text_color(rgb(t().text)).child(query.to_string())
                                    }),
                            ),
                    )
                    .when(left.is_empty(), |d| {
                        d.child(div().text_sm().text_color(rgb(t().dim)).child("no key does that here"))
                    })
                    .child(
                        div()
                            .flex()
                            .gap_6()
                            .child(div().flex().flex_col().gap_5().children(left.into_iter().map(group)))
                            .child(div().flex().flex_col().gap_5().children(right.into_iter().map(group))),
                    ),
            )
    }

    /// The right-click menu at the pointer, over a clear layer that closes it on a click
    /// anywhere else (that click does nothing more). Each entry shows its keys.
    fn render_menu(&self, cx: &mut Context<Self>) -> Option<impl IntoElement> {
        let t = t();
        let (menu, at) = self.menu.as_ref()?;
        let close = |riff: &mut Riff, _: &MouseDownEvent, _: &mut Window, cx: &mut Context<Riff>| {
            riff.menu = None;
            cx.notify();
        };
        let entries = menu.entries.iter().enumerate().map(|(i, &(label, action))| {
            let lit = i == menu.row;
            div()
                .id(("menu-entry", i))
                .h(px(28.))
                .px_2()
                .flex()
                .items_center()
                .gap_4()
                .rounded_md()
                .cursor_pointer()
                .when(lit, |d| d.bg(rgb(t.raised)))
                .text_sm()
                .text_color(rgb(if lit { t.accent } else { t.text }))
                .child(div().flex_1().child(label))
                .child(
                    div()
                        .font_family(MONO)
                        .text_xs()
                        .text_color(rgb(t.dim))
                        .child(keys::keys_for(action, self.mode(), self.settings.vim).join(" ")),
                )
                .on_hover(cx.listener(move |riff, hovered: &bool, _, cx| {
                    if let Some((menu, _)) = riff.menu.as_mut().filter(|_| *hovered) {
                        menu.row = i;
                        cx.notify();
                    }
                }))
                .on_click(cx.listener(move |riff, _, _, cx| riff.choose(action, cx)))
        });
        Some(deferred(
            div()
                .id("menu-layer")
                .absolute()
                .top_0()
                .left_0()
                .size_full()
                .occlude()
                .on_mouse_down(MouseButton::Left, cx.listener(close))
                .on_mouse_down(MouseButton::Right, cx.listener(close))
                .child(
                    anchored().position(*at).snap_to_window_with_margin(px(8.)).child(
                        div()
                            .id("menu")
                            .occlude()
                            .min_w(px(220.))
                            .p_1()
                            .flex()
                            .flex_col()
                            .rounded_lg()
                            .bg(rgb(t.surface))
                            .border_1()
                            .border_color(rgb(t.faint))
                            .font_family(UI)
                            // A press inside the menu is not a press outside it.
                            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                            .on_mouse_down(MouseButton::Right, |_, _, cx| cx.stop_propagation())
                            .children(entries),
                    ),
                ),
        ))
    }

    /// The practice view: the whole song as a loudness contour with the shown stretch
    /// boxed, and below it that stretch in detail, the loop on it. Both take clicks and
    /// drags like the progress bar.
    fn render_practice(&self, at: Duration, cx: &mut Context<Self>) -> impl IntoElement {
        let t = t();
        let looping = self.audio.looping;
        let wave = self
            .wave
            .as_ref()
            .filter(|(p, _)| self.audio.files.first() == Some(p))
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
        let beats = self.beats_now();
        let mut about = format!("{:.1} s shown", (to - from).as_secs_f64());
        if self.current_variation().is_some_and(|v| v.offset.is_none()) {
            about.push_str(", its own timeline");
        }
        if let Some(bpm) = beats.as_ref().and_then(|b| b.tempo_at(at)) {
            about.push_str(&format!(", about {bpm:.0} BPM here"));
        }
        if self.grid {
            about.push_str(if beats.is_some() { ", grid on" } else { ", grid on (finding beats…)" });
        }
        if self.speed.now() != 100 {
            about.push_str(&format!(", at {}% speed", self.speed.now()));
        }
        if self.speed.pitch() != 0 {
            about.push_str(&format!(", pitch {}", crate::speed::pitch_label(self.speed.pitch())));
        }
        let toolbar = self.practice_toolbar(about, cx);
        let lane = self.region_lane(song.unwrap_or(self.span), cx);
        let loop_bar = self.loop_bar(cx);
        let lanes = self.stem_lanes(song.unwrap_or(self.span), at, cx);
        let strip = |id: &'static str, strip: &Strip, view: Painting| {
            let (down, right) = (strip.clone(), strip.clone());
            div()
                .id(id)
                .tooltip(self.tip("Click to jump; drag a loop edge or the marker; shift-click ends the loop; scroll to zoom; right-click for more", &[]))
                .children(edge_grips(view.looping, view.marker, view.from, view.to))
                // The wheel zooms, as - and = do: a mouse notch is about one step, and a
                // trackpad's many small deltas add up to the same.
                .on_scroll_wheel(cx.listener(|riff, e: &gpui::ScrollWheelEvent, _, cx| {
                    let dy = f32::from(e.delta.pixel_delta(px(20.)).y) as f64;
                    if dy != 0. {
                        riff.perform(Action::Zoom(keys::ZOOM.powf(-dy / 20.)));
                        cx.notify();
                    }
                }))
                .relative()
                .rounded_lg()
                .bg(rgb(t.surface))
                .overflow_hidden()
                .cursor_pointer()
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |riff, e, _, cx| riff.strip_down(&down, e, cx)),
                )
                .on_mouse_down(
                    MouseButton::Right,
                    cx.listener(move |riff, e, _, cx| riff.strip_menu(&right, e, cx)),
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
            beats: None,
            grid: self.grid,
            marker: self.marker(),
        };
        div()
            .id("practice")
            .flex_1()
            .min_h_0()
            // Files dropped here become a variation of the playing song.
            .drag_over::<ExternalPaths>(move |style, _, _, _| style.bg(tint(t.loops, 0x18)))
            .on_drop(cx.listener(|riff, dropped: &ExternalPaths, _, cx| {
                if let Some(song) = riff.now.as_ref().map(|n| n.path.clone()) {
                    riff.add_variation(song, dropped.paths().to_vec());
                }
                cx.notify();
            }))
            .flex()
            .flex_col()
            .gap_2()
            .child(toolbar)
            .children(lane)
            .child(strip("overview", &self.overview, Painting { boxed: Some((from, to)), ..common.clone() }).h(px(40.)))
            .child(
                strip("detail", &self.detail, Painting { from, to, contour: false, beats, ..common })
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
            .child(loop_bar)
            .children(lanes)
    }

    /// A stem set's stems, a lane each: number, name, mute and solo buttons, and the
    /// stem's waveform over the whole song, so a part can be found anywhere in it
    /// (the overview above already boxes the detail's stretch). A click in a lane moves the playhead there, like
    /// the overview. Dimmed when the stem isn't heard.
    fn stem_lanes(&self, song: Duration, at: Duration, cx: &mut Context<Self>) -> Option<gpui::AnyElement> {
        let t = t();
        let variation = self.current_variation()?;
        if self.stem_waves.len() < 2 || self.stem_waves.len() != variation.stems.len() {
            return None;
        }
        let gains = self.gains();
        let lanes = variation.stems.iter().zip(&self.stem_waves).enumerate().map(|(i, (name, wave))| {
            let heard = gains.get(i).is_some_and(|g| *g > 0.0);
            let (soloed, muted) = (self.solo == Some(i), self.muted.get(i) == Some(&true));
            div()
                .flex()
                .items_center()
                .gap_2()
                .h(px(30.))
                .child(
                    div()
                        .id(("stem", i))
                        .w(px(190.))
                        .flex_none()
                        .flex()
                        .gap_1()
                        .pr_1()
                        .h_full()
                        .items_center()
                        .rounded_md()
                        .overflow_hidden()
                        .bg(rgb(t.surface))
                        .text_xs()
                        .text_color(rgb(if heard { t.text } else { t.dim }))
                        .on_mouse_down(
                            MouseButton::Right,
                            cx.listener(move |riff, e: &MouseDownEvent, _, cx| {
                                riff.menu = Some((Menu::stem(i, muted, soloed), e.position));
                                cx.notify();
                            }),
                        )
                        // The number and name are the level's slider, filled to it.
                        .child(
                            self.level_area(("stem-level", i), Slider::Stem(i), tint(t.loops, 0x30), cx)
                                .tooltip(self.tip(
                                    format!("{name} at {:.0}%: drag or scroll to change; right-click for more", self.level(Slider::Stem(i)) * 100.),
                                    &[],
                                ))
                                .flex_1()
                                .min_w_0()
                                .h_full()
                                .pl_2()
                                .flex()
                                .items_center()
                                .gap_1()
                                .child(div().w(px(14.)).font_family(MONO).text_color(rgb(t.dim)).child((i + 1).to_string()))
                                .child(div().flex_1().min_w_0().truncate().child(name.clone())),
                        )
                        .child(self.tool(
                            ("mute", i),
                            if muted { "volume-x" } else { "volume-2" },
                            &format!("Mute {name}"),
                            Action::Mute(i),
                            muted,
                            cx,
                        ))
                        .child(self.tool(("solo", i), "headphones", &format!("Solo {name}"), Action::Solo(i), soloed, cx)),
                )
                .child({
                    let strip = Strip::default();
                    let (down, right) = (strip.clone(), strip.clone());
                    div()
                        .id(("stem-wave", i))
                        .tooltip(self.tip("Click to jump", &[]))
                        .children(edge_grips(self.audio.looping, self.marker(), Duration::ZERO, song))
                        .flex_1()
                        .h_full()
                        .relative()
                        .rounded_md()
                        .overflow_hidden()
                        .bg(rgb(t.surface))
                        .cursor_pointer()
                        .opacity(if heard { 1.0 } else { 0.35 })
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |riff, e, _, cx| riff.strip_down(&down, e, cx)),
                        )
                        .on_mouse_down(
                            MouseButton::Right,
                            cx.listener(move |riff, e, _, cx| riff.strip_menu(&right, e, cx)),
                        )
                        .child(
                            paint_wave(
                                strip,
                                Painting {
                                    wave: Some(wave.clone()),
                                    from: Duration::ZERO,
                                    to: song,
                                    at,
                                    looping: self.audio.looping,
                                    boxed: None,
                                    contour: false,
                                    beats: None,
                                    grid: false,
                                    marker: self.marker(),
                                },
                            )
                            .absolute()
                            .size_full(),
                        )
                })
        });
        Some(div().flex().flex_col().gap_1().children(lanes).into_any_element())
    }

    /// The playing song's regions as named blocks along the song, the loop's own
    /// highlighted. A click plays one. Nothing when the song has none.
    fn region_lane(&self, song: Duration, cx: &mut Context<Self>) -> Option<gpui::AnyElement> {
        let t = t();
        let recording = self.recording()?;
        let list = self.regions.of(&recording).to_vec();
        let current = self.regions.current(&recording, self.audio.looping, self.audio.position());
        let length = song.as_secs_f64().max(1e-3);
        let empty = list.is_empty();
        let strip = self.lane.clone();
        let lane = div()
            .flex_1()
            .relative()
            .h(px(26.))
            .child(canvas(move |bounds, _, _| strip.set(bounds, Duration::ZERO, song), |_, _, _, _| {}).absolute().size_full())
            .when(empty, |d| {
                d.flex().items_center().px_2().text_xs().text_color(rgb(t.dim)).child("No sections yet: set a loop, then save it with r")
            });
        // A section's edge, grabbed to drag it (snapping to beats on the grid, ⌥ freely).
        let handle = |i: usize, start: bool| {
            div()
                .absolute()
                .top_0()
                .h_full()
                .w(px(6.))
                .when(start, |d| d.left_0())
                .when(!start, |d| d.right_0())
                .cursor(gpui::CursorStyle::ResizeLeftRight)
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |riff, _, _, cx| {
                        cx.stop_propagation();
                        riff.shaping = Some((i, start));
                        cx.notify();
                    }),
                )
        };
        let lane = lane.children(list.into_iter().enumerate().map(|(i, region)| {
            let lit = current == Some(i);
            let left = (region.start.as_secs_f64() / length) as f32;
            let wide = ((region.end - region.start).as_secs_f64() / length) as f32;
            let name = region.name.clone();
            div()
                .id(("region", i))
                .tooltip(self.tip(format!("Loop {}; drag its edges to change it; right-click for more", region.name), &[Action::Region(false), Action::Region(true)]))
                .absolute()
                .top_0()
                .h_full()
                .left(relative(left))
                .w(relative(wide))
                .px_1()
                .flex()
                .items_center()
                .overflow_hidden()
                .rounded_sm()
                .bg(tint(t.sections, if lit { 0x60 } else { 0x26 }))
                .border_l_2()
                .border_color(tint(t.sections, 0xff))
                .cursor_pointer()
                .text_xs()
                .text_color(rgb(if lit { t.text } else { t.dim }))
                .child(div().truncate().child(name))
                .child(handle(i, true))
                .child(handle(i, false))
                .on_click(cx.listener(move |riff, _, _, cx| {
                    riff.play_region(&region);
                    cx.notify();
                }))
                .on_mouse_down(
                    MouseButton::Right,
                    cx.listener(move |riff, e: &MouseDownEvent, _, cx| {
                        riff.menu = Some((Menu::section(i), e.position));
                        cx.notify();
                    }),
                )
        }));
        Some(
            div()
                .flex()
                .items_center()
                .gap_2()
                .child(pill().child(self.tool("region-prev", "region-prev", "Loop the previous section", Action::Region(false), false, cx)))
                .child(lane)
                .child(
                    pill()
                        .child(self.tool("region-next", "region-next", "Loop the next section", Action::Region(true), false, cx))
                        .child(self.tool("region-save", "bookmark-plus", "Save the loop as a section, or rename its section", Action::SaveRegion, false, cx))
                        .child(self.tool("region-delete", "bookmark-minus", "Delete the section playing", Action::RemoveRegion, false, cx)),
                )
                .into_any_element(),
        )
    }

    /// Above the waveform: which recording plays (a chip each, plus adding one), what is
    /// shown, and the beat grid and zoom.
    fn practice_toolbar(&self, about: String, cx: &mut Context<Self>) -> gpui::AnyElement {
        let t = t();
        let names: Vec<String> = std::iter::once("Song".to_string())
            .chain(self.now.iter().flat_map(|n| self.variations.of(&n.path).iter().map(|v| v.label.clone())))
            .collect();
        let chips = names.into_iter().enumerate().map(|(i, name)| {
            let lit = self.variation == i;
            div()
                .id(("recording", i))
                .h(px(28.))
                .px_2()
                .flex()
                .items_center()
                .gap_2()
                .rounded_md()
                .cursor_pointer()
                .when(lit, |d| d.bg(rgb(t.accent_soft)))
                .hover(|d| d.bg(rgb(t.faint)))
                .text_sm()
                .text_color(rgb(if lit { t.accent } else { t.dim }))
                .child(ui::icon(if i == 0 { "music" } else { "layers" }, 14., if lit { t.accent } else { t.dim }))
                .child(div().max_w(px(160.)).truncate().child(name.clone()))
                .tooltip(self.tip(format!("Play {}", if i == 0 { "the song itself".to_string() } else { name }), &[Action::Variation]))
                .on_click(cx.listener(move |riff, _, _, cx| {
                    riff.select_recording(i);
                    cx.notify();
                }))
        });
        div()
            .flex()
            .items_center()
            .gap_3()
            .child(
                pill()
                    .children(chips)
                    .child(self.tool("add-variation", "layers-plus", "Add a variation (or drop files here)", Action::AddVariation, false, cx))
                    .when(self.variation > 0, |d| {
                        d.child(self.tool("remove-variation", "x", "Remove the playing variation", Action::RemoveVariation, false, cx))
                    }),
            )
            .child(div().flex_1().min_w_0().truncate().font_family(MONO).text_xs().text_color(rgb(t.dim)).child(about))
            .child(self.pitch_control(cx))
            // Any speed from 25 to 110%, the trainer beside it. Right-click for presets.
            .child(
                pill()
                    .id("speed-slider")
                    .on_mouse_down(
                        MouseButton::Right,
                        cx.listener(|riff, e: &MouseDownEvent, _, cx| {
                            riff.menu = Some((Menu::speed(), e.position));
                            cx.notify();
                        }),
                    )
                    .child(div().pl_1().font_family(MONO).text_xs().text_color(rgb(t.dim)).child("25"))
                    .child(
                        self.level_area("speed-level", Slider::Speed, gpui::rgb(if self.speed.on { t.accent } else { t.dim }), cx)
                            .tooltip(self.tip(
                                format!("{}% speed: drag or scroll for any speed; right-click for presets", self.speed.now()),
                                &[Action::Speed(false), Action::Speed(true)],
                            ))
                            .w(px(110.))
                            .h(px(10.))
                            .mx_1()
                            .rounded_full()
                            .bg(rgb(t.faint)),
                    )
                    .child(div().font_family(MONO).text_xs().text_color(rgb(t.dim)).child("110"))
                    .child(self.tool(
                        "trainer",
                        "trending-up",
                        "Speed trainer: the loop speeds up as it repeats; right-click for its settings",
                        Action::Trainer,
                        self.training.is_some(),
                        cx,
                    )),
            )
            .child(
                pill()
                    .child(self.tool("grid", "beats", "Beat grid: loops and steps follow the beats", Action::Grid, self.grid, cx))
                    .child(self.tool("zoom-out", "zoom-out", "Zoom out", Action::Zoom(keys::ZOOM), false, cx))
                    .child(self.tool("zoom-in", "zoom-in", "Zoom in", Action::Zoom(1.0 / keys::ZOOM), false, cx)),
            )
            .into_any_element()
    }

    /// Under the waveform: every loop tool, grouped (its edges, widening and narrowing,
    /// sliding, playing and switching), and where the loop is.
    fn loop_bar(&self, cx: &mut Context<Self>) -> gpui::AnyElement {
        let t = t();
        let looping = self.audio.looping;
        let label = match (looping.start, looping.end) {
            (_, Some(end)) => {
                let start = looping.start.unwrap_or_default();
                let beats = self
                    .beats_now()
                    .map(|b| b.times.iter().filter(|&&x| x >= start && x < end).count())
                    .filter(|&n| n > 0)
                    .map_or(String::new(), |n| format!(", {n} beats"));
                let off = if looping.off { ", off" } else { "" };
                format!("{} to {}{beats}{off}", ui::clock(start), ui::clock(end))
            }
            (Some(start), None) => format!("from {}", ui::clock(start)),
            (None, None) => "no loop".to_string(),
        };
        let nudge = keys::NUDGE;
        div()
            .flex()
            .items_center()
            .gap_2()
            .child(
                pill()
                    .child(self.tool("bar-loop-start", "loop-start", "Loop starts here", Action::LoopStart, false, cx))
                    .child(self.tool("bar-loop-end", "loop-end", "Loop ends here", Action::LoopEnd, false, cx)),
            )
            .child(
                pill()
                    .child(self.tool("widen-start", "widen-start", "Widen at the start", Action::NudgeStart(-nudge), false, cx))
                    .child(self.tool("narrow-start", "narrow-start", "Narrow at the start", Action::NudgeStart(nudge), false, cx))
                    .child(self.tool("narrow-end", "narrow-end", "Narrow at the end", Action::NudgeEnd(-nudge), false, cx))
                    .child(self.tool("widen-end", "widen-end", "Widen at the end", Action::NudgeEnd(nudge), false, cx)),
            )
            .child(
                pill()
                    .child(self.tool("slide-back", "slide-back", "Slide back by its length", Action::Slide(false), false, cx))
                    .child(self.tool("slide-on", "slide-on", "Slide on by its length", Action::Slide(true), false, cx)),
            )
            .child(
                pill()
                    .child(self.tool("marker", "flag", "Marker here, again to clear it", Action::Marker, self.marker().is_some(), cx))
                    .child(self.tool("loop-play", "loop-play", "Play from the marker, or the loop start", Action::FromLoopStart, false, cx))
                    .child(self.tool("bar-loop", "repeat", "Loop on or off, keeping its edges", Action::LoopSwitch, looping.looping(), cx))
                    .child(self.tool("loop-clear", "loop-clear", "Clear the loop", Action::LoopClear, false, cx)),
            )
            .child(
                div()
                    .flex_1()
                    .text_right()
                    .font_family(MONO)
                    .text_xs()
                    .text_color(rgb(if looping.looping() { t.loops } else { t.dim }))
                    .child(label),
            )
            .into_any_element()
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
                    .child(format!("{} back", keys::keys_for(Action::Album, keys::Mode::Library, false).join(" "))),
            )
    }

    /// Under the playing song's title: its artist and album, each opening its view, or
    /// a hint when nothing plays.
    fn now_subtitle(&self, fallback: String, cx: &mut Context<Self>) -> gpui::AnyElement {
        let t = t();
        let Some(now) = self.now.clone() else {
            return div().truncate().text_sm().text_color(rgb(t.dim)).child(fallback).into_any_element();
        };
        let link = |id: &'static str, text: String, what: &str, action: Action, artist: bool| {
            let track = now.clone();
            div()
                .id(id)
                .flex_none()
                .cursor_pointer()
                .hover(|d| d.text_color(rgb(t.accent)).underline())
                .tooltip(self.tip(what.to_string(), &[action]))
                .on_click(cx.listener(move |riff, _, _, cx| {
                    riff.show(track.clone(), artist);
                    cx.notify();
                }))
                .child(text)
        };
        div()
            .flex()
            .min_w_0()
            .overflow_hidden()
            .text_sm()
            .text_color(rgb(t.dim))
            .child(link("now-artist", now.artist.clone(), "Everything by this artist", Action::Artist, true))
            .child(div().flex_none().child(", "))
            .child(link("now-album", now.album.clone(), "This album", Action::Album, false))
            .into_any_element()
    }

    /// One trainer setting up or down a notch, kept in settings.
    fn nudge_trainer(&mut self, field: crate::trainer::Field, up: bool) {
        self.settings.trainer.nudge(field, up);
        self.save_settings();
    }

    /// The trainer's settings: from, to, step and how many repeats between steps, each
    /// with − and + buttons. `j k` pick one, `h l` change it, Enter or Esc close.
    fn render_trainer_sheet(&self, at: usize, cx: &mut Context<Self>) -> impl IntoElement {
        use crate::trainer::{FIELDS, Field};
        let t = t();
        let trainer = self.settings.trainer;
        let rows = FIELDS.iter().enumerate().map(|(i, &field)| {
            let (label, value) = match field {
                Field::From => ("Start at", format!("{}%", trainer.get(field))),
                Field::To => ("Speed up to", format!("{}%", trainer.get(field))),
                Field::Step => ("Faster by", format!("{}%", trainer.get(field))),
                Field::Every => ("Every", format!("{} repeats", trainer.get(field))),
            };
            let button = |id: &'static str, icon: &'static str, up: bool| {
                div()
                    .id((id, i))
                    .size(px(28.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded_md()
                    .cursor_pointer()
                    .hover(|d| d.bg(rgb(t.faint)))
                    .child(ui::icon(icon, 14., t.text))
                    .on_click(cx.listener(move |riff, _, _, cx| {
                        riff.trainer_sheet = Some(i);
                        riff.nudge_trainer(field, up);
                        cx.notify();
                    }))
            };
            div()
                .h(px(36.))
                .px_3()
                .flex()
                .items_center()
                .gap_2()
                .rounded_md()
                .when(i == at, |d| d.bg(rgb(t.raised)))
                .child(div().flex_1().text_sm().text_color(rgb(if i == at { t.accent } else { t.text })).child(label))
                .child(button("trainer-down", "minus", false))
                .child(div().w(px(84.)).flex().justify_center().font_family(MONO).text_sm().child(value))
                .child(button("trainer-up", "plus", true))
        });
        overlay("trainer-sheet")
            .on_click(cx.listener(|riff, _, _, cx| {
                riff.trainer_sheet = None;
                cx.notify();
            }))
            .child(
                div()
                    .id("trainer-settings")
                    .w(px(400.))
                    .flex()
                    .flex_col()
                    .gap_1()
                    .p_5()
                    .rounded_lg()
                    .bg(rgb(t.surface))
                    .border_1()
                    .border_color(rgb(t.faint))
                    // A click inside the sheet is not a click outside it.
                    .on_click(|_, _, cx| cx.stop_propagation())
                    .child(
                        div()
                            .pb_3()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(ui::icon("trending-up", 16., t.accent))
                            .child(div().text_color(rgb(t.text)).child("Speed trainer")),
                    )
                    .children(rows)
                    .child(
                        div()
                            .pt_3()
                            .font_family(MONO)
                            .text_xs()
                            .text_color(rgb(t.dim))
                            .child("j k pick   h l change   Enter or Esc close   t starts the trainer"),
                    ),
            )
    }

    /// The dialog naming the loop as a region (`r`), or renaming a section from its menu:
    /// typing goes to the name, Enter saves (an empty name saves one saying where it is),
    /// Esc or a click outside cancels.
    fn render_naming(&self, name: String, cx: &mut Context<Self>) -> impl IntoElement {
        let t = t();
        let looping = self.audio.looping;
        // From a section's menu, that section; otherwise the loop, maybe a section already.
        let section = self.renaming.zip(self.recording()).and_then(|(i, r)| self.regions.of(&r).get(i).cloned());
        let where_ = match &section {
            Some(s) => Region::default_name(s.start, s.end),
            None => looping
                .end
                .map(|end| Region::default_name(looping.start.unwrap_or_default(), end))
                .unwrap_or_default(),
        };
        let renaming = section.is_some()
            || self.recording().is_some_and(|r| self.regions.matching(&r, looping).is_some());
        let action = |id: &'static str, label: &'static str, key: &'static str, primary: bool| {
            div()
                .id(id)
                .h(px(34.))
                .px_3()
                .flex()
                .items_center()
                .gap_2()
                .rounded_md()
                .cursor_pointer()
                .bg(rgb(if primary { t.accent_soft } else { t.raised }))
                .hover(|d| d.bg(rgb(t.faint)))
                .text_sm()
                .text_color(rgb(if primary { t.accent } else { t.text }))
                .child(label)
                .child(div().font_family(MONO).text_xs().text_color(rgb(t.dim)).child(key))
        };
        overlay("naming")
            .on_click(cx.listener(|riff, _, _, cx| {
                (riff.naming, riff.renaming) = (None, None);
                cx.notify();
            }))
            .child(
                div()
                    .id("naming-sheet")
                    .w(px(440.))
                    .flex()
                    .flex_col()
                    .gap_4()
                    .p_5()
                    .rounded_lg()
                    .bg(rgb(t.surface))
                    .border_1()
                    .border_color(rgb(t.faint))
                    // A click inside the dialog is not a click outside it.
                    .on_click(|_, _, cx| cx.stop_propagation())
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(ui::icon("bookmark-plus", 16., t.sections))
                            .child(div().text_color(rgb(t.text)).child(if renaming { "Rename this section" } else { "Name this section" }))
                            .child(div().flex_1())
                            .child(div().font_family(MONO).text_xs().text_color(rgb(t.loops)).child(where_.clone())),
                    )
                    .child(
                        div()
                            .h(px(40.))
                            .px_3()
                            .flex()
                            .items_center()
                            .rounded_md()
                            .bg(rgb(t.bg))
                            .border_1()
                            .border_color(rgb(t.accent_soft))
                            .text_color(rgb(t.text))
                            .child(name.clone())
                            .child(div().ml(px(1.)).w(px(2.)).h(px(20.)).bg(rgb(t.accent)))
                            .when(name.is_empty(), |d| {
                                d.child(div().ml_2().text_color(rgb(t.faint)).child(where_.clone()))
                            }),
                    )
                    .child(
                        div()
                            .flex()
                            .justify_end()
                            .gap_2()
                            .child(action("naming-cancel", "Cancel", "Esc", false).on_click(cx.listener(|riff, _, _, cx| {
                                (riff.naming, riff.renaming) = (None, None);
                                cx.notify();
                            })))
                            .child(action("naming-save", "Save", "Enter", true).on_click(cx.listener(|riff, _, _, cx| {
                                riff.name_region();
                                cx.notify();
                            }))),
                    ),
            )
    }

    /// The artist view's header: their albums' covers, their name, and how much there is.
    /// The sidebar: places in the list (the mouse's way out of an album or artist view)
    /// and the queue. Collapsed, as in VS Code, only their icons stay.
    fn render_sidebar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let t = t();
        let open = self.sidebar;
        let full = !self.practice && self.album.is_none() && self.artist.is_none() && self.query.trim().is_empty();
        let place = |id: &'static str,
                     icon: &'static str,
                     label: &'static str,
                     lit: bool,
                     to: Place,
                     tip: &[Action],
                     cx: &mut Context<Self>| {
            div()
                .id(id)
                .h(px(30.))
                .px_2()
                .flex()
                .items_center()
                .when(!open, |d| d.justify_center())
                .gap_2()
                .rounded_md()
                .cursor_pointer()
                .text_sm()
                .text_color(rgb(if lit { t.accent } else { t.text }))
                .when(lit, |d| d.bg(rgb(t.accent_soft)))
                .hover(|d| d.bg(rgb(t.faint)))
                .child(ui::icon(icon, 14., if lit { t.accent } else { t.dim }))
                .when(open, |d| d.child(div().truncate().child(label)))
                .tooltip(self.tip(
                    match to {
                        Place::Library => "The full list",
                        Place::Favourites => "The favourites",
                        Place::Playing => "The playing song",
                    },
                    tip,
                ))
                .on_click(cx.listener(move |riff, _, _, cx| {
                    riff.go_to(to);
                    cx.notify();
                }))
        };
        // The open album or artist view, under Library, with an × that leaves it.
        let view = |id: &'static str, label: String, leave: Action, cx: &mut Context<Self>| {
            div()
                .h(px(28.))
                .pl(px(30.))
                .pr_1()
                .flex()
                .items_center()
                .gap_1()
                .rounded_md()
                .text_sm()
                .bg(rgb(t.accent_soft))
                .text_color(rgb(t.accent))
                .child(div().flex_1().min_w_0().truncate().child(label))
                .child(self.tool(id, "x", "Back to the full list", leave, false, cx).h(px(22.)).w(px(22.)))
        };
        let toggle = self
            .tool("sidebar", "panel-left", if open { "Collapse the sidebar" } else { "Open the sidebar" }, Action::Sidebar, false, cx)
            .h(px(26.));
        if !open {
            // The queue's icon opens the sidebar, with its count beside it.
            let queue = div()
                .id("open-queue")
                .h(px(30.))
                .flex()
                .flex_col()
                .items_center()
                .justify_center()
                .rounded_md()
                .cursor_pointer()
                .hover(|d| d.bg(rgb(t.faint)))
                .child(ui::icon("list-plus", 14., if self.queue.is_empty() { t.dim } else { t.accent }))
                .when(!self.queue.is_empty(), |d| {
                    d.child(div().font_family(MONO).text_xs().text_color(rgb(t.accent)).child(self.queue.len().to_string()))
                })
                .tooltip(self.tip("The queue", &[Action::QueueFocus]))
                .on_click(cx.listener(|riff, _, _, cx| {
                    riff.sidebar = true;
                    cx.notify();
                }));
            return div()
                .w(px(44.))
                .flex_none()
                .flex()
                .flex_col()
                .items_center()
                .gap_1()
                .p_1()
                .rounded_lg()
                .bg(rgb(t.surface))
                .child(toggle)
                .child(div().w_full().flex().flex_col().gap_0p5().children([
                    place("go-library", "music", "Library", full, Place::Library, &[Action::Clear], cx),
                    place("go-favourites", "star", "Favourites", false, Place::Favourites, &[], cx),
                    place("go-playing", "audio-waveform", "Playing", false, Place::Playing, &[Action::ToPlaying], cx),
                ]))
                .child(div().w_full().h(px(1.)).bg(rgb(t.faint)))
                .child(div().w_full().child(queue))
                .into_any_element();
        }
        let lit = self.queued_at();
        let vim = self.settings.vim;
        let rows: Vec<_> = self
            .queue
            .songs()
            .iter()
            .enumerate()
            .filter_map(|(i, path)| {
                let track = self.track_at(path)?;
                let on = lit == Some(i);
                let button = |id: &'static str, icon: &'static str, what: &'static str, act: fn(&mut Riff, usize)| {
                    div()
                        .id((id, i))
                        .size(px(22.))
                        .flex_none()
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded_md()
                        .cursor_pointer()
                        .hover(|d| d.bg(rgb(t.faint)))
                        .child(ui::icon(icon, 13., t.text))
                        .tooltip(self.tip(what, &[]))
                        .on_click(cx.listener(move |riff, _, _, cx| {
                            cx.stop_propagation();
                            act(riff, i);
                            cx.notify();
                        }))
                };
                Some(
                    div()
                        .id(("queued", i))
                        .group("queue-row")
                        .relative()
                        .px_1()
                        .py_1()
                        .flex()
                        .items_center()
                        .gap_2()
                        .rounded_md()
                        .cursor_pointer()
                        .when(on, |d| d.bg(rgb(t.raised)))
                        .hover(|d| d.bg(rgb(t.raised)))
                        .on_click(cx.listener(move |riff, _, _, cx| {
                            riff.play_queued(i);
                            cx.notify();
                        }))
                        .on_mouse_down(
                            MouseButton::Right,
                            cx.listener(move |riff, e: &MouseDownEvent, _, cx| {
                                riff.menu = Some((Menu::queued(i), e.position));
                                cx.notify();
                            }),
                        )
                        .child(cover(track.cover.clone(), 34.))
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .flex()
                                .flex_col()
                                .line_height(relative(1.3))
                                .child(
                                    div()
                                        .truncate()
                                        .text_sm()
                                        .text_color(rgb(if on { t.accent } else { t.text }))
                                        .child(track.title.clone()),
                                )
                                .child(div().truncate().text_xs().text_color(rgb(t.dim)).child(track.artist.clone())),
                        )
                        // Over the text's end, only while hovered or highlighted, so
                        // titles use the whole width otherwise.
                        .child(
                            div()
                                .absolute()
                                .right_1()
                                .top_0()
                                .bottom_0()
                                .flex()
                                .items_center()
                                .gap_0p5()
                                .pl_2()
                                .bg(rgb(t.raised))
                                .when(!on, |d| d.opacity(0.).group_hover("queue-row", |s| s.opacity(1.)))
                                .child(button("queue-top", "arrow-up-to-line", "To the top", |riff, i| riff.queue.to_top(i)))
                                .child(button("queue-remove", "x", "Remove from the queue", |riff, i| {
                                    riff.queue.remove(i);
                                })),
                        ),
                )
            })
            .collect();
        div()
            .w(px(260.))
            .flex_none()
            .flex()
            .flex_col()
            .gap_4()
            .p_2()
            .rounded_lg()
            .bg(rgb(t.surface))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_0p5()
                    .child(div().flex().child(toggle))
                    .child(place("go-library", "music", "Library", full, Place::Library, &[Action::Clear], cx))
                    .children(self.album.as_ref().filter(|_| !self.practice).map(|a| {
                        let name = if a.album.is_empty() { "Unknown album".into() } else { a.album.clone() };
                        view("leave-album", name, Action::Album, cx)
                    }))
                    .children(
                        self.artist
                            .as_ref()
                            .filter(|_| !self.practice)
                            .map(|a| view("leave-artist", a.artist.clone(), Action::Artist, cx)),
                    )
                    .child(place("go-favourites", "star", "Favourites", false, Place::Favourites, &[], cx))
                    .when(self.now.is_some(), |d| {
                        d.child(place("go-playing", "audio-waveform", "Playing", false, Place::Playing, &[Action::ToPlaying], cx))
                    }),
            )
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(
                        div()
                            .h(px(24.))
                            .px_1()
                            .flex()
                            .items_center()
                            .gap_2()
                            .font_family(MONO)
                            .text_xs()
                            .text_color(rgb(if self.queue_at.is_some() { t.accent } else { t.dim }))
                            .child(div().flex_1().child(match self.queue.len() {
                                0 => "Queue".to_string(),
                                n => format!("Queue  {n}"),
                            }))
                            .when(!self.queue.is_empty(), |d| {
                                d.child(
                                    div()
                                        .id("queue-clear")
                                        .px_1()
                                        .rounded_md()
                                        .cursor_pointer()
                                        .hover(|d| d.bg(rgb(t.faint)).text_color(rgb(t.text)))
                                        .child("clear")
                                        .on_click(cx.listener(|riff, _, _, cx| {
                                            (riff.queue, riff.queue_at) = (Queue::default(), None);
                                            cx.notify();
                                        })),
                                )
                            }),
                    )
                    .when(self.queue.is_empty(), |d| {
                        d.child(div().px_1().text_xs().text_color(rgb(t.dim)).child(format!(
                            "{} queues the highlighted song",
                            keys::keys_for(Action::Queue(false), keys::Mode::Library, vim).join(" ")
                        )))
                    })
                    .child(div().id("queue").flex_1().min_h_0().overflow_y_scroll().flex().flex_col().children(rows)),
            )
            .into_any_element()
    }

    fn render_artist(&self, of: &Track) -> impl IntoElement {
        let t = t();
        let songs = library::artist(&self.tracks, of);
        let seconds = songs.iter().map(|&i| self.tracks[i].seconds).sum::<Option<u64>>();
        // One cover per album, oldest first, as the rows below list them.
        let mut albums: Vec<&Track> = Vec::new();
        for &i in &songs {
            let t = &self.tracks[i];
            if albums.last().is_none_or(|a| a.album != t.album || a.path.parent() != t.path.parent()) {
                albums.push(t);
            }
        }
        let count = if albums.len() == 1 { "1 album".to_string() } else { format!("{} albums", albums.len()) };
        let about = format!("{count}, {}", ui::album_stats(songs.len(), seconds));
        div()
            .flex()
            .items_end()
            .gap_5()
            .p_4()
            .rounded_lg()
            .bg(rgb(t.surface))
            .child(div().flex().gap_2().children(albums.iter().take(4).map(|a| cover(a.cover.clone(), 96.))))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(div().font_family(MONO).text_xs().text_color(rgb(t.dim)).child("Artist"))
                    .child(
                        div()
                            .truncate()
                            .text_3xl()
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .text_color(rgb(t.text))
                            .child(if of.artist.is_empty() { "Unknown artist".into() } else { of.artist.clone() }),
                    )
                    .child(div().truncate().text_sm().text_color(rgb(t.dim)).child(about)),
            )
            .child(
                div()
                    .flex_none()
                    .font_family(MONO)
                    .text_xs()
                    .text_color(rgb(t.dim))
                    .child(format!("{} back", keys::keys_for(Action::Artist, keys::Mode::Library, false).join(" "))),
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
        let system = prefs.system.as_ref().map_or("none".to_string(), |o| o.name.clone());
        let output = match (&self.settings.output, &prefs.chosen) {
            (None, _) => format!("The system's output, now {system}"),
            (Some(_), Some(name)) => name.clone(),
            (Some(_), None) => format!("A device not connected now; playing through {system}"),
        };
        body = body.child(title("Output"));
        body = match prefs.choosing {
            None => body.child(
                row(("output", 0), lit(folders.len() + 3), "headphones", output).on_click(cx.listener(|riff, _, _, cx| {
                    riff.open_outputs();
                    cx.notify();
                })),
            ),
            Some(_) if prefs.listing.is_some() => body.child(row(("output", 0), false, "headphones", "Listing devices…".into())),
            Some(at) => {
                let entries = std::iter::once((format!("The system's output, now {system}"), None))
                    .chain(prefs.outputs.iter().map(|o| (o.name.clone(), Some(o.id.clone()))));
                body.children(entries.enumerate().map(|(i, (name, id))| {
                    row(("pick-output", i), i == at, "headphones", name)
                        .children((id == self.settings.output).then(|| ui::icon("check", 14., t.dim)))
                        .on_click(cx.listener(move |riff, _, _, cx| {
                            riff.choose_output(i);
                            cx.notify();
                        }))
                }))
            }
        };
        let hint = match (prefs.picker.is_some(), self.settings.vim) {
            _ if prefs.choosing.is_some() => "↑↓ move   Enter plays through it   Esc closes",
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
    /// Beat ticks behind the waveform, brighter while the grid is on.
    beats: Option<Rc<Beats>>,
    grid: bool,
    /// Where `o` plays from, drawn as a flag.
    marker: Option<Duration>,
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
            // GPUI draws plain quads without antialiasing, so an edge between device
            // pixels snaps to one, and a slowly moving edge (the overview's box and
            // playhead, about a pixel every quarter second) steps visibly. A pixel an
            // edge only partly covers is drawn at partial strength instead: it glides.
            let device = window.scale_factor();
            let soft = |window: &mut Window, a: f32, b: f32, y: f32, ht: f32, color: gpui::Rgba| {
                let (a, b) = (a * device, b * device);
                let mut paint = |from: f32, to: f32, cover: f32| {
                    if to > from && cover > 0. {
                        let partial = gpui::Rgba { a: color.a * cover, ..color };
                        window.paint_quad(fill(rect(from / device, y, (to - from) / device, ht), partial));
                    }
                };
                let (left, right) = (a.floor(), b.floor());
                if left == right {
                    paint(left, left + 1., b - a);
                } else {
                    paint(left, left + 1., left + 1. - a);
                    paint(left + 1., right, 1.);
                    paint(right, right + 1., b - right);
                }
            };
            // A loop switched off keeps its place, faded.
            let (region, edge) = if p.looping.off { (0x10, 0x70) } else { (0x26, 0xff) };
            if let Some(end) = p.looping.end {
                let (a, b) = (x_of(p.looping.start.unwrap_or_default()), x_of(end));
                soft(window, a, b, 0., h, tint(t.loops, region));
            }
            // The stretch the detail shows: outlined, so it reads at a glance.
            if let Some((from, to)) = p.boxed {
                let (a, b) = (x_of(from), x_of(to).max(x_of(from) + 2.));
                soft(window, a, b, 0., h, tint(t.accent, 0x24));
                for x in [a, b - 2.] {
                    soft(window, x, x + 2., 0., h, tint(t.accent, 0xff));
                }
                for y in [0., h - 2.] {
                    soft(window, a, b, y, 2., tint(t.accent, 0xff));
                }
            }
            // Beats as small chevrons pointing in from the top and bottom edges: a full
            // line through the waveform was too much.
            if let Some(beats) = &p.beats {
                let color = tint(t.text, if p.grid { 0xb0 } else { 0x38 });
                let first = beats.times.partition_point(|&b| b < p.from);
                let shown: Vec<Duration> =
                    beats.times[first..].iter().copied().take_while(|&b| b <= p.to).collect();
                // Zoomed far out, every second or fourth beat, so chevrons stay apart.
                let apart = |stride: usize| {
                    shown.len() < 2 || w / (shown.len() as f32 / stride as f32) >= 24.
                };
                let stride = [1, 2, 4, 8, 16].into_iter().find(|&n| apart(n)).unwrap_or(32);
                // Counted from the song's first beat, not the view's, so a scrolling view
                // keeps the same beats.
                let kept = shown.iter().enumerate().filter(|(i, _)| (first + i) % stride == 0);
                for (_, &beat) in kept {
                    let x = x_of(beat);
                    for row in 0..5 {
                        let half = (4 - row) as f32;
                        let row = row as f32;
                        window.paint_quad(fill(rect(x - half, row, half * 2. + 1., 1.), color));
                        window.paint_quad(fill(rect(x - half, h - 1. - row, half * 2. + 1., 1.), color));
                    }
                }
            }
            if let Some(wave) = &p.wave {
                let (behind, columns) = wave.columns(p.from, p.to, w as usize);
                let scale = if p.contour {
                    columns.iter().map(|c| c.rms()).fold(0., f32::max)
                } else {
                    wave.loudest()
                };
                let mid = h / 2.;
                // Whole device pixels, so a column never smears across two.
                let snap = |x: f32| (x * device).round() / device;
                let played = x_of(p.at);
                for (i, c) in columns.iter().enumerate() {
                    let x = snap(i as f32 - behind as f32);
                    if x >= w {
                        break;
                    }
                    let half = (c.rms() / scale.max(1e-6)).min(1.) * (mid - 2.);
                    let color = if x < played { t.accent } else { t.dim };
                    let bar = rect(x.max(0.), mid - half, 1., (half * 2.).max(1.));
                    window.paint_quad(fill(bar, tint(color, 0xff)));
                }
            }
            // Each edge as soon as it is set, a start on its own too.
            for at in [p.looping.start, p.looping.end].into_iter().flatten() {
                soft(window, x_of(at) - 1., x_of(at) + 1., 0., h, tint(t.loops, edge));
            }
            if let Some(m) = p.marker.filter(|m| *m >= p.from && *m <= p.to) {
                let x = x_of(m);
                soft(window, x - 1., x + 1., 0., h, tint(t.accent, 0xd0));
                window.paint_quad(fill(rect(x - 1., 0., 8., 6.), tint(t.accent, 0xff)));
            }
            soft(window, x_of(p.at), x_of(p.at) + 1., 0., h, tint(t.text, 0xff));
        },
    )
}

/// Grips over the loop's edges and the marker on a strip showing `from` to `to`, as wide as a click
/// that grabs an edge: there the cursor becomes a resize arrow. The strip under them
/// still takes the press.
fn edge_grips(
    looping: crate::audio::Loop,
    marker: Option<Duration>,
    from: Duration,
    to: Duration,
) -> impl Iterator<Item = Div> {
    let span = (to.saturating_sub(from)).as_secs_f64();
    [looping.start, looping.end, marker]
        .into_iter()
        .flatten()
        .filter(move |&e| span > 0. && e >= from && e <= to)
        .map(move |e| {
            div()
                .absolute()
                .top_0()
                .bottom_0()
                .left(relative(((e - from).as_secs_f64() / span) as f32))
                .ml(px(-GRAB))
                .w(px(2. * GRAB))
                .cursor(gpui::CursorStyle::ResizeLeftRight)
        })
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
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // The practice view glides with the music: drawn every frame while it plays, the
        // playhead smoothed between the player's position updates.
        let playing = self.audio.playing();
        let at = self.smooth.at(self.audio.position(), playing, self.started.elapsed(), self.audio.speed());
        // Between frames the smoothed playhead can run a moment past the loop's end,
        // where the audio has already jumped back; draw it where the music is.
        let looping = self.audio.looping;
        let at = match (looping.jump(at), looping.end) {
            (Some(start), Some(end)) => start + (at - end),
            _ => at,
        };
        if self.practice && playing {
            window.request_animation_frame();
        }
        // Any scroll shows the scrollbar for a moment.
        if let Some((_, _, offset)) = self.list_extent()
            && offset != self.seen_offset
        {
            self.seen_offset = offset;
            self.scrolled_at = Some(std::time::Instant::now());
        }
        // A rail jump glides: eased out over 220 ms.
        if let Some((from, to, began)) = self.glide {
            let done = (began.elapsed().as_secs_f32() / 0.22).min(1.);
            let eased = 1. - (1. - done).powi(3);
            let base = &self.scroll.0.borrow().base_handle;
            base.set_offset(gpui::point(base.offset().x, from + (to - from) * eased));
            if done < 1. {
                window.request_animation_frame();
            } else {
                self.glide = None;
            }
        }
        // The detail view keeps the loop in sight, growing only when it must.
        let looping = self.audio.looping;
        if let Some(end) = looping.end.filter(|_| looping.looping() && self.dragging.is_none()) {
            self.span = wave::fit(self.span, end - looping.start.unwrap_or_default());
        }
        let hint = match (&self.naming, &self.message) {
            (_, message) => match message {
            Some((text, error)) => div()
                .text_color(rgb(if *error { t().error } else { t().dim }))
                .child(text.clone()),
            None if self.queue_at.is_some() => div().child(
                "Enter plays now   t to the top   dd removes   Esc back   ? all keys",
            ),
            None if self.practice => div().child(
                "a b loop   \\ loop on or off   r section   g grid   s slow down   Esc library   ? all keys",
            ),
            None if self.searching => div().child(
                "Enter plays and stops searching   Esc stops searching   ⌘⌫ clears",
            ),
            None if self.settings.vim => div().child(
                "/ search   Space pause   p practice   ? all keys",
            ),
            None => div().child("⌘P search   Space pause   p practice   ⌘T Transcribe!   ⌘? all keys"),
            },
        };
        div()
            .track_focus(&self.focus)
            .on_key_down(cx.listener(Self::key))
            .on_mouse_move(cx.listener(Self::drag))
            .on_mouse_up(MouseButton::Left, cx.listener(|riff, _, _, cx| {
                // `|`, not `||`: both drags end, whichever was on.
                if riff.scrubbing.take().is_some() | riff.grab.take().is_some() | riff.drop_slider() {
                    cx.notify();
                }
                riff.drop_edge(cx)
            }))
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
                    .flex()
                    .items_center()
                    .justify_end()
                    .gap_1()
                    .pr_3()
                    .on_click(|event, window, _| {
                        if event.click_count() == 2 {
                            window.titlebar_double_click();
                        }
                    })
                    .when(!self.practice, |d| d.child(self.list_switch(cx)))
                    .child(self.mode_switch(cx))
                    .child(self.tool("keys", "keyboard", "Every key", Action::Help, self.help, cx))
                    .child(self.tool("settings", "settings-2", "Settings", Action::Settings, self.prefs.is_some(), cx)),
            )
            .font_family(UI)
            .text_color(rgb(t().text))
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .gap_3()
                    .child(self.render_sidebar(cx))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .gap_3()
                            .child(self.render_search(cx))
                            .when(self.practice, |d| d.child(self.render_practice(at, cx)))
                            .when(!self.practice, |d| {
                                d.children(self.album.as_ref().map(|of| self.render_album(of)))
                                    .children(self.artist.as_ref().map(|of| self.render_artist(of)))
                                    .child(
                                        div()
                                            .flex_1()
                                            .min_h_0()
                                            .relative()
                                            .flex()
                                            .gap_1()
                                            .child(
                                                div()
                                                    .relative()
                                                    .flex_1()
                                                    .h_full()
                                                    // The pinned artist slides up out of sight here.
                                                    .overflow_hidden()
                                                    .child(
                                                        uniform_list(
                                                            "library",
                                                            self.rows.len(),
                                                            cx.processor(|riff, range: std::ops::Range<usize>, _window, cx| {
                                                                range.map(|ix| riff.render_row(ix, cx)).collect::<Vec<_>>()
                                                            }),
                                                        )
                                                        .track_scroll(self.scroll.clone())
                                                        .size_full(),
                                                    )
                                                    .children(self.render_pinned(cx))
                                                    .child(self.render_scrollbar(cx)),
                                            )
                                            .when(self.browsing(), |d| d.child(self.render_rail(cx)))
                                            // The letter being dragged to, large, while the list glides.
                                            .children(self.scrubbing.map(|letter| {
                                                div().absolute().size_full().flex().items_center().justify_center().child(
                                                    div()
                                                        .size(px(88.))
                                                        .flex()
                                                        .items_center()
                                                        .justify_center()
                                                        .rounded_lg()
                                                        .bg(rgb(t().raised))
                                                        .border_1()
                                                        .border_color(rgb(t().faint))
                                                        .text_3xl()
                                                        .font_weight(gpui::FontWeight::MEDIUM)
                                                        .text_color(rgb(t().accent))
                                                        .child(letter.to_string()),
                                                )
                                            })),
                                    )
                            })
                    ),
            )
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
            .children(self.naming.clone().map(|name| self.render_naming(name, cx)))
            .children(self.prefs.as_ref().map(|p| self.render_prefs(p, cx)))
            .children(self.trainer_sheet.map(|at| self.render_trainer_sheet(at, cx)))
            .children(self.render_menu(cx))
    }
}
