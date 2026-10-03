//! Colour themes, as in Jasmijn: the built-in gold, plus every theme in the Zed theme
//! files installed on this Mac, mapped onto riff's colour roles.

use crate::library::fold;
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::sync::RwLock;

#[derive(Debug, Clone, PartialEq)]
pub struct Theme {
    pub name: String,
    pub bg: u32,
    pub surface: u32,
    pub raised: u32,
    pub accent: u32,
    pub accent_soft: u32,
    pub text: u32,
    pub dim: u32,
    pub faint: u32,
    pub error: u32,
    /// The loop region: a second colour, so the accent only ever means "selected".
    pub on: u32,
}

pub const DEFAULT_NAME: &str = "Gold";

/// Warm near-black and foundry gold, Jasmijn's palette.
pub fn gold() -> Theme {
    Theme {
        name: DEFAULT_NAME.to_string(),
        bg: 0x0e0c0a,
        surface: 0x17140f,
        raised: 0x211c15,
        accent: 0xd9a441,
        accent_soft: 0x3b2d14,
        text: 0xede4d3,
        dim: 0x8f8574,
        faint: 0x3f392f,
        error: 0xe0563f,
        on: 0x9fbf6e,
    }
}

static ACTIVE: RwLock<Option<Theme>> = RwLock::new(None);

/// The theme the window draws with.
pub fn active() -> Theme {
    ACTIVE
        .read()
        .ok()
        .and_then(|t| t.clone())
        .unwrap_or_else(gold)
}

pub fn set_active(theme: Theme) {
    if let Ok(mut active) = ACTIVE.write() {
        *active = Some(theme);
    }
}

/// `#rrggbb` or `#rrggbbaa`, the alpha blended over `under`.
fn colour(value: &Value, under: u32) -> Option<u32> {
    let hex = value.as_str()?.strip_prefix('#')?;
    let channel = |i: usize| u32::from_str_radix(hex.get(i..i + 2)?, 16).ok();
    let (r, g, b) = (channel(0)?, channel(2)?, channel(4)?);
    let a = match hex.len() {
        6 => 255,
        8 => channel(6)?,
        _ => return None,
    };
    Some(blend((r << 16) | (g << 8) | b, under, a as f32 / 255.0))
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

/// One Zed theme (an entry of a theme file's `themes` list), mapped onto riff's roles.
pub fn from_zed(theme: &Value) -> Option<Theme> {
    let name = theme["name"].as_str()?.to_string();
    let style = &theme["style"];
    let fallback = gold();
    let bg = colour(&style["background"], 0).unwrap_or(fallback.bg);
    let pick = |keys: &[&str], default: u32| {
        keys.iter()
            .find_map(|k| colour(&style[*k], bg))
            .unwrap_or(default)
    };
    let accent = pick(&["text.accent"], fallback.accent);
    Some(Theme {
        name,
        bg,
        surface: pick(&["surface.background"], fallback.surface),
        raised: pick(
            &["ghost_element.selected", "element.selected"],
            fallback.raised,
        ),
        accent,
        accent_soft: blend(accent, bg, 0.25),
        text: pick(&["text"], fallback.text),
        dim: pick(&["text.muted"], fallback.dim),
        faint: pick(&["border.variant", "border"], fallback.faint),
        error: pick(&["error"], fallback.error),
        on: pick(&["success"], fallback.on),
    })
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

/// Every theme on offer: gold first, then the Zed themes by name. Unreadable files are
/// skipped; a theme name seen twice keeps its first file.
pub fn available(dirs: &[PathBuf]) -> Vec<Theme> {
    let mut zed: Vec<Theme> = dirs
        .iter()
        .flat_map(|dir| std::fs::read_dir(dir).into_iter().flatten().flatten())
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|e| e == "json"))
        .flat_map(|path| from_file(&path))
        .collect();
    zed.sort_by_key(|t| t.name.to_lowercase());
    zed.dedup_by(|a, b| a.name == b.name);
    std::iter::once(gold()).chain(zed).collect()
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

fn from_file(path: &Path) -> Vec<Theme> {
    std::fs::read(path)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
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
                "error": "#f7768e",
                "success": "#9ece6a"
            }
        })
    }

    #[test]
    fn maps_the_roles() {
        let t = from_zed(&tokyo()).unwrap();
        assert_eq!(t.name, "Tokyo Night");
        assert_eq!(t.bg, 0x1a1b26);
        assert_eq!(t.surface, 0x16161e);
        assert_eq!(t.raised, 0x565f89);
        assert_eq!(t.accent, 0x7dcfff);
        assert_eq!(t.text, 0xa9b1d6);
        assert_eq!(t.dim, 0x787c99);
        assert_eq!(t.faint, 0x101014);
        assert_eq!(t.error, 0xf7768e);
        assert_eq!(t.on, 0x9ece6a);
    }

    #[test]
    fn accent_soft_is_a_quarter_accent_over_the_background() {
        let t = from_zed(&tokyo()).unwrap();
        assert_eq!(t.accent_soft, blend(0x7dcfff, 0x1a1b26, 0.25));
        assert_ne!(t.accent_soft, t.accent);
    }

    #[test]
    fn alpha_is_blended_over_the_background() {
        let mut theme = tokyo();
        theme["style"]["ghost_element.selected"] = json!("#ffffff80");
        let t = from_zed(&theme).unwrap();
        assert_eq!(t.raised, blend(0xffffff, 0x1a1b26, 128.0 / 255.0));
    }

    #[test]
    fn missing_keys_fall_back_to_gold() {
        let t = from_zed(&json!({"name": "Bare", "style": {"background": "#000000"}})).unwrap();
        assert_eq!(t.accent, gold().accent);
        assert_eq!(t.text, gold().text);
        assert_eq!(t.bg, 0x000000);
    }

    #[test]
    fn filtering_needs_every_word() {
        let mut light = from_zed(&tokyo()).unwrap();
        light.name = "Tokyo Night Light".into();
        let themes = [gold(), from_zed(&tokyo()).unwrap(), light];
        let names = |q: &str| -> Vec<String> {
            filter(&themes, q).into_iter().map(|t| t.name.clone()).collect()
        };
        assert_eq!(names(""), [DEFAULT_NAME, "Tokyo Night", "Tokyo Night Light"]);
        assert_eq!(names("light tok"), ["Tokyo Night Light"]);
        assert_eq!(names("GOLD"), [DEFAULT_NAME]);
        assert!(names("dracula").is_empty());
    }

    #[test]
    fn nameless_themes_are_skipped() {
        assert_eq!(from_zed(&json!({"style": {}})), None);
    }

    #[test]
    fn every_variant_in_a_file_and_gold_first() {
        let dir = std::env::temp_dir().join(format!("riff-themes-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let mut light = tokyo();
        light["name"] = json!("Tokyo Night Light");
        std::fs::write(
            dir.join("tokyo.json"),
            json!({"themes": [tokyo(), light]}).to_string(),
        )
        .unwrap();
        std::fs::write(dir.join("broken.json"), b"{not json").unwrap();
        std::fs::write(dir.join("notes.txt"), b"ignored").unwrap();
        let names: Vec<String> = available(&[dir.clone(), dir.join("missing")])
            .into_iter()
            .map(|t| t.name)
            .collect();
        assert_eq!(names, [DEFAULT_NAME, "Tokyo Night", "Tokyo Night Light"]);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
