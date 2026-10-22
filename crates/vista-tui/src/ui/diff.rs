//! Drawing diff tabs (mockup A · Commit & diff).

use std::path::Path;

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Modifier;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use unicode_width::UnicodeWidthStr;
use vista_diff::{DisplayRow, RowKind, Unified};
use vista_render::StyledLine;
use vista_render::filetype::human_size;
use vista_render::styled::{line_width, overlay, slice_cols, wrap_cols};
use vista_theme::Theme;

use crate::commit::CommitView;
use crate::diffview::{DiffBody, DiffDoc};
use crate::style::{ThemeStyle, styled_line};
use crate::viewer::{Content, ImageState, Tab};

/// How long lines are shown: scrolled sideways by `hscroll`, or wrapped.
#[derive(Clone, Copy)]
struct View {
    hscroll: usize,
    wrap: bool,
}

/// One side of a row: number + content, `width` columns wide. One entry per
/// screen row: a single one when scrolling sideways, several when wrapping.
#[allow(clippy::too_many_arguments)]
fn cell(
    t: &Theme,
    line: Option<(usize, &StyledLine, bool)>,
    ln_w: usize,
    width: usize,
    bg: Option<&'static str>,
    words: &[(usize, usize)],
    word_bg: &'static str,
    cursor: bool,
    view: View,
) -> Vec<Vec<Span<'static>>> {
    let Some((n, styled, has_tab)) = line else {
        // Filler for lines that exist only on the other side.
        return vec![filler(t, width)];
    };
    let num_style = if cursor {
        t.fg("ui.text.strong")
    } else {
        t.fg("ui.linenr")
    };
    let content_w = width.saturating_sub(ln_w + 1);
    let mut l = if has_tab || words.is_empty() {
        styled.clone()
    } else {
        overlay(styled, words, |s| s.bg = Some(word_bg))
    };
    if let Some(bg) = bg {
        for s in &mut l {
            if s.style.bg.is_none() {
                s.style.bg = Some(bg);
            }
        }
    }
    let parts = if view.wrap {
        wrap_cols(&l, content_w.max(1))
    } else {
        vec![slice_cols(&l, view.hscroll, content_w)]
    };
    parts
        .into_iter()
        .enumerate()
        .map(|(k, part)| {
            let num = if k == 0 {
                format!("{:>ln_w$} ", n + 1)
            } else {
                " ".repeat(ln_w + 1)
            };
            let mut spans = vec![Span::styled(num, num_style)];
            let used = line_width(&part);
            spans.extend(styled_line(t, &part).spans);
            let pad = " ".repeat(content_w.saturating_sub(used));
            spans.push(match bg {
                Some(bg) => Span::styled(pad, t.bg(bg)),
                None => Span::raw(pad),
            });
            spans
        })
        .collect()
}

fn filler(t: &Theme, width: usize) -> Vec<Span<'static>> {
    vec![Span::styled("╱".repeat(width), t.fg("ui.filler"))]
}

/// A continuation row for the side whose line wrapped to fewer rows.
fn blank(t: &Theme, width: usize, bg: Option<&'static str>) -> Vec<Span<'static>> {
    let pad = " ".repeat(width);
    vec![match bg {
        Some(bg) => Span::styled(pad, t.bg(bg)),
        None => Span::raw(pad),
    }]
}

fn has_tab(text: &str, i: usize) -> bool {
    text.lines().nth(i).is_some_and(|l| l.contains('\t'))
}

fn fold_line(t: &Theme, len: usize, width: usize) -> Line<'static> {
    let text = format!("⋯ {len} unchanged line{} ⋯", if len == 1 { "" } else { "s" });
    let pad = width.saturating_sub(text.width()) / 2;
    Line::from(vec![Span::raw(" ".repeat(pad)), Span::styled(text, t.fg("ui.dim"))]).style(t.bg("ui.panel.alt"))
}

pub fn draw(
    f: &mut Frame,
    area: Rect,
    tab: &mut Tab,
    t: &Theme,
    focused: bool,
    root: &Path,
    commit: Option<&CommitView>,
) {
    if area.height < 4 {
        return;
    }
    let rel = tab.path.strip_prefix(root).unwrap_or(&tab.path).to_path_buf();
    let name = rel
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let dir = rel.parent().map(|d| d.display().to_string()).unwrap_or_default();
    let Content::Diff(doc) = &tab.content else { return };

    // Header: file + options + position.
    let header = Rect { height: 1, ..area };
    f.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(format!(" {name}"), t.fg("ui.text.strong").add_modifier(Modifier::BOLD)),
            Span::styled(format!("  {dir}"), t.fg("ui.dim")),
        ]))
        .style(t.bg("ui.panel")),
        header,
    );
    let (k, n) = doc.hunk_position(tab.cursor);
    let ticked = doc
        .sbs
        .hunks
        .iter()
        .filter(|h| commit.is_some_and(|c| c.hunk_included(&tab.path, h.id)))
        .count();
    let position = if n == 0 {
        // Git counts a missing final newline; the lines themselves match.
        if doc.old_text != doc.new_text {
            "only the final newline changed ".to_string()
        } else {
            "no changes ".to_string()
        }
    } else if commit.is_none() {
        format!("read-only   [ ◂ change {k} of {n} ▸ ] ")
    } else {
        format!("{ticked} of {n} in commit (␣)   [ ◂ change {k} of {n} ▸ ] ")
    };
    f.render_widget(
        Paragraph::new(Span::styled(position, t.fg("ui.text"))).right_aligned(),
        header,
    );

    let body = Rect {
        y: area.y + 2,
        height: area.height - 3,
        ..area
    };
    let hints = Rect {
        y: area.y + area.height - 1,
        height: 1,
        ..area
    };
    f.render_widget(
        Paragraph::new(Span::styled(
            " u unified  i whitespace  w wrap  z collapse  J/K file  ⏎ open  e edit",
            t.fg("ui.dim"),
        ))
        .style(t.bg("ui.panel")),
        hints,
    );
    let opts = format!(
        "{} · whitespace: {} · lines: {} · collapse: {} ",
        if doc.unified { "unified" } else { "side-by-side" },
        if doc.opts.ignore_whitespace {
            "ignore"
        } else {
            "compare"
        },
        if tab.wrap { "wrap" } else { "scroll" },
        if doc.collapse { "on" } else { "off" },
    );
    f.render_widget(
        Paragraph::new(Span::styled(opts, t.fg("ui.dim"))).right_aligned(),
        hints,
    );
    let cols = Rect {
        y: area.y + 1,
        height: 1,
        ..area
    };

    match &doc.body {
        DiffBody::Binary { old, new } => {
            let size = |s: &Option<u64>| s.map(human_size).unwrap_or_else(|| "absent".into());
            f.render_widget(
                Paragraph::new(Span::styled(
                    format!(" Binary files differ: HEAD {} → working tree {}", size(old), size(new)),
                    t.fg("ui.dim"),
                )),
                body,
            );
            return;
        }
        DiffBody::Image { .. } => {
            draw_images(f, cols, body, tab, t);
            return;
        }
        DiffBody::Text => {}
    }

    tab.view_height = body.height as usize;
    tab.scroll_into_view();
    let (body, bar) = crate::ui::chrome::split_scrollbar(body);
    let Content::Diff(doc) = &tab.content else { return };
    let ln_w = doc
        .sbs
        .old_lines
        .len()
        .max(doc.sbs.new_lines.len())
        .max(1)
        .to_string()
        .len()
        .max(3);
    // Sideways scroll stops once the widest visible line is fully shown.
    let content_w = if doc.unified {
        (body.width as usize).saturating_sub(ln_w * 2 + 6)
    } else {
        ((body.width as usize).saturating_sub(2) / 2).saturating_sub(ln_w + 1)
    };
    let widest = doc.widest(tab.scroll, body.height as usize);
    tab.hscroll = tab.hscroll.min(widest.saturating_sub(content_w));
    // Wrapped lines take several rows: scroll limits and the scrollbar count
    // rows, not lines (mirrors the widths `draw_side_by_side` / `draw_unified` use).
    if tab.wrap {
        let w = body.width as usize;
        let half = w.saturating_sub(2) / 2;
        let right_w = w.saturating_sub(2 + half);
        let heights = doc.wrapped_heights(
            half.saturating_sub(ln_w + 1),
            right_w.saturating_sub(ln_w + 1),
            content_w,
        );
        tab.fit_wrapped(&heights);
    } else {
        tab.row_heights = None;
    }
    let (total, top) = tab.row_position();
    crate::ui::chrome::scrollbar(f, bar, t, total, top);
    let Content::Diff(doc) = &tab.content else { return };
    if doc.unified {
        draw_unified(f, cols, body, tab, doc, t, ln_w, focused, commit);
    } else {
        draw_side_by_side(f, cols, body, tab, doc, t, ln_w, focused, commit);
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_side_by_side(
    f: &mut Frame,
    cols: Rect,
    body: Rect,
    tab: &Tab,
    doc: &DiffDoc,
    t: &Theme,
    ln_w: usize,
    focused: bool,
    commit: Option<&CommitView>,
) {
    let starts = hunk_start_rows(doc);
    let gutter = 2usize;
    let half = (body.width as usize).saturating_sub(gutter) / 2;
    let right_w = (body.width as usize).saturating_sub(gutter + half);
    let header = |label: String, w: usize| {
        let mut s = format!(" {label}");
        let pad = w.saturating_sub(s.width());
        s.push_str(&" ".repeat(pad));
        s
    };
    f.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(header(doc.sides.0.clone(), half), t.fg("ui.header")),
            Span::raw(" ".repeat(gutter)),
            Span::styled(header(doc.sides.1.clone(), right_w), t.fg("ui.header")),
        ]))
        .style(t.bg("ui.panel.alt")),
        cols,
    );
    let view = View {
        hscroll: tab.hscroll,
        wrap: tab.wrap,
    };
    let height = body.height as usize;
    let mut lines = Vec::with_capacity(height);
    for (di, d) in doc.display.iter().enumerate().skip(tab.scroll) {
        if lines.len() >= height {
            break;
        }
        let row_i = match d {
            DisplayRow::Fold { len, .. } => {
                lines.push(fold_line(t, *len, body.width as usize));
                continue;
            }
            DisplayRow::Row(r) => *r,
        };
        let row = &doc.sbs.rows[row_i];
        let cursor = focused && di == tab.cursor;
        let (lbg, rbg, gutter_tok, word_bg) = match row.kind {
            RowKind::Context => (None, None, None, "diff.word.modified"),
            RowKind::Added => (None, Some("diff.inserted"), Some("gutter.added"), "diff.word.inserted"),
            RowKind::Deleted => (Some("diff.deleted"), None, Some("gutter.deleted"), "diff.word.deleted"),
            RowKind::Modified => (
                Some("diff.modified"),
                Some("diff.modified"),
                Some("gutter.modified"),
                "diff.word.modified",
            ),
        };
        let left = row
            .left
            .and_then(|i| doc.old_hl.get(i).map(|l| (i, l, has_tab(&doc.old_text, i))));
        let right = row
            .right
            .and_then(|i| doc.new_hl.get(i).map(|l| (i, l, has_tab(&doc.new_text, i))));
        let has_left = left.is_some();
        let has_right = right.is_some();
        let mut lrows = cell(t, left, ln_w, half, lbg, &row.left_words, word_bg, cursor, view);
        let mut rrows = cell(t, right, ln_w, right_w, rbg, &row.right_words, word_bg, cursor, view);
        // Wrapped lines: pad the shorter side so both stay aligned.
        let n = lrows.len().max(rrows.len());
        while lrows.len() < n {
            lrows.push(if has_left { blank(t, half, lbg) } else { filler(t, half) });
        }
        while rrows.len() < n {
            rrows.push(if has_right {
                blank(t, right_w, rbg)
            } else {
                filler(t, right_w)
            });
        }
        // A checkbox where each change starts (partial commits), else the cursor.
        let g = match starts.iter().find(|(r, _)| *r == di) {
            Some((_, id)) if !doc.opts.ignore_whitespace && commit.is_some() => {
                if commit.is_some_and(|c| c.hunk_included(&tab.path, *id)) {
                    "☑ "
                } else {
                    "☐ "
                }
            }
            _ if cursor => "▶ ",
            _ => "  ",
        };
        // The view may start partway into a wrapped row.
        let hidden = if di == tab.scroll { tab.sub_row() } else { 0 };
        for (k, (mut spans, rspans)) in lrows.into_iter().zip(rrows).enumerate().skip(hidden) {
            let g = if k == 0 { g } else { "  " };
            spans.push(match gutter_tok {
                Some(tok) => Span::styled(g, t.fg_bg("ui.text.strong", tok)),
                None => Span::styled(g, t.fg_bg("ui.text.strong", "ui.panel.alt")),
            });
            spans.extend(rspans);
            let mut line = Line::from(spans);
            if cursor && row.kind == RowKind::Context {
                line = line.style(t.bg("ui.cursorline"));
            }
            lines.push(line);
        }
    }
    lines.truncate(height);
    if doc.display.is_empty() {
        lines.push(Line::styled(" No differences", t.fg("ui.dim")));
    }
    f.render_widget(Paragraph::new(lines), body);
}

#[allow(clippy::too_many_arguments)]
/// (display row, hunk id) where each change starts.
fn hunk_start_rows(doc: &DiffDoc) -> Vec<(usize, u64)> {
    doc.hunk_starts()
        .into_iter()
        .filter_map(|r| doc.hunk_at(r).map(|id| (r, id)))
        .collect()
}

#[allow(clippy::too_many_arguments)]
fn draw_unified(
    f: &mut Frame,
    cols: Rect,
    body: Rect,
    tab: &Tab,
    doc: &DiffDoc,
    t: &Theme,
    ln_w: usize,
    focused: bool,
    commit: Option<&CommitView>,
) {
    let starts = hunk_start_rows(doc);
    f.render_widget(
        Paragraph::new(Span::styled(
            format!(" {} ⇄ {}", doc.sides.0, doc.sides.1),
            t.fg("ui.header"),
        ))
        .style(t.bg("ui.panel.alt")),
        cols,
    );
    let width = body.width as usize;
    let view = View {
        hscroll: tab.hscroll,
        wrap: tab.wrap,
    };
    let height = body.height as usize;
    let mut lines = Vec::with_capacity(height);
    for (di, u) in doc.udisplay.iter().enumerate().skip(tab.scroll) {
        if lines.len() >= height {
            break;
        }
        let cursor = focused && di == tab.cursor;
        let (old_n, new_n, sign, text, bg, words, word_bg, tab_flag) = match *u {
            Unified::Fold { len } => {
                lines.push(fold_line(t, len, width));
                continue;
            }
            Unified::Both(r) => {
                let row = &doc.sbs.rows[r];
                let (o, n) = (row.left.unwrap_or(0), row.right.unwrap_or(0));
                (
                    Some(o),
                    Some(n),
                    ' ',
                    doc.new_hl.get(n),
                    None,
                    &[][..],
                    "diff.word.inserted",
                    has_tab(&doc.new_text, n),
                )
            }
            Unified::Old(r) => {
                let row = &doc.sbs.rows[r];
                let o = row.left.unwrap_or(0);
                (
                    Some(o),
                    None,
                    '-',
                    doc.old_hl.get(o),
                    Some("diff.deleted"),
                    &row.left_words[..],
                    "diff.word.deleted",
                    has_tab(&doc.old_text, o),
                )
            }
            Unified::New(r) => {
                let row = &doc.sbs.rows[r];
                let n = row.right.unwrap_or(0);
                (
                    None,
                    Some(n),
                    '+',
                    doc.new_hl.get(n),
                    Some("diff.inserted"),
                    &row.right_words[..],
                    "diff.word.inserted",
                    has_tab(&doc.new_text, n),
                )
            }
        };
        let num = |n: Option<usize>| {
            n.map(|v| format!("{:>ln_w$}", v + 1))
                .unwrap_or_else(|| " ".repeat(ln_w))
        };
        let num_style = if cursor {
            t.fg("ui.text.strong")
        } else {
            t.fg("ui.linenr")
        };
        let tick = match starts.iter().find(|(r, _)| *r == di) {
            Some((_, id)) if !doc.opts.ignore_whitespace && commit.is_some() => {
                if commit.is_some_and(|c| c.hunk_included(&tab.path, *id)) {
                    "☑ "
                } else {
                    "☐ "
                }
            }
            _ => "  ",
        };
        let prefix = vec![
            Span::styled(tick, t.fg("ui.text.strong")),
            Span::styled(format!("{} {} ", num(old_n), num(new_n)), num_style),
            Span::styled(
                format!("{sign} "),
                match bg {
                    Some(b) => t.fg_bg("ui.text.strong", b),
                    None => t.fg("ui.dim"),
                },
            ),
        ];
        let used = ln_w * 2 + 6;
        let rows = match text {
            Some(styled) => cell(
                t,
                Some((0, styled, tab_flag)),
                0,
                width.saturating_sub(used) + 1,
                bg,
                words,
                word_bg,
                false,
                view,
            ),
            None => vec![Vec::new()],
        };
        let hidden = if di == tab.scroll { tab.sub_row() } else { 0 };
        for (k, row) in rows.into_iter().enumerate().skip(hidden) {
            let mut spans = if k == 0 {
                prefix.clone()
            } else {
                vec![Span::raw(" ".repeat(used))]
            };
            // `cell` adds a 1-column number field; drop it.
            spans.extend(row.into_iter().skip(1));
            let mut line = Line::from(spans);
            if cursor && bg.is_none() {
                line = line.style(t.bg("ui.cursorline"));
            }
            lines.push(line);
        }
    }
    lines.truncate(height);
    f.render_widget(Paragraph::new(lines), body);
}

fn draw_images(f: &mut Frame, cols: Rect, body: Rect, tab: &mut Tab, t: &Theme) {
    let half = body.width / 2;
    let left = Rect { width: half, ..body };
    let right = Rect {
        x: body.x + half + 1,
        width: body.width.saturating_sub(half + 1),
        ..body
    };
    f.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(format!(" {:<w$}", "HEAD", w = half as usize - 1), t.fg("ui.header")),
            Span::styled("  Working tree", t.fg("ui.header")),
        ]))
        .style(t.bg("ui.panel.alt")),
        cols,
    );
    let Content::Diff(doc) = &mut tab.content else { return };
    let DiffBody::Image { old, new } = &mut doc.body else {
        return;
    };
    for (state, r) in [(old, left), (new, right)] {
        match state {
            ImageState::Ready(p) => f.render_stateful_widget(
                ratatui_image::StatefulImage::<ratatui_image::protocol::StatefulProtocol>::default(),
                r,
                p.as_mut(),
            ),
            ImageState::Loading => f.render_widget(Paragraph::new(Span::styled(" Loading…", t.fg("ui.dim"))), r),
            ImageState::Failed(e) => {
                f.render_widget(Paragraph::new(Span::styled(format!(" ({e})"), t.fg("ui.dim"))), r)
            }
            ImageState::Disabled => f.render_widget(Paragraph::new(Span::styled(" Images are off", t.fg("ui.dim"))), r),
        }
    }
}
