//! Clickable labels: tool-window names, hints, toggles, buttons. Drawing code
//! records where each one landed and what a click on it does; a left click
//! there does the same as its key ([`crate::app::App::click`]).

use std::cell::RefCell;

use ratatui::Frame;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::layout::{Alignment, Rect};
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use unicode_width::UnicodeWidthStr;

use crate::app::Focus;
use crate::find::{Field, ScopeKind};
use crate::goto::GotoTab;
use crate::keymap::Action;

/// What a click on a label does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Click {
    /// Runs an action, as its key would.
    Action(Action),
    /// Presses a key, after moving the focus to the window it belongs to.
    Key(KeyEvent, Option<Focus>),
    FindScope(ScopeKind),
    FindField(Field),
    GotoTab(GotoTab),
    FindWindowTab(usize),
    /// The Commit window's Amend checkbox.
    Amend,
    /// A row of the Preferences page (index into `prefs::all()`): select and change it.
    Pref(usize),
    /// Settings or Keymap, at the top of Preferences.
    PrefsPage(crate::prefs::Page),
    /// A row of the Keymap page (index into its rows), and one of its keys.
    KeymapRow(usize, Option<usize>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Button {
    pub rect: Rect,
    pub click: Click,
}

// Drawing happens on the UI thread only; widgets that get just a theme can
// still record their buttons.
thread_local! {
    static DRAWN: RefCell<Vec<Button>> = const { RefCell::new(Vec::new()) };
}

/// Starts a frame.
pub fn begin() {
    DRAWN.with(|d| d.borrow_mut().clear());
}

/// How many buttons were recorded so far (see [`only_since`]).
pub fn mark() -> usize {
    DRAWN.with(|d| d.borrow().len())
}

/// Forgets the buttons recorded before `mark`: they are under a popup.
pub fn only_since(mark: usize) {
    DRAWN.with(|d| {
        let mut d = d.borrow_mut();
        let n = mark.min(d.len());
        d.drain(..n);
    });
}

/// The frame's buttons.
pub fn take() -> Vec<Button> {
    DRAWN.with(|d| std::mem::take(&mut *d.borrow_mut()))
}

pub fn add(rect: Rect, click: Click) {
    DRAWN.with(|d| d.borrow_mut().push(Button { rect, click }));
}

/// The button at a cell: the last drawn wins (it's on top).
pub fn at(buttons: &[Button], col: u16, row: u16) -> Option<Click> {
    let pos = ratatui::layout::Position::new(col, row);
    buttons.iter().rev().find(|b| b.rect.contains(pos)).map(|b| b.click)
}

pub fn key(code: KeyCode, focus: Option<Focus>) -> Click {
    Click::Key(KeyEvent::from(code), focus)
}

pub fn chord(code: KeyCode, mods: KeyModifiers, focus: Option<Focus>) -> Click {
    Click::Key(KeyEvent::new(code, mods), focus)
}

/// One piece of a clickable line.
pub type Item = (Span<'static>, Option<Click>);

/// Hint text such as `u unified  i whitespace`: each part is clickable,
/// `gap` goes between them.
pub fn hints(parts: &[(&str, Click)], gap: &str, style: Style) -> Vec<Item> {
    let mut items = Vec::new();
    for (i, (text, click)) in parts.iter().enumerate() {
        if i > 0 {
            items.push((Span::styled(gap.to_string(), style), None));
        }
        items.push((Span::styled(text.to_string(), style), Some(*click)));
    }
    items
}

/// Draws `items` on the first row of `area` and records where the clickable
/// ones landed.
pub fn line(f: &mut Frame, area: Rect, items: Vec<Item>, align: Alignment, style: Style) {
    if area.height == 0 || area.width == 0 {
        return;
    }
    let row = Rect { height: 1, ..area };
    let total: u16 = items.iter().map(|(s, _)| s.content.width() as u16).sum();
    // Where ratatui puts the line; when it doesn't fit it starts at the left.
    let mut x = match align {
        _ if total >= row.width => row.x,
        Alignment::Left => row.x,
        Alignment::Center => row.x + (row.width - total) / 2,
        Alignment::Right => row.x + row.width - total,
    };
    for (span, click) in &items {
        let w = span.content.width() as u16;
        if let Some(click) = click
            && x + w <= row.x + row.width
        {
            add(Rect::new(x, row.y, w, 1), *click);
        }
        x += w;
    }
    let spans: Vec<Span> = items.into_iter().map(|(s, _)| s).collect();
    f.render_widget(Paragraph::new(Line::from(spans)).alignment(align).style(style), row);
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    fn draw(items: Vec<Item>, align: Alignment) -> (Vec<Button>, String) {
        let mut term = Terminal::new(TestBackend::new(30, 1)).unwrap();
        begin();
        term.draw(|f| line(f, Rect::new(2, 0, 26, 1), items, align, Style::default()))
            .unwrap();
        let buf = term.backend().buffer().clone();
        let text = (0..30).map(|x| buf[(x, 0)].symbol().to_string()).collect();
        (take(), text)
    }

    fn word(buttons: &[Button], text: &str, i: usize) -> String {
        let r = buttons[i].rect;
        text.chars().skip(r.x as usize).take(r.width as usize).collect()
    }

    #[test]
    fn buttons_land_where_their_text_is_drawn() {
        let a = Click::Action(Action::Help);
        let b = key(KeyCode::Esc, None);
        let items = || hints(&[("⏎ open", a), ("Esc close", b)], " · ", Style::default());
        for align in [Alignment::Left, Alignment::Center, Alignment::Right] {
            let (buttons, text) = draw(items(), align);
            assert_eq!(buttons.len(), 2, "{align:?}");
            assert_eq!(word(&buttons, &text, 0), "⏎ open", "{align:?}: {text:?}");
            assert_eq!(word(&buttons, &text, 1), "Esc close", "{align:?}: {text:?}");
            assert_eq!(at(&buttons, buttons[1].rect.x, 0), Some(b));
            assert_eq!(at(&buttons, buttons[1].rect.x - 1, 0), None, "the gap isn't a button");
        }
    }

    #[test]
    fn a_popup_hides_the_buttons_under_it() {
        begin();
        add(Rect::new(0, 0, 5, 1), Click::Action(Action::Help));
        let m = mark();
        add(Rect::new(10, 0, 5, 1), Click::Amend);
        only_since(m);
        let buttons = take();
        assert_eq!(at(&buttons, 1, 0), None);
        assert_eq!(at(&buttons, 11, 0), Some(Click::Amend));
    }

    #[test]
    fn what_does_not_fit_is_not_a_button() {
        let long = "x".repeat(24);
        let (buttons, _) = draw(
            vec![
                (Span::raw(long), Some(Click::Amend)),
                (Span::raw("cut off"), Some(Click::Action(Action::Help))),
            ],
            Alignment::Left,
        );
        assert_eq!(buttons.len(), 1);
    }
}
