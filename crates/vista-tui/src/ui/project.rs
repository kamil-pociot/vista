//! Drawing the Project tree.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Modifier;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use unicode_width::UnicodeWidthStr;
use vista_git::FileStatus;
use vista_theme::GitIndicator;

use crate::app::{App, Focus};
use crate::style::ThemeStyle;

/// Token and indicator for a tree row.
pub fn row_status(app: &App, path: &std::path::Path, is_dir: bool) -> Option<FileStatus> {
    if is_dir {
        let s = app.status.dir_status(path)?;
        (s == FileStatus::Ignored || app.config.tree.folder_colours).then_some(s)
    } else {
        app.status.file_status(path)
    }
}

pub fn draw(f: &mut Frame, area: Rect, app: &mut App) {
    app.project.view_height = area.height as usize;
    app.project.scroll_into_view();
    let t = &app.theme;
    let focused = app.focus == Focus::Left;
    let letters = t.chrome.git_indicator == GitIndicator::ColourLetter;
    let pv = &app.project;
    let mut lines = Vec::with_capacity(area.height as usize);
    for (i, row) in pv.rows.iter().enumerate().skip(pv.scroll).take(area.height as usize) {
        let status = row_status(app, &row.path, row.is_dir);
        let token = status.map(FileStatus::token).unwrap_or("ui.text");
        let mut name_style = t.fg(token);
        if row.depth == 0 {
            name_style = name_style.add_modifier(Modifier::BOLD);
        }
        if status == Some(FileStatus::Ignored) {
            name_style = name_style.add_modifier(Modifier::ITALIC);
        }
        let arrow = match (row.is_dir, row.expanded) {
            (true, true) => "▾ ",
            (true, false) => "▸ ",
            _ => "  ",
        };
        let indent = "  ".repeat(row.depth);
        let mut spans = vec![
            Span::styled(format!(" {indent}{arrow}"), t.fg("ui.dim")),
            Span::styled(row.name.clone(), name_style),
        ];
        if letters && let Some(s) = status.filter(|s| *s != FileStatus::Ignored) {
            let used = 1 + indent.width() + 2 + row.name.width();
            let letter = if row.is_dir { '•' } else { s.letter() };
            let pad = (area.width as usize).saturating_sub(used + 2);
            spans.push(Span::raw(" ".repeat(pad)));
            spans.push(Span::styled(format!("{letter} "), t.fg(token)));
        }
        let used: usize = spans.iter().map(|s| s.content.width()).sum();
        spans.push(Span::raw(" ".repeat((area.width as usize).saturating_sub(used))));
        let mut line = Line::from(spans);
        if i == pv.selected {
            let bg = if focused {
                "ui.selection"
            } else {
                "ui.selection.inactive"
            };
            line = line.style(t.bg(bg));
        }
        lines.push(line);
    }
    f.render_widget(Paragraph::new(lines), area);
}
