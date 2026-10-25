//! Chrome pieces whose shape depends on the look's `[chrome]` options:
//! panels, separators, tab strips, the tool-window bar and key hints.

use ratatui::Frame;
use ratatui::layout::{Alignment, Rect};
use ratatui::style::Modifier;
use ratatui::symbols::border;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Paragraph};
use vista_theme::{Borders, PanelHeaders, Tabs, Theme, ToolWindows};

use crate::app::{App, Focus, LeftPanel, MsgKind};
use crate::keymap::Action;
use crate::style::ThemeStyle;
use crate::ui::click::{self, Click};

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

/// Where a tab was drawn: its row, and its label and close button (screen
/// columns; empty when scrolled out of view).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TabHit {
    pub row: u16,
    pub label: (u16, u16),
    pub close: (u16, u16),
}

/// How the tab strip lays out tabs that don't fit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct StripOptions {
    /// Several rows (IntelliJ's "multiple rows") instead of one that scrolls.
    pub wrap: bool,
    /// Columns kept free at the right (the File / Diff switch).
    pub reserve: u16,
    /// Columns of the one row scrolled out at the left.
    pub scroll: u16,
    /// A tab to scroll into view (it just became the active one).
    pub show: Option<usize>,
}

/// Where tabs go: (row, column within the strip's content) for each.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StripLayout {
    pub places: Vec<(u16, u16)>,
    pub rows: u16,
    /// The one row is wider than the room: it scrolls, between ◂ and ▸.
    pub overflow: bool,
    /// The scroll, kept within its limits.
    pub scroll: u16,
}

/// Lays out tabs `widths` wide in `room` columns: wrapped onto rows, or on
/// one row that scrolls (one column at each end goes to ◂ ▸ then).
pub fn layout_strip(widths: &[u16], room: u16, opts: StripOptions) -> StripLayout {
    let mut places = Vec::with_capacity(widths.len());
    if opts.wrap {
        let (mut row, mut x) = (0, 0);
        for w in widths {
            if x > 0 && x + w > room {
                row += 1;
                x = 0;
            }
            places.push((row, x));
            x += w;
        }
        return StripLayout {
            places,
            rows: row + 1,
            overflow: false,
            scroll: 0,
        };
    }
    let mut x = 0;
    for w in widths {
        places.push((0, x));
        x += w;
    }
    let total = x;
    if total <= room {
        return StripLayout {
            places,
            rows: 1,
            overflow: false,
            scroll: 0,
        };
    }
    let inner = room.saturating_sub(2).max(1);
    let mut scroll = opts.scroll.min(total.saturating_sub(inner));
    if let Some(i) = opts.show.filter(|i| *i < widths.len()) {
        let (start, end) = (places[i].1, places[i].1 + widths[i]);
        if start < scroll || widths[i] > inner {
            scroll = start;
        } else if end > scroll + inner {
            scroll = end - inner;
        }
    }
    StripLayout {
        places,
        rows: 1,
        overflow: true,
        scroll,
    }
}

/// Draws the tab strip at the top of `area` (one row, or several when
/// wrapping) and returns the rest, where each tab (and its ×) landed, and
/// the scroll it ended at.
pub fn tab_strip(
    f: &mut Frame,
    area: Rect,
    tabs: &[TabLabel],
    active: Option<usize>,
    focused: bool,
    t: &Theme,
    opts: StripOptions,
) -> (Rect, Vec<TabHit>, u16) {
    if area.height == 0 {
        return (area, Vec::new(), 0);
    }
    let strip_bg = match t.chrome.tabs {
        Tabs::Underline => "ui.panel",
        _ => "ui.bg",
    };
    let width = |s: &str| unicode_width::UnicodeWidthStr::width(s) as u16;
    // Each tab's spans: the title, its ×, and a gap in looks that space tabs.
    let drawn: Vec<Vec<Span>> = tabs
        .iter()
        .enumerate()
        .map(|(i, tab)| {
            let is_active = Some(i) == active;
            let mark = if tab.dirty { "● " } else { "" };
            let label = format!(" {mark}{} ", tab.title);
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
            // The close button, IntelliJ-style, right of the title; dim, on
            // the tab's own background.
            let close_style = match style.bg {
                Some(bg) => t.fg("ui.dim").bg(bg),
                None => t.fg("ui.dim"),
            };
            let mut spans = vec![Span::styled(label, style), Span::styled("× ", close_style)];
            if t.chrome.tabs != Tabs::Underline {
                spans.push(Span::raw(" "));
            }
            spans
        })
        .collect();
    let widths: Vec<u16> = drawn
        .iter()
        .map(|spans| spans.iter().map(|s| width(&s.content)).sum())
        .collect();
    let room = area.width.saturating_sub(opts.reserve);
    let mut layout = layout_strip(&widths, room, opts);
    // Rows beyond what the area can spare stay off screen.
    layout.rows = layout.rows.min(area.height.saturating_sub(1).max(1));
    let strip = Rect {
        height: layout.rows,
        ..area
    };
    f.render_widget(Block::new().style(t.bg(strip_bg)), strip);
    // The visible stretch of the content, and where it starts on screen.
    let (view_x, view_w, scroll) = if layout.overflow {
        (strip.x + 1, room.saturating_sub(2).max(1), layout.scroll)
    } else {
        (strip.x, room, 0)
    };
    let mut hits = Vec::with_capacity(tabs.len());
    for (i, spans) in drawn.into_iter().enumerate() {
        let ((row, x), w) = (layout.places[i], widths[i]);
        let y = strip.y + row;
        let label_w = width(&spans[0].content);
        // Screen columns of [from, to) in content columns, cut to the view.
        let screen = |from: u16, to: u16| -> (u16, u16) {
            let a = from.max(scroll).min(scroll + view_w);
            let b = to.max(scroll).min(scroll + view_w);
            (view_x + a - scroll, view_x + b - scroll)
        };
        hits.push(TabHit {
            row: y,
            label: screen(x, x + w),
            close: screen(x + label_w, x + label_w + 1),
        });
        let (a, b) = screen(x, x + w);
        if row >= layout.rows || a >= b {
            continue;
        }
        // Cut on the left: the paragraph skips those columns.
        let skip = scroll.saturating_sub(x);
        f.render_widget(
            Paragraph::new(Line::from(spans)).scroll((0, skip)),
            Rect::new(a, y, b - a, 1),
        );
    }
    if layout.overflow {
        let end = scroll + view_w >= widths.iter().sum::<u16>();
        let arrow = |on: bool| if on { t.fg("ui.text") } else { t.fg("ui.dim") };
        let left = Rect::new(strip.x, strip.y, 1, 1);
        let right = Rect::new(view_x + view_w, strip.y, 1, 1);
        f.render_widget(Paragraph::new(Span::styled("◂", arrow(scroll > 0))), left);
        f.render_widget(Paragraph::new(Span::styled("▸", arrow(!end))), right);
        click::add(left, Click::TabScroll(false));
        click::add(right, Click::TabScroll(true));
    }
    (
        Rect {
            y: area.y + layout.rows,
            height: area.height - layout.rows,
            ..area
        },
        hits,
        layout.scroll,
    )
}

fn tool_label(app: &App, key: &str, name: &str, active: bool, action: Action) -> click::Item {
    let t = &app.theme;
    let style = if active {
        t.fg("ui.text.strong").add_modifier(Modifier::BOLD)
    } else {
        t.fg("ui.header")
    };
    (
        Span::styled(format!(" {key}: {name}  "), style),
        Some(Click::Action(action)),
    )
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
                    if tab.has_cursor()
                        && let Some((line, column, _)) = tab.position()
                    {
                        spans.push(Span::styled(format!("{line}:{column}   "), t.fg("ui.text")));
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
        let tool = |a: Action, name: &str, active: bool| tool_label(app, &km.tool_number(a), name, active, a);
        let mut items = vec![
            tool(Action::ToggleProject, "Project", left_active(LeftPanel::Project)),
            tool(Action::ToggleCommit, "Commit", left_active(LeftPanel::Commit)),
            tool(
                Action::ToggleFind,
                "Find",
                app.focus == Focus::Bottom && !app.log.visible && !app.shell_visible,
            ),
            tool(
                Action::ToggleTerminal,
                "Terminal",
                app.focus == Focus::Bottom && app.shell_visible,
            ),
            tool(Action::ToggleLog, "Git", app.focus == Focus::Bottom && app.log.visible),
            tool_label(app, "?", "Help", app.show_help, Action::Help),
        ];
        // Labels give way to the status on the right when the row is narrow.
        let status_w = Line::from(status_spans(app)).width();
        let width = |items: &[click::Item]| items.iter().map(|(s, _)| s.width()).sum::<usize>();
        while items.len() > 1 && width(&items) + status_w > area.width as usize {
            items.pop();
        }
        click::line(f, area, items, Alignment::Left, ratatui::style::Style::default());
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
    let actions = [
        Action::ToggleProject,
        Action::ToggleCommit,
        Action::FindInFiles,
        Action::Help,
    ];
    for (i, a) in actions.into_iter().enumerate() {
        let y = area.y + 2 * i as u16;
        if y < area.y + area.height {
            click::add(Rect::new(area.x, y, area.width, 1), Click::Action(a));
        }
    }
}

/// A line of key hints (looks with `keyhints = true`).
pub fn keyhints(f: &mut Frame, area: Rect, app: &App) {
    let t = &app.theme;
    let km = &app.keymap;
    let mut items: Vec<click::Item> = vec![(Span::raw(" "), None)];
    for (a, label) in [
        (Action::GotoFile, "files"),
        (Action::FindInFiles, "find in files"),
        (Action::ToggleCommit, "changes"),
        (Action::FocusNext, "focus"),
        (Action::Help, "help"),
    ] {
        let c = Some(Click::Action(a));
        items.push((Span::styled(km.hint(a), t.fg("ui.warning")), c));
        items.push((Span::styled(format!(" {label}"), t.fg("ui.dim")), c));
        items.push((Span::styled("    ", t.fg("ui.dim")), None));
    }
    click::line(f, area, items, Alignment::Left, t.bg("ui.bg"));
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
    use super::{StripOptions, TabLabel, tab_strip};
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
            hits = tab_strip(
                f,
                Rect::new(2, 0, 58, 3),
                &tabs,
                Some(0),
                true,
                &theme,
                StripOptions::default(),
            )
            .1;
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

#[cfg(test)]
mod strip_layout_tests {
    use super::{StripLayout, StripOptions, layout_strip};

    fn layout(widths: &[u16], room: u16, wrap: bool, scroll: u16, show: Option<usize>) -> StripLayout {
        let opts = StripOptions {
            wrap,
            reserve: 0,
            scroll,
            show,
        };
        layout_strip(widths, room, opts)
    }

    #[test]
    fn tabs_that_fit_sit_side_by_side() {
        let l = layout(&[10, 12, 8], 40, false, 5, Some(2));
        assert_eq!(l.places, vec![(0, 0), (0, 10), (0, 22)]);
        assert_eq!((l.rows, l.overflow, l.scroll), (1, false, 0));
    }

    #[test]
    fn one_row_scrolls_within_limits_and_to_a_tab_shown() {
        // 50 columns of tabs in 22: 20 between the arrows.
        let w = [10; 5];
        let l = layout(&w, 22, false, 0, None);
        assert!(l.overflow);
        assert_eq!(l.scroll, 0);
        assert_eq!(layout(&w, 22, false, 99, None).scroll, 30, "no further than the end");
        // The last tab shown: its end at the right edge.
        assert_eq!(layout(&w, 22, false, 0, Some(4)).scroll, 30);
        // A tab at the left of the view: its start at the left edge.
        assert_eq!(layout(&w, 22, false, 30, Some(1)).scroll, 10);
        // Already in view: no scrolling.
        assert_eq!(layout(&w, 22, false, 12, Some(2)).scroll, 12);
        // Wider than the view: its start.
        assert_eq!(layout(&[5, 40, 5], 22, false, 0, Some(1)).scroll, 5);
    }

    #[test]
    fn wrapping_fills_rows_in_order() {
        let l = layout(&[10, 10, 10, 25, 5], 30, true, 7, Some(4));
        assert_eq!(l.places, vec![(0, 0), (0, 10), (0, 20), (1, 0), (1, 25)]);
        assert_eq!((l.rows, l.overflow, l.scroll), (2, false, 0));
        // A tab wider than the room gets a row of its own.
        let l = layout(&[40, 5], 30, true, 0, None);
        assert_eq!(l.places, vec![(0, 0), (1, 0)]);
    }
}
