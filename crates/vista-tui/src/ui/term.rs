//! Drawing a terminal tab: the program's screen, full size.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::Span;
use ratatui::widgets::Paragraph;
use vista_theme::Theme;

use crate::style::ThemeStyle;
use crate::term_tab::TermTab;

/// `on_exit` says what Enter does once the program ended.
pub fn draw(f: &mut Frame, area: Rect, tab: &TermTab, t: &Theme, focused: bool, on_exit: &str) {
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
                format!(" {} {status} — {on_exit} ", tab.title),
                t.fg_bg("ui.button.text", "ui.button"),
            )),
            last,
        );
    } else if focused && !screen.hide_cursor() {
        let (r, c) = screen.cursor_position();
        if r < area.height && c < area.width {
            f.set_cursor_position((area.x + c, area.y + r));
        }
    }
}

/// The Terminal tool window (Alt-T): the project's shell under the editor.
pub fn draw_panel(f: &mut Frame, area: Rect, app: &mut crate::app::App) {
    use ratatui::style::Modifier;
    use ratatui::text::Line;
    let focused = app.focus == crate::app::Focus::Bottom;
    let t = &app.theme;
    let header = Rect { height: 1, ..area };
    let title_style = if focused {
        t.fg_bg("ui.text.strong", "ui.selection.inactive")
            .add_modifier(Modifier::BOLD)
    } else {
        t.fg("ui.header").add_modifier(Modifier::BOLD)
    };
    let key = app.keymap.tool_number(crate::keymap::Action::ToggleTerminal);
    f.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(format!(" {key}: Terminal"), title_style),
            Span::styled(format!("  {}", crate::projects::tilde(&app.root)), t.fg("ui.dim")),
        ]))
        .style(t.bg("ui.panel")),
        header,
    );
    f.render_widget(
        Paragraph::new(Span::styled("Ctrl-\\ back to the editor ", t.fg("ui.dim"))).right_aligned(),
        header,
    );
    let body = Rect {
        y: area.y + 1,
        height: area.height.saturating_sub(1),
        ..area
    };
    app.areas.shell = Some(body);
    if let Some(shell) = app.shell.as_mut() {
        shell.resize(body.width, body.height);
        draw(f, body, shell, &app.theme, focused, "Enter starts a new shell");
    }
}
