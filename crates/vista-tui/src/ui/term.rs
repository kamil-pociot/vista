//! Drawing a terminal tab: the program's screen, full size.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::Span;
use ratatui::widgets::Paragraph;
use vista_theme::Theme;

use crate::style::ThemeStyle;
use crate::term_tab::TermTab;

pub fn draw(f: &mut Frame, area: Rect, tab: &TermTab, t: &Theme) {
    let parser = tab.screen();
    let screen = parser.screen();
    let buf = f.buffer_mut();
    for row in 0..area.height {
        for col in 0..area.width {
            let Some(cell) = screen.cell(row, col) else { continue };
            // The right half of a wide character belongs to its left half.
            if cell.is_wide_continuation() {
                continue;
            }
            let symbol = match cell.contents() {
                "" => " ",
                s => s,
            };
            buf[(area.x + col, area.y + row)]
                .set_symbol(symbol)
                .set_style(crate::style::term_cell(t, cell));
        }
    }
    if let Some(status) = &tab.exited {
        let last = Rect {
            y: area.y + area.height.saturating_sub(1),
            height: 1,
            ..area
        };
        f.render_widget(
            Paragraph::new(Span::styled(
                format!(" {} {status} — Enter closes the tab, Ctrl-\\ goes back ", tab.title),
                t.fg_bg("ui.button.text", "ui.button"),
            )),
            last,
        );
    } else if !screen.hide_cursor() {
        let (r, c) = screen.cursor_position();
        if r < area.height && c < area.width {
            f.set_cursor_position((area.x + c, area.y + r));
        }
    }
}
