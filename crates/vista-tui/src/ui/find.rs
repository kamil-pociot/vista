//! Drawing the Find in Files popup and the Find tool window.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Modifier;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use unicode_width::UnicodeWidthStr;
use vista_render::StyledLine;
use vista_render::styled::{StyledSpan, TokStyle, expand_tabs, overlay, slice_cols};

use crate::app::{App, Focus};
use crate::find::{Field, ScopeKind, WindowRow};
use crate::style::{ThemeStyle, styled_line};

/// A hit's line with matches highlighted, leading whitespace trimmed.
fn hit_line(text: &str, ranges: &[(usize, usize)], width: usize) -> StyledLine {
    let trimmed = text.trim_start();
    let cut = text.chars().count() - trimmed.chars().count();
    let shifted: Vec<(usize, usize)> = ranges
        .iter()
        .map(|(a, b)| (a.saturating_sub(cut), b.saturating_sub(cut)))
        .collect();
    let line = vec![StyledSpan::new(trimmed.replace('\t', " "), TokStyle::fg("ui.text"))];
    let line = overlay(&line, &shifted, |s| {
        s.bg = Some("ui.match");
        s.fg = "ui.match.text";
    });
    slice_cols(&line, 0, width)
}

fn toggle(label: &str, on: bool, t: &vista_theme::Theme) -> Span<'static> {
    if on {
        Span::styled(format!(" {label} "), t.fg_bg("ui.button.text", "ui.button"))
    } else {
        Span::styled(format!(" {label} "), t.fg("ui.dim"))
    }
}

/// Lines of the file around `line` (1-based), highlighted, with the hit
/// ranges marked.
/// The last preview: (path, mtime, first line index, highlighted lines, line has a tab).
type PreviewCache = Option<(
    std::path::PathBuf,
    Option<std::time::SystemTime>,
    usize,
    Vec<StyledLine>,
    Vec<bool>,
)>;
static PREVIEW: std::sync::Mutex<PreviewCache> = std::sync::Mutex::new(None);

/// Lines highlighted before the visible window so multi-line constructs
/// (comments, fences) usually start in the right state.
const PREVIEW_LEAD: usize = 60;

fn preview(
    path: &std::path::Path,
    line: usize,
    height: usize,
    ranges: &[(usize, usize)],
    highlight: bool,
) -> Vec<(usize, StyledLine)> {
    if !highlight {
        // While results stream in, the selection moves often: show plain text.
        let Ok(bytes) = std::fs::read(path) else {
            return Vec::new();
        };
        let start = line.saturating_sub(height / 2 + 1);
        return String::from_utf8_lossy(&bytes)
            .lines()
            .enumerate()
            .skip(start)
            .take(height)
            .map(|(i, l)| (i + 1, vista_render::styled::plain(&expand_tabs(l, 4))))
            .collect();
    }
    let mtime = std::fs::metadata(path).and_then(|m| m.modified()).ok();
    let start = line.saturating_sub(height / 2 + 1);
    let mut cache = PREVIEW.lock().unwrap_or_else(|e| e.into_inner());
    // Reuse the cache when it covers the visible window of the same file.
    let fresh = cache.as_ref().is_some_and(|(p, t, from, lines, _)| {
        p == path && *t == mtime && *from <= start && start + height <= from + lines.len().max(1)
    });
    if !fresh {
        let Ok(bytes) = std::fs::read(path) else {
            return Vec::new();
        };
        let text = String::from_utf8_lossy(&bytes);
        let from = start.saturating_sub(PREVIEW_LEAD);
        let window: Vec<&str> = text.lines().skip(from).take(start - from + height * 3).collect();
        let chunk = window.join("\n");
        let lines = vista_render::Highlighter::global().highlight_file(path, &chunk);
        let tabs = window.iter().map(|l| l.contains('\t')).collect();
        *cache = Some((path.to_path_buf(), mtime, from, lines, tabs));
    }
    let Some((_, _, from, lines, tabs)) = cache.as_ref() else {
        return Vec::new();
    };
    let from = *from;
    lines
        .iter()
        .enumerate()
        .map(|(k, l)| (k + from, l))
        .skip_while(|(i, _)| *i < start)
        .take(height)
        .map(|(i, l)| {
            // Matches are char ranges into the raw line; the highlighter
            // expanded tabs, so only mark lines without tabs.
            if i + 1 == line && !tabs.get(i - from).copied().unwrap_or(false) {
                let marked = overlay(l, ranges, |s| {
                    s.bg = Some("ui.match");
                    s.fg = "ui.match.text";
                });
                (i + 1, marked)
            } else {
                (i + 1, l.clone())
            }
        })
        .collect()
}

pub fn draw_popup(f: &mut Frame, area: Rect, app: &mut App) {
    if app.find.is_none() {
        return;
    }
    let t = app.theme.clone();
    let t = &t;
    let w = area.width.saturating_sub(6).clamp(40, 130);
    let h = area.height.saturating_sub(2).clamp(12, 44);
    let rect = Rect {
        x: area.x + (area.width.saturating_sub(w)) / 2,
        y: area.y + 1.min(area.height.saturating_sub(h)),
        width: w.min(area.width),
        height: h.min(area.height),
    };
    f.render_widget(Clear, rect);
    let block = Block::new()
        .borders(Borders::ALL)
        .border_style(t.fg("ui.popup.border"))
        .style(t.fg_bg("ui.text", "ui.popup"));
    let inner = block.inner(rect);
    f.render_widget(block, rect);
    if inner.height < 8 {
        return;
    }
    let root = app.root.clone();
    let p = app.find.as_mut().expect("popup");
    let row = |i: u16| Rect {
        y: inner.y + i,
        height: 1,
        ..inner
    };

    // Title + counts + mask.
    f.render_widget(
        Paragraph::new(Span::styled(
            " Find in Files",
            t.fg("ui.text.strong").add_modifier(Modifier::BOLD),
        )),
        row(0),
    );
    let mut right = Vec::new();
    match (&p.error, &p.summary) {
        (Some(e), _) => right.push(Span::styled(format!("{e}   "), t.fg("ui.error"))),
        (None, _) if p.running => right.push(Span::styled("searching…   ".to_string(), t.fg("ui.dim"))),
        (None, Some(s)) if !p.query.is_empty() => right.push(Span::styled(
            format!(
                "{}{} matches in {} files   ",
                if s.truncated { "1000+ " } else { "" },
                if s.truncated {
                    String::new()
                } else {
                    p.results.len().to_string()
                },
                p.file_count()
            ),
            t.fg("ui.dim"),
        )),
        _ => {}
    }
    right.push(Span::styled("File mask: ", t.fg("ui.dim")));
    let mask_style = if p.field == Field::Mask {
        t.fg_bg("ui.text.strong", "ui.input")
    } else {
        t.fg_bg("ui.text", "ui.input")
    };
    let mask_text = if p.mask.is_empty() && p.field != Field::Mask {
        "*".to_string()
    } else {
        p.mask.clone()
    };
    right.push(Span::styled(
        format!(" {mask_text}{} ", if p.field == Field::Mask { "▏" } else { "" }),
        mask_style,
    ));
    f.render_widget(Paragraph::new(Line::from(right)).right_aligned(), row(0));

    // Scopes.
    let mut scopes = vec![Span::raw(" ")];
    for s in ScopeKind::ALL {
        let style = if s == p.scope {
            t.fg("ui.text.strong")
                .add_modifier(Modifier::BOLD | Modifier::UNDERLINED)
        } else {
            t.fg("ui.text")
        };
        scopes.push(Span::styled(format!(" {} ", s.label()), style));
        scopes.push(Span::raw(" "));
    }
    if p.scope == ScopeKind::Directory {
        let d = p.dir.strip_prefix(&root).unwrap_or(&p.dir).display().to_string();
        scopes.push(Span::styled(
            format!("  {}/", if d.is_empty() { "." } else { &d }),
            t.fg("ui.dim"),
        ));
    }
    f.render_widget(Paragraph::new(Line::from(scopes)), row(1));

    // Query + toggles.
    let qstyle = if p.fresh {
        t.fg_bg("ui.text.strong", "ui.selection")
    } else {
        t.fg("ui.text.strong")
    };
    let caret = if p.field == Field::Query { "▏" } else { "" };
    f.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(" ⌕ ", t.fg("ui.dim")),
            Span::styled(p.query.clone(), qstyle),
            Span::styled(caret, t.fg("ui.text")),
        ]))
        .style(t.bg("ui.input")),
        row(2),
    );
    f.render_widget(
        Paragraph::new(Line::from(vec![
            toggle("Cc", p.case, t),
            Span::raw(" "),
            toggle("W", p.word, t),
            Span::raw(" "),
            toggle(".*", p.regex, t),
            Span::raw(" "),
        ]))
        .right_aligned(),
        row(2),
    );

    // Results, separator, preview, footer.
    let body = inner.height - 4;
    let list_h = (body * 2 / 5).max(3);
    let prev_h = body.saturating_sub(list_h + 1);
    p.view_height = list_h as usize;
    let list = Rect {
        y: inner.y + 3,
        height: list_h,
        ..inner
    };
    let mut lines = Vec::new();
    for (i, h) in p.results.iter().enumerate().skip(p.scroll).take(list_h as usize) {
        let name = h
            .path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let token = app.status.file_status(&h.path).map(|s| s.token()).unwrap_or("ui.dim");
        let right = format!("  {name} {} ", h.line);
        let width = (list.width as usize).saturating_sub(right.width() + 1);
        let mut spans = vec![Span::raw(" ")];
        let text = expand_tabs(&h.text, 4);
        let hl = hit_line(&text, &h.ranges, width);
        let used = vista_render::styled::line_width(&hl) + 1;
        spans.extend(styled_line(t, &hl).spans);
        spans.push(Span::raw(
            " ".repeat((list.width as usize).saturating_sub(used + right.width())),
        ));
        spans.push(Span::styled(format!("  {name}"), t.fg(token)));
        spans.push(Span::styled(format!(" {} ", h.line), t.fg("ui.dim")));
        let mut l = Line::from(spans);
        if i == p.selected {
            l = l.style(t.bg("ui.selection"));
        }
        lines.push(l);
    }
    if p.results.is_empty() && !p.running && p.error.is_none() && !p.query.is_empty() {
        lines.push(Line::styled(" Nothing found", t.fg("ui.dim")));
    }
    f.render_widget(Paragraph::new(lines), list);
    app.areas.find_list = Some(list);

    let p = app.find.as_ref().expect("popup");
    if let Some(h) = p.selected_hit() {
        let sep = Rect {
            y: list.y + list_h,
            height: 1,
            ..inner
        };
        let rel = h.path.strip_prefix(&root).unwrap_or(&h.path).display().to_string();
        let n = p.results.iter().filter(|x| x.path == h.path).count();
        f.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled(format!(" {rel}"), t.fg("ui.text")),
                Span::styled(
                    format!("   {n} occurrence{}", if n == 1 { "" } else { "s" }),
                    t.fg("ui.dim"),
                ),
            ]))
            .style(t.bg("ui.panel.alt")),
            sep,
        );
        let prev = Rect {
            y: sep.y + 1,
            height: prev_h,
            ..inner
        };
        let gw = 6;
        let plines: Vec<Line> = preview(&h.path, h.line, prev_h as usize, &h.ranges, !p.running)
            .into_iter()
            .map(|(n, l)| {
                let mut spans = vec![Span::styled(format!("{n:>5} "), t.fg("ui.linenr"))];
                spans.extend(styled_line(t, &slice_cols(&l, 0, (prev.width as usize).saturating_sub(gw))).spans);
                let mut line = Line::from(spans);
                if n == h.line {
                    line = line.style(t.bg("ui.cursorline"));
                }
                line
            })
            .collect();
        f.render_widget(Paragraph::new(plines).style(t.bg("ui.bg")), prev);
    }

    let footer = row(inner.height - 1);
    f.render_widget(
        Paragraph::new(Span::styled(
            " ⏎ open   Alt-⏎ edit   Alt-O Find window   Alt-↑↓ history   Alt-C/W/X toggles   Alt-S scope   Tab mask",
            t.fg("ui.dim"),
        )),
        footer,
    );
    if let Some(s) = &p.summary {
        f.render_widget(
            Paragraph::new(Span::styled(format!("{} ms ", s.elapsed.as_millis()), t.fg("ui.dim"))).right_aligned(),
            footer,
        );
    }
}

/// The docked Find tool window.
pub fn draw_window(f: &mut Frame, area: Rect, app: &mut App) {
    let t = app.theme.clone();
    let t = &t;
    let focused = app.focus == Focus::Bottom;
    let inner = crate::ui::chrome::panel(f, area, "3: Find", focused, "ui.panel.alt", t);
    if inner.height < 2 {
        return;
    }
    let root = app.root.clone();
    let win = &mut app.find_window;
    // Result tabs.
    let mut spans = vec![Span::raw(" ")];
    for (i, tab) in win.tabs.iter().enumerate() {
        let style = if i == win.active {
            t.fg("ui.text.strong")
                .add_modifier(Modifier::BOLD | Modifier::UNDERLINED)
        } else {
            t.fg("ui.dim")
        };
        spans.push(Span::styled(format!(" {} ({}) ", tab.title, tab.hits.len()), style));
    }
    f.render_widget(Paragraph::new(Line::from(spans)), Rect { height: 1, ..inner });
    let list = Rect {
        y: inner.y + 1,
        height: inner.height - 1,
        ..inner
    };
    win.view_height = list.height as usize;
    let Some(tab) = win.tabs.get(win.active) else { return };
    let mut lines = Vec::new();
    for (i, row) in tab.rows.iter().enumerate().skip(tab.scroll).take(list.height as usize) {
        let mut l = match row {
            WindowRow::File { path, count } => {
                let rel = path.strip_prefix(&root).unwrap_or(path);
                let name = rel
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default();
                let dir = rel.parent().map(|d| d.display().to_string()).unwrap_or_default();
                let token = app.status.file_status(path).map(|s| s.token()).unwrap_or("ui.text");
                Line::from(vec![
                    Span::styled(" ▾ ", t.fg("ui.dim")),
                    Span::styled(name, t.fg(token).add_modifier(Modifier::BOLD)),
                    Span::styled(format!("  {dir}  "), t.fg("ui.dim")),
                    Span::styled(format!("{count}"), t.fg("ui.dim")),
                ])
            }
            WindowRow::Hit(k) => {
                let h = &tab.hits[*k];
                let prefix = format!("     {:>4}  ", h.line);
                let width = (list.width as usize).saturating_sub(prefix.width());
                let mut spans = vec![Span::styled(prefix, t.fg("ui.linenr"))];
                spans.extend(styled_line(t, &hit_line(&expand_tabs(&h.text, 4), &h.ranges, width)).spans);
                Line::from(spans)
            }
        };
        if i == tab.selected {
            let bg = if focused {
                "ui.selection"
            } else {
                "ui.selection.inactive"
            };
            l = l.style(t.bg(bg));
        }
        lines.push(l);
    }
    f.render_widget(Paragraph::new(lines), list);
    app.areas.find_window = Some(list);
}
