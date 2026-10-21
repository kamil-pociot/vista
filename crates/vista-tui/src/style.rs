//! The bridge from theme tokens to ratatui styles.
//!
//! This is the only module allowed to construct `Color` values; every widget
//! asks for a token. `tests/lints.rs` enforces that.

use ratatui::style::{Color, Modifier, Style};
use vista_theme::{Rgb, Theme};

pub fn color(rgb: Rgb) -> Color {
    Color::Rgb(rgb.0, rgb.1, rgb.2)
}

pub fn rgb_of(color: Color) -> Option<Rgb> {
    match color {
        Color::Rgb(r, g, b) => Some(Rgb(r, g, b)),
        _ => None,
    }
}

/// Token-based style helpers on [`Theme`].
pub trait ThemeStyle {
    fn c(&self, token: &str) -> Color;
    /// Foreground only.
    fn fg(&self, token: &str) -> Style;
    /// Background only.
    fn bg(&self, token: &str) -> Style;
    fn fg_bg(&self, fg: &str, bg: &str) -> Style;
    /// Base style of the app: `ui.text` on `ui.bg`.
    fn base(&self) -> Style;
}

impl ThemeStyle for Theme {
    fn c(&self, token: &str) -> Color {
        color(self.color(token))
    }
    fn fg(&self, token: &str) -> Style {
        Style::new().fg(self.c(token))
    }
    fn bg(&self, token: &str) -> Style {
        Style::new().bg(self.c(token))
    }
    fn fg_bg(&self, fg: &str, bg: &str) -> Style {
        Style::new().fg(self.c(fg)).bg(self.c(bg))
    }
    fn base(&self) -> Style {
        self.fg_bg("ui.text", "ui.bg")
    }
}

pub fn bold() -> Modifier {
    Modifier::BOLD
}
