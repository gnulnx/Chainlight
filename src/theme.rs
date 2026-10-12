use ratatui::style::Color;
use std::{fs, path::Path};

/// Terminal ANSI colors are the portable default; an Omarchy palette is opt-in.
#[derive(Debug, Clone)]
pub struct Palette {
    pub accent: Color,
    pub foreground: Color,
    pub background: Color,
    pub muted: Color,
    pub green: Color,
    pub yellow: Color,
    pub red: Color,
}
impl Default for Palette {
    fn default() -> Self {
        Self {
            accent: Color::Indexed(3),
            foreground: Color::Indexed(7),
            background: Color::Reset,
            muted: Color::Indexed(8),
            green: Color::Indexed(2),
            yellow: Color::Indexed(3),
            red: Color::Indexed(1),
        }
    }
}
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ThemeChoice {
    #[default]
    System,
    Dark,
    Light,
    Ember,
    Aurora,
    Ocean,
}
impl ThemeChoice {
    pub fn next(self) -> Self {
        match self {
            Self::System => Self::Dark,
            Self::Dark => Self::Light,
            Self::Light => Self::Ember,
            Self::Ember => Self::Aurora,
            Self::Aurora => Self::Ocean,
            Self::Ocean => Self::System,
        }
    }
    pub fn name(self) -> &'static str {
        match self {
            Self::System => "System",
            Self::Dark => "Dark",
            Self::Light => "Light",
            Self::Ember => "Ember",
            Self::Aurora => "Aurora",
            Self::Ocean => "Ocean",
        }
    }
    pub fn palette(self, path: Option<&Path>) -> Palette {
        if self == Self::System {
            return Palette::load(path);
        }
        let colors = match self {
            Self::Dark => [
                "#161821", "#e8eaf2", "#959cb4", "#83a6ff", "#a6d879", "#e6b56e", "#ef8087",
            ],
            Self::Light => [
                "#f5f2e9", "#252b38", "#636879", "#2856a6", "#39752e", "#935900", "#ae3440",
            ],
            Self::Ember => [
                "#211815", "#f3e5d2", "#bca590", "#f7a054", "#dfc177", "#ea826e", "#ff7272",
            ],
            Self::Aurora => [
                "#171b28", "#e4edf6", "#99abc4", "#7ed8c4", "#a5b6ff", "#e8a5d8", "#f58b9c",
            ],
            Self::Ocean => [
                "#0e202b", "#e3f0f2", "#94b4bf", "#67c8ea", "#82dab4", "#e7cf81", "#f28b86",
            ],
            Self::System => unreachable!(),
        };
        let [background, foreground, muted, accent, green, yellow, red] =
            colors.map(|c| parse_color(c).unwrap());
        Palette {
            background,
            foreground,
            muted,
            accent,
            green,
            yellow,
            red,
        }
        .ensure_readable()
    }
}
impl Palette {
    pub fn load(path: Option<&Path>) -> Self {
        let mut palette = Self::default();
        let Some(contents) = path.and_then(|p| fs::read_to_string(p).ok()) else {
            return palette;
        };
        for line in contents.lines() {
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            let Some(color) = parse_color(value.trim().trim_matches(['\'', '"'])) else {
                continue;
            };
            match key.trim() {
                "accent" => palette.accent = color,
                "foreground" => palette.foreground = color,
                "background" => palette.background = color,
                "muted" => palette.muted = color,
                "green" => palette.green = color,
                "yellow" => palette.yellow = color,
                "red" => palette.red = color,
                _ => {}
            }
        }
        palette.ensure_readable()
    }
    fn ensure_readable(mut self) -> Self {
        if let Color::Rgb(..) = self.background {
            self.foreground = readable(self.background, self.foreground, self.foreground);
            self.muted = readable(self.background, self.muted, self.foreground);
            self.accent = readable(self.background, self.accent, self.foreground);
            self.green = readable(self.background, self.green, self.foreground);
            self.yellow = readable(self.background, self.yellow, self.foreground);
            self.red = readable(self.background, self.red, self.foreground);
        }
        self
    }
}

fn parse_color(s: &str) -> Option<Color> {
    if s.len() != 7 || !s.starts_with('#') || !s[1..].bytes().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    let value = u32::from_str_radix(&s[1..], 16).ok()?;
    Some(Color::Rgb(
        (value >> 16) as u8,
        (value >> 8) as u8,
        value as u8,
    ))
}
fn luminance(color: Color) -> f64 {
    let Color::Rgb(r, g, b) = color else {
        return 0.0;
    };
    let linear = |v: u8| {
        let x = f64::from(v) / 255.0;
        if x <= 0.04045 {
            x / 12.92
        } else {
            ((x + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * linear(r) + 0.7152 * linear(g) + 0.0722 * linear(b)
}
fn contrast(a: Color, b: Color) -> f64 {
    let (x, y) = (luminance(a), luminance(b));
    (x.max(y) + 0.05) / (x.min(y) + 0.05)
}
fn readable(bg: Color, dim: Color, fg: Color) -> Color {
    let Color::Rgb(r, g, b) = bg else { return dim };
    let fallback = if luminance(Color::Rgb(r, g, b)) > 0.179 {
        Color::Rgb(0, 0, 0)
    } else {
        Color::Rgb(255, 255, 255)
    };
    let target = if matches!(fg, Color::Rgb(..)) && contrast(bg, fg) >= 4.5 {
        fg
    } else {
        fallback
    };
    let start = if matches!(dim, Color::Rgb(..)) {
        dim
    } else {
        target
    };
    let (Color::Rgb(sr, sg, sb), Color::Rgb(tr, tg, tb)) = (start, target) else {
        return target;
    };
    for step in 0..=100 {
        let mix = |x: u8, y: u8| {
            (f64::from(x) + (f64::from(y) - f64::from(x)) * f64::from(step) / 100.0).round() as u8
        };
        let color = Color::Rgb(mix(sr, tr), mix(sg, tg), mix(sb, tb));
        if contrast(bg, color) >= 4.5 {
            return color;
        }
    }
    target
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn built_in_palettes_cycle_and_keep_all_text_readable() {
        let mut choice = ThemeChoice::System;
        for _ in 0..5 {
            choice = choice.next();
            let p = choice.palette(None);
            for color in [p.foreground, p.muted, p.accent, p.green, p.yellow, p.red] {
                assert!(contrast(p.background, color) >= 4.5, "{}", choice.name());
            }
            assert_ne!(p.accent, p.green);
            assert_ne!(p.green, p.yellow);
        }
        assert_eq!(choice.next(), ThemeChoice::System);
        assert_eq!(ThemeChoice::System.palette(None).background, Color::Reset);
    }

    #[test]
    fn default_inherits_terminal_background() {
        assert_eq!(Palette::load(None).background, Color::Reset);
    }
    #[test]
    fn dark_and_light_muted_remain_readable() {
        for bg in [Color::Rgb(20, 20, 20), Color::Rgb(245, 245, 245)] {
            let fixed = readable(bg, Color::Rgb(120, 120, 120), Color::Rgb(120, 120, 120));
            assert!(contrast(bg, fixed) >= 4.5);
        }
    }
    #[test]
    fn light_palette_keeps_pixel_and_status_colors_readable() {
        let file = tempfile::NamedTempFile::new().unwrap();
        fs::write(file.path(), "background = '#ffffff'\nforeground = '#222222'\nmuted = '#eeeeee'\naccent = '#aabbff'\ngreen = '#bbffaa'\nyellow = '#ffddaa'\nred = '#ffbbbb'\n").unwrap();
        let p = Palette::load(Some(file.path()));
        for color in [p.foreground, p.muted, p.accent, p.green, p.yellow, p.red] {
            assert!(contrast(p.background, color) >= 4.5);
        }
        assert_ne!(p.accent, p.green);
        assert_ne!(p.green, p.yellow);
    }

    #[test]
    fn reload_palette_and_ignore_escape_sequences() {
        let file = tempfile::NamedTempFile::new().unwrap();
        fs::write(file.path(),"background = '#ffffff'\nforeground = '#222222'\nmuted = '#aaaaaa'\naccent = '\u{1b}[31m'\n").unwrap();
        let light = Palette::load(Some(file.path()));
        assert!(contrast(light.background, light.muted) >= 4.5);
        assert!(contrast(light.background, light.accent) >= 4.5);
        fs::write(file.path(),"background = '#000000'\nforeground = '#ffffff'\nmuted = '#111111'\naccent = '#f7931a'\n").unwrap();
        let dark = Palette::load(Some(file.path()));
        assert!(contrast(dark.background, dark.muted) >= 4.5);
        assert_eq!(dark.accent, Color::Rgb(247, 147, 26));
    }
}
