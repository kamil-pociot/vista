//! Chrome pieces whose shape depends on the look's `[chrome]` options:
//! panels, separators, tab strips, the tool-window bar and key hints.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Modifier;
use ratatui::symbols::border;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Paragraph};
use vista_theme::{Borders, PanelHeaders, Tabs, Theme, ToolWindows};

use crate::app::{App, Focus, LeftPanel, MsgKind};
use crate::keymap::Action;
use crate::style::ThemeStyle;

fn border_set(b: Borders) -> border::Set<'static> {
    match b {
        Borders::Rounded => border::ROUNDED,
        _ => border::PLAIN,
    }
}

/// Draws a panel frame with a title and returns the content area.
/// `bg` is the token for the panel's background.
pub fn panel(f: &mut Frame, area: Rect, title: &str, focused: bool, bg: &str, t: &Theme) -> Rect {
    if area.height == 0 {
        return area;
    }
    let title_style = if focused {
        t.fg("ui.text.strong").add_modifier(Modifier::BOLD)
    } else {
        t.fg("ui.header").add_modifier(Modifier::BOLD)
    };
    match t.chrome.panel_headers {
        PanelHeaders::TitledBar => {
            f.render_widget(Block::new().style(t.bg(bg)), area);
            let header = Rect { height: 1, ..area };
            let header_bg = if focused { "ui.selection.inactive" } else { "ui.panel" };
            f.render_widget(Block::new().style(t.bg(header_bg)), header);
            f.render_widget(Paragraph::new(Span::styled(format!(" {title}"), title_style)), header);
            Rect {
                y: area.y + 1,
                height: area.height - 1,
                ..area
            }
        }
        PanelHeaders::Inline => {
            f.render_widget(Block::new().style(t.bg(bg)), area);
            let header = Rect { height: 1, ..area };
            let style = if focused { t.fg("ui.text") } else { t.fg("ui.dim") };
            f.render_widget(
                Paragraph::new(Span::styled(format!(" {}", title.to_lowercase()), style)),
                header,
            );
            Rect {
                y: area.y + 1,
                height: area.height - 1,
                ..area
            }
        }
        PanelHeaders::BorderTitle => {
            let border_style = if focused {
                t.fg("ui.border.focused")
            } else {
                t.fg("ui.border")
            };
            let mut block = Block::new()
                .style(t.bg(bg))
                .title(Span::styled(format!(" {title} "), title_style));
            if t.chrome.borders != Borders::None {
                block = block
                    .borders(ratatui::widgets::Borders::ALL)
                    .border_set(border_set(t.chrome.borders))
                    .border_style(border_style);
            }
            let inner = block.inner(area);
            f.render_widget(block, area);
            inner
        }
    }
}

/// A titleless box around an area (looks with boxed panels); returns the inside.
pub fn frame(f: &mut Frame, area: Rect, focused: bool, t: &Theme) -> Rect {
    if t.chrome.borders == Borders::None || area.width < 3 || area.height < 3 {
        return area;
    }
    let border_style = if focused {
        t.fg("ui.border.focused")
    } else {
        t.fg("ui.border")
    };
    let block = Block::new()
        .style(t.bg("ui.bg"))
        .borders(ratatui::widgets::Borders::ALL)
        .border_set(border_set(t.chrome.borders))
        .border_style(border_style);
    let inner = block.inner(area);
    f.render_widget(block, area);
    inner
}

/// The gap between the left tool window and the main area.
pub fn separator(f: &mut Frame, area: Rect, t: &Theme) {
    let ch = if t.chrome.borders == Borders::None || t.chrome.panel_headers == PanelHeaders::BorderTitle {
        " "
    } else {
        "│"
    };
    let lines: Vec<Line> = (0..area.height)
        .map(|_| Line::raw(ch.repeat(area.width as usize)))
        .collect();
    f.render_widget(Paragraph::new(lines).style(t.fg_bg("ui.border", "ui.bg")), area);
}

/// One tab in a tab strip.
pub struct TabLabel<'a> {
    pub title: &'a str,
    /// Token for the title colour (git status of the file).
    pub fg: &'a str,
    pub dirty: bool,
}

/// Where a tab was drawn: its label and its close button (screen columns).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TabHit {
    pub label: (u16, u16),
    pub close: (u16, u16),
}

/// Draws the tab strip on the first row of `area` and returns the rest, and
/// where each tab (and its ×) landed, for clicks.
pub fn tab_strip(
    f: &mut Frame,
    area: Rect,
    tabs: &[TabLabel],
    active: Option<usize>,
    focused: bool,
    t: &Theme,
) -> (Rect, Vec<TabHit>) {
    if area.height == 0 {
        return (area, Vec::new());
    }
    let strip = Rect { height: 1, ..area };
    let strip_bg = match t.chrome.tabs {
        Tabs::Underline => "ui.panel",
        _ => "ui.bg",
    };
    f.render_widget(Block::new().style(t.bg(strip_bg)), strip);
    let mut spans = Vec::new();
    let mut hits = Vec::with_capacity(tabs.len());
    let mut x = strip.x;
    let width = |s: &str| unicode_width::UnicodeWidthStr::width(s) as u16;
    for (i, tab) in tabs.iter().enumerate() {
        let is_active = Some(i) == active;
        let mark = if tab.dirty { "● " } else { "" };
        let label = format!(" {mark}{} ", tab.title);
        // The close button, IntelliJ-style, right of the title.
        let close = "× ";
        let style = match (t.chrome.tabs, is_active) {
            (Tabs::Underline, true) => {
                let s = t.fg_bg(tab.fg, "ui.bg");
                if focused {
                    s.add_modifier(Modifier::UNDERLINED)
                } else {
                    s
                }
            }
            (Tabs::Underline, false) => t.fg_bg(tab.fg, "ui.panel"),
            (Tabs::Pill, true) => t.fg_bg(tab.fg, "ui.selection.inactive"),
            (Tabs::Pill, false) => t.fg(tab.fg),
            (Tabs::Text, true) => t.fg(tab.fg).add_modifier(Modifier::BOLD),
            (Tabs::Text, false) => t.fg("ui.dim"),
        };
        let (lw, cw) = (width(&label), width(close));
        hits.push(TabHit {
            label: (x, x + lw + cw),
            close: (x + lw, x + lw + 1),
        });
        x += lw + cw;
        // The × is dim, on the tab's own background.
        let close_style = match style.bg {
            Some(bg) => t.fg("ui.dim").bg(bg),
            None => t.fg("ui.dim"),
        };
        spans.push(Span::styled(label, style));
        spans.push(Span::styled(close, close_style));
        if t.chrome.tabs != Tabs::Underline {
            spans.push(Span::raw(" "));
            x += 1;
        }
    }
    f.render_widget(Paragraph::new(Line::from(spans)), strip);
    (
        Rect {
            y: area.y + 1,
            height: area.height - 1,
            ..area
        },
        hits,
    )
}

fn tool_label(app: &App, key: &str, name: &str, active: bool) -> Span<'static> {
    let t = &app.theme;
    let style = if active {
        t.fg("ui.text.strong").add_modifier(Modifier::BOLD)
    } else {
        t.fg("ui.header")
    };
    Span::styled(format!(" {key}: {name}  "), style)
}

fn status_spans(app: &App) -> Vec<Span<'static>> {
    let t = &app.theme;
    match &app.message {
        Some(s) => {
            let token = match s.kind {
                MsgKind::Error => "ui.error",
                MsgKind::Info => "ui.text",
            };
            vec![Span::styled(format!("{} ", s.text), t.fg(token))]
        }
        None => {
            let mut spans = Vec::new();
            let unversioned = app
                .status
                .changes()
                .iter()
                .filter(|c| c.status == vista_git::FileStatus::Unversioned)
                .count();
            let changed = app.status.changes().len() - unversioned;
            if changed > 0 {
                spans.push(Span::styled(format!("{changed} changed"), t.fg("git.modified")));
            }
            if unversioned > 0 {
                if changed > 0 {
                    spans.push(Span::styled(" · ", t.fg("ui.dim")));
                }
                spans.push(Span::styled(
                    format!("{unversioned} unversioned"),
                    t.fg("git.unversioned"),
                ));
            }
            if !spans.is_empty() {
                spans.push(Span::raw("   "));
            }
            match app.viewer.active_tab() {
                Some(tab) => {
                    if tab.has_cursor() {
                        spans.push(Span::styled(format!("{}:1   ", tab.cursor + 1), t.fg("ui.text")));
                    }
                    if !tab.language.is_empty() {
                        spans.push(Span::styled(format!("{} ", tab.language), t.fg("ui.dim")));
                    }
                }
                None => spans.push(Span::styled(format!("{} ", app.theme.name), t.fg("ui.dim"))),
            }
            spans
        }
    }
}

/// Bottom row: tool-window switcher (when the look uses one) and the status.
pub fn bottom_bar(f: &mut Frame, area: Rect, app: &App) {
    let t = &app.theme;
    f.render_widget(Block::new().style(t.bg("ui.panel")), area);
    if t.chrome.tool_windows == ToolWindows::BottomBar {
        let left_active = |p| app.left == Some(p) && app.focus == Focus::Left;
        let km = &app.keymap;
        let spans = vec![
            tool_label(
                app,
                &km.tool_number(Action::ToggleProject),
                "Project",
                left_active(LeftPanel::Project),
            ),
            tool_label(
                app,
                &km.tool_number(Action::ToggleCommit),
                "Commit",
                left_active(LeftPanel::Commit),
            ),
            tool_label(
                app,
                &km.tool_number(Action::ToggleFind),
                "Find",
                app.focus == Focus::Bottom && !app.log.visible && !app.shell_visible,
            ),
            tool_label(
                app,
                &km.tool_number(Action::ToggleTerminal),
                "Terminal",
                app.focus == Focus::Bottom && app.shell_visible,
            ),
            tool_label(
                app,
                &km.tool_number(Action::ToggleLog),
                "Git",
                app.focus == Focus::Bottom && app.log.visible,
            ),
            tool_label(app, "?", "Help", app.show_help),
        ];
        // Labels give way to the status on the right when the row is narrow.
        let status_w = Line::from(status_spans(app)).width();
        let mut spans = spans;
        while spans.len() > 1 && Line::from(spans.clone()).width() + status_w > area.width as usize {
            spans.pop();
        }
        f.render_widget(Paragraph::new(Line::from(spans)), area);
    } else {
        let mode = Span::styled(
            format!(" {} ", if app.focus == Focus::Left { "TREE" } else { "VIEW" }),
            t.fg_bg("ui.bg", "ui.accent").add_modifier(Modifier::BOLD),
        );
        f.render_widget(Paragraph::new(Line::from(vec![mode])), area);
    }
    f.render_widget(Paragraph::new(Line::from(status_spans(app))).right_aligned(), area);
}

/// A vertical strip of tool-window icons (IntelliJ New UI style).
pub fn icon_rail(f: &mut Frame, area: Rect, app: &App) {
    let t = &app.theme;
    f.render_widget(Block::new().style(t.bg("ui.bg")), area);
    let item = |glyph: &str, active: bool| {
        let style = if active {
            t.fg_bg("ui.accent", "ui.selection.inactive")
        } else {
            t.fg("ui.dim")
        };
        Line::from(Span::styled(format!(" {glyph}  "), style))
    };
    let lines = vec![
        item("◫", app.left == Some(LeftPanel::Project)),
        Line::raw(""),
        item("⎇", app.left == Some(LeftPanel::Commit)),
        Line::raw(""),
        item("⌕", false),
        Line::raw(""),
        item("?", app.show_help),
    ];
    f.render_widget(Paragraph::new(lines), area);
}

/// A line of key hints (looks with `keyhints = true`).
pub fn keyhints(f: &mut Frame, area: Rect, app: &App) {
    let t = &app.theme;
    let km = &app.keymap;
    let mut spans = vec![Span::raw(" ")];
    for (a, label) in [
        (Action::GotoFile, "files"),
        (Action::FindInFiles, "find in files"),
        (Action::ToggleCommit, "changes"),
        (Action::FocusNext, "focus"),
        (Action::Help, "help"),
    ] {
        spans.push(Span::styled(km.hint(a), t.fg("ui.warning")));
        spans.push(Span::styled(format!(" {label}    "), t.fg("ui.dim")));
    }
    f.render_widget(Paragraph::new(Line::from(spans)).style(t.bg("ui.bg")), area);
}

/// Splits off the rightmost column of `area` for a scrollbar.
pub fn split_scrollbar(area: Rect) -> (Rect, Rect) {
    let w = area.width.saturating_sub(1);
    (
        Rect { width: w, ..area },
        Rect {
            x: area.x + w,
            width: area.width - w,
            ..area
        },
    )
}

/// Thumb (start row, length) for `total` content rows with `top` of them
/// above a view of `height` rows; `None` when everything fits.
pub fn scrollbar_thumb(total: usize, top: usize, height: usize) -> Option<(usize, usize)> {
    if height == 0 || (total <= height && top == 0) {
        return None;
    }
    // Scrolling past the end counts as content, so the thumb ends at the bottom.
    let len = total.max(top + height);
    let size = (height * height / len).clamp(1, height);
    let start = (top * height).div_ceil(len).min(height - size);
    Some((start, size))
}

/// A thin vertical scrollbar in a one-column `bar`, measured in screen rows
/// (a wrapped line counts once per row it takes).
pub fn scrollbar(f: &mut Frame, bar: Rect, t: &Theme, total: usize, top: usize) {
    let Some((start, size)) = scrollbar_thumb(total, top, bar.height as usize) else {
        return;
    };
    let lines: Vec<Line> = (0..bar.height as usize)
        .map(|i| {
            if (start..start + size).contains(&i) {
                Line::from(Span::styled("┃", t.fg("ui.dim")))
            } else {
                Line::raw(" ")
            }
        })
        .collect();
    f.render_widget(Paragraph::new(lines), bar);
}

#[cfg(test)]
mod scrollbar_tests {
    use super::scrollbar_thumb;

    #[test]
    fn thumb_tracks_rows_not_lines() {
        // Everything fits: no bar.
        assert_eq!(scrollbar_thumb(10, 0, 20), None);
        // Twice the view: half-height thumb, top then bottom.
        assert_eq!(scrollbar_thumb(40, 0, 20), Some((0, 10)));
        assert_eq!(scrollbar_thumb(40, 20, 20), Some((10, 10)));
        // Past the end the thumb stays pinned to the bottom.
        let (start, size) = scrollbar_thumb(40, 26, 20).unwrap();
        assert_eq!(start + size, 20);
        // Huge content: the thumb never vanishes.
        assert_eq!(scrollbar_thumb(100_000, 0, 20), Some((0, 1)));
        let (start, size) = scrollbar_thumb(100_000, 99_990, 20).unwrap();
        assert_eq!((start, size), (19, 1));
    }
}

#[cfg(test)]
mod tab_strip_tests {
    use super::{TabLabel, tab_strip};
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use ratatui::layout::Rect;

    #[test]
    fn hits_match_where_labels_and_close_buttons_are_drawn() {
        let theme = vista_theme::Theme::default_theme();
        let tabs = [
            TabLabel {
                title: "main.rs",
                fg: "ui.text",
                dirty: true,
            },
            TabLabel {
                title: "Δ lib.rs",
                fg: "ui.text",
                dirty: false,
            },
        ];
        let mut term = Terminal::new(TestBackend::new(60, 3)).unwrap();
        let mut hits = Vec::new();
        term.draw(|f| {
            hits = tab_strip(f, Rect::new(2, 0, 58, 3), &tabs, Some(0), true, &theme).1;
        })
        .unwrap();
        let buf = term.backend().buffer().clone();
        let at = |x: u16| buf[(x, 0)].symbol().to_string();
        assert_eq!(hits.len(), 2);
        // Each × sits exactly where its hit says, after the ● of a dirty tab.
        for h in &hits {
            assert_eq!(at(h.close.0), "×", "{hits:?}");
            assert!(h.label.0 <= h.close.0 && h.close.1 <= h.label.1);
        }
        assert_eq!(at(hits[0].label.0 + 1), "●");
        assert_eq!(at(hits[1].label.0 + 1), "Δ");
        assert!(hits[0].label.1 <= hits[1].label.0, "no overlap: {hits:?}");
    }
}
