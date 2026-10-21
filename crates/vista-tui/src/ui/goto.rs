//! Drawing the Go to File popup.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Modifier;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use unicode_width::UnicodeWidthStr;

use crate::app::App;
use crate::goto::GotoTab;
use crate::style::ThemeStyle;

/// Preview lines of the selected file (read lazily, plain text).
fn preview(path: &std::path::Path, n: usize) -> Vec<String> {
    let Ok(bytes) = std::fs::read(path) else {
        return Vec::new();
    };
    let head = &bytes[..bytes.len().min(16 * 1024)];
    if vista_render::filetype::looks_binary(head) {
        return vec!["(binary file)".into()];
    }
    String::from_utf8_lossy(head)
        .lines()
        .take(n)
        .map(|l| vista_render::styled::expand_tabs(l, 4))
        .collect()
}

pub fn draw(f: &mut Frame, area: Rect, app: &mut App) {
    let Some(p) = app.goto.as_ref() else { return };
    let t = &app.theme;
    let w = area.width.saturating_sub(6).clamp(30, 110);
    let h = area.height.saturating_sub(4).clamp(10, 34);
    let rect = Rect {
        x: area.x + (area.width.saturating_sub(w)) / 2,
        y: area.y + 2.min(area.height.saturating_sub(h)),
        width: w.min(area.width),
        height: h.min(area.height),
    };
    f.render_widget(Clear, rect);
    let block = Block::new()
        .borders(Borders::ALL)
        .border_style(t.fg("ui.popup.border"))
        .style(t.fg_bg("ui.text", "ui.popup"));
    let inner = block.inner(rect);
    f.render_widget(block, rect);
    if inner.height < 4 {
        return;
    }

    // Row 0: scope tabs + include-ignored toggle.
    let mut tabs = vec![Span::raw(" ")];
    for tab in GotoTab::ALL {
        let style = if tab == p.tab {
            t.fg("ui.text.strong")
                .add_modifier(Modifier::BOLD | Modifier::UNDERLINED)
        } else {
            t.fg("ui.text")
        };
        tabs.push(Span::styled(format!(" {} ", tab.label()), style));
        tabs.push(Span::raw("  "));
    }
    f.render_widget(Paragraph::new(Line::from(tabs)), Rect { height: 1, ..inner });
    let toggle = format!(
        "[{}] include ignored (Alt-I) ",
        if p.include_ignored { "x" } else { " " }
    );
    f.render_widget(
        Paragraph::new(Span::styled(toggle, t.fg("ui.dim"))).right_aligned(),
        Rect { height: 1, ..inner },
    );

    // Row 1: query input.
    let input = Rect {
        y: inner.y + 1,
        height: 1,
        ..inner
    };
    f.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(" ⌕ ", t.fg("ui.dim")),
            Span::styled(p.query.clone(), t.fg("ui.text.strong")),
            Span::styled("▏", t.fg("ui.text")),
        ]))
        .style(t.bg("ui.input")),
        input,
    );

    // Results, preview and footer.
    let footer_h = 1;
    let rest = inner.height - 2 - footer_h;
    let preview_h = if rest >= 20 { rest / 3 } else { 0 };
    let list_h = rest - preview_h;
    let list = Rect {
        y: inner.y + 2,
        height: list_h,
        ..inner
    };
    let root = app.root.clone();
    let p = app.goto.as_mut().expect("popup");
    p.view_height = list_h as usize;
    let p = &*p;
    let mut lines = Vec::new();
    for (i, m) in p.results.iter().enumerate().skip(p.scroll).take(list_h as usize) {
        let chars: Vec<char> = m.path.chars().collect();
        let name_at = m.path.rfind('/').map(|b| m.path[..=b].chars().count()).unwrap_or(0);
        let abs = root.join(&m.path);
        let token = app.status.file_status(&abs).map(|s| s.token()).unwrap_or("ui.text");
        let mut spans = vec![Span::raw(" ")];
        let mut run = String::new();
        let mut run_hit = false;
        for (ci, ch) in chars.iter().enumerate().skip(name_at) {
            let hit = m.indices.binary_search(&(ci as u32)).is_ok();
            if hit != run_hit && !run.is_empty() {
                spans.push(name_span(&run, run_hit, token, t));
                run.clear();
            }
            run_hit = hit;
            run.push(*ch);
        }
        if !run.is_empty() {
            spans.push(name_span(&run, run_hit, token, t));
        }
        let dir: String = chars[..name_at]
            .iter()
            .collect::<String>()
            .trim_end_matches('/')
            .to_string();
        let used: usize = spans.iter().map(|s| s.content.width()).sum();
        let pad = (list.width as usize).saturating_sub(used + dir.width() + 1);
        spans.push(Span::raw(" ".repeat(pad)));
        spans.push(Span::styled(format!("{dir} "), t.fg("ui.dim")));
        let mut line = Line::from(spans);
        if i == p.selected {
            line = line.style(t.bg("ui.selection"));
        }
        lines.push(line);
    }
    if p.results.is_empty() {
        let msg = if app.file_index.is_none() {
            " Indexing files…"
        } else {
            " Nothing found"
        };
        lines.push(Line::styled(msg, t.fg("ui.dim")));
    }
    f.render_widget(Paragraph::new(lines), list);
    app.areas.goto_list = Some(list);

    if preview_h > 0
        && let Some(path) = p.selected_path(&root)
    {
        let r = Rect {
            y: list.y + list_h,
            height: preview_h,
            ..inner
        };
        let mut pl = vec![Line::styled("─".repeat(r.width as usize), t.fg("ui.border"))];
        pl.extend(
            preview(&path, preview_h as usize - 1)
                .into_iter()
                .map(|l| Line::styled(format!(" {l}"), t.fg("ui.dim"))),
        );
        f.render_widget(Paragraph::new(pl).style(t.bg("ui.input")), r);
    }

    let footer = Rect {
        y: inner.y + inner.height - 1,
        height: 1,
        ..inner
    };
    let count = format!("{} of {} files ", p.results.len(), p.indexed);
    f.render_widget(
        Paragraph::new(Span::styled(
            " ↑↓ select   ⏎ open   Alt-⏎ edit   Tab scope   name:42 → line",
            t.fg("ui.dim"),
        )),
        footer,
    );
    f.render_widget(
        Paragraph::new(Span::styled(count, t.fg("ui.dim"))).right_aligned(),
        footer,
    );
}

fn name_span(text: &str, hit: bool, token: &'static str, t: &vista_theme::Theme) -> Span<'static> {
    if hit {
        Span::styled(
            text.to_string(),
            t.fg("syntax.function")
                .add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
        )
    } else {
        Span::styled(text.to_string(), t.fg(token))
    }
}
