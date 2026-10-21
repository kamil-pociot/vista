//! Drawing. Every colour comes from the theme via [`crate::style::ThemeStyle`].

pub mod chrome;
pub mod help;
pub mod project;

use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Paragraph};
use vista_theme::{Borders, PanelHeaders, Spacing, ToolWindows};

use crate::app::{App, Focus, LeftPanel};
use crate::keymap::Action;
use crate::style::ThemeStyle;

/// Screen regions from the last draw, used for mouse hit-testing.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Areas {
    pub top: Rect,
    pub rail: Option<Rect>,
    pub left: Option<Rect>,
    /// Content area of the left tool window (below its header).
    pub left_inner: Option<Rect>,
    pub separator: Option<Rect>,
    pub main: Rect,
    pub bottom: Rect,
    pub keyhints: Option<Rect>,
}

pub fn compute_areas(area: Rect, app: &App) -> Areas {
    let chrome = app.theme.chrome;
    let mut rows = vec![Constraint::Length(1), Constraint::Min(1), Constraint::Length(1)];
    if chrome.keyhints {
        rows.push(Constraint::Length(1));
    }
    let v = Layout::default()
        .direction(Direction::Vertical)
        .constraints(rows)
        .split(area);
    let (top, mut body, bottom) = (v[0], v[1], v[2]);
    let keyhints = chrome.keyhints.then(|| v[3]);

    let mut rail = None;
    if chrome.tool_windows == ToolWindows::IconRail && body.width > 20 {
        rail = Some(Rect { width: 4, ..body });
        body.x += 4;
        body.width -= 4;
    }
    if chrome.spacing == Spacing::Comfortable && body.width > 10 {
        body.x += 1;
        body.width -= 2;
    }

    let Some(_) = app.left else {
        return Areas {
            top,
            rail,
            left: None,
            left_inner: None,
            separator: None,
            main: body,
            bottom,
            keyhints,
        };
    };
    let left_w = (body.width as u32 * 28 / 100).clamp(24, 48).min(body.width as u32 / 2) as u16;
    let gap = if chrome.panel_headers == PanelHeaders::BorderTitle || chrome.borders == Borders::None {
        u16::from(chrome.spacing == Spacing::Comfortable)
    } else {
        1
    };
    let left = Rect { width: left_w, ..body };
    let separator = (gap > 0).then(|| Rect {
        x: body.x + left_w,
        width: gap,
        ..body
    });
    let main = Rect {
        x: body.x + left_w + gap,
        width: body.width.saturating_sub(left_w + gap),
        ..body
    };
    Areas {
        top,
        rail,
        left: Some(left),
        left_inner: None,
        separator,
        main,
        bottom,
        keyhints,
    }
}

pub fn draw(f: &mut Frame, app: &mut App) {
    let area = f.area();
    f.render_widget(Block::new().style(app.theme.base()), area);
    let mut areas = compute_areas(area, app);

    top_bar(f, areas.top, app);
    if let Some(rail) = areas.rail {
        chrome::icon_rail(f, rail, app);
    }
    if let Some(left) = areas.left {
        areas.left_inner = Some(left_panel(f, left, app));
    }
    if let Some(sep) = areas.separator {
        chrome::separator(f, sep, &app.theme);
    }
    main_area(f, areas.main, app);
    chrome::bottom_bar(f, areas.bottom, app);
    if let Some(k) = areas.keyhints {
        chrome::keyhints(f, k, app);
    }
    if app.show_help {
        help::draw(f, area, app);
    }
    app.areas = areas;
}

fn top_bar(f: &mut Frame, area: Rect, app: &App) {
    let t = &app.theme;
    f.render_widget(Block::new().style(t.bg("ui.panel")), area);
    let left = Line::from(vec![Span::styled(
        format!(" {}", app.project_name),
        t.fg("ui.text.strong").add_modifier(crate::style::bold()),
    )]);
    f.render_widget(Paragraph::new(left), area);
    let km = &app.keymap;
    let hint = |a: Action, label: &str| {
        vec![
            Span::styled(km.hint(a), t.fg("ui.dim")),
            Span::styled(format!(" {label}   "), t.fg("ui.text")),
        ]
    };
    let mut spans = hint(Action::GotoFile, "Go to File");
    spans.extend(hint(Action::FindInFiles, "Find in Files"));
    if app.git.is_some() {
        let b = &app.status.branch;
        spans.push(Span::styled("⎇ ", t.fg("ui.dim")));
        spans.push(Span::styled(b.label(), t.fg("ui.text")));
        if b.ahead > 0 {
            spans.push(Span::styled(format!(" ↑{}", b.ahead), t.fg("git.added")));
        }
        if b.behind > 0 {
            spans.push(Span::styled(format!(" ↓{}", b.behind), t.fg("git.modified")));
        }
        spans.push(Span::raw(" "));
    }
    f.render_widget(Paragraph::new(Line::from(spans)).right_aligned(), area);
}

fn left_panel(f: &mut Frame, area: Rect, app: &mut App) -> Rect {
    let focused = app.focus == Focus::Left;
    let (title, bg) = match app.left {
        Some(LeftPanel::Commit) => ("0: Commit", "ui.panel.alt"),
        _ => ("1: Project", "ui.panel.alt"),
    };
    let inner = chrome::panel(f, area, title, focused, bg, &app.theme);
    if app.left == Some(LeftPanel::Project) {
        project::draw(f, inner, app);
    }
    inner
}

fn main_area(f: &mut Frame, area: Rect, app: &App) {
    let t = &app.theme;
    let focused = app.focus == Focus::Main;
    let inner = chrome::tab_strip(f, area, &[], None, focused, t);
    empty_state(f, inner, app);
}

fn empty_state(f: &mut Frame, area: Rect, app: &App) {
    let t = &app.theme;
    let km = &app.keymap;
    let row = |a: Action, label: &str| {
        Line::from(vec![
            Span::styled(format!("{:>12}  ", km.hint(a)), t.fg("ui.accent")),
            Span::styled(format!("{label:<16}"), t.fg("ui.dim")),
        ])
    };
    let lines = vec![
        Line::styled("No file open", t.fg("ui.text")),
        Line::raw(""),
        row(Action::GotoFile, "Go to File"),
        row(Action::FindInFiles, "Find in Files"),
        row(Action::ToggleProject, "Project"),
        row(Action::ToggleCommit, "Commit"),
        row(Action::Help, "Help"),
    ];
    let h = lines.len() as u16;
    let y = area.y + area.height.saturating_sub(h) / 2;
    let rect = Rect {
        y,
        height: h.min(area.height),
        ..area
    };
    f.render_widget(Paragraph::new(lines).centered(), rect);
}
