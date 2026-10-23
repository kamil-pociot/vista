//! Drawing. Every colour comes from the theme via [`crate::style::ThemeStyle`].

pub mod chrome;
pub mod click;
pub mod commit;
pub mod diff;
pub mod find;
pub mod goto;
pub mod help;
pub mod log;
pub mod project;
pub mod projects;
pub mod sidebar;
pub mod term;
pub mod viewer;

use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Paragraph};
use vista_theme::{Borders, PanelHeaders, Spacing, ToolWindows};

use crate::app::{App, Focus, LeftPanel};
use crate::keymap::Action;
use crate::style::ThemeStyle;

/// The tree / Commit panel is at least this wide when resized…
pub const MIN_LEFT: u16 = 16;
/// …and leaves the editor at least this much.
pub const MIN_MAIN: u16 = 20;

/// Screen regions from the last draw, used for mouse hit-testing.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Areas {
    pub top: Rect,
    pub rail: Option<Rect>,
    pub left: Option<Rect>,
    /// Content area of the left tool window (below its header).
    pub left_inner: Option<Rect>,
    pub separator: Option<Rect>,
    /// The tab strip and content of the viewer (last draw).
    pub tabs: Option<Rect>,
    pub viewer: Option<Rect>,
    /// The result list of the Go to File popup.
    pub goto_list: Option<Rect>,
    /// The bottom tool window (Find results).
    pub panel_bottom: Option<Rect>,
    pub find_list: Option<Rect>,
    pub find_window: Option<Rect>,
    pub commit_list: Option<Rect>,
    pub commit_message: Option<Rect>,
    /// "Select Opened File" (⌖) in the Project panel's header.
    pub reveal_button: Option<Rect>,
    /// The entry list of the Projects popup.
    pub projects_list: Option<Rect>,
    /// The Git window's commit and file lists.
    pub log_commits: Option<Rect>,
    /// The Terminal tool window's screen.
    pub shell: Option<Rect>,
    pub log_files: Option<Rect>,
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
    // Bottom tool window (Find) spans the full width under the panels.
    let mut bottom_panel = None;
    let find = app.find_window.visible && !app.find_window.tabs.is_empty();
    if (find || app.log.visible || app.shell_visible) && body.height > 12 {
        let h = (body.height * 35 / 100).max(8);
        bottom_panel = Some(Rect {
            y: body.y + body.height - h,
            height: h,
            ..body
        });
        body.height -= h;
    }
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
            tabs: None,
            viewer: None,
            goto_list: None,
            panel_bottom: bottom_panel,
            find_list: None,
            find_window: None,
            commit_list: None,
            commit_message: None,
            reveal_button: None,
            projects_list: None,
            log_commits: None,
            log_files: None,
            shell: None,
            main: body,
            bottom,
            keyhints,
        };
    };
    // Dragged / Ctrl-Shift-←→ width (remembered), else about a quarter.
    let left_w = match app.ui.borrow().layout.left_width {
        Some(w) => w.clamp(MIN_LEFT, body.width.saturating_sub(MIN_MAIN).max(MIN_LEFT)),
        None => (body.width as u32 * 28 / 100).clamp(24, 48).min(body.width as u32 / 2) as u16,
    };
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
        tabs: None,
        viewer: None,
        goto_list: None,
        panel_bottom: bottom_panel,
        find_list: None,
        find_window: None,
        commit_list: None,
        commit_message: None,
        reveal_button: None,
        projects_list: None,
        log_commits: None,
        log_files: None,
        shell: None,
        main,
        bottom,
        keyhints,
    }
}

/// Draws project `app` in `area` (the window, or what the sidebar leaves).
pub fn draw(f: &mut Frame, area: Rect, app: &mut App) {
    f.render_widget(Block::new().style(app.theme.base()), area);
    click::begin();
    let mut areas = compute_areas(area, app);
    // Regions recorded while drawing (cleared first, kept at the end).
    app.areas.commit_list = None;
    app.areas.commit_message = None;

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
    app.areas.tabs = None;
    app.areas.viewer = None;
    app.areas.find_window = None;
    main_area(f, areas.main, app);
    if app.change_hop.is_some() {
        viewer::draw_change_hop(f, areas.main, app);
    }
    app.areas.log_commits = None;
    app.areas.log_files = None;
    app.areas.shell = None;
    if let Some(b) = areas.panel_bottom {
        if app.shell_visible {
            term::draw_panel(f, b, app);
        } else if app.log.visible {
            log::draw(f, b, app);
        } else {
            find::draw_window(f, b, app);
        }
    }
    chrome::bottom_bar(f, areas.bottom, app);
    if let Some(k) = areas.keyhints {
        chrome::keyhints(f, k, app);
    }
    // Buttons under a popup can't be clicked.
    let under_popups = click::mark();
    let popup = app.recent_popup.is_some()
        || app.goto.is_some()
        || app.find.is_some()
        || app.projects_popup.is_some()
        || app.confirm.is_some()
        || app.show_help;
    if app.recent_popup.is_some() {
        viewer::draw_recent(f, area, app);
    }
    app.areas.goto_list = None;
    if app.goto.is_some() {
        goto::draw(f, area, app);
    }
    app.areas.find_list = None;
    if app.find.is_some() {
        find::draw_popup(f, area, app);
    }
    app.areas.projects_list = None;
    if app.projects_popup.is_some() {
        projects::draw_popup(f, area, app);
    }
    if app.confirm.is_some() {
        commit::draw_confirm(f, area, app);
    }
    if app.show_help {
        help::draw(f, area, app);
    }
    if popup {
        click::only_since(under_popups);
    }
    app.buttons = click::take();
    let (tabs, viewer_area, goto_list) = (app.areas.tabs, app.areas.viewer, app.areas.goto_list);
    let (find_list, find_window) = (app.areas.find_list, app.areas.find_window);
    let (commit_list, commit_message) = (app.areas.commit_list, app.areas.commit_message);
    let (reveal_button, projects_list) = (app.areas.reveal_button, app.areas.projects_list);
    let (log_commits, log_files, shell) = (app.areas.log_commits, app.areas.log_files, app.areas.shell);
    app.areas = Areas {
        tabs,
        viewer: viewer_area,
        goto_list,
        find_list,
        find_window,
        commit_list,
        commit_message,
        reveal_button,
        projects_list,
        log_commits,
        log_files,
        shell,
        ..areas
    };
}

/// The top bar (project and terminal tabs, hints) on its own, for screens
/// that aren't a project (terminal tabs).
pub fn top_bar(f: &mut Frame, area: Rect, app: &mut App) {
    app.areas.top = area;
    f.render_widget(Block::new().style(app.theme.bg("ui.panel")), area);
    // The right side (hints, branch) first: project tabs draw over it when
    // there are many.
    right_of_top_bar(f, area, app);
    app.project_hits = projects::tabs(f, area, app);
}

fn right_of_top_bar(f: &mut Frame, area: Rect, app: &App) {
    let t = &app.theme;
    let km = &app.keymap;
    let hint = |a: Action, label: &str| -> Vec<click::Item> {
        let c = Some(click::Click::Action(a));
        vec![
            (Span::styled(km.hint(a), t.fg("ui.dim")), c),
            (Span::styled(format!(" {label}"), t.fg("ui.text")), c),
            (Span::raw("   "), None),
        ]
    };
    let mut items = hint(Action::GotoFile, "Go to File");
    items.extend(hint(Action::FindInFiles, "Find in Files"));
    if app.git.is_some() {
        // The branch opens the Git window.
        let c = Some(click::Click::Action(Action::ToggleLog));
        let b = &app.status.branch;
        items.push((Span::styled("⎇ ", t.fg("ui.dim")), c));
        items.push((Span::styled(b.label(), t.fg("ui.text")), c));
        if b.ahead > 0 {
            items.push((Span::styled(format!(" ↑{}", b.ahead), t.fg("git.added")), c));
        }
        if b.behind > 0 {
            items.push((Span::styled(format!(" ↓{}", b.behind), t.fg("git.modified")), c));
        }
        items.push((Span::raw(" "), None));
    }
    click::line(f, area, items, Alignment::Right, ratatui::style::Style::default());
}

fn left_panel(f: &mut Frame, area: Rect, app: &mut App) -> Rect {
    let focused = app.focus == Focus::Left;
    // The number is the window's key (Alt-1, Alt-2), as in IntelliJ.
    let (title, bg) = match app.left {
        Some(LeftPanel::Commit) => (
            format!("{}: Commit", app.keymap.tool_number(Action::ToggleCommit)),
            "ui.panel.alt",
        ),
        _ => (
            format!("{}: Project", app.keymap.tool_number(Action::ToggleProject)),
            "ui.panel.alt",
        ),
    };
    let inner = chrome::panel(f, area, &title, focused, bg, &app.theme);
    app.areas.reveal_button = None;
    if app.left == Some(LeftPanel::Project) {
        app.areas.reveal_button = reveal_button(f, area, app);
    }
    match app.left {
        Some(LeftPanel::Project) => project::draw(f, inner, app),
        Some(LeftPanel::Commit) => commit::draw(f, inner, app),
        None => {}
    }
    inner
}

/// IntelliJ's "Select Opened File" (⌖) at the right of the Project header;
/// dimmed when no file is open.
fn reveal_button(f: &mut Frame, area: Rect, app: &App) -> Option<Rect> {
    // Inside the frame's corner for boxed looks.
    let inset = u16::from(app.theme.chrome.panel_headers == PanelHeaders::BorderTitle);
    if area.width < 16 || area.height == 0 {
        return None;
    }
    let rect = Rect {
        x: area.x + area.width - 3 - inset,
        y: area.y,
        width: 3,
        height: 1,
    };
    let tok = if app.viewer.active_tab().is_some() {
        "ui.accent"
    } else {
        "ui.dim"
    };
    f.render_widget(Paragraph::new(Span::styled(" ⌖ ", app.theme.fg(tok))), rect);
    Some(rect)
}

fn main_area(f: &mut Frame, area: Rect, app: &mut App) {
    // Looks with boxed panels frame the editor area too.
    let inner = if app.theme.chrome.panel_headers == PanelHeaders::BorderTitle {
        chrome::frame(f, area, app.focus == Focus::Main, &app.theme)
    } else {
        area
    };
    viewer::draw(f, inner, app);
}

pub(crate) fn empty_state(f: &mut Frame, area: Rect, app: &App) {
    let t = &app.theme;
    let km = &app.keymap;
    let actions = [
        (Action::GotoFile, "Go to File"),
        (Action::FindInFiles, "Find in Files"),
        (Action::ToggleProject, "Project"),
        (Action::ToggleCommit, "Commit"),
        (Action::Help, "Help"),
    ];
    let h = actions.len() as u16 + 2;
    let y = area.y + area.height.saturating_sub(h) / 2;
    let row = |i: u16| Rect {
        y: y + i,
        height: 1,
        ..area
    };
    if y + h > area.y + area.height {
        return;
    }
    f.render_widget(
        Paragraph::new(Line::styled("No file open", t.fg("ui.text"))).centered(),
        row(0),
    );
    // Each row runs its action when clicked.
    for (i, (a, label)) in actions.into_iter().enumerate() {
        let c = Some(click::Click::Action(a));
        let items = vec![
            (Span::styled(format!("{:>12}  ", km.hint(a)), t.fg("ui.accent")), c),
            (Span::styled(format!("{label:<16}"), t.fg("ui.dim")), c),
        ];
        click::line(
            f,
            row(i as u16 + 2),
            items,
            Alignment::Center,
            ratatui::style::Style::default(),
        );
    }
}
