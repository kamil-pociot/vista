//! Drawing the Preferences page.

use ratatui::Frame;
use ratatui::crossterm::event::KeyCode;
use ratatui::layout::{Alignment, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap};
use unicode_width::UnicodeWidthStr;

use crate::app::App;
use crate::prefs::{Pref, SECTIONS};
use crate::style::ThemeStyle;
use crate::ui::click::{self, Click};

/// One line of the page: a heading, a setting (its index among all), or a gap.
enum Row {
    Heading(&'static str),
    Setting(usize, Pref),
    Gap,
}

fn rows() -> Vec<Row> {
    let mut rows = Vec::new();
    let mut i = 0;
    for (n, (title, prefs)) in SECTIONS.iter().enumerate() {
        if n > 0 {
            rows.push(Row::Gap);
        }
        rows.push(Row::Heading(title));
        for p in *prefs {
            rows.push(Row::Setting(i, *p));
            i += 1;
        }
    }
    rows
}

pub fn draw(f: &mut Frame, area: Rect, app: &App) {
    let Some(page) = app.prefs.as_ref() else { return };
    let t = &app.theme;
    let rows = rows();
    let w = area.width.saturating_sub(4).min(78);
    let h = (rows.len() as u16 + 6).min(area.height.saturating_sub(2));
    let rect = crate::ui::help::centered(area, w, h);
    f.render_widget(Clear, rect);
    let block = Block::new()
        .borders(Borders::ALL)
        .border_style(t.fg("ui.popup.border"))
        .style(t.fg_bg("ui.text", "ui.popup"))
        .title(Span::styled(
            " Preferences ",
            t.fg("ui.text.strong").add_modifier(Modifier::BOLD),
        ));
    let inner = block.inner(rect);
    f.render_widget(block, rect);
    if inner.height < 5 || inner.width < 20 {
        return;
    }
    // The list, then a rule, the selected setting's explanation (2 rows) and the keys.
    let list_h = inner.height.saturating_sub(4);
    let selected = page.selected;
    let at = rows
        .iter()
        .position(|r| matches!(r, Row::Setting(i, _) if *i == selected))
        .unwrap_or(0);
    // Keep the selected row (and its heading, near the top) in view.
    let scroll = (at + 1).saturating_sub(list_h as usize);
    let width = inner.width as usize;
    for (k, row) in rows.iter().skip(scroll).take(list_h as usize).enumerate() {
        let r = Rect {
            y: inner.y + k as u16,
            height: 1,
            ..inner
        };
        match row {
            Row::Gap => {}
            Row::Heading(title) => {
                let style = t.fg("ui.header").add_modifier(Modifier::BOLD);
                f.render_widget(Paragraph::new(Span::styled(format!(" {title}"), style)), r);
            }
            Row::Setting(i, p) => {
                let value = app.pref_value(*p);
                let value = match p {
                    Pref::Look => format!("◂ {value} ▸"),
                    _ => value,
                };
                let value_style = match value.as_str() {
                    "On" => t.fg("ui.accent").add_modifier(Modifier::BOLD),
                    "Off" => t.fg("ui.dim"),
                    _ => t.fg("ui.text.strong"),
                };
                let label = format!("   {}", p.label());
                let pad = width.saturating_sub(label.width() + value.width() + 2);
                let line = Line::from(vec![
                    Span::styled(label, t.fg("ui.text")),
                    Span::raw(" ".repeat(pad)),
                    Span::styled(value, value_style),
                    Span::raw("  "),
                ]);
                let style = if *i == selected {
                    t.bg("ui.selection")
                } else {
                    Style::default()
                };
                f.render_widget(Paragraph::new(line).style(style), r);
                click::add(r, Click::Pref(*i));
            }
        }
    }
    // Below the list: what the selected setting does, then the keys.
    let rule = Rect {
        y: inner.y + list_h,
        height: 1,
        ..inner
    };
    f.render_widget(
        Paragraph::new(Span::styled("─".repeat(width), t.fg("ui.popup.border"))),
        rule,
    );
    let about = Rect {
        y: rule.y + 1,
        height: 2,
        ..inner
    };
    f.render_widget(
        Paragraph::new(Span::styled(
            format!(" {}", page.selected_pref().about()),
            t.fg("ui.dim"),
        ))
        .wrap(Wrap { trim: false }),
        about,
    );
    let keys = Rect {
        y: rule.y + 3,
        height: 1,
        ..inner
    };
    let mut items: Vec<click::Item> = vec![(Span::styled(" ↑↓ select · ←→ / Space change · ", t.fg("ui.dim")), None)];
    items.extend(click::hints(
        &[("Esc close", click::key(KeyCode::Esc, None))],
        "",
        t.fg("ui.dim"),
    ));
    click::line(f, keys, items, Alignment::Left, Style::default());
}
