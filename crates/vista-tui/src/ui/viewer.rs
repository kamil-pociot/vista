//! Drawing open files: code with gutter, Markdown, images, binaries.

use std::path::{Path, PathBuf};

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Modifier;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Paragraph};
use ratatui_image::StatefulImage;
use ratatui_image::protocol::StatefulProtocol;
use vista_diff::LineMark;
use vista_render::filetype::human_size;
use vista_render::markdown;
use vista_render::styled::{line_width, overlay, slice_cols, wrap_cols};
use vista_render::{Highlighter, StyledLine};

use crate::app::{App, Focus};
use crate::style::{ThemeStyle, styled_line};
use crate::ui::chrome::{self, TabLabel};
use crate::viewer::{Content, ImageState, Tab};

pub fn draw(f: &mut Frame, area: Rect, app: &mut App) {
    let focused = app.focus == Focus::Main;
    let labels: Vec<(String, &'static str)> = app
        .viewer
        .tabs
        .iter()
        .map(|t| {
            let name = t
                .path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            let token = app.status.file_status(&t.path).map(|s| s.token()).unwrap_or("ui.text");
            (name, token)
        })
        .collect();
    let tabs: Vec<TabLabel> = labels
        .iter()
        .map(|(n, tok)| TabLabel {
            title: n,
            fg: tok,
            dirty: false,
        })
        .collect();
    let inner = chrome::tab_strip(f, area, &tabs, app.viewer.active, focused, &app.theme);
    app.areas.tabs = Some(Rect { height: 1, ..area });
    app.areas.viewer = Some(inner);
    if app.viewer.active.is_none() {
        crate::ui::empty_state(f, inner, app);
        return;
    }
    let mut body = inner;
    let bar = search_or_prompt_line(app);
    if bar.is_some() && body.height > 1 {
        body.height -= 1;
    }
    draw_active(f, body, app);
    if let Some(line) = bar {
        let r = Rect {
            y: inner.y + inner.height - 1,
            height: 1,
            ..inner
        };
        f.render_widget(Paragraph::new(line).style(app.theme.bg("ui.panel")), r);
    }
}

fn search_or_prompt_line(app: &App) -> Option<Line<'static>> {
    let t = &app.theme;
    if let Some(p) = &app.prompt {
        return Some(Line::from(vec![
            Span::styled(format!(" {}: ", p.label), t.fg("ui.text.strong")),
            Span::styled(format!("{}▏", p.input), t.fg("ui.text")),
        ]));
    }
    let tab = app.viewer.active_tab()?;
    let s = tab.search.as_ref()?;
    let mut spans = vec![
        Span::styled(" Find: ", t.fg("ui.text.strong")),
        Span::styled(s.query.clone(), t.fg("ui.text")),
    ];
    if s.editing {
        spans.push(Span::styled("▏", t.fg("ui.text")));
    }
    if s.query.is_empty() {
    } else if s.matches.is_empty() {
        spans.push(Span::styled("   no matches", t.fg("ui.error")));
    } else {
        spans.push(Span::styled(
            format!("   {}/{}", s.current + 1, s.matches.len()),
            t.fg("ui.dim"),
        ));
        spans.push(Span::styled("   n/N next/prev · Esc close", t.fg("ui.dim")));
    }
    Some(Line::from(spans))
}

fn draw_active(f: &mut Frame, area: Rect, app: &mut App) {
    let Some(i) = app.viewer.active else { return };
    let focused = app.focus == Focus::Main;
    let font = app.font_size();
    // Split borrows: the tab is mutated (scroll, render cache) while the
    // theme and image cache are read.
    let App {
        viewer,
        theme,
        md_images,
        picker,
        tx,
        ..
    } = app;
    let tab = &mut viewer.tabs[i];
    tab.view_height = area.height as usize;
    tab.view_width = area.width as usize;
    let t = &*theme;
    match &mut tab.content {
        Content::Text { .. } => draw_code(f, area, tab, t, focused),
        Content::Markdown { raw: true, .. } => draw_code(f, area, tab, t, focused),
        Content::Markdown { .. } => {
            let base = tab.path.parent().map(Path::to_path_buf).unwrap_or_default();
            let width = area.width.saturating_sub(2);
            let images_on = picker.is_some();
            if let Content::Markdown { source, rendered, .. } = &mut tab.content
                && rendered.as_ref().is_none_or(|(w, _)| *w != width)
            {
                let rows = |src: &str| md_image_rows(&base, src, width, font, images_on);
                *rendered = Some((
                    width,
                    markdown::render(source, width as usize, Highlighter::global(), &rows),
                ));
                crate::viewer::refresh_search(tab);
            }
            draw_markdown(f, area, tab, t, &base, md_images, picker.as_ref(), tx);
        }
        Content::Image { info, state } => {
            let header = Rect { height: 1, ..area };
            f.render_widget(Paragraph::new(Span::styled(format!(" {info}"), t.fg("ui.dim"))), header);
            let body = Rect {
                y: area.y + 1,
                height: area.height.saturating_sub(1),
                ..area
            };
            match state {
                ImageState::Ready(proto) => draw_image(f, body, proto),
                ImageState::Loading => message(f, body, "Loading image…", "ui.dim", t),
                ImageState::Failed(e) => message(f, body, &format!("Cannot show image: {e}"), "ui.error", t),
                ImageState::Disabled => message(f, body, "Images are off ([images] protocol = \"off\")", "ui.dim", t),
            }
        }
        Content::Binary { size, dump } => {
            let mut lines = vec![Line::styled(
                format!(
                    " Binary file · {} · first {} bytes:",
                    human_size(*size),
                    dump.len() * 16
                ),
                t.fg("ui.dim"),
            )];
            lines.extend(
                dump.iter()
                    .skip(tab.scroll)
                    .take(area.height as usize)
                    .map(|l| Line::styled(format!(" {l}"), t.fg("ui.text"))),
            );
            f.render_widget(Paragraph::new(lines), area);
        }
        Content::TooLarge { size } => message(
            f,
            area,
            &format!(
                "File too large to view ({}). Press e to open it in the editor.",
                human_size(*size)
            ),
            "ui.dim",
            t,
        ),
        Content::Error(e) => message(f, area, e, "ui.error", t),
    }
}

fn message(f: &mut Frame, area: Rect, text: &str, token: &str, t: &vista_theme::Theme) {
    let r = Rect {
        y: area.y + area.height / 3,
        height: 1.min(area.height),
        ..area
    };
    f.render_widget(
        Paragraph::new(Span::styled(text.to_string(), t.fg(token))).centered(),
        r,
    );
}

fn draw_image(f: &mut Frame, area: Rect, proto: &mut StatefulProtocol) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    f.render_stateful_widget(StatefulImage::<StatefulProtocol>::default(), area, proto);
}

/// Rows to reserve for a Markdown image (`None`: show a text placeholder).
fn md_image_rows(base: &Path, src: &str, width: u16, font: (u16, u16), images_on: bool) -> Option<u16> {
    if !images_on || src.contains("://") {
        return None;
    }
    let path = base.join(src);
    let (w, h) = image::image_dimensions(&path).ok()?;
    let (fw, fh) = (font.0.max(1) as f64, font.1.max(1) as f64);
    let max_px_w = width.saturating_sub(2) as f64 * fw;
    let scale = (max_px_w / w as f64).min(1.0);
    let rows = ((h as f64 * scale) / fh).ceil() as u16;
    Some(rows.clamp(1, 20))
}

fn gutter_width(total: usize) -> usize {
    total.max(1).to_string().len().max(3) + 3
}

/// Char ranges of search matches on a line: (other matches, current match).
type MatchRanges = (Vec<(usize, usize)>, Option<(usize, usize)>);

fn search_ranges(tab: &Tab, line: usize) -> MatchRanges {
    let Some(s) = &tab.search else {
        return (Vec::new(), None);
    };
    let mut all = Vec::new();
    let mut current = None;
    for (i, (l, a, b)) in s.matches.iter().enumerate() {
        if *l == line {
            if i == s.current {
                current = Some((*a, *b));
            } else {
                all.push((*a, *b));
            }
        }
    }
    (all, current)
}

fn decorate(line: &StyledLine, tab: &Tab, idx: usize) -> StyledLine {
    let (others, current) = search_ranges(tab, idx);
    let mut l = overlay(line, &others, |s| s.bg = Some("ui.match"));
    if let Some(c) = current {
        l = overlay(&l, &[c], |s| {
            s.bg = Some("ui.accent");
            s.fg = "ui.match.text";
        });
    }
    l
}

fn draw_code(f: &mut Frame, area: Rect, tab: &mut Tab, t: &vista_theme::Theme, focused: bool) {
    if tab.has_cursor() {
        tab.scroll_into_view();
    }
    // Horizontal scroll stops once the widest visible line is fully shown.
    if let Content::Text { lines, .. } | Content::Markdown { lines, .. } = &tab.content {
        let gw = gutter_width(lines.len());
        let content_w = (area.width as usize).saturating_sub(gw);
        let widest = lines
            .iter()
            .skip(tab.scroll)
            .take(area.height as usize)
            .map(line_width)
            .max()
            .unwrap_or(0);
        tab.hscroll = tab.hscroll.min(widest.saturating_sub(content_w.saturating_sub(4)));
    }
    let tab = &*tab;
    let (lines, markers) = match &tab.content {
        Content::Text { lines, markers, .. } => (lines, markers),
        Content::Markdown { lines, markers, .. } => (lines, markers),
        _ => return,
    };
    let gw = gutter_width(lines.len());
    let content_w = (area.width as usize).saturating_sub(gw);
    let mut out: Vec<Line> = Vec::with_capacity(area.height as usize);
    let mut idx = tab.scroll;
    while out.len() < area.height as usize && idx < lines.len() {
        let is_cursor = idx == tab.cursor;
        let decorated = decorate(&lines[idx], tab, idx);
        let rows = if tab.wrap {
            wrap_cols(&decorated, content_w.max(1))
        } else {
            vec![slice_cols(&decorated, tab.hscroll, content_w)]
        };
        for (ri, row) in rows.into_iter().enumerate() {
            if out.len() >= area.height as usize {
                break;
            }
            let num = if ri == 0 {
                format!("{:>w$} ", idx + 1, w = gw - 3)
            } else {
                " ".repeat(gw - 2)
            };
            let num_style = if is_cursor {
                t.fg("ui.text.strong")
            } else {
                t.fg("ui.linenr")
            };
            let (mark, mark_tok) = match (ri, markers.mark(idx), markers.deleted_above(idx)) {
                (0, LineMark::Added, _) => ("▌", "gutter.added"),
                (0, LineMark::Modified, _) => ("▌", "gutter.modified"),
                (0, LineMark::None, true) => ("▔", "gutter.deleted"),
                (_, LineMark::Added, _) => ("▌", "gutter.added"),
                (_, LineMark::Modified, _) => ("▌", "gutter.modified"),
                _ => (" ", "ui.linenr"),
            };
            let mut spans = vec![
                Span::styled(num, num_style),
                Span::styled(mark, t.fg(mark_tok)),
                Span::raw(" "),
            ];
            spans.extend(styled_line(t, &row).spans);
            let used = gw + line_width(&row);
            spans.push(Span::raw(" ".repeat((area.width as usize).saturating_sub(used))));
            let mut l = Line::from(spans);
            if is_cursor && focused {
                l = l.style(t.bg("ui.cursorline"));
            }
            out.push(l);
        }
        idx += 1;
    }
    f.render_widget(Paragraph::new(out), area);
}

#[allow(clippy::too_many_arguments)]
fn draw_markdown(
    f: &mut Frame,
    area: Rect,
    tab: &mut Tab,
    t: &vista_theme::Theme,
    base: &Path,
    images: &mut crate::viewer::MdImages,
    picker: Option<&ratatui_image::picker::Picker>,
    tx: &std::sync::mpsc::Sender<crate::event::AppEvent>,
) {
    let Content::Markdown {
        rendered: Some((_, rows)),
        ..
    } = &tab.content
    else {
        return;
    };
    let max_scroll = rows.len().saturating_sub(area.height as usize);
    tab.scroll = tab.scroll.min(max_scroll);
    let text_area = Rect {
        x: area.x + 1,
        width: area.width.saturating_sub(2),
        ..area
    };
    let mut out: Vec<Line> = Vec::new();
    let mut image_rects: Vec<(PathBuf, Rect)> = Vec::new();
    for (i, row) in rows.iter().enumerate().skip(tab.scroll).take(area.height as usize) {
        let decorated = decorate(&row.line, tab, i);
        let mut line = styled_line(t, &decorated);
        if let Some(fill) = row.fill {
            let used = line_width(&decorated);
            line.spans
                .push(Span::raw(" ".repeat((text_area.width as usize).saturating_sub(used))));
            line = line.style(t.bg(fill));
        }
        if let Some(slot) = &row.image
            && slot.row == 0
        {
            let y = area.y + (i - tab.scroll) as u16;
            let fits = (y + slot.rows) <= area.y + area.height;
            if fits {
                image_rects.push((
                    base.join(&slot.src),
                    Rect {
                        x: text_area.x,
                        y,
                        width: text_area.width,
                        height: slot.rows,
                    },
                ));
            }
        }
        out.push(line);
    }
    f.render_widget(Paragraph::new(out), text_area);
    for (path, rect) in image_rects {
        match images.entries.get_mut(&path) {
            Some(ImageState::Ready(proto)) => {
                f.render_widget(Block::new().style(t.bg("ui.bg")), rect);
                draw_image(f, rect, proto);
            }
            Some(_) => {}
            None if picker.is_some() => {
                images.entries.insert(path.clone(), ImageState::Loading);
                let tx = tx.clone();
                std::thread::spawn(move || {
                    let res = image::open(&path).map(Box::new).map_err(|e| e.to_string());
                    let _ = tx.send(crate::event::AppEvent::MdImageDecoded { path, image: res });
                });
            }
            None => {}
        }
    }
}

/// The recent-files popup.
pub fn draw_recent(f: &mut Frame, area: Rect, app: &App) {
    let Some(p) = &app.recent_popup else { return };
    let t = &app.theme;
    let items = p.visible(&app.recent.list);
    let h = (items.len() as u16 + 4).clamp(6, area.height.saturating_sub(4).max(6));
    let w = 70.min(area.width.saturating_sub(4)).max(30);
    let rect = crate::ui::help::centered(area, w, h);
    f.render_widget(ratatui::widgets::Clear, rect);
    let block = Block::bordered()
        .border_style(t.fg("ui.popup.border"))
        .style(t.fg_bg("ui.text", "ui.popup"))
        .title(Span::styled(
            " Recent Files ",
            t.fg("ui.text.strong").add_modifier(Modifier::BOLD),
        ));
    let inner = block.inner(rect);
    f.render_widget(block, rect);
    let mut lines = vec![Line::from(vec![
        Span::styled(" ⌕ ", t.fg("ui.dim")),
        Span::styled(format!("{}▏", p.filter), t.fg("ui.text.strong")),
    ])];
    for (i, path) in items.iter().enumerate().take(inner.height.saturating_sub(1) as usize) {
        let rel = path.strip_prefix(&app.root).unwrap_or(path);
        let name = rel
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let dir = rel.parent().map(|d| d.display().to_string()).unwrap_or_default();
        let token = app.status.file_status(path).map(|s| s.token()).unwrap_or("ui.text");
        let mut l = Line::from(vec![
            Span::styled(format!(" {name}"), t.fg(token)),
            Span::styled(format!("  {dir}"), t.fg("ui.dim")),
        ]);
        if i == p.selected {
            l = l.style(t.bg("ui.selection"));
        }
        lines.push(l);
    }
    if items.is_empty() {
        lines.push(Line::styled(" No recent files", t.fg("ui.dim")));
    }
    f.render_widget(Paragraph::new(lines), inner);
}
