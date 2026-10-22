//! Drawing the Git tool window: commits on the left, the selected commit's
//! files on the right.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Modifier;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Paragraph};
use unicode_width::UnicodeWidthStr;

use crate::app::{App, Focus};
use crate::style::ThemeStyle;

/// Cuts `s` to `w` columns (with … when cut).
fn fit(s: &str, w: usize) -> String {
    if s.width() <= w {
        return s.to_string();
    }
    let mut out = String::new();
    for c in s.chars() {
        if out.width() + 2 > w {
            break;
        }
        out.push(c);
    }
    out.push('…');
    out
}

pub fn draw(f: &mut Frame, area: Rect, app: &mut App) {
    let t = &app.theme;
    let focused = app.focus == Focus::Bottom;
    f.render_widget(Block::new().style(t.bg("ui.panel.alt")), area);
    let w = &app.log;
    // Header: what's shown, and the keys.
    let title = match &w.path {
        Some(p) => format!(
            " 9: Git · History of {}",
            p.strip_prefix(&app.root).unwrap_or(p).display()
        ),
        None => " 9: Git · Log".to_string(),
    };
    let count = if w.loading {
        "  loading…".to_string()
    } else {
        format!("  {} commits", w.commits.len())
    };
    let header = Rect { height: 1, ..area };
    let title_style = if focused {
        t.fg_bg("ui.text.strong", "ui.selection.inactive")
            .add_modifier(Modifier::BOLD)
    } else {
        t.fg("ui.header").add_modifier(Modifier::BOLD)
    };
    f.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(title, title_style),
            Span::styled(count, t.fg("ui.dim")),
        ]))
        .style(t.bg("ui.panel")),
        header,
    );
    let hint = if w.path.is_some() {
        "↑↓ commits · Tab files · ⏎ diff · a all commits · r refresh · Esc "
    } else {
        "↑↓ commits · Tab files · ⏎ diff · r refresh · Esc "
    };
    f.render_widget(
        Paragraph::new(Span::styled(hint, t.fg("ui.dim"))).right_aligned(),
        header,
    );
    let body = Rect {
        y: area.y + 1,
        height: area.height.saturating_sub(1),
        ..area
    };
    let left_w = body.width * 62 / 100;
    let commits_area = Rect { width: left_w, ..body };
    let files_area = Rect {
        x: body.x + left_w + 1,
        width: body.width.saturating_sub(left_w + 1),
        ..body
    };
    let sep = Rect {
        x: body.x + left_w,
        width: 1,
        ..body
    };
    f.render_widget(
        Paragraph::new(vec![Line::styled("│", t.fg("ui.border")); body.height as usize]),
        sep,
    );

    // Commits.
    let mut lines = Vec::new();
    if let Some(e) = &w.error {
        lines.push(Line::styled(format!(" {e}"), t.fg("ui.error")));
    } else if w.commits.is_empty() && !w.loading {
        lines.push(Line::styled(" No commits", t.fg("ui.dim")));
    }
    let cw = commits_area.width as usize;
    for (i, c) in w.commits.iter().enumerate().skip(w.scroll).take(body.height as usize) {
        let right = format!("  {}  {} ", c.author, c.date);
        let refs = if c.refs.is_empty() {
            String::new()
        } else {
            format!("({}) ", c.refs)
        };
        let room = cw.saturating_sub(c.short.width() + 2 + right.width());
        let refs = fit(&refs, room);
        let subject = fit(&c.subject, room.saturating_sub(refs.width()));
        let pad = room.saturating_sub(refs.width() + subject.width());
        let mut line = Line::from(vec![
            Span::styled(format!(" {} ", c.short), t.fg("ui.dim")),
            Span::styled(refs, t.fg("ui.accent")),
            Span::styled(subject, t.fg("ui.text")),
            Span::raw(" ".repeat(pad)),
            Span::styled(right, t.fg("ui.dim")),
        ]);
        if i == w.selected {
            line = line.style(if focused && !w.on_files {
                t.bg("ui.selection")
            } else {
                t.bg("ui.selection.inactive")
            });
        }
        lines.push(line);
    }
    f.render_widget(Paragraph::new(lines), commits_area);

    // Files of the selected commit.
    let files = w.current_files();
    let mut lines = Vec::new();
    match w.selected_commit().and_then(|c| w.files.get(&c.hash)) {
        None if w.selected_commit().is_some() => lines.push(Line::styled(" loading…", t.fg("ui.dim"))),
        Some(Err(e)) => lines.push(Line::styled(format!(" {e}"), t.fg("ui.error"))),
        _ => {}
    }
    for (i, file) in files.iter().enumerate().skip(w.file_scroll).take(body.height as usize) {
        let tok = match file.status {
            'A' => "git.added",
            'D' => "git.deleted",
            'R' | 'C' => "git.renamed",
            _ => "git.modified",
        };
        let rel = file.path.strip_prefix(&app.root).unwrap_or(&file.path);
        let mut spans = vec![
            Span::styled(format!(" {} ", file.status), t.fg(tok).add_modifier(Modifier::BOLD)),
            Span::styled(rel.display().to_string(), t.fg(tok)),
        ];
        if let Some(old) = &file.old_path {
            let old = old.strip_prefix(&app.root).unwrap_or(old);
            spans.push(Span::styled(format!("  ← {}", old.display()), t.fg("ui.dim")));
        }
        let mut line = Line::from(spans);
        if i == w.file_selected {
            line = line.style(if focused && w.on_files {
                t.bg("ui.selection")
            } else {
                t.bg("ui.selection.inactive")
            });
        }
        lines.push(line);
    }
    f.render_widget(Paragraph::new(lines), files_area);
    app.log.view_height = body.height as usize;
    app.areas.log_commits = Some(commits_area);
    app.areas.log_files = Some(files_area);
}
