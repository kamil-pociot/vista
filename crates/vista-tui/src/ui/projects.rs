//! Drawing the project tabs (top bar) and the Projects popup.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Modifier;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use unicode_width::UnicodeWidthStr;

use crate::app::App;
use crate::projects::ProjectHit;
use crate::style::ThemeStyle;

/// The open projects as tabs at the left of the top bar, then `+`.
/// Returns where each landed, for clicks.
pub fn tabs(f: &mut Frame, area: Rect, app: &App) -> Vec<ProjectHit> {
    let t = &app.theme;
    let single = [(app.project_name.clone(), app.root.clone())];
    let open = if app.projects.open.is_empty() {
        &single[..]
    } else {
        &app.projects.open[..]
    };
    let many = open.len() > 1;
    let mut spans = vec![Span::raw(" ")];
    let mut hits = Vec::new();
    let mut x = area.x + 1;
    for (i, (name, _)) in open.iter().enumerate() {
        let active = i == app.projects.active;
        let label = format!(" {name} ");
        let style = if active {
            t.fg_bg("ui.text.strong", "ui.selection.inactive")
                .add_modifier(Modifier::BOLD)
        } else {
            t.fg("ui.dim")
        };
        let w = label.width() as u16;
        let close = many.then(|| (x + w, x + w + 1));
        spans.push(Span::styled(label, style));
        let mut end = x + w;
        if many {
            let close_style = match style.bg {
                Some(bg) => t.fg("ui.dim").bg(bg),
                None => t.fg("ui.dim"),
            };
            spans.push(Span::styled("× ", close_style));
            end += 2;
        }
        hits.push(ProjectHit {
            index: Some(i),
            x: (x, end),
            close,
        });
        spans.push(Span::raw(" "));
        x = end + 1;
    }
    spans.push(Span::styled(" + ", t.fg("ui.accent")));
    hits.push(ProjectHit {
        index: None,
        x: (x, x + 3),
        close: None,
    });
    f.render_widget(Paragraph::new(Line::from(spans)), area);
    hits
}

/// The Projects popup (Ctrl-O).
pub fn draw_popup(f: &mut Frame, area: Rect, app: &mut App) {
    let Some(p) = &app.projects_popup else { return };
    let t = &app.theme;
    let w = area.width.saturating_sub(4).min(90);
    let h = (p.entries.len() as u16 + 5).clamp(7, area.height.saturating_sub(2).max(7));
    let rect = crate::ui::help::centered(area, w, h);
    f.render_widget(Clear, rect);
    let block = Block::new()
        .borders(Borders::ALL)
        .border_style(t.fg("ui.popup.border"))
        .style(t.fg_bg("ui.text", "ui.popup"))
        .title(Span::styled(
            " Projects ",
            t.fg("ui.text.strong").add_modifier(Modifier::BOLD),
        ))
        .title_bottom(Span::styled(
            " Enter open · Tab complete folder · Ctrl-W close project · Esc ",
            t.fg("ui.dim"),
        ));
    let inner = block.inner(rect);
    f.render_widget(block, rect);
    if inner.height < 3 {
        return;
    }
    f.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(" ⌕ ", t.fg("ui.dim")),
            Span::styled(p.query.clone(), t.fg("ui.text.strong")),
            Span::styled("▏", t.fg("ui.accent")),
        ]))
        .style(t.bg("ui.input")),
        Rect { height: 1, ..inner },
    );
    let list = Rect {
        y: inner.y + 2,
        height: inner.height - 2,
        ..inner
    };
    let name_w = p
        .entries
        .iter()
        .map(|e| p.describe(e).0.width())
        .max()
        .unwrap_or(0)
        .min(40);
    let lines: Vec<Line> = if p.entries.is_empty() {
        vec![Line::styled(
            " No match. Type a folder name, or a path (/…, ~/…) to pick a folder.",
            t.fg("ui.dim"),
        )]
    } else {
        p.entries
            .iter()
            .enumerate()
            .take(list.height as usize)
            .map(|(i, e)| {
                let (name, detail, active) = p.describe(e);
                let mark = match e {
                    crate::projects::Entry::Open(_) if active => "● ",
                    crate::projects::Entry::Open(_) => "○ ",
                    crate::projects::Entry::Recent(_) => "  ",
                    crate::projects::Entry::Add(_) => "+ ",
                };
                let mut line = Line::from(vec![
                    Span::styled(format!(" {mark}"), t.fg("ui.accent")),
                    Span::styled(format!("{name:<name_w$}  "), t.fg("ui.text.strong")),
                    Span::styled(detail, t.fg("ui.dim")),
                ]);
                if i == p.selected {
                    line = line.style(t.bg("ui.selection"));
                }
                line
            })
            .collect()
    };
    f.render_widget(Paragraph::new(lines), list);
    app.areas.projects_list = Some(list);
}
