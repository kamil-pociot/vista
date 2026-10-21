//! The `?` overlay listing every action and its keys.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Modifier;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};

use crate::app::App;
use crate::keymap::Action;
use crate::style::ThemeStyle;

pub fn centered(area: Rect, w: u16, h: u16) -> Rect {
    let w = w.min(area.width);
    let h = h.min(area.height);
    Rect {
        x: area.x + (area.width - w) / 2,
        y: area.y + (area.height - h) / 2,
        width: w,
        height: h,
    }
}

pub fn draw(f: &mut Frame, area: Rect, app: &App) {
    let t = &app.theme;
    let rows: Vec<(String, &str)> = Action::ALL
        .iter()
        .map(|a| {
            let keys: Vec<String> = app.keymap.chords_for(*a).iter().map(|c| c.to_string()).collect();
            (keys.join(", "), a.description())
        })
        .filter(|(k, _)| !k.is_empty())
        .collect();
    let key_w = rows.iter().map(|(k, _)| k.chars().count()).max().unwrap_or(0);
    let lines: Vec<Line> = rows
        .iter()
        .map(|(k, d)| {
            Line::from(vec![
                Span::styled(format!(" {k:>key_w$}  "), t.fg("ui.accent")),
                Span::styled(d.to_string(), t.fg("ui.text")),
            ])
        })
        .collect();
    let width = (key_w + 34) as u16;
    let rect = centered(area, width, lines.len() as u16 + 2);
    f.render_widget(Clear, rect);
    let block = Block::new()
        .borders(Borders::ALL)
        .border_style(t.fg("ui.popup.border"))
        .style(t.fg_bg("ui.text", "ui.popup"))
        .title(Span::styled(
            " Keyboard shortcuts ",
            t.fg("ui.text.strong").add_modifier(Modifier::BOLD),
        ))
        .title_bottom(Span::styled(" any key to close ", t.fg("ui.dim")));
    let inner = block.inner(rect);
    f.render_widget(block, rect);
    f.render_widget(Paragraph::new(lines), inner);
}
