use gpui::{Hsla, rgb};
use std::path::PathBuf;

pub use notes_core::preferences::Appearance;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Theme {
    pub navigation: Hsla,
    pub list: Hsla,
    pub editor: Hsla,
    pub overlay: Hsla,
    pub text: Hsla,
    pub muted: Hsla,
    pub accent: Hsla,
    pub border: Hsla,
    pub selected: Hsla,
    pub on_accent: Hsla,
}

fn mix(a: u32, b: u32, amount: f32) -> u32 {
    [16, 8, 0].into_iter().fold(0, |value, shift| {
        let a = ((a >> shift) & 255) as f32;
        let b = ((b >> shift) & 255) as f32;
        value | (((a + (b - a) * amount).round() as u32) << shift)
    })
}

fn luminance(color: u32) -> f32 {
    [16, 8, 0]
        .into_iter()
        .zip([0.2126, 0.7152, 0.0722])
        .map(|(shift, weight)| {
            let c = ((color >> shift) & 255) as f32 / 255.;
            weight
                * if c <= 0.04045 {
                    c / 12.92
                } else {
                    ((c + 0.055) / 1.055).powf(2.4)
                }
        })
        .sum()
}
fn contrast(a: u32, b: u32) -> f32 {
    let a = luminance(a);
    let b = luminance(b);
    (a.max(b) + 0.05) / (a.min(b) + 0.05)
}
fn readable(color: u32, background: u32, ratio: f32) -> u32 {
    let target = if contrast(0xffffff, background) > contrast(0, background) {
        0xffffff
    } else {
        0
    };
    (0..=100)
        .map(|step| mix(color, target, step as f32 / 100.))
        .find(|&c| contrast(c, background) >= ratio)
        .unwrap_or(target)
}

impl Theme {
    pub fn reference() -> Self {
        Self::derive(0x202225, 0xf3eee5, 0xe7ca84)
    }
    pub fn light() -> Self {
        Self::derive(0xfaf8f2, 0x292821, 0x856323)
    }
    pub fn derive(background: u32, foreground: u32, accent: u32) -> Self {
        let dark = luminance(background) < 0.4;
        let extreme = if dark { 0 } else { 0xffffff };
        let navigation = mix(background, extreme, 0.10);
        let list = mix(background, extreme, 0.04);
        let selected = mix(background, accent, 0.13);
        let overlay = mix(background, foreground, 0.06);
        let surfaces = [background, navigation, list, selected, overlay];
        let ensure_all = |color, ratio| {
            surfaces
                .iter()
                .fold(color, |c, &surface| readable(c, surface, ratio))
        };
        let accent = ensure_all(accent, 4.5);
        Self {
            navigation: rgb(navigation).into(),
            list: rgb(list).into(),
            editor: rgb(background).into(),
            overlay: rgb(overlay).into(),
            text: rgb(ensure_all(foreground, 4.5)).into(),
            muted: rgb(ensure_all(mix(background, foreground, 0.64), 4.5)).into(),
            accent: rgb(accent).into(),
            border: rgb(mix(background, foreground, 0.16)).into(),
            selected: rgb(selected).into(),
            on_accent: rgb(readable(background, accent, 4.5)).into(),
        }
    }
}

pub fn theme_directories() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Some(path) = std::env::var_os("OMARCHY_NOTES_THEME_DIR") {
        dirs.push(PathBuf::from(path));
    }
    if let Some(home) = std::env::var_os("HOME") {
        let home = PathBuf::from(home);
        let state = std::env::var_os("XDG_STATE_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| home.join(".local/state"));
        let config = std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| home.join(".config"));
        dirs.push(state.join("omarchy/current/theme"));
        dirs.push(config.join("omarchy/current/theme"));
    }
    dirs
}

// Preview support for Omarchy's simple top-level colors.toml; no desktop files are written.
pub fn read_omarchy(dirs: &[PathBuf]) -> Theme {
    dirs.iter()
        .find_map(|dir| {
            let path = dir.join("colors.toml");
            if std::fs::metadata(&path).ok()?.len() > 65536 {
                return None;
            }
            let text = std::fs::read_to_string(path).ok()?;
            palette(&text)
        })
        .unwrap_or_else(Theme::reference)
}

fn palette(text: &str) -> Option<Theme> {
    let color = |key: &str| {
        text.lines().find_map(|line| {
            let (name, value) = line.split_once('=')?;
            if name.trim() != key {
                return None;
            }
            let value = value.trim();
            let quote = value.chars().next()?;
            if quote != '"' && quote != '\'' {
                return None;
            }
            let value = value[1..]
                .split(quote)
                .next()?
                .trim_start_matches('#')
                .trim_start_matches("0x");
            if value.len() != 6 {
                return None;
            }
            u32::from_str_radix(value, 16).ok()
        })
    };
    Some(Theme::derive(
        color("background")?,
        color("foreground")?,
        color("accent").unwrap_or(0xe7ca84),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn palette_accepts_comments_and_rejects_incomplete_input() {
        assert!(palette("background = '#ffffff' # paper\nforeground = \"#111111\"").is_some());
        assert!(palette("background = '#12345'").is_none());
    }
    #[test]
    fn unreadable_theme_inputs_are_corrected() {
        for bg in [0x202225, 0xffffff, 0x777777, 0x000000] {
            assert!(contrast(readable(bg, bg, 4.5), bg) >= 4.5);
        }
    }
}
