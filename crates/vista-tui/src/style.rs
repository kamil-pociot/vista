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

/// A token style from the renderer as a ratatui style.
pub fn tok(theme: &Theme, s: &vista_render::TokStyle) -> Style {
    let mut style = Style::new().fg(color(theme.color(s.fg)));
    if let Some(bg) = s.bg {
        style = style.bg(color(theme.color(bg)));
    }
    let mut m = Modifier::empty();
    if s.bold {
        m |= Modifier::BOLD;
    }
    if s.italic {
        m |= Modifier::ITALIC;
    }
    if s.underline {
        m |= Modifier::UNDERLINED;
    }
    if s.strike {
        m |= Modifier::CROSSED_OUT;
    }
    style.add_modifier(m)
}

/// A renderer line as a ratatui line.
pub fn styled_line(theme: &Theme, line: &vista_render::StyledLine) -> ratatui::text::Line<'static> {
    ratatui::text::Line::from(
        line.iter()
            .map(|s| ratatui::text::Span::styled(s.text.clone(), tok(theme, &s.style)))
            .collect::<Vec<_>>(),
    )
}
