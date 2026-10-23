//! Drawing the Review panel.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Modifier;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use unicode_width::UnicodeWidthStr;

use crate::app::App;
use crate::review::{Row, status_of};
use crate::style::ThemeStyle;

pub fn draw(f: &mut Frame, area: Rect, app: &mut App) {
    let focused = app.focus == crate::app::Focus::Left;
    let root = app.root.clone();
    let t = app.theme.clone();
    let t = &t;
    let Some(v) = app.review.as_mut() else { return };
    if area.height < 3 {
        return;
    }
    // Header: what's reviewed, then the counts.
    let n = v.review.files.len();
    let mut counts = vec![
        Span::styled(" Review · ", t.fg("ui.dim")),
        Span::styled(
            format!("{n} file{}", if n == 1 { "" } else { "s" }),
            t.fg("ui.header").add_modifier(Modifier::BOLD),
        ),
        Span::styled(format!(" since {}", v.review.short), t.fg("ui.dim")),
    ];
    let k = v.review.commits;
    let commits = (k > 0).then(|| format!(" · {k} commit{}", if k == 1 { "" } else { "s" }));
    let used: usize = counts.iter().map(|s| s.content.width()).sum();
    let mut header = Vec::new();
    match commits {
        // On the same line when it fits, else below.
        Some(c) if used + c.width() <= area.width as usize => {
            counts.push(Span::styled(c, t.fg("ui.dim")));
            header.push(Line::from(counts));
        }
        Some(c) => {
            header.push(Line::from(counts));
            header.push(Line::styled(
                format!(" {}", c.trim_start_matches(" · ")),
                t.fg("ui.dim"),
            ));
        }
        None => header.push(Line::from(counts)),
    }
    if let Some(label) = &v.label {
        header.insert(0, Line::styled(format!(" {label}"), t.fg("ui.text.strong")));
    }
    let header_h = header.len() as u16;
    f.render_widget(
        Paragraph::new(header),
        Rect {
            height: header_h,
            ..area
        },
    );
    let list = Rect {
        y: area.y + header_h,
        height: area.height.saturating_sub(header_h).max(1),
        ..area
    };
    v.view_height = list.height as usize;
    v.follow_selection();
    let v = &*v;
    let mut lines = Vec::new();
    for (i, row) in v.rows.iter().enumerate().skip(v.scroll).take(list.height as usize) {
        let mut spans = Vec::new();
        match row {
            Row::Dir(rel) => {
                spans.push(Span::styled(format!(" {}", rel.display()), t.fg("ui.dim")));
            }
            Row::File { index, depth } => {
                let file = &v.review.files[*index];
                let rel = file.path.strip_prefix(&root).unwrap_or(&file.path);
                let name = rel
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default();
                let dir = rel.parent().map(|d| d.display().to_string()).unwrap_or_default();
                spans.push(Span::raw(" ".repeat(1 + depth * 2)));
                let status = status_of(file);
                let mut name_style = t.fg(status.token());
                if status == vista_git::FileStatus::Deleted {
                    name_style = name_style.add_modifier(Modifier::CROSSED_OUT);
                }
                spans.push(Span::styled(name, name_style));
                if let Some(old) = &file.old_path {
                    let o = old.strip_prefix(&root).unwrap_or(old);
                    spans.push(Span::styled(format!(" ← {}", o.display()), t.fg("ui.dim")));
                }
                let used: usize = spans.iter().map(|s| s.content.width()).sum();
                // The folder goes right-aligned when it fits.
                if !v.by_dir && !dir.is_empty() && used + dir.width() + 3 <= list.width as usize {
                    let pad = (list.width as usize).saturating_sub(used + dir.width() + 1);
                    spans.push(Span::raw(" ".repeat(pad)));
                    spans.push(Span::styled(dir, t.fg("ui.dim")));
                }
            }
        }
        let used: usize = spans.iter().map(|s| s.content.width()).sum();
        spans.push(Span::raw(" ".repeat((list.width as usize).saturating_sub(used))));
        let mut line = Line::from(spans);
        if i == v.selected {
            line = line.style(t.bg(if focused {
                "ui.selection"
            } else {
                "ui.selection.inactive"
            }));
        }
        lines.push(line);
    }
    if v.rows.is_empty() {
        lines.push(Line::styled(" Nothing changed since the base", t.fg("ui.dim")));
    }
    f.render_widget(Paragraph::new(lines), list);
    app.areas.review_list = Some(list);
}
