//! How Odeion looks: the active colours, the embedded fonts and icons, and small drawing
//! helpers. Gold Dark keeps Jasmijn's gold, so the two read as siblings.

use gpui::{App, AssetSource, IntoElement, Result, SharedString, Styled, px, rgb, svg};
use std::borrow::Cow;

/// The active colour theme.
pub fn t() -> crate::theme::Theme {
    crate::theme::active()
}

pub const UI: &str = "Space Grotesk";
pub const MONO: &str = "JetBrains Mono";

/// The fonts, embedded so the app looks the same on any Mac. Both are OFL-licensed.
pub fn load_fonts(cx: &App) {
    let fonts: Vec<Cow<'static, [u8]>> = vec![
        Cow::Borrowed(include_bytes!("../assets/fonts/SpaceGrotesk-Regular.ttf")),
        Cow::Borrowed(include_bytes!("../assets/fonts/SpaceGrotesk-Medium.ttf")),
        Cow::Borrowed(include_bytes!("../assets/fonts/JetBrainsMono-Regular.ttf")),
        Cow::Borrowed(include_bytes!("../assets/fonts/JetBrainsMono-Medium.ttf")),
    ];
    let _ = cx.text_system().add_fonts(fonts);
}

/// Lucide icons (ISC licence), embedded.
pub struct Assets;

macro_rules! icons {
    ($($name:literal),* $(,)?) => {
        fn icon_bytes(path: &str) -> Option<&'static [u8]> {
            match path {
                $(concat!("icons/", $name, ".svg") => Some(include_bytes!(concat!("../assets/icons/", $name, ".svg"))),)*
                _ => None,
            }
        }
    };
}

icons!(
    "panel-left", "arrow-up-to-line", "list-plus", "columns-3", "list-tree", "sliders-vertical", "chevron-left", "chevron-right", "list-music", "pencil", "trash-2",
    "play", "pause", "repeat", "flag", "music", "search", "star", "folder", "x", "plus",
    "palette", "check", "skip-back", "skip-forward", "shuffle", "keyboard", "gauge", "audio-waveform", "settings-2",
    "zoom-in", "zoom-out", "layers", "layers-plus", "volume-2", "volume-x", "headphones",
    "bookmark-plus", "bookmark-minus", "loop-start", "loop-end", "widen-start", "narrow-start",
    "narrow-end", "widen-end", "slide-back", "slide-on", "loop-play", "loop-clear", "beats",
    "region-prev", "region-next", "disc", "minus", "trending-up", "rotate-ccw", "crosshair",
    "chevron-down", "chevron-up", "circle-alert",
);

impl AssetSource for Assets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        Ok(icon_bytes(path).map(Cow::Borrowed))
    }

    fn list(&self, _path: &str) -> Result<Vec<SharedString>> {
        Ok(Vec::new())
    }
}

pub fn icon(name: &str, size: f32, color: u32) -> impl IntoElement {
    svg()
        .path(SharedString::from(format!("icons/{name}.svg")))
        .size(px(size))
        .flex_none()
        .text_color(rgb(color))
}

/// 2,673: a count with thousands separated.
pub fn count(n: usize) -> String {
    let digits = n.to_string();
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(c);
    }
    out
}

/// What a finished scan changed, or nothing when it changed nothing.
pub fn scan_result(new: usize, gone: usize) -> Option<String> {
    (new + gone > 0).then(|| format!("{} new, {} gone", count(new), count(gone)))
}

/// An album's size: "12 songs, 57 min", or "1 song, 1 hr 4 min". Without a known length
/// only the count.
pub fn album_stats(songs: usize, seconds: Option<u64>) -> String {
    let count = if songs == 1 { "1 song".to_string() } else { format!("{songs} songs") };
    let minutes = seconds.filter(|&s| s > 0).map(|s| (s + 30) / 60);
    match minutes {
        Some(m) if m >= 60 => format!("{count}, {} hr {} min", m / 60, m % 60),
        Some(m) => format!("{count}, {m} min"),
        None => count,
    }
}

/// How many rows the wheel moves a short list by: `dy` pixels (below 0 is down the
/// list, as the content follows the fingers) added to what `left` kept, a row every
/// `row` pixels. A trackpad's many small deltas add up; what isn't a whole row is kept.
pub fn wheel_rows(left: &mut f32, dy: f32, row: f32) -> i32 {
    *left += dy;
    let rows = (*left / row).trunc();
    *left -= rows * row;
    -rows as i32
}

/// 3:07, or 1:02:07 for something very long.
pub fn clock(d: std::time::Duration) -> String {
    let s = d.as_secs();
    if s >= 3600 {
        format!("{}:{:02}:{:02}", s / 3600, s / 60 % 60, s % 60)
    } else {
        format!("{}:{:02}", s / 60, s % 60)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn album_stats_count_and_round_to_minutes() {
        assert_eq!(album_stats(12, Some(57 * 60 + 42)), "12 songs, 58 min");
        assert_eq!(album_stats(1, Some(64 * 60)), "1 song, 1 hr 4 min");
        assert_eq!(album_stats(3, None), "3 songs");
    }

    #[test]
    fn counts_get_thousands_separators() {
        assert_eq!(count(7), "7");
        assert_eq!(count(2673), "2,673");
        assert_eq!(count(1_234_567), "1,234,567");
    }

    #[test]
    fn a_scan_without_changes_says_nothing() {
        assert_eq!(scan_result(0, 0), None);
        assert_eq!(scan_result(12, 1), Some("12 new, 1 gone".into()));
    }

    #[test]
    fn the_wheel_moves_whole_rows_and_keeps_the_rest() {
        let mut left = 0.;
        // A mouse notch is a row.
        assert_eq!(wheel_rows(&mut left, -20., 20.), 1);
        assert_eq!(wheel_rows(&mut left, 20., 20.), -1);
        // A trackpad's small steps add up.
        assert_eq!(wheel_rows(&mut left, -8., 20.), 0);
        assert_eq!(wheel_rows(&mut left, -8., 20.), 0);
        assert_eq!(wheel_rows(&mut left, -8., 20.), 1);
        assert_eq!(left, -4.);
        // A fast flick moves several.
        assert_eq!(wheel_rows(&mut left, -64., 20.), 3);
    }

    #[test]
    fn clock_reads_like_a_player() {
        assert_eq!(clock(Duration::from_secs(187)), "3:07");
        assert_eq!(clock(Duration::from_millis(999)), "0:00");
        assert_eq!(clock(Duration::from_secs(3727)), "1:02:07");
    }
}
