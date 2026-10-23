//! The sidebar (herdr-style, `Alt-B`): open projects on top; terminal tabs
//! and Claude sessions below, each session marked with its state.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Paragraph};
use unicode_width::UnicodeWidthStr;
use vista_theme::Theme;

use crate::sessions::Session;
use crate::style::ThemeStyle;

/// What a sidebar row opens.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SidebarHit {
    Project(usize),
    Terminal(usize),
    /// Index into the sessions list.
    Session(usize),
    /// The `+` by "Projects": the Projects popup.
    NewProject,
}

pub struct Item<'a> {
    pub name: &'a str,
    /// On screen now.
    pub active: bool,
    /// Asked for attention (a notification) while off screen.
    pub waiting: bool,
    /// A project's git branch, shown under its name.
    pub branch: Option<&'a vista_git::BranchInfo>,
}

pub struct View<'a> {
    pub projects: Vec<Item<'a>>,
    /// Terminal tabs not attached to a session.
    pub terminals: Vec<(usize, Item<'a>)>,
    pub sessions: &'a [Session],
    /// The session whose tab is on screen.
    pub shown_session: Option<&'a str>,
    /// Why there are no sessions (the command failed), if it did.
    pub note: Option<&'a str>,
}

/// Cuts `s` to `w` columns, ending in `…` when cut.
fn fit(s: &str, w: usize) -> String {
    if s.width() <= w {
        return s.to_string();
    }
    let mut out = String::new();
    for c in s.chars() {
        if out.width() + c.to_string().width() + 1 > w {
            break;
        }
        out.push(c);
    }
    out.push('…');
    out
}

/// Draws the sidebar; returns each clickable row.
pub fn draw(f: &mut Frame, area: Rect, view: &View, t: &Theme) -> Vec<(Rect, SidebarHit)> {
    let mut hits = Vec::new();
    if area.width < 8 || area.height == 0 {
        return hits;
    }
    f.render_widget(Block::new().style(t.bg("ui.panel")), area);
    // The last column is the border to the right.
    let border = Rect {
        x: area.x + area.width - 1,
        width: 1,
        ..area
    };
    let lines: Vec<Line> = (0..border.height).map(|_| Line::raw("│")).collect();
    f.render_widget(Paragraph::new(lines).style(t.fg("ui.border")), border);
    let inner = Rect {
        width: area.width - 1,
        ..area
    };
    let w = inner.width as usize;
    let mut y = inner.y;
    let bottom = inner.y + inner.height;
    let mut row = |f: &mut Frame, line: Line, hit: Option<SidebarHit>, style: Style| {
        if y >= bottom {
            return;
        }
        let r = Rect { y, height: 1, ..inner };
        f.render_widget(Paragraph::new(line).style(style), r);
        if let Some(h) = hit {
            hits.push((r, h));
        }
        y += 1;
    };
    let header = t.fg("ui.header").add_modifier(Modifier::BOLD);
    let selected = |on: bool| {
        if on {
            t.bg("ui.selection.inactive")
        } else {
            Style::default()
        }
    };
    // A row: mark, name, and something dim on the right.
    let entry = |mark: (&str, &str), name: &str, right: &str, active: bool| {
        let right_w = right.width().min(w / 3);
        let right = fit(right, right_w);
        let name_w = w.saturating_sub(4 + right.width() + 1);
        let name = fit(name, name_w);
        let pad = w.saturating_sub(3 + name.width() + right.width() + 1);
        let name_style = if active {
            t.fg("ui.text.strong").add_modifier(Modifier::BOLD)
        } else {
            t.fg("ui.text")
        };
        Line::from(vec![
            Span::styled(format!(" {} ", mark.0), t.fg(mark.1)),
            Span::styled(name, name_style),
            Span::raw(" ".repeat(pad)),
            Span::styled(right, t.fg("ui.dim")),
            Span::raw(" "),
        ])
    };

    let plus = format!("{:>width$}", "+ ", width = w.saturating_sub(" Projects".len()));
    row(
        f,
        Line::from(vec![
            Span::styled(" Projects", header),
            Span::styled(plus, t.fg("ui.accent")),
        ]),
        Some(SidebarHit::NewProject),
        Style::default(),
    );
    for (i, p) in view.projects.iter().enumerate() {
        let mark = if p.waiting {
            ("●", "ui.warning")
        } else {
            ("▸", "ui.dim")
        };
        let hit = Some(SidebarHit::Project(i));
        row(f, entry(mark, p.name, "", p.active), hit, selected(p.active));
        // Its branch below, with the commits it is ahead of / behind upstream.
        if let Some(b) = p.branch {
            let counts: String = [(b.ahead, '↑'), (b.behind, '↓')]
                .iter()
                .filter(|(n, _)| *n > 0)
                .map(|(n, arrow)| format!(" {arrow}{n}"))
                .collect();
            let name = fit(&b.label(), w.saturating_sub(6 + counts.width()));
            let mut spans = vec![Span::styled(format!("   ⎇ {name}"), t.fg("ui.dim"))];
            if b.ahead > 0 {
                spans.push(Span::styled(format!(" ↑{}", b.ahead), t.fg("git.added")));
            }
            if b.behind > 0 {
                spans.push(Span::styled(format!(" ↓{}", b.behind), t.fg("git.modified")));
            }
            row(f, Line::from(spans), hit, selected(p.active));
        }
    }
    row(f, Line::raw(""), None, Style::default());
    row(f, Line::styled(" Claude", header), None, Style::default());
    for (i, tab) in &view.terminals {
        let mark = if tab.waiting {
            ("●", "ui.warning")
        } else {
            ("▶", "ui.dim")
        };
        row(
            f,
            entry(mark, tab.name, "", tab.active),
            Some(SidebarHit::Terminal(*i)),
            selected(tab.active),
        );
    }
    // A session takes two rows: its name, then its folder.
    for (i, s) in view.sessions.iter().enumerate() {
        let folder = s
            .cwd
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let active = view.shown_session == Some(s.id.as_str());
        let hit = Some(SidebarHit::Session(i));
        row(f, entry(s.state.mark(), &s.name, "", active), hit, selected(active));
        let folder = Line::styled(format!("   {}", fit(&folder, w.saturating_sub(4))), t.fg("ui.dim"));
        row(f, folder, hit, selected(active));
    }
    if let Some(note) = view.note {
        row(
            f,
            Line::styled(format!(" {}", fit(note, w - 1)), t.fg("ui.dim")),
            None,
            Style::default(),
        );
    } else if view.sessions.is_empty() {
        row(f, Line::styled(" no sessions", t.fg("ui.dim")), None, Style::default());
    }
    hits
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sessions::SessionState;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    #[test]
    fn rows_show_projects_tabs_and_sessions_where_their_hits_are() {
        let theme = Theme::default_theme();
        let branch = vista_git::BranchInfo {
            head: Some("main".into()),
            ahead: 2,
            behind: 1,
            ..Default::default()
        };
        let sessions = vec![
            Session {
                id: "a1".into(),
                name: "product-owner".into(),
                cwd: "/r/lms-context".into(),
                state: SessionState::Blocked,
            },
            Session {
                id: "b2".into(),
                name: "a-very-long-session-name-that-is-cut".into(),
                cwd: "/r/fb".into(),
                state: SessionState::Working,
            },
        ];
        let view = View {
            projects: vec![
                Item {
                    name: "file-browser",
                    active: true,
                    waiting: false,
                    branch: Some(&branch),
                },
                Item {
                    name: "lms-context",
                    active: false,
                    waiting: true,
                    branch: None,
                },
            ],
            terminals: vec![(
                0,
                Item {
                    name: "claude agents",
                    active: false,
                    waiting: false,
                    branch: None,
                },
            )],
            sessions: &sessions,
            shown_session: None,
            note: None,
        };
        let mut term = Terminal::new(TestBackend::new(30, 16)).unwrap();
        let mut hits = Vec::new();
        term.draw(|f| hits = draw(f, Rect::new(0, 0, 30, 16), &view, &theme))
            .unwrap();
        let buf = term.backend().buffer().clone();
        let line = |y: u16| (0..30).map(|x| buf[(x, y)].symbol().to_string()).collect::<String>();
        let at = |h: SidebarHit| hits.iter().find(|(_, x)| *x == h).map(|(r, _)| r.y).unwrap();
        assert!(line(at(SidebarHit::NewProject)).starts_with(" Projects"));
        let p0 = at(SidebarHit::Project(0));
        assert!(line(p0).contains("▸ file-browser"));
        assert!(line(p0 + 1).starts_with("   ⎇ main ↑2 ↓1"), "{}", line(p0 + 1));
        assert!(line(at(SidebarHit::Project(1))).contains("● lms-context"));
        assert!(line(at(SidebarHit::Terminal(0))).contains("▶ claude agents"));
        // A session's folder is on the row under its name.
        let s0 = at(SidebarHit::Session(0));
        assert!(line(s0).contains("● product-owner"), "{}", line(s0));
        assert!(line(s0 + 1).starts_with("   lms-context"), "{}", line(s0 + 1));
        let s1 = line(at(SidebarHit::Session(1)));
        assert!(s1.contains("◐ a-very-long") && s1.contains('…'), "{s1}");
        let both = hits.iter().filter(|(_, h)| *h == SidebarHit::Session(0)).count();
        assert_eq!(both, 2, "both rows open the session");
        assert!(line(at(SidebarHit::Session(1))).ends_with('│'));
        // Marks take the state's colour.
        let y = at(SidebarHit::Session(0));
        assert_eq!(Some(buf[(1, y)].fg), theme.fg("ui.warning").fg);
    }
}
