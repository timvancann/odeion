//! Colours, by role. Riff's own palettes, Dark and Light (the Appearance setting picks
//! one, Auto following macOS), and the Zed themes installed on this Mac as editor themes
//! (experimental): only their surfaces, text and accent are used, the practice colours
//! (loop, sections, marker, playhead, waveform) always riff's for the theme's lightness.

use crate::library::fold;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::sync::RwLock;

/// What the window draws with, by role. Several roles share a value in Dark; they stay
/// apart because Light needs them apart (gold as a line, as text, as a fill).
#[derive(Debug, Clone, PartialEq)]
pub struct Theme {
    pub name: String,
    /// One of the installed Zed themes rather than riff's own.
    pub editor: bool,
    pub dark: bool,
    /// The window, title bar, list and input fields.
    pub bg: u32,
    /// Cards, waveform strips, sheets and menus.
    pub surface: u32,
    /// Pills, unlit big buttons, key chips, controls.
    pub raised: u32,
    pub hover: u32,
    /// Hairlines around sheets, menus, chips; dividers.
    pub border: u32,
    /// Slider and progress tracks, a resting scrollbar.
    pub track: u32,
    pub text: u32,
    pub text_dim: u32,
    pub text_disabled: u32,
    /// Lit icons, fills of progress and volume, the caret, a lit button's ring.
    pub accent: u32,
    /// Lit labels, the playing title, links on hover, messages.
    pub accent_text: u32,
    /// Solid fills with `on_accent` on them: the mode switch, the solo badge.
    pub accent_fill: u32,
    pub on_accent: u32,
    /// The fill behind lit buttons, chips and places.
    pub accent_soft: u32,
    pub focus: u32,
    /// The selected row, the highlighted menu or picker entry.
    pub selection: u32,
    pub selection_ring: u32,
    pub danger: u32,
    pub danger_soft: u32,
    pub tooltip_bg: u32,
    pub tooltip_text: u32,
    pub tooltip_key: u32,
    pub wave_played: u32,
    pub wave_unplayed: u32,
    pub playhead: u32,
    /// The loop's edges and span (`loop` is a Rust keyword).
    pub loops: u32,
    pub loop_text: u32,
    pub section: u32,
    pub marker: u32,
    /// A stem's level, filled behind its name.
    pub level: u32,
    /// Alphas over what they sit on: the loop region, the region of a loop switched off
    /// and its edges, a section, the section in play, the dimming behind sheets.
    pub loop_fill: u8,
    pub loop_fill_off: u8,
    pub loop_edge_off: u8,
    pub section_fill: u8,
    pub section_fill_lit: u8,
    pub scrim: u8,
}

/// The name riff's own theme goes by in the theme picker.
pub const OWN: &str = "Riff";
/// Riff's default scheme, light and dark: the greys and the blue of T3 Code
/// (github.com/pingdotgg/t3code), whose look it follows.
pub const T3: &str = "T3";

impl Theme {
    /// What `settings.json` keeps for this theme: its name, nothing for riff's own.
    pub fn saved(&self) -> String {
        if !self.editor && self.name == OWN { String::new() } else { self.name.clone() }
    }
}

/// Whether a saved theme is one of riff's own schemes, which follow the Appearance.
pub fn is_own(saved: &str) -> bool {
    saved.is_empty() || saved == T3
}

/// Riff Dark: warm near-black and the gold, a touch brighter than Jasmijn's.
pub fn dark() -> Theme {
    Theme {
        name: OWN.to_string(),
        editor: false,
        dark: true,
        bg: 0x0d0b09,
        surface: 0x16130f,
        raised: 0x221d17,
        hover: 0x2c2720,
        border: 0x38322a,
        track: 0x423c34,
        text: 0xefe7d8,
        text_dim: 0xada394,
        text_disabled: 0x746e63,
        accent: 0xe8b048,
        accent_text: 0xe8b048,
        accent_fill: 0xe8b048,
        on_accent: 0x0d0b09,
        accent_soft: 0x403012,
        focus: 0xe8b048,
        selection: 0x352c1e,
        selection_ring: 0x9c7a3d,
        danger: 0xf3715a,
        danger_soft: 0x47211b,
        tooltip_bg: 0x37322b,
        tooltip_text: 0xf4eee1,
        tooltip_key: 0xeebb58,
        wave_played: 0xe8b048,
        wave_unplayed: 0x746e63,
        playhead: 0xf4eee1,
        loops: 0x67d5f5,
        loop_text: 0x67d5f5,
        section: 0x5a89e6,
        marker: 0xd86069,
        level: 0x453c2b,
        loop_fill: 0x26,
        loop_fill_off: 0x10,
        loop_edge_off: 0x70,
        section_fill: 0x2a,
        section_fill_lit: 0x60,
        scrim: 0xcc,
    }
}

/// Riff Light: warm paper, ochre for lines, bronze for text, the bright gold only as a
/// fill with dark text on it.
pub fn light() -> Theme {
    Theme {
        name: OWN.to_string(),
        editor: false,
        dark: false,
        bg: 0xf6f3ee,
        surface: 0xfefdfb,
        raised: 0xebe7e0,
        hover: 0xe3dfd7,
        border: 0xd8d4cc,
        track: 0xcecac2,
        text: 0x241e18,
        text_dim: 0x625950,
        text_disabled: 0x867f77,
        accent: 0xad720d,
        accent_text: 0x845012,
        accent_fill: 0xedb83f,
        on_accent: 0x241e18,
        accent_soft: 0xf1daa8,
        focus: 0xad720d,
        selection: 0xe9dfca,
        selection_ring: 0xad720d,
        danger: 0xc02c26,
        danger_soft: 0xffe0db,
        tooltip_bg: 0x29231c,
        tooltip_text: 0xf4f1ec,
        tooltip_key: 0xefbc4b,
        wave_played: 0xad720d,
        wave_unplayed: 0x96918a,
        playhead: 0x241e18,
        loops: 0x2382ba,
        loop_text: 0x0e6a9b,
        section: 0x1c3891,
        marker: 0x89211e,
        level: 0xdfd7c5,
        loop_fill: 0x2e,
        loop_fill_off: 0x12,
        loop_edge_off: 0x70,
        section_fill: 0x24,
        section_fill_lit: 0x4c,
        scrim: 0xb8,
    }
}

/// T3 Light: near-white zinc greys, white cards, a deep clear blue. The played wave
/// is the blue, so sections are amber here; the loop stays cyan and the marker red.
pub fn t3_light() -> Theme {
    Theme {
        name: T3.to_string(),
        editor: false,
        dark: false,
        bg: 0xfafafa,
        surface: 0xffffff,
        raised: 0xf4f4f5,
        hover: 0xececee,
        border: 0xe4e4e7,
        track: 0xd4d4d8,
        text: 0x18181b,
        text_dim: 0x62626b,
        text_disabled: 0x8a8a94,
        accent: 0x2350d6,
        accent_text: 0x1f48c2,
        accent_fill: 0x1f48c2,
        on_accent: 0xffffff,
        accent_soft: 0xe0e8fc,
        focus: 0x2350d6,
        selection: 0xedf1fd,
        selection_ring: 0x2350d6,
        danger: 0xc5221f,
        danger_soft: 0xfde3e0,
        tooltip_bg: 0x18181b,
        tooltip_text: 0xfafafa,
        tooltip_key: 0x9db8ff,
        wave_played: 0x2350d6,
        wave_unplayed: 0x8e8e98,
        playhead: 0x18181b,
        loops: 0x0b90a2,
        loop_text: 0x08717f,
        section: 0xb98403,
        marker: 0xaf2938,
        level: 0xe6ebf8,
        loop_fill: 0x2e,
        loop_fill_off: 0x12,
        loop_edge_off: 0x70,
        section_fill: 0x22,
        section_fill_lit: 0x48,
        scrim: 0xb8,
    }
}

/// T3 Dark: black, near-black cards, the blue lighter, with dark text on its fills.
pub fn t3_dark() -> Theme {
    Theme {
        name: T3.to_string(),
        editor: false,
        dark: true,
        bg: 0x000000,
        surface: 0x111111,
        raised: 0x1a1b1e,
        hover: 0x232428,
        border: 0x2a2b2f,
        track: 0x3a3b40,
        text: 0xf1f3f7,
        text_dim: 0xa3a3a3,
        text_disabled: 0x6e6e73,
        accent: 0x4a72ea,
        accent_text: 0x8aa8f8,
        accent_fill: 0x9db6fa,
        on_accent: 0x000000,
        accent_soft: 0x1a2443,
        focus: 0x6b8ff0,
        selection: 0x181d2c,
        selection_ring: 0x5577d8,
        danger: 0xf2705e,
        danger_soft: 0x401c18,
        tooltip_bg: 0x26272b,
        tooltip_text: 0xf1f3f7,
        tooltip_key: 0x9db6fa,
        wave_played: 0x4a72ea,
        wave_unplayed: 0x67676d,
        playhead: 0xf1f3f7,
        loops: 0x0ec7de,
        loop_text: 0x0ec7de,
        section: 0xd6b02a,
        marker: 0xd9483f,
        level: 0x1f2a48,
        loop_fill: 0x26,
        loop_fill_off: 0x10,
        loop_edge_off: 0x70,
        section_fill: 0x2a,
        section_fill_lit: 0x60,
        scrim: 0xcc,
    }
}

/// Which of riff's palettes to draw with: Auto follows macOS.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Appearance {
    #[default]
    Auto,
    Light,
    Dark,
}

impl Appearance {
    /// The next one in the settings row's cycle: Auto, Light, Dark.
    pub fn next(self) -> Appearance {
        match self {
            Appearance::Auto => Appearance::Light,
            Appearance::Light => Appearance::Dark,
            Appearance::Dark => Appearance::Auto,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Appearance::Auto => "Auto (follows macOS)",
            Appearance::Light => "Light",
            Appearance::Dark => "Dark",
        }
    }

    /// What settings from before this choice get: Dark, which is what they showed.
    pub fn before() -> Appearance {
        Appearance::Dark
    }
}

/// The palette of riff's own scheme `saved` (T3, else Riff) for `appearance`, with
/// macOS dark or not right now.
pub fn own(saved: &str, appearance: Appearance, system_dark: bool) -> Theme {
    let dark_now = match appearance {
        Appearance::Auto => system_dark,
        Appearance::Light => false,
        Appearance::Dark => true,
    };
    match (saved == T3, dark_now) {
        (true, true) => t3_dark(),
        (true, false) => t3_light(),
        (false, true) => dark(),
        (false, false) => light(),
    }
}

/// Riff's own schemes for `appearance`, as the theme picker lists them: T3, the
/// default, first.
pub fn owns(appearance: Appearance, system_dark: bool) -> Vec<Theme> {
    vec![own(T3, appearance, system_dark), own("", appearance, system_dark)]
}

static ACTIVE: RwLock<Option<Theme>> = RwLock::new(None);

/// The theme the window draws with.
pub fn active() -> Theme {
    ACTIVE.read().ok().and_then(|t| t.clone()).unwrap_or_else(dark)
}

pub fn set_active(theme: Theme) {
    if let Ok(mut active) = ACTIVE.write() {
        *active = Some(theme);
    }
}

// Colour maths: WCAG contrast, OKLab and OKLCH, colour blindness simulated (Machado et
// al. 2009, full severity, on linear sRGB).

fn lin(v: f64) -> f64 {
    if v <= 0.04045 { v / 12.92 } else { ((v + 0.055) / 1.055).powf(2.4) }
}

fn unlin(v: f64) -> f64 {
    if v <= 0.0031308 { 12.92 * v } else { 1.055 * v.powf(1.0 / 2.4) - 0.055 }
}

fn linear(c: u32) -> [f64; 3] {
    [16, 8, 0].map(|s| lin(((c >> s) & 0xff) as f64 / 255.0))
}

fn pack(rgb: [f64; 3]) -> u32 {
    rgb.iter()
        .fold(0, |acc, &v| (acc << 8) | (unlin(v.clamp(0.0, 1.0)).clamp(0.0, 1.0) * 255.0).round() as u32)
}

fn luminance(c: u32) -> f64 {
    let [r, g, b] = linear(c);
    0.2126 * r + 0.7152 * g + 0.0722 * b
}

/// The WCAG 2 contrast ratio of two colours, 1 to 21.
pub fn contrast(a: u32, b: u32) -> f64 {
    let (la, lb) = (luminance(a), luminance(b));
    (la.max(lb) + 0.05) / (la.min(lb) + 0.05)
}

fn to_oklab([r, g, b]: [f64; 3]) -> [f64; 3] {
    let l = 0.4122214708 * r + 0.5363325363 * g + 0.0514459929 * b;
    let m = 0.2119034982 * r + 0.6806995451 * g + 0.1073969566 * b;
    let s = 0.0883024619 * r + 0.2817188376 * g + 0.6299787005 * b;
    let [l, m, s] = [l, m, s].map(f64::cbrt);
    [
        0.2104542553 * l + 0.7936177850 * m - 0.0040720468 * s,
        1.9779984951 * l - 2.4285922050 * m + 0.4505937099 * s,
        0.0259040371 * l + 0.7827717662 * m - 0.8086757660 * s,
    ]
}

fn from_oklab([lightness, a, b]: [f64; 3]) -> [f64; 3] {
    let l = lightness + 0.3963377774 * a + 0.2158037573 * b;
    let m = lightness - 0.1055613458 * a - 0.0638541728 * b;
    let s = lightness - 0.0894841775 * a - 1.2914855480 * b;
    let [l, m, s] = [l, m, s].map(|x| x * x * x);
    [
        4.0767416621 * l - 3.3077115913 * m + 0.2309699292 * s,
        -1.2684380046 * l + 2.6097574011 * m - 0.3413193965 * s,
        -0.0041960863 * l - 0.7034186147 * m + 1.7076147010 * s,
    ]
}

/// Lightness, chroma and hue (radians).
fn oklch(c: u32) -> [f64; 3] {
    let [l, a, b] = to_oklab(linear(c));
    [l, a.hypot(b), b.atan2(a)]
}

/// An OKLCH colour, its chroma reduced until it fits sRGB.
fn from_oklch(l: f64, chroma: f64, hue: f64) -> u32 {
    let mut chroma = chroma.max(0.0);
    loop {
        let rgb = from_oklab([l, chroma * hue.cos(), chroma * hue.sin()]);
        if chroma <= 0.0 || rgb.iter().all(|v| (-1e-4..=1.0 + 1e-4).contains(v)) {
            return pack(rgb);
        }
        chroma = (chroma - 0.002).max(0.0);
    }
}

/// `c` moved in OKLCH lightness, keeping its hue and chroma.
fn shift(c: u32, by: f64) -> u32 {
    let [l, chroma, hue] = oklch(c);
    from_oklch((l + by).clamp(0.0, 1.0), chroma, hue)
}

type Matrix = [[f64; 3]; 3];
const DEUTAN: Matrix = [[0.367322, 0.860646, -0.227968], [0.280085, 0.672501, 0.047413], [-0.011820, 0.042940, 0.968881]];
const PROTAN: Matrix = [[0.152286, 1.052583, -0.204868], [0.114503, 0.786281, 0.099216], [-0.003882, -0.048116, 1.051998]];

fn simulate(c: u32, m: Option<&Matrix>) -> [f64; 3] {
    let l = linear(c);
    match m {
        None => l,
        Some(m) => m.map(|row| (row[0] * l[0] + row[1] * l[1] + row[2] * l[2]).clamp(0.0, 1.0)),
    }
}

/// How far apart two colours look: OKLab distance times 100, as seen with `m`.
fn distance(a: u32, b: u32, m: Option<&Matrix>) -> f64 {
    let (x, y) = (to_oklab(simulate(a, m)), to_oklab(simulate(b, m)));
    100.0 * ((x[0] - y[0]).powi(2) + (x[1] - y[1]).powi(2) + (x[2] - y[2]).powi(2)).sqrt()
}

/// Whether two practice marks are easy to tell apart: 20 for normal vision, 12 with
/// red-green colour blindness.
fn told_apart(a: u32, b: u32) -> bool {
    distance(a, b, None) >= 20.0 && distance(a, b, Some(&DEUTAN)) >= 12.0 && distance(a, b, Some(&PROTAN)) >= 12.0
}

/// `c` moved in lightness, away from the background (lighter on a dark theme), until it
/// has `min` contrast on every one of `on`.
fn readable(c: u32, on: &[u32], min: f64, dark: bool) -> u32 {
    let mut c = c;
    for _ in 0..100 {
        if on.iter().all(|&o| contrast(c, o) >= min) {
            break;
        }
        c = shift(c, if dark { 0.01 } else { -0.01 });
    }
    c
}

/// `over` at `alpha` on top of `under`.
fn blend(over: u32, under: u32, alpha: f32) -> u32 {
    let mix = |shift: u32| {
        let o = ((over >> shift) & 0xff) as f32;
        let u = ((under >> shift) & 0xff) as f32;
        ((o * alpha + u * (1.0 - alpha)).round() as u32) << shift
    };
    mix(16) | mix(8) | mix(0)
}

/// A hex colour, `#` optional: `rgb`, `rgba`, `rrggbb` or `rrggbbaa`, the alpha blended
/// over `under`.
fn colour(value: &Value, under: u32) -> Option<u32> {
    let text = value.as_str()?.trim();
    let hex = text.strip_prefix('#').unwrap_or(text);
    let digits: Vec<u32> = hex.chars().map(|c| c.to_digit(16)).collect::<Option<_>>()?;
    let channels: Vec<u32> = match digits.len() {
        3 | 4 => digits.iter().map(|d| d * 17).collect(),
        6 | 8 => digits.chunks(2).map(|p| p[0] * 16 + p[1]).collect(),
        _ => return None,
    };
    let alpha = channels.get(3).copied().unwrap_or(255);
    Some(blend((channels[0] << 16) | (channels[1] << 8) | channels[2], under, alpha as f32 / 255.0))
}

/// One Zed theme (an entry of a theme file's `themes` list) as an editor theme. Its
/// lightness from `appearance` or its background; surfaces stepped from its background
/// as riff's own are; its text made readable; its accent only when it shows on its
/// controls and stands apart from the practice marks. The rest is riff's palette of the
/// same lightness, never the other one's.
pub fn from_zed(theme: &Value) -> Option<Theme> {
    let name = theme["name"].as_str()?.to_string();
    let style = &theme["style"];
    let background = colour(&style["background"], 0);
    let dark = match theme["appearance"].as_str() {
        Some("dark") => true,
        Some("light") => false,
        _ => background.is_none_or(|bg| oklch(bg)[0] < 0.5),
    };
    // A background so mid-grey that even white text can't read on its stepped surfaces
    // gets smaller steps.
    let readable_on = |t: &Theme| {
        [t.bg, t.surface, t.raised, t.hover, t.selection, t.accent_soft]
            .iter()
            .all(|&on| contrast(t.text, on) >= 7.0 && contrast(t.text_dim, on) >= 4.5)
    };
    let tries = [1.0, 0.75, 0.5, 0.25, 0.0].map(|scale| derive(&name, style, background, dark, scale));
    let fallback = tries[tries.len() - 1].clone();
    Some(tries.into_iter().find(readable_on).unwrap_or(fallback))
}

/// An editor theme with its surfaces stepped `scale` times as far as riff's own are.
fn derive(name: &str, style: &Value, background: Option<u32>, dark: bool, scale: f64) -> Theme {
    let own = if dark { self::dark() } else { light() };
    let bg = background.unwrap_or(own.bg);
    let steps = if dark { [0.04, 0.085, 0.125, 0.17, 0.21] } else { [0.03, -0.035, -0.06, -0.095, -0.125] };
    let [surface, raised, hover, border, track] = steps.map(|s| shift(bg, s * scale));
    let pick = |key: &str| colour(&style[key], bg);
    let accent = pick("text.accent")
        .filter(|&a| contrast(a, raised) >= 3.0 && [own.loops, own.section, own.marker].iter().all(|&m| told_apart(a, m)))
        .unwrap_or_else(|| readable(own.accent, &[raised], 3.0, dark));
    // Softer fills too when the surfaces step less.
    let fills = (0.5 + scale / 2.) as f32;
    let accent_soft = blend(accent, bg, 0.24 * fills);
    let selection = blend(accent, bg, 0.16 * fills);
    let under_text = [bg, surface, raised, hover, selection, accent_soft];
    let text = readable(pick("text").unwrap_or(own.text), &under_text, 7.0, dark);
    let text_dim = readable(pick("text.muted").unwrap_or(blend(text, bg, 0.65)), &under_text, 4.5, dark);
    Theme {
        name: name.to_string(),
        editor: true,
        dark,
        bg,
        surface,
        raised,
        hover,
        border,
        track,
        text,
        text_dim,
        text_disabled: readable(blend(text, bg, 0.45), &[bg], 3.0, dark),
        accent,
        accent_text: readable(accent, &[surface, raised, accent_soft], 4.5, dark),
        accent_fill: accent,
        on_accent: if contrast(bg, accent) >= contrast(text, accent) { bg } else { text },
        accent_soft,
        focus: accent,
        selection,
        selection_ring: readable(blend(accent, bg, 0.7), &[bg, selection], 3.0, dark),
        level: shift(bg, if dark { 0.21 } else { -0.085 } * scale.max(0.5)),
        ..own
    }
}

/// Where Zed keeps installed and user themes.
pub fn zed_dirs() -> Vec<PathBuf> {
    let home = PathBuf::from(std::env::var_os("HOME").unwrap_or_default());
    let mut dirs = vec![home.join(".config/zed/themes")];
    let installed = home.join("Library/Application Support/Zed/extensions/installed");
    if let Ok(entries) = std::fs::read_dir(installed) {
        dirs.extend(entries.flatten().map(|e| e.path().join("themes")));
    }
    dirs
}

/// Every theme on offer: riff's own schemes (`owns`) first, then the editor themes by
/// name. Unreadable files are skipped; a theme name seen twice keeps its first file, and
/// an editor theme named like one of riff's own is left out.
pub fn available(dirs: &[PathBuf], owns: Vec<Theme>) -> Vec<Theme> {
    let mut zed: Vec<Theme> = dirs
        .iter()
        .flat_map(|dir| std::fs::read_dir(dir).into_iter().flatten().flatten())
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|e| e == "json"))
        .flat_map(|path| from_file(&path))
        .collect();
    zed.sort_by_key(|t| t.name.to_lowercase());
    zed.dedup_by(|a, b| a.name == b.name);
    zed.retain(|t| t.name != OWN && !is_own(&t.name));
    owns.into_iter().chain(zed).collect()
}

/// The installed editor theme called `name`.
pub fn editor(dirs: &[PathBuf], name: &str) -> Option<Theme> {
    available(dirs, Vec::new()).into_iter().find(|t| t.editor && t.name == name)
}

/// The themes whose name holds every word of `query`, ignoring case and accents, in
/// their listed order.
pub fn filter<'a>(themes: &'a [Theme], query: &str) -> Vec<&'a Theme> {
    let words: Vec<String> = fold(query).split_whitespace().map(String::from).collect();
    themes
        .iter()
        .filter(|t| {
            let name = fold(&t.name);
            words.iter().all(|w| name.contains(w.as_str()))
        })
        .collect()
}

/// JSON with `//` and `/* */` comments, as theme files may have, made plain JSON.
fn strip_comments(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    let mut in_string = false;
    while let Some(c) = chars.next() {
        if in_string {
            out.push(c);
            match c {
                '\\' => out.extend(chars.next()),
                '"' => in_string = false,
                _ => {}
            }
        } else if c == '/' && chars.peek() == Some(&'/') {
            while chars.next_if(|&n| n != '\n').is_some() {}
        } else if c == '/' && chars.peek() == Some(&'*') {
            chars.next();
            let mut last = ' ';
            for n in chars.by_ref() {
                if last == '*' && n == '/' {
                    break;
                }
                last = n;
            }
            out.push(' ');
        } else {
            in_string = c == '"';
            out.push(c);
        }
    }
    out
}

fn from_file(path: &Path) -> Vec<Theme> {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|text| serde_json::from_str::<Value>(&strip_comments(&text)).ok())
        .and_then(|file| file["themes"].as_array().cloned())
        .into_iter()
        .flatten()
        .filter_map(|t| from_zed(&t))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// The contrast checks the colour proposal sets: (what, colour, on, at least).
    fn checks(t: &Theme) -> Vec<(&'static str, u32, u32, f64)> {
        let loop_fill = blend(t.loops, t.surface, t.loop_fill as f32 / 255.0);
        let section = blend(t.section, t.surface, t.section_fill as f32 / 255.0);
        let section_lit = blend(t.section, t.surface, t.section_fill_lit as f32 / 255.0);
        vec![
            ("text on bg", t.text, t.bg, 7.0),
            ("text on raised", t.text, t.raised, 7.0),
            ("text on hover", t.text, t.hover, 7.0),
            ("text on selection", t.text, t.selection, 7.0),
            ("text on lit fill", t.text, t.accent_soft, 7.0),
            ("text on accent fill", t.on_accent, t.accent_fill, 7.0),
            ("tooltip text", t.tooltip_text, t.tooltip_bg, 7.0),
            ("name on lit section", t.text, section_lit, 7.0),
            ("dim on bg", t.text_dim, t.bg, 4.5),
            ("dim on raised", t.text_dim, t.raised, 4.5),
            ("dim on hover", t.text_dim, t.hover, 4.5),
            ("dim on selection", t.text_dim, t.selection, 4.5),
            ("dim on lit fill", t.text_dim, t.accent_soft, 4.5),
            ("dim name on section", t.text_dim, section, 4.5),
            ("accent text on surface", t.accent_text, t.surface, 4.5),
            ("accent text on raised", t.accent_text, t.raised, 4.5),
            ("accent text on lit fill", t.accent_text, t.accent_soft, 4.5),
            ("loop text on surface", t.loop_text, t.surface, 4.5),
            ("loop text on raised", t.loop_text, t.raised, 4.5),
            ("tooltip key", t.tooltip_key, t.tooltip_bg, 4.5),
            ("danger on surface", t.danger, t.surface, 4.5),
            ("danger on raised", t.danger, t.raised, 4.5),
            ("danger on its fill", t.danger, t.danger_soft, 4.5),
            ("disabled on bg", t.text_disabled, t.bg, 3.0),
            ("accent on bg", t.accent, t.bg, 3.0),
            ("accent on raised", t.accent, t.raised, 3.0),
            ("focus on surface", t.focus, t.surface, 3.0),
            ("selection ring on bg", t.selection_ring, t.bg, 3.0),
            ("played wave on surface", t.wave_played, t.surface, 3.0),
            ("unplayed wave on surface", t.wave_unplayed, t.surface, 3.0),
            ("playhead on surface", t.playhead, t.surface, 3.0),
            ("playhead on loop fill", t.playhead, loop_fill, 3.0),
            ("loop edge on surface", t.loops, t.surface, 3.0),
            ("loop edge on loop fill", t.loops, loop_fill, 3.0),
            ("section on surface", t.section, t.surface, 3.0),
            ("marker on surface", t.marker, t.surface, 3.0),
            ("marker on loop fill", t.marker, loop_fill, 3.0),
        ]
    }

    fn failing(t: &Theme, only: &[&str]) -> Vec<String> {
        checks(t)
            .into_iter()
            .filter(|(what, ..)| only.is_empty() || only.contains(what))
            .filter(|&(_, c, on, min)| contrast(c, on) < min)
            .map(|(what, c, on, min)| format!("{what}: {:.2} < {min}", contrast(c, on)))
            .collect()
    }

    #[test]
    fn every_palette_of_riffs_own_passes_every_contrast_check() {
        assert_eq!(failing(&dark(), &[]), Vec::<String>::new());
        assert_eq!(failing(&light(), &[]), Vec::<String>::new());
        assert_eq!(failing(&t3_dark(), &[]), Vec::<String>::new());
        assert_eq!(failing(&t3_light(), &[]), Vec::<String>::new());
    }

    #[test]
    fn contrast_matches_wcag() {
        assert!((contrast(0x000000, 0xffffff) - 21.0).abs() < 1e-9);
        assert!((contrast(0xefe7d8, 0x0d0b09) - 15.99).abs() < 0.01);
        assert!((contrast(0x625950, 0xf6f3ee) - 6.19).abs() < 0.01);
    }

    #[test]
    fn practice_marks_stay_apart_even_with_red_green_colour_blindness() {
        for t in [dark(), light(), t3_dark(), t3_light()] {
            let marks = [("loop", t.loops), ("section", t.section), ("marker", t.marker), ("played", t.wave_played)];
            for (i, (a, x)) in marks.iter().enumerate() {
                for (b, y) in &marks[i + 1..] {
                    let which = if t.dark { "dark" } else { "light" };
                    assert!(told_apart(*x, *y), "{a} and {b} too close in {} {which}", t.name);
                }
            }
        }
        // The proposal's own numbers: loop and section in Dark, 20.7 apart.
        assert!((distance(0x67d5f5, 0x5a89e6, None) - 20.7).abs() < 0.1);
    }

    #[test]
    fn appearance_cycles_and_auto_follows_the_system() {
        assert_eq!(Appearance::Auto.next(), Appearance::Light);
        assert_eq!(Appearance::Light.next(), Appearance::Dark);
        assert_eq!(Appearance::Dark.next(), Appearance::Auto);
        assert_eq!(own("", Appearance::Auto, true), dark());
        assert_eq!(own("", Appearance::Auto, false), light());
        assert_eq!(own("", Appearance::Light, true), light());
        assert_eq!(own("", Appearance::Dark, false), dark());
        assert_eq!(own(T3, Appearance::Auto, true), t3_dark());
        assert_eq!(own(T3, Appearance::Light, true), t3_light());
        // A name that isn't one of riff's own (an editor theme gone) gets Riff.
        assert_eq!(own("Gone", Appearance::Dark, false), dark());
        assert_eq!(Appearance::default(), Appearance::Auto);
    }

    fn tokyo() -> Value {
        json!({
            "name": "Tokyo Night",
            "appearance": "dark",
            "style": {
                "background": "#1a1b26",
                "surface.background": "#16161e",
                "element.selected": "#565f89",
                "border": "#101014",
                "text": "#a9b1d6",
                "text.muted": "#787c99",
                "text.accent": "#7dcfff",
                "error": "#f7768e"
            }
        })
    }

    fn tokyo_light() -> Value {
        json!({
            "name": "Tokyo Night Light",
            "appearance": "light",
            "style": {
                "background": "#d5d6db",
                "text": "#343b58",
                "text.muted": "#9699a3",
                "text.accent": "#8c4351"
            }
        })
    }

    /// What an editor theme is held to: its text, and its accent where it is used.
    const EDITOR_CHECKS: &[&str] = &[
        "text on bg", "text on raised", "text on hover", "text on selection", "text on lit fill",
        "dim on bg", "dim on raised", "dim on hover", "dim on selection", "dim on lit fill",
        "accent text on surface", "accent text on raised", "accent text on lit fill",
        "accent on raised", "selection ring on bg", "disabled on bg",
    ];

    #[test]
    fn an_editor_theme_keeps_its_surfaces_and_text_and_riffs_practice_colours() {
        let t = from_zed(&tokyo()).unwrap();
        assert!(t.editor && t.dark);
        assert_eq!(t.name, "Tokyo Night");
        assert_eq!(t.bg, 0x1a1b26);
        // Its text, lightened until it reads on every surface, still its own blue.
        assert!((oklch(t.text)[2] - oklch(0xa9b1d6)[2]).abs() < 0.1);
        // Lighter step by step, in its own blue.
        assert!(luminance(t.bg) < luminance(t.surface) && luminance(t.surface) < luminance(t.raised));
        assert!(luminance(t.raised) < luminance(t.hover) && luminance(t.hover) < luminance(t.track));
        // Its cyan accent is too like the loop's: riff's gold instead.
        assert_eq!(t.accent, dark().accent);
        for (mine, riffs) in [(t.loops, dark().loops), (t.section, dark().section), (t.marker, dark().marker), (t.playhead, dark().playhead), (t.wave_played, dark().wave_played), (t.danger, dark().danger)] {
            assert_eq!(mine, riffs);
        }
        assert_eq!(failing(&t, EDITOR_CHECKS), Vec::<String>::new());
        assert_eq!(t.saved(), "Tokyo Night");
    }

    #[test]
    fn a_light_editor_theme_gets_the_light_palette_and_readable_text() {
        let t = from_zed(&tokyo_light()).unwrap();
        assert!(!t.dark);
        // Its muted text was too faint; nudged darker until it reads.
        assert_ne!(t.text_dim, 0x9699a3);
        assert_eq!(failing(&t, EDITOR_CHECKS), Vec::<String>::new());
        // Its dark red accent is too like the marker: riff's ochre, darkened to show on
        // this theme's greyer controls.
        assert_ne!(t.accent, 0x8c4351);
        assert!((oklch(t.accent)[2] - oklch(light().accent)[2]).abs() < 0.1);
        assert_eq!(t.marker, light().marker);
        assert_eq!(t.loops, light().loops);
    }

    #[test]
    fn a_theme_with_only_a_background_derives_the_rest_from_its_own_lightness() {
        let t = from_zed(&json!({"name": "Paper", "style": {"background": "fff"}})).unwrap();
        assert!(!t.dark);
        assert_eq!(t.bg, 0xffffff);
        assert_eq!(t.text, light().text);
        assert_eq!(t.accent, light().accent);
        assert_eq!(failing(&t, EDITOR_CHECKS), Vec::<String>::new());
        let t = from_zed(&json!({"name": "Bare", "style": {"background": "#000000"}})).unwrap();
        assert!(t.dark);
        assert_eq!(t.text, dark().text);
        assert_eq!(failing(&t, EDITOR_CHECKS), Vec::<String>::new());
    }

    #[test]
    fn a_kept_accent_must_show_on_controls() {
        let mut theme = tokyo();
        theme["style"]["text.accent"] = json!("#cbc24a"); // a yellow apart from the marks
        assert_eq!(from_zed(&theme).unwrap().accent, 0xcbc24a);
        theme["style"]["text.accent"] = json!("#ff9e64"); // orange, too near the red marker
        assert_eq!(from_zed(&theme).unwrap().accent, dark().accent);
        theme["style"]["text.accent"] = json!("#2a2b36"); // hardly shows on its controls
        assert_eq!(from_zed(&theme).unwrap().accent, dark().accent);
    }

    #[test]
    fn hex_with_or_without_hash_short_and_with_alpha() {
        let c = |s: &str| colour(&json!(s), 0x000000);
        assert_eq!(c("#1a1b26"), Some(0x1a1b26));
        assert_eq!(c("1a1b26"), Some(0x1a1b26));
        assert_eq!(c("#fa0"), Some(0xffaa00));
        assert_eq!(c("fa0f"), Some(0xffaa00));
        assert_eq!(c("#ffffff80"), Some(blend(0xffffff, 0, 128.0 / 255.0)));
        assert_eq!(c("#fff8"), Some(blend(0xffffff, 0, 136.0 / 255.0)));
        assert_eq!(c("#12345"), None);
        assert_eq!(c("#ggg"), None);
        assert_eq!(colour(&json!(3), 0), None);
    }

    #[test]
    fn comments_are_stripped_but_not_inside_strings() {
        let text = r#"{
            // a line comment
            "name": "A // not a comment", /* a block
            comment */ "url": "http://x/*y*/", "quote": "say \"hi\" // still text"
        }"#;
        let v: Value = serde_json::from_str(&strip_comments(text)).unwrap();
        assert_eq!(v["name"], "A // not a comment");
        assert_eq!(v["url"], "http://x/*y*/");
        assert_eq!(v["quote"], "say \"hi\" // still text");
    }

    #[test]
    fn filtering_needs_every_word() {
        let themes = [dark(), from_zed(&tokyo()).unwrap(), from_zed(&tokyo_light()).unwrap()];
        let names = |q: &str| -> Vec<String> { filter(&themes, q).into_iter().map(|t| t.name.clone()).collect() };
        assert_eq!(names(""), [OWN, "Tokyo Night", "Tokyo Night Light"]);
        assert_eq!(names("light tok"), ["Tokyo Night Light"]);
        assert_eq!(names("RIFF"), [OWN]);
        assert!(names("dracula").is_empty());
    }

    #[test]
    fn nameless_themes_are_skipped() {
        assert_eq!(from_zed(&json!({"style": {}})), None);
    }

    #[test]
    fn riffs_own_saves_as_nothing_and_t3_by_name() {
        assert_eq!(dark().saved(), "");
        assert_eq!(light().saved(), "");
        assert_eq!(t3_dark().saved(), T3);
        assert!(is_own("") && is_own(T3) && !is_own("Tokyo Night"));
    }

    #[test]
    fn every_variant_in_a_file_commented_files_too_and_riff_first() {
        let dir = std::env::temp_dir().join(format!("riff-themes-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("tokyo.json"), json!({"themes": [tokyo(), tokyo_light()]}).to_string()).unwrap();
        std::fs::write(
            dir.join("commented.json"),
            "// Mine\n{\"themes\": [{\"name\": \"Commented\", /* dark */ \"style\": {\"background\": \"#111\"}}]}",
        )
        .unwrap();
        std::fs::write(dir.join("broken.json"), b"{not json").unwrap();
        std::fs::write(dir.join("notes.txt"), b"ignored").unwrap();
        // An editor theme can't take the name of one of riff's own.
        std::fs::write(
            dir.join("t3.json"),
            json!({"themes": [{"name": T3, "style": {"background": "#222"}}]}).to_string(),
        )
        .unwrap();
        let names: Vec<String> = available(&[dir.clone(), dir.join("missing")], owns(Appearance::Light, false))
            .into_iter()
            .map(|t| t.name)
            .collect();
        assert_eq!(names, [T3, OWN, "Commented", "Tokyo Night", "Tokyo Night Light"]);
        assert_eq!(editor(&[dir.clone()], "Tokyo Night").map(|t| t.bg), Some(0x1a1b26));
        assert_eq!(editor(&[dir.clone()], OWN), None);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
