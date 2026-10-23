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
    let focused = app.focus == Focus::Left;
    // A speed search lasts while the tree has the keyboard; it takes the
    // bottom row for what was typed.
    if !focused {
        app.project.search = None;
    }
    let searching = app.project.search.is_some() && area.height > 1;
    let rows_h = area.height - u16::from(searching);
    app.project.view_height = rows_h as usize;
    app.project.follow_selection();
    let t = &app.theme;
    let letters = t.chrome.git_indicator == GitIndicator::ColourLetter;
    let pv = &app.project;
    let mut lines = Vec::with_capacity(area.height as usize);
    for (i, row) in pv.rows.iter().enumerate().skip(pv.scroll).take(rows_h as usize) {
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
        let mut spans = vec![Span::styled(format!(" {indent}{arrow}"), t.fg("ui.dim"))];
        match pv.search_match(i).filter(|m| !m.is_empty()) {
            // Matched letters stand out, like IntelliJ's speed search.
            Some(hits) => {
                let hit = t.fg_bg("ui.match.text", "ui.match");
                for (k, c) in row.name.chars().enumerate() {
                    let style = if hits.contains(&k) { hit } else { name_style };
                    spans.push(Span::styled(c.to_string(), style));
                }
            }
            None => spans.push(Span::styled(row.name.clone(), name_style)),
        }
        if app.dirty.contains(&row.path) {
            spans.push(Span::styled(" ●", t.fg("ui.warning")));
        }
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
    f.render_widget(Paragraph::new(lines), Rect { height: rows_h, ..area });
    if let (true, Some(query)) = (searching, &pv.search) {
        let bar = Rect {
            y: area.y + rows_h,
            height: 1,
            ..area
        };
        let count = (0..pv.rows.len()).filter(|&i| pv.search_match(i).is_some()).count();
        let (query_style, right) = match count {
            0 => (t.fg("ui.error"), "no matches ".to_string()),
            1 => (t.fg("ui.text.strong"), "1 match ".to_string()),
            n => (t.fg("ui.text.strong"), format!("{n} matches ")),
        };
        f.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled(" ⌕ ", t.fg("ui.dim")),
                Span::styled(query.clone(), query_style),
                Span::styled("▏", t.fg("ui.text")),
            ]))
            .style(t.bg("ui.input")),
            bar,
        );
        f.render_widget(Paragraph::new(Span::styled(right, t.fg("ui.dim"))).right_aligned(), bar);
    }
}
