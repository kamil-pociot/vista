//! Drawing the Commit tool window and the confirmation dialog.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Modifier;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap};
use unicode_width::UnicodeWidthStr;

use crate::app::App;
use crate::commit::Row;
use crate::style::ThemeStyle;

fn checkbox(state: Option<bool>) -> &'static str {
    match state {
        Some(true) => "[x] ",
        Some(false) => "[ ] ",
        None => "[~] ",
    }
}

pub fn draw(f: &mut Frame, area: Rect, app: &mut App) {
    let t = app.theme.clone();
    let t = &t;
    let focused = app.focus == crate::app::Focus::Left;
    if area.height < 6 {
        return;
    }
    let cv = &mut app.commit;
    // Toolbar.
    f.render_widget(
        Paragraph::new(Span::styled(
            " ␣ tick  r rollback  a add  g group  m amend",
            t.fg("ui.dim"),
        )),
        Rect { height: 1, ..area },
    );
    // Rows the (wrapped) commit output needs.
    let wrap_w = area.width.saturating_sub(2).max(1) as usize;
    let output_h = match &cv.output {
        Some((_, text)) => (text.lines().map(|l| l.width().div_ceil(wrap_w).max(1)).sum::<usize>() as u16).clamp(1, 6),
        None => 0,
    };
    let footer_h = 9 + output_h;
    let list = Rect {
        y: area.y + 1,
        height: area.height.saturating_sub(1 + footer_h).max(1),
        ..area
    };
    cv.view_height = list.height as usize;
    if cv.selected < cv.scroll {
        cv.scroll = cv.selected;
    } else if cv.selected >= cv.scroll + cv.view_height.max(1) {
        cv.scroll = cv.selected + 1 - cv.view_height.max(1);
    }
    let cv = &app.commit;
    let mut lines = Vec::new();
    for (i, row) in cv.rows.iter().enumerate().skip(cv.scroll).take(list.height as usize) {
        let mut spans = Vec::new();
        match row {
            Row::Group(g) => {
                let n = app
                    .status
                    .changes()
                    .iter()
                    .filter(|c| {
                        (c.status == vista_git::FileStatus::Unversioned) == (*g == crate::commit::Group::Unversioned)
                    })
                    .filter(|c| c.status != vista_git::FileStatus::Ignored)
                    .count();
                let arrow = if cv.collapsed.contains(g) { "▸ " } else { "▾ " };
                spans.push(Span::styled(format!(" {arrow}"), t.fg("ui.dim")));
                spans.push(Span::styled(checkbox(cv.tick_state(i, &app.status)), t.fg("ui.text")));
                spans.push(Span::styled(
                    g.label().to_string(),
                    t.fg("ui.header").add_modifier(Modifier::BOLD),
                ));
                spans.push(Span::styled(
                    format!("  {n} file{}", if n == 1 { "" } else { "s" }),
                    t.fg("ui.dim"),
                ));
            }
            Row::Dir { rel, .. } => {
                spans.push(Span::styled("   ", t.fg("ui.dim")));
                spans.push(Span::styled(checkbox(cv.tick_state(i, &app.status)), t.fg("ui.text")));
                spans.push(Span::styled(rel.clone(), t.fg("ui.dim")));
            }
            Row::File { change, depth } => {
                let rel = change.path.strip_prefix(&app.root).unwrap_or(&change.path);
                let name = rel
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default();
                let dir = rel.parent().map(|d| d.display().to_string()).unwrap_or_default();
                let ticked = cv.included.contains(&change.path);
                spans.push(Span::raw(" ".repeat(1 + depth * 2)));
                spans.push(Span::styled(checkbox(Some(ticked)), t.fg("ui.text")));
                let mut name_style = t.fg(change.status.token());
                if change.status == vista_git::FileStatus::Deleted {
                    name_style = name_style.add_modifier(Modifier::CROSSED_OUT);
                }
                spans.push(Span::styled(name, name_style));
                if let Some(orig) = &change.orig_path {
                    let o = orig.strip_prefix(&app.root).unwrap_or(orig);
                    spans.push(Span::styled(format!(" ← {}", o.display()), t.fg("ui.dim")));
                }
                let used: usize = spans.iter().map(|s| s.content.width()).sum();
                // The folder goes right-aligned when it fits.
                if !cv.by_dir && !dir.is_empty() && used + dir.width() + 3 <= list.width as usize {
                    let pad = (list.width as usize).saturating_sub(used + dir.width() + 1);
                    spans.push(Span::raw(" ".repeat(pad)));
                    spans.push(Span::styled(dir, t.fg("ui.dim")));
                }
            }
        }
        let used: usize = spans.iter().map(|s| s.content.width()).sum();
        spans.push(Span::raw(" ".repeat((list.width as usize).saturating_sub(used))));
        let mut line = Line::from(spans);
        if i == cv.selected && !cv.editing_message {
            line = line.style(t.bg(if focused {
                "ui.selection"
            } else {
                "ui.selection.inactive"
            }));
        }
        lines.push(line);
    }
    if cv.rows.is_empty() {
        let msg = if app.git.is_some() {
            " No changes"
        } else {
            " Not a git repository"
        };
        lines.push(Line::styled(msg, t.fg("ui.dim")));
    }
    f.render_widget(Paragraph::new(lines), list);
    app.areas.commit_list = Some(list);

    let mut y = list.y + list.height;
    if let Some((ok, text)) = &cv.output {
        let r = Rect {
            y,
            height: output_h,
            ..area
        };
        let token = if *ok { "ui.success" } else { "ui.error" };
        f.render_widget(
            Paragraph::new(text.clone())
                .style(t.fg(token))
                .wrap(Wrap { trim: false }),
            Rect {
                x: r.x + 1,
                width: r.width.saturating_sub(2),
                ..r
            },
        );
        y += output_h;
    }

    // Message box.
    let msg_rect = Rect {
        x: area.x + 1,
        y,
        width: area.width.saturating_sub(2),
        height: 7,
    };
    let border = if cv.editing_message && focused {
        "ui.border.focused"
    } else {
        "ui.border"
    };
    let block = Block::new()
        .borders(Borders::ALL)
        .border_style(t.fg(border))
        .title(Span::styled(" Commit message ", t.fg("ui.dim")))
        .style(t.bg("ui.input"));
    let inner = block.inner(msg_rect);
    f.render_widget(block, msg_rect);
    let mut text = cv.message.text.clone();
    if cv.editing_message && focused {
        text.insert(cv.message.cursor, '▏');
    }
    if cv.message.text.is_empty() && !(cv.editing_message && focused) {
        f.render_widget(
            Paragraph::new(Span::styled("Tab to write a message", t.fg("ui.dim"))),
            inner,
        );
    } else {
        // Keep the cursor line visible.
        let cursor_line = cv.message.text[..cv.message.cursor].matches('\n').count() as u16;
        let scroll = cursor_line.saturating_sub(inner.height.saturating_sub(1));
        f.render_widget(
            Paragraph::new(text)
                .style(t.fg("ui.text"))
                .wrap(Wrap { trim: false })
                .scroll((scroll, 0)),
            inner,
        );
    }
    app.areas.commit_message = Some(msg_rect);

    // Amend + buttons.
    let row = Rect {
        x: area.x + 1,
        y: msg_rect.y + msg_rect.height,
        width: area.width.saturating_sub(2),
        height: 1,
    };
    let included = cv.included.len();
    let total = cv.total(&app.status);
    f.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(if cv.amend { "[x] Amend" } else { "[ ] Amend" }, t.fg("ui.text")),
            Span::styled(format!("   {included} of {total} files"), t.fg("ui.dim")),
        ])),
        row,
    );
    let buttons = Rect { y: row.y + 1, ..row };
    let label = if cv.busy { " Committing… " } else { " Commit ^K " };
    f.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(
                label,
                t.fg_bg("ui.button.text", "ui.button").add_modifier(Modifier::BOLD),
            ),
            Span::raw("  "),
            Span::styled(" Commit & Push (P) ", t.fg_bg("ui.text", "ui.panel")),
        ])),
        buttons,
    );
}

/// A yes/no dialog over the whole screen.
pub fn draw_confirm(f: &mut Frame, area: Rect, app: &App) {
    let Some(c) = &app.confirm else { return };
    let t = &app.theme;
    let w = (c.text.width() as u16 + 6).clamp(30, area.width.saturating_sub(4));
    let rect = crate::ui::help::centered(area, w, 5);
    f.render_widget(Clear, rect);
    let block = Block::new()
        .borders(Borders::ALL)
        .border_style(t.fg("ui.popup.border"))
        .style(t.fg_bg("ui.text", "ui.popup"))
        .title(Span::styled(
            " Confirm ",
            t.fg("ui.text.strong").add_modifier(Modifier::BOLD),
        ));
    let inner = block.inner(rect);
    f.render_widget(block, rect);
    f.render_widget(
        Paragraph::new(vec![
            Line::styled(format!(" {}", c.text), t.fg("ui.text")),
            Line::raw(""),
            Line::from(vec![
                Span::styled(" y ", t.fg_bg("ui.button.text", "ui.button")),
                Span::styled(" yes   ", t.fg("ui.text")),
                Span::styled(" n ", t.fg_bg("ui.text", "ui.panel")),
                Span::styled(" no", t.fg("ui.text")),
            ]),
        ]),
        inner,
    );
}
