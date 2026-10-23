//! Helpers for render-snapshot tests: draw the app into an in-memory terminal
//! and turn the result into reviewable text, including colours as theme tokens.

use std::fmt::Write as _;
use std::path::Path;

use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;
use ratatui::style::Modifier;
use vista_core::{Config, Paths};
use vista_theme::Theme;

use crate::app::{App, AppOptions};
use crate::style::rgb_of;
use crate::ui;

/// An app rooted at `root`, with config and caches under `home` so the
/// developer's own configuration never leaks into tests.
pub fn app_at(root: &Path, home: &Path, theme: Option<&str>) -> App {
    let paths = Paths {
        config_dir: home.join("config"),
        cache_dir: home.join("cache"),
        log_file: home.join("vista.log"),
    };
    App::new(AppOptions {
        events: None,
        ui: None,
        root: root.to_path_buf(),
        config: Config::default(),
        paths,
        theme: theme.map(str::to_string),
    })
    .expect("test app")
}

pub fn render(app: &mut App, width: u16, height: u16) -> Buffer {
    let mut term = Terminal::new(TestBackend::new(width, height)).expect("test terminal");
    term.draw(|f| ui::draw(f, f.area(), app)).expect("draw");
    term.backend().buffer().clone()
}

/// The buffer's text, one line per row, trailing spaces trimmed.
pub fn text(buf: &Buffer) -> String {
    let mut out = String::new();
    for y in 0..buf.area.height {
        let mut line = String::new();
        for x in 0..buf.area.width {
            line.push_str(buf[(x, y)].symbol());
        }
        out.push_str(line.trim_end());
        out.push('\n');
    }
    out
}

fn token(theme: &Theme, c: ratatui::style::Color) -> String {
    match rgb_of(c) {
        Some(rgb) => theme.token_for(rgb).map(str::to_string).unwrap_or_else(|| rgb.to_hex()),
        None => "-".to_string(),
    }
}

/// Text plus, per row, runs of cells whose style differs from the base
/// (`ui.text` on `ui.bg`), written as theme tokens:
/// `  3: 0..12 fg=ui.text.strong bg=ui.panel +bold`.
pub fn snapshot(buf: &Buffer, theme: &Theme) -> String {
    let mut out = text(buf);
    out.push_str("--- styles ---\n");
    for y in 0..buf.area.height {
        let mut runs: Vec<(u16, u16, String)> = Vec::new();
        for x in 0..buf.area.width {
            let cell = &buf[(x, y)];
            let mut desc = format!("fg={} bg={}", token(theme, cell.fg), token(theme, cell.bg));
            for (m, name) in [
                (Modifier::BOLD, "bold"),
                (Modifier::ITALIC, "italic"),
                (Modifier::UNDERLINED, "underline"),
                (Modifier::REVERSED, "reversed"),
            ] {
                if cell.modifier.contains(m) {
                    desc.push_str(" +");
                    desc.push_str(name);
                }
            }
            match runs.last_mut() {
                Some((_, end, d)) if *d == desc => *end = x + 1,
                _ => runs.push((x, x + 1, desc)),
            }
        }
        let base = "fg=ui.text bg=ui.bg";
        let interesting: Vec<_> = runs.iter().filter(|(_, _, d)| d != base).collect();
        if interesting.is_empty() {
            continue;
        }
        let _ = write!(out, "{y:>3}:");
        for (s, e, d) in interesting {
            let _ = write!(out, " [{s}..{e} {d}]");
        }
        out.push('\n');
    }
    out
}
