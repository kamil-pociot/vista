//! Drawing open files: code with gutter, Markdown, images, binaries.

use std::path::{Path, PathBuf};

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Modifier;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
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
use crate::ui::click;
use crate::viewer::{Content, ImageState, MdPicture, Tab};
use ratatui_image::sliced::{SignedPosition, SlicedImage, SlicedProtocol};

pub fn draw(f: &mut Frame, area: Rect, app: &mut App) {
    let focused = app.focus == Focus::Main;
    let labels: Vec<(String, &'static str, bool)> = app
        .viewer
        .tabs
        .iter()
        .map(|t| {
            let name = t
                .path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            let name = match &t.diff {
                Some(spec) => match (&spec.revs, &spec.base) {
                    (_, Some(b)) => format!("Δ {name} ⇠ {}", b.short),
                    (Some(r), None) => format!("Δ {name} @{}", r.short),
                    (None, None) => format!("Δ {name}"),
                },
                None => name,
            };
            let token = app.status.file_status(&t.path).map(|s| s.token()).unwrap_or("ui.text");
            (name, token, app.dirty.contains(&t.path))
        })
        .collect();
    let tabs: Vec<TabLabel> = labels
        .iter()
        .map(|(n, tok, dirty)| TabLabel {
            title: n,
            fg: tok,
            dirty: *dirty,
        })
        .collect();
    // A tab that just became the active one scrolls into view; the wheel
    // can scroll away from it after that.
    let active_id = app.viewer.active_tab().map(|t| t.id);
    let show = (active_id != app.viewer.strip_shown)
        .then_some(app.viewer.active)
        .flatten();
    app.viewer.strip_shown = active_id;
    let switch = app.diff_switch();
    let opts = chrome::StripOptions {
        wrap: app.tab_rows(),
        reserve: if switch.is_some() { DIFF_SWITCH_WIDTH } else { 0 },
        scroll: app.viewer.strip_scroll,
        show,
    };
    let (inner, hits, scroll) = chrome::tab_strip(f, area, &tabs, app.viewer.active, focused, &app.theme, opts);
    app.viewer.strip_scroll = scroll;
    app.tab_hits = hits;
    if let Some(diff) = switch {
        diff_switch(f, Rect { height: 1, ..area }, app, diff);
    }
    app.areas.tabs = Some(Rect {
        height: inner.y - area.y,
        ..area
    });
    app.areas.viewer = Some(inner);
    if app.viewer.active.is_none() {
        crate::ui::empty_state(f, inner, app);
        return;
    }
    let mut body = inner;
    let bar = search_line(app);
    if bar.is_some() && body.height > 1 {
        body.height -= 1;
    }
    draw_active(f, body, app);
    if let Some(items) = bar {
        let r = Rect {
            y: inner.y + inner.height - 1,
            height: 1,
            ..inner
        };
        click::line(f, r, items, ratatui::layout::Alignment::Left, app.theme.bg("ui.panel"));
    }
}

/// Columns the File / Diff switch takes at the right of the tab strip.
const DIFF_SWITCH_WIDTH: u16 = 17;

/// `File | Diff` at the right of the tab strip: the view shown stands out,
/// the other one switches to it (as `d` does).
fn diff_switch(f: &mut Frame, row: Rect, app: &App, diff: bool) {
    let t = &app.theme;
    let toggle = Some(click::Click::DiffSwitch);
    let shown = t.fg_bg("ui.text.strong", "ui.selection").add_modifier(Modifier::BOLD);
    let other = t.fg("ui.dim");
    let (file, diff) = if diff {
        ((other, toggle), (shown, None))
    } else {
        ((shown, None), (other, toggle))
    };
    let items: Vec<click::Item> = vec![
        (Span::styled("  d ", t.fg("ui.dim")), toggle),
        (Span::styled(" File ", file.0), file.1),
        (Span::styled(" Diff ", diff.0), diff.1),
        (Span::raw(" "), None),
    ];
    let bg = match t.chrome.tabs {
        vista_theme::Tabs::Underline => "ui.panel",
        _ => "ui.bg",
    };
    // Its own background, over a tab title that runs that far.
    let width = items.iter().map(|(s, _)| s.content.chars().count() as u16).sum::<u16>();
    let r = Rect {
        x: row.x + row.width.saturating_sub(width),
        width: width.min(row.width),
        ..row
    };
    f.render_widget(Block::new().style(t.bg(bg)), r);
    click::line(
        f,
        r,
        items,
        ratatui::layout::Alignment::Right,
        ratatui::style::Style::default(),
    );
}

fn search_line(app: &App) -> Option<Vec<click::Item>> {
    let t = &app.theme;
    let tab = app.viewer.active_tab()?;
    let s = tab.search.as_ref()?;
    let mut items = vec![
        (Span::styled(" Find: ", t.fg("ui.text.strong")), None),
        (Span::styled(s.query.clone(), t.fg("ui.text")), None),
    ];
    if s.editing {
        items.push((Span::styled("▏", t.fg("ui.text")), None));
    }
    if s.query.is_empty() {
    } else if s.matches.is_empty() {
        items.push((Span::styled("   no matches", t.fg("ui.error")), None));
    } else {
        use ratatui::crossterm::event::KeyCode;
        let k = |c: KeyCode| Some(click::key(c, Some(Focus::Main)));
        let dim = t.fg("ui.dim");
        items.extend([
            (
                Span::styled(format!("   {}/{}", s.current + 1, s.matches.len()), dim),
                None,
            ),
            (Span::styled("   ", dim), None),
            (Span::styled("n", dim), k(KeyCode::Char('n'))),
            (Span::styled("/", dim), None),
            (Span::styled("N", dim), k(KeyCode::Char('N'))),
            (Span::styled(" next/prev · ", dim), None),
            (Span::styled("Esc close", dim), k(KeyCode::Esc)),
        ]);
    }
    Some(items)
}

fn draw_active(f: &mut Frame, area: Rect, app: &mut App) {
    let Some(i) = app.viewer.active else { return };
    if app.viewer.tabs[i].editing && app.nvim.is_some() {
        draw_editor(f, area, app);
        return;
    }
    let focused = app.focus == Focus::Main;
    let font = app.font_size();
    let numbers = app.line_numbers();
    let root = app.root.clone();
    // Split borrows: the tab is mutated (scroll, render cache) while the
    // theme and image cache are read.
    let App {
        viewer,
        theme,
        md_images,
        picker,
        tx,
        commit,
        ..
    } = app;
    let commit = &*commit;
    let tab = &mut viewer.tabs[i];
    tab.view_height = area.height as usize;
    tab.view_width = area.width as usize;
    // Set again below by views that wrap.
    tab.row_heights = None;
    let t = &*theme;
    match &mut tab.content {
        Content::Text { .. } => draw_code(f, area, tab, t, focused, numbers),
        Content::Markdown { raw: true, .. } => draw_code(f, area, tab, t, focused, numbers),
        Content::Markdown { .. } => {
            let base = tab.path.parent().map(Path::to_path_buf).unwrap_or_default();
            // Line numbers (source lines of each block) take a left column.
            let gutter = match &tab.content {
                Content::Markdown { lines, .. } if numbers => md_gutter(lines.len()),
                _ => 0,
            };
            let width = area.width.saturating_sub(2 + gutter);
            let pictures = MdPictures::new(&base, t, picker.as_ref());
            let wide = !tab.wrap;
            if let Content::Markdown { source, rendered, .. } = &mut tab.content
                && rendered.as_ref().is_none_or(|(w, wd, _)| *w != width || *wd != wide)
            {
                let rows = |src: &str| pictures.rows(md_images, src, width, font);
                let tables = if wide {
                    markdown::Tables::Wide
                } else {
                    markdown::Tables::Wrap
                };
                *rendered = Some((
                    width,
                    wide,
                    markdown::render_with(source, width as usize, tables, Highlighter::global(), &rows),
                ));
                crate::viewer::refresh_search(tab);
            }
            draw_markdown(f, area, gutter, tab, t, &pictures, md_images, picker.as_ref(), tx);
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
        Content::Diff(_) => {
            // History and review diffs are read-only: no picking changes for the commit.
            let history = tab.diff.as_ref().is_some_and(|d| d.read_only());
            let commit = (!history).then_some(commit);
            crate::ui::diff::draw(f, area, tab, t, focused, &root, commit)
        }
        Content::DiffLoading => message(f, area, "Loading diff…", "ui.dim", t),
        Content::Error(e) => message(f, area, e, "ui.error", t),
    }
}

/// Draws Neovim's grid, resizing Neovim to the area first.
fn draw_editor(f: &mut Frame, area: Rect, app: &mut App) {
    if let Some(n) = app.nvim.as_mut() {
        let _ = n.resize(area.width, area.height);
    }
    let g = &app.grid;
    let focused = app.focus == Focus::Main;
    let default_fg = g.default_fg.map(crate::style::rgb_u32);
    let default_bg = g.default_bg.map(crate::style::rgb_u32);
    let mut lines = Vec::with_capacity(area.height as usize);
    for row in 0..(area.height as usize).min(g.height) {
        let mut spans: Vec<Span> = Vec::new();
        let mut run = String::new();
        let mut run_hl = u64::MAX;
        let mut run_cursor = false;
        let style_for = |hl: u64, cursor: bool| {
            let a = g.hl.get(&hl).cloned().unwrap_or_default();
            let mut fg = a.fg.map(crate::style::rgb_u32).or(default_fg);
            let mut bg = a.bg.map(crate::style::rgb_u32).or(default_bg);
            if a.reverse != cursor {
                std::mem::swap(&mut fg, &mut bg);
            }
            let mut s = ratatui::style::Style::new();
            if let Some(c) = fg {
                s = s.fg(c);
            }
            if let Some(c) = bg {
                s = s.bg(c);
            }
            let mut m = Modifier::empty();
            if a.bold {
                m |= Modifier::BOLD;
            }
            if a.italic {
                m |= Modifier::ITALIC;
            }
            if a.underline {
                m |= Modifier::UNDERLINED;
            }
            if a.strikethrough {
                m |= Modifier::CROSSED_OUT;
            }
            if cursor && fg.is_none() && bg.is_none() {
                m |= Modifier::REVERSED;
            }
            s.add_modifier(m)
        };
        for col in 0..(area.width as usize).min(g.width) {
            let Some(cell) = g.cell(row, col) else { continue };
            if cell.text.is_empty() {
                // Right half of a wide character.
                continue;
            }
            let cursor = focused && g.cursor == (row, col);
            if cell.hl != run_hl || cursor != run_cursor {
                if !run.is_empty() {
                    spans.push(Span::styled(std::mem::take(&mut run), style_for(run_hl, run_cursor)));
                }
                run_hl = cell.hl;
                run_cursor = cursor;
            }
            run.push_str(&cell.text);
        }
        if !run.is_empty() {
            spans.push(Span::styled(run, style_for(run_hl, run_cursor)));
        }
        lines.push(Line::from(spans));
    }
    let base = match default_bg {
        Some(bg) => app.theme.base().bg(bg),
        None => app.theme.base(),
    };
    f.render_widget(Paragraph::new(lines).style(base), area);
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

/// How a Markdown document draws its images and diagrams.
struct MdPictures<'a> {
    base: &'a Path,
    /// Images are on (not `off`).
    images: bool,
    /// Mermaid diagrams as pictures: only with real pixels (kitty graphics);
    /// in half-blocks their text is unreadable, so they stay text.
    diagrams: bool,
    /// The look's background, which diagrams are drawn on (part of their key).
    background: vista_theme::Rgb,
    dark: bool,
}

impl MdPictures<'_> {
    fn new<'a>(
        base: &'a Path,
        t: &vista_theme::Theme,
        picker: Option<&ratatui_image::picker::Picker>,
    ) -> MdPictures<'a> {
        let bg = t.color("ui.bg");
        MdPictures {
            base,
            images: picker.is_some(),
            diagrams: picker.is_some_and(|p| p.protocol_type() != ratatui_image::picker::ProtocolType::Halfblocks),
            background: bg,
            dark: bg.luminance() < 0.4,
        }
    }

    /// Where a picture is cached: the image file beside the document, or the
    /// Mermaid diagram as drawn on this look's background.
    fn path(&self, src: &str) -> PathBuf {
        if src.starts_with("mermaid:") {
            PathBuf::from(format!("{src}@{}", self.background.to_hex()))
        } else {
            self.base.join(src)
        }
    }

    /// Rows to reserve for a picture (`None`: show a text placeholder, or a
    /// diagram's text). A diagram has rows once its picture is made, at its
    /// natural size or narrower (taller than the pane is fine: it scrolls).
    fn rows(&self, images: &crate::viewer::MdImages, src: &str, width: u16, font: (u16, u16)) -> Option<u16> {
        let (fw, fh) = (font.0.max(1) as f64, font.1.max(1) as f64);
        let max_px_w = width.saturating_sub(2) as f64 * fw;
        if src.starts_with("mermaid:") {
            if !self.diagrams {
                return None;
            }
            // Made at twice its size for sharpness: its natural size is half.
            let (w, h) = images.size(&self.path(src))?;
            let (w, h) = (w as f64 / 2.0, h as f64 / 2.0);
            let scale = (max_px_w / w).min(1.0);
            return Some((((h * scale) / fh).ceil() as u16).clamp(1, 400));
        }
        if !self.images || src.contains("://") {
            return None;
        }
        let (w, h) = image::image_dimensions(self.path(src)).ok()?;
        let scale = (max_px_w / w as f64).min(1.0);
        let rows = ((h as f64 * scale) / fh).ceil() as u16;
        Some(rows.clamp(1, 20))
    }
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
    let len: usize = line.iter().map(|s| s.text.chars().count()).sum();
    if let Some(r) = tab.selection.and_then(|sel| sel.range_on(idx, len)) {
        l = overlay(&l, &[r], |s| s.bg = Some("ui.selection"));
    }
    // The column Go to Line went to.
    if let Some((line, c)) = tab.caret
        && line == idx
        && idx == tab.cursor
        && c < len
    {
        l = overlay(&l, &[(c, c + 1)], |s| {
            s.bg = Some("ui.selection");
            s.underline = true;
        });
    }
    l
}

fn draw_code(f: &mut Frame, area: Rect, tab: &mut Tab, t: &vista_theme::Theme, focused: bool, numbers: bool) {
    let gutter_width = |n: usize| if numbers { gutter_width(n) } else { 2 };
    let (area, bar) = crate::ui::chrome::split_scrollbar(area);
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
    // Wrapped lines take several rows: scroll limits and the scrollbar
    // count rows, not lines.
    if tab.wrap
        && let Content::Text { lines, .. } | Content::Markdown { lines, .. } = &tab.content
    {
        let content_w = (area.width as usize).saturating_sub(gutter_width(lines.len()));
        let heights: Vec<usize> = lines
            .iter()
            .map(|l| crate::viewer::wrapped_rows(line_width(l), content_w.max(1)))
            .collect();
        tab.fit_wrapped(&heights);
    } else {
        tab.row_heights = None;
    }
    let (total, top) = tab.row_position();
    crate::ui::chrome::scrollbar(f, bar, t, total, top);
    let tab = &*tab;
    let (lines, markers) = match &tab.content {
        Content::Text { lines, markers, .. } => (lines, markers),
        Content::Markdown { lines, markers, .. } => (lines, markers),
        _ => return,
    };
    let gw = gutter_width(lines.len());
    let content_w = (area.width as usize).saturating_sub(gw);
    let mut out: Vec<Line> = Vec::with_capacity(area.height as usize);
    let mut row_map = Vec::with_capacity(area.height as usize);
    let mut idx = tab.scroll;
    while out.len() < area.height as usize && idx < lines.len() {
        let is_cursor = idx == tab.cursor;
        let decorated = decorate(&lines[idx], tab, idx);
        let rows = if tab.wrap {
            wrap_cols(&decorated, content_w.max(1))
        } else {
            vec![slice_cols(&decorated, tab.hscroll, content_w)]
        };
        // The view may start partway into a wrapped line.
        let hidden = if idx == tab.scroll { tab.sub_row() } else { 0 };
        for (ri, row) in rows.into_iter().enumerate().skip(hidden) {
            if out.len() >= area.height as usize {
                break;
            }
            row_map.push((idx, if tab.wrap { ri * content_w.max(1) } else { tab.hscroll }));
            let num = if ri == 0 && numbers {
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
    *tab.layout.borrow_mut() = crate::viewer::TextLayout {
        rows: row_map,
        x: area.x + gw as u16,
        y: area.y,
        height: area.height,
        pictures: Vec::new(),
    };
}

#[allow(clippy::too_many_arguments)]
/// Width of the line-number column in rendered Markdown (`lines` source lines).
fn md_gutter(lines: usize) -> u16 {
    lines.max(1).to_string().len() as u16 + 1
}

#[allow(clippy::too_many_arguments)]
fn draw_markdown(
    f: &mut Frame,
    area: Rect,
    gutter: u16,
    tab: &mut Tab,
    t: &vista_theme::Theme,
    pictures: &MdPictures,
    images: &mut crate::viewer::MdImages,
    picker: Option<&ratatui_image::picker::Picker>,
    tx: &crate::event::EventTx,
) {
    let Content::Markdown {
        rendered: Some((_, _, rows)),
        ..
    } = &tab.content
    else {
        return;
    };
    tab.scroll = tab
        .scroll
        .min(crate::viewer::max_scroll(rows.len(), area.height as usize));
    let text_area = Rect {
        x: area.x + 1 + gutter,
        width: area.width.saturating_sub(2 + gutter),
        ..area
    };
    // Wide tables scroll sideways; stop once the widest visible row is shown.
    let widest = rows
        .iter()
        .skip(tab.scroll)
        .take(area.height as usize)
        .map(|r| line_width(&r.line))
        .max()
        .unwrap_or(0);
    tab.hscroll = tab.hscroll.min(widest.saturating_sub(text_area.width as usize));
    let hscroll = tab.hscroll;
    let mut out: Vec<Line> = Vec::new();
    let mut nums: Vec<Line> = Vec::new();
    // Pictures in view: (src, first row relative to the view, rows).
    let mut slots: Vec<(String, i16, u16)> = Vec::new();
    let mut row_map = Vec::new();
    for (i, row) in rows.iter().enumerate().skip(tab.scroll).take(area.height as usize) {
        row_map.push((i, hscroll));
        if gutter > 0 {
            let w = gutter as usize - 1;
            nums.push(Line::styled(
                match row.src_line {
                    Some(n) => format!("{n:>w$} "),
                    None => " ".repeat(w + 1),
                },
                t.fg("ui.linenr"),
            ));
        }
        let decorated = decorate(&row.line, tab, i);
        let decorated = if hscroll > 0 {
            slice_cols(&decorated, hscroll, text_area.width as usize)
        } else {
            decorated
        };
        let mut line = styled_line(t, &decorated);
        if let Some(fill) = row.fill {
            let used = line_width(&decorated);
            line.spans
                .push(Span::raw(" ".repeat((text_area.width as usize).saturating_sub(used))));
            line = line.style(t.bg(fill));
        }
        // A picture starts here, or above the view and shows from its middle.
        if let Some(slot) = &row.image
            && (slot.row == 0 || i == tab.scroll)
            && hscroll == 0
        {
            let top = (i - tab.scroll) as i16 - slot.row as i16;
            slots.push((slot.src.clone(), top, slot.rows));
        }
        out.push(line);
    }
    f.render_widget(Paragraph::new(out), text_area);
    *tab.layout.borrow_mut() = crate::viewer::TextLayout {
        rows: row_map,
        x: text_area.x,
        y: area.y,
        height: area.height,
        pictures: Vec::new(),
    };
    if gutter > 0 {
        f.render_widget(
            Paragraph::new(nums),
            Rect {
                x: area.x,
                width: gutter,
                ..area
            },
        );
    }
    // The right margin column holds the scrollbar.
    if area.width > 2 {
        let bar = Rect {
            x: area.x + area.width - 1,
            width: 1,
            ..area
        };
        crate::ui::chrome::scrollbar(f, bar, t, rows.len(), tab.scroll);
    }
    let text_column = Rect {
        y: area.y,
        height: area.height,
        ..text_area
    };
    for (src, top, height) in slots {
        let path = pictures.path(&src);
        let size = ratatui::layout::Size::new(text_area.width, height);
        let y0 = top.max(0) as u16;
        let y1 = (top + height as i16).clamp(0, area.height as i16) as u16;
        let visible = Rect {
            y: area.y + y0,
            height: y1.saturating_sub(y0),
            ..text_column
        };
        match images.entries.get_mut(&path) {
            Some(MdPicture::Ready { image, sliced }) => {
                // Encoded once per size; a resize encodes again.
                if sliced.as_ref().is_none_or(|(s, _)| *s != size)
                    && let Some(p) = picker
                {
                    *sliced = SlicedProtocol::new(p, (**image).clone(), Some(size))
                        .ok()
                        .map(|proto| (size, proto));
                }
                if let Some((_, proto)) = sliced {
                    f.render_widget(Block::new().style(t.bg("ui.bg")), visible);
                    f.render_widget(SlicedImage::new(proto, SignedPosition { x: 0, y: top }), text_column);
                    tab.layout.borrow_mut().pictures.push((visible, src));
                }
            }
            Some(_) => {}
            None if picker.is_some() && !src.starts_with("mermaid:") => {
                images.entries.insert(path.clone(), MdPicture::Loading);
                let tx = tx.clone();
                std::thread::spawn(move || {
                    let res = image::open(&path).map(Box::new).map_err(|e| e.to_string());
                    let _ = tx.send(crate::event::AppEvent::MdImageDecoded { path, image: res });
                });
            }
            None => {}
        }
    }
    // Diagrams shown as text get their picture made in the background (the
    // first takes about a second); the document is laid out again when it
    // arrives.
    if pictures.diagrams {
        for src in rows.iter().filter_map(|r| r.diagram.as_deref()) {
            let key = vista_render::mermaid::key(src);
            let path = pictures.path(&key);
            images.diagrams.entry(key).or_insert_with(|| src.to_string());
            if images.entries.contains_key(&path) {
                continue;
            }
            images.entries.insert(path.clone(), MdPicture::Loading);
            let (tx, src, dark, bg) = (tx.clone(), src.to_string(), pictures.dark, pictures.background);
            std::thread::spawn(move || {
                // Twice its size, for sharp lines and text.
                let res = vista_render::mermaid::Diagram::new(&src, dark)
                    .and_then(|d| {
                        let (w, h, px) = d.draw_all(2.0, 2400, (bg.0, bg.1, bg.2));
                        image::RgbaImage::from_raw(w, h, px).ok_or_else(|| "empty diagram".to_string())
                    })
                    .map(|img| Box::new(image::DynamicImage::ImageRgba8(img)));
                let _ = tx.send(crate::event::AppEvent::MdImageDecoded { path, image: res });
            });
        }
    }
}

/// The recent-files popup.
/// The Go to Line:Column popup, IntelliJ's small dialog.
pub fn draw_goto_line(f: &mut Frame, area: Rect, app: &App) {
    let Some(g) = &app.goto_line else { return };
    let t = &app.theme;
    let rect = crate::ui::help::centered(area, 46, 4);
    f.render_widget(Clear, rect);
    let block = Block::new()
        .borders(Borders::ALL)
        .border_style(t.fg("ui.popup.border"))
        .style(t.fg_bg("ui.text", "ui.popup"))
        .title(Span::styled(
            " Go to Line:Column ",
            t.fg("ui.text.strong").add_modifier(Modifier::BOLD),
        ));
    let inner = block.inner(rect);
    f.render_widget(block, rect);
    if inner.height < 2 {
        return;
    }
    // The prefilled position shows selected: typing replaces it.
    let input_style = if g.fresh {
        t.fg_bg("ui.text.strong", "ui.selection")
    } else {
        t.fg("ui.text.strong")
    };
    let field = Line::from(vec![
        Span::styled(" [Line] [:column]: ", t.fg("ui.dim")),
        Span::styled(g.input.clone(), input_style),
        Span::styled("▏", t.fg("ui.text")),
    ]);
    f.render_widget(Paragraph::new(field), Rect { height: 1, ..inner });
    use ratatui::crossterm::event::KeyCode;
    let dim = t.fg("ui.dim");
    let mut items: Vec<click::Item> = vec![(Span::styled(format!(" Lines 1–{} · ", g.lines.max(1)), dim), None)];
    items.extend(click::hints(
        &[
            ("Enter go", click::key(KeyCode::Enter, None)),
            ("Esc cancel", click::key(KeyCode::Esc, None)),
        ],
        " · ",
        dim,
    ));
    let hints = Rect {
        y: inner.y + 1,
        height: 1,
        ..inner
    };
    click::line(
        f,
        hints,
        items,
        ratatui::layout::Alignment::Left,
        ratatui::style::Style::default(),
    );
}

pub fn draw_recent(f: &mut Frame, area: Rect, app: &App) {
    let Some(p) = &app.recent_popup else { return };
    let t = &app.theme;
    let items = p.visible(&app.recent.list);
    let h = (items.len() as u16 + 4).clamp(6, area.height.saturating_sub(4).max(6));
    let w = 70.min(area.width.saturating_sub(4)).max(30);
    let rect = crate::ui::help::centered(area, w, h);
    f.render_widget(Clear, rect);
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

/// "Press ] again to go to <file>" near the bottom of the editor area.
pub fn draw_change_hop(f: &mut Frame, area: Rect, app: &App) {
    let Some(hop) = &app.change_hop else { return };
    let t = &app.theme;
    let w = (unicode_width::UnicodeWidthStr::width(hop.text.as_str()) as u16 + 4).min(area.width);
    if area.height < 5 || w < 10 {
        return;
    }
    let rect = Rect {
        x: area.x + (area.width - w) / 2,
        y: area.y + area.height - 4,
        width: w,
        height: 3,
    };
    f.render_widget(Clear, rect);
    let block = Block::new()
        .borders(Borders::ALL)
        .border_style(t.fg("ui.popup.border"))
        .style(t.fg_bg("ui.text", "ui.popup"));
    let inner = block.inner(rect);
    f.render_widget(block, rect);
    f.render_widget(
        Paragraph::new(Span::styled(format!(" {}", hop.text), t.fg("ui.text"))),
        inner,
    );
}
