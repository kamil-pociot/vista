//! Markdown → styled terminal lines, wrapped to a width.
//!
//! Supports headings, emphasis, strikethrough, inline code, links, lists
//! (ordered, nested, task lists), block quotes, fenced code with syntax
//! highlighting, tables, rules, HTML (shown dimmed) and images (as reserved
//! rows the viewer fills with the picture).

use pulldown_cmark::{Alignment, CodeBlockKind, Event, HeadingLevel, Options, Parser, Tag, TagEnd};
use unicode_width::UnicodeWidthStr;

use crate::highlight::Highlighter;
use crate::styled::{StyledLine, StyledSpan, TokStyle, line_width, slice_cols};

/// Where an image goes: `rows` lines starting at the line carrying `row == 0`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImageSlot {
    pub src: String,
    pub alt: String,
    pub row: u16,
    pub rows: u16,
}

/// One rendered line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MdLine {
    pub line: StyledLine,
    /// Background token for the whole row (code blocks).
    pub fill: Option<&'static str>,
    pub image: Option<ImageSlot>,
    /// The source line (1-based) this row starts, for line numbers: set on
    /// the first row of each block (and every row of a code block).
    pub src_line: Option<usize>,
    /// On the first row of a Mermaid diagram shown as text or source: its
    /// source, for the picture to be made (it then takes the rows instead).
    pub diagram: Option<String>,
}

impl MdLine {
    fn text(line: StyledLine) -> MdLine {
        MdLine {
            line,
            fill: None,
            image: None,
            src_line: None,
            diagram: None,
        }
    }
}

enum Container {
    Quote,
    List { next: Option<u64>, depth: usize },
    Item { marker: String, used: bool },
}

struct Table {
    aligns: Vec<Alignment>,
    rows: Vec<Vec<StyledLine>>,
    /// Source line of each row.
    row_lines: Vec<usize>,
    head_rows: usize,
}

/// How tables wider than the view are laid out.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Tables {
    /// Cells wrap so the table fits the width; a rule separates rows.
    #[default]
    Wrap,
    /// Every cell at full width; the table may be wider than the view.
    Wide,
}

struct Renderer<'a> {
    width: usize,
    tables: Tables,
    hl: &'a Highlighter,
    image_rows: &'a dyn Fn(&str) -> Option<u16>,
    out: Vec<MdLine>,
    inline: StyledLine,
    styles: Vec<TokStyle>,
    containers: Vec<Container>,
    heading: Option<HeadingLevel>,
    code: Option<(String, String)>,
    table: Option<Table>,
    image: Option<(String, String)>,
    /// Byte offset where each source line starts.
    line_starts: Vec<usize>,
    /// Source line of the event being handled.
    cur_line: usize,
    /// Source line for the next row written (a block just started).
    pending_line: Option<usize>,
    /// First source line of the code block being collected.
    code_line: usize,
}

const BULLETS: &[&str] = &["• ", "◦ ", "▪ "];

impl<'a> Renderer<'a> {
    fn style(&self) -> TokStyle {
        self.styles.last().copied().unwrap_or_default()
    }

    fn push_style(&mut self, f: impl FnOnce(TokStyle) -> TokStyle) {
        let s = f(self.style());
        self.styles.push(s);
    }

    fn pop_style(&mut self) {
        self.styles.pop();
    }

    fn in_item_unused(&self) -> bool {
        matches!(self.containers.last(), Some(Container::Item { used: false, .. }))
    }

    /// Prefix for the next output line (quote bars, list markers/indent).
    fn prefix(&mut self, consume_marker: bool) -> StyledLine {
        let mut p: StyledLine = Vec::new();
        for c in self.containers.iter_mut() {
            match c {
                Container::Quote => p.push(StyledSpan::new("│ ", TokStyle::fg("ui.dim"))),
                Container::List { .. } => {}
                Container::Item { marker, used } => {
                    if !*used && consume_marker {
                        p.push(StyledSpan::new(marker.clone(), TokStyle::fg("ui.accent")));
                        *used = true;
                    } else {
                        p.push(StyledSpan::new(" ".repeat(marker.width()), TokStyle::default()));
                    }
                }
            }
        }
        p
    }

    /// Writes a row; the first one after a block starts carries its line.
    fn emit(&mut self, mut l: MdLine) {
        if l.src_line.is_none() {
            l.src_line = self.pending_line.take();
        }
        self.out.push(l);
    }

    /// The next row starts a block at the current source line.
    fn block_starts(&mut self) {
        self.pending_line = Some(self.cur_line);
    }

    fn blank(&mut self) {
        // Blank lines keep quote bars but never list markers.
        let mut p: StyledLine = Vec::new();
        for c in &self.containers {
            if let Container::Quote = c {
                p.push(StyledSpan::new("│ ", TokStyle::fg("ui.dim")));
            }
        }
        self.out.push(MdLine::text(p));
    }

    fn maybe_blank(&mut self) {
        let last_blank = self
            .out
            .last()
            .is_none_or(|l| l.line.iter().all(|s| s.text.trim().is_empty() || s.text.trim() == "│"));
        if !last_blank && !self.in_item_unused() {
            self.blank();
        }
    }

    fn text(&mut self, t: &str) {
        let style = self.style();
        if let Some(cell) = self
            .table
            .as_mut()
            .and_then(|tb| tb.rows.last_mut())
            .and_then(|r| r.last_mut())
        {
            cell.push(StyledSpan::new(t, style));
            return;
        }
        self.inline.push(StyledSpan::new(t, style));
    }

    /// Word-wraps the pending inline text into output lines.
    fn flush(&mut self) {
        if self.inline.is_empty() {
            return;
        }
        let spans = std::mem::take(&mut self.inline);
        let mut first = true;
        let first_prefix = self.prefix(true);
        let rest_prefix = self.prefix(false);
        let avail = self.width.saturating_sub(line_width(&first_prefix)).max(8);
        for row in wrap_words(&spans, avail) {
            let mut line = if first {
                first_prefix.clone()
            } else {
                rest_prefix.clone()
            };
            first = false;
            line.extend(row);
            self.emit(MdLine::text(line));
        }
    }

    fn rule(&mut self, ch: &str, token: &'static str) {
        let mut line = self.prefix(false);
        let w = self.width.saturating_sub(line_width(&line));
        line.push(StyledSpan::new(ch.repeat(w), TokStyle::fg(token)));
        self.emit(MdLine::text(line));
    }

    /// A ```` ```mermaid ```` block: the picture when `image_rows` has one
    /// for it, else the diagram drawn as text, else (types text can't draw,
    /// invalid sources) the source. The first row of the last two carries
    /// the source so the picture can be made.
    fn mermaid_block(&mut self, code: &str) {
        let key = crate::mermaid::key(code);
        let first = self.out.len();
        if (self.image_rows)(&key).is_some_and(|r| r > 0) {
            self.image_block(key, "Mermaid diagram".into());
        } else {
            let avail = self.width.saturating_sub(line_width(&self.prefix(true)) + 2);
            match crate::mermaid::text(code, avail) {
                Ok(lines) => {
                    self.pending_line = None;
                    for (k, l) in lines.into_iter().enumerate() {
                        let mut line = self.prefix(true);
                        line.push(StyledSpan::new(format!("  {l}"), TokStyle::fg("ui.text")));
                        self.emit(MdLine {
                            src_line: (k == 0).then_some(self.code_line),
                            ..MdLine::text(line)
                        });
                    }
                }
                Err(_) => self.code_block("mermaid", code),
            }
            if let Some(row) = self.out.get_mut(first) {
                row.diagram = Some(code.to_string());
            }
        }
        if let Some(row) = self.out.get_mut(first) {
            row.src_line = Some(self.code_line);
        }
    }

    fn code_block(&mut self, lang: &str, code: &str) {
        let lines = if lang.is_empty() {
            crate::highlight::plain_lines(code)
        } else {
            self.hl.highlight_lang(lang, code)
        };
        self.pending_line = None;
        for (k, l) in lines.into_iter().enumerate() {
            let mut lead = self.prefix(true);
            lead.push(StyledSpan::new("  ", TokStyle::default()));
            let avail = self.width.saturating_sub(line_width(&lead));
            // Wrapped like the text around it, or (wide layout) whole, to
            // scroll sideways: never cut off.
            let rows = match self.tables {
                Tables::Wrap => wrap_code(&l, avail, lang == "diff"),
                Tables::Wide => vec![l],
            };
            for (r, row) in rows.into_iter().enumerate() {
                let mut line = lead.clone();
                line.extend(row);
                self.emit(MdLine {
                    src_line: (r == 0).then_some(self.code_line + k),
                    fill: Some("ui.panel.alt"),
                    ..MdLine::text(line)
                });
            }
        }
    }

    fn image_block(&mut self, src: String, alt: String) {
        match (self.image_rows)(&src) {
            Some(rows) if rows > 0 => {
                self.flush();
                for row in 0..rows {
                    let mut line = self.prefix(true);
                    if row == 0 {
                        line.push(StyledSpan::new(format!("[image: {alt}]"), TokStyle::fg("ui.dim")));
                    }
                    self.emit(MdLine {
                        image: Some(ImageSlot {
                            src: src.clone(),
                            alt: alt.clone(),
                            row,
                            rows,
                        }),
                        ..MdLine::text(line)
                    });
                }
            }
            _ => {
                let label = if alt.is_empty() { src } else { alt };
                self.inline.push(StyledSpan::new(
                    format!("[image: {label}]"),
                    TokStyle::fg("syntax.link").underline(),
                ));
            }
        }
    }

    fn table_end(&mut self) {
        let Some(table) = self.table.take() else { return };
        let ncols = table.rows.iter().map(Vec::len).max().unwrap_or(0);
        if ncols == 0 {
            return;
        }
        let mut widths = vec![1usize; ncols];
        for row in &table.rows {
            for (i, cell) in row.iter().enumerate() {
                widths[i] = widths[i].max(line_width(cell));
            }
        }
        let prefix = self.prefix(true);
        let avail = self.width.saturating_sub(line_width(&prefix) + 3 * ncols + 1);
        if self.tables == Tables::Wrap {
            // Narrow the widest column first, so short columns stay intact.
            while widths.iter().sum::<usize>() > avail && widths.iter().any(|w| *w > 3) {
                let i = (0..ncols).max_by_key(|i| widths[*i]).unwrap_or(0);
                widths[i] -= 1;
            }
        }
        let border = TokStyle::fg("ui.dim");
        let edge = |l: &str, m: &str, r: &str, fill: &str| -> StyledLine {
            let mut s = String::from(l);
            for (i, w) in widths.iter().enumerate() {
                s.push_str(&fill.repeat(w + 2));
                s.push_str(if i + 1 == ncols { r } else { m });
            }
            vec![StyledSpan::new(s, border)]
        };
        let mut lines: Vec<(StyledLine, Option<usize>)> = vec![(edge("┌", "┬", "┐", "─"), None)];
        for (ri, row) in table.rows.iter().enumerate() {
            // Each cell as rows of at most its column's width.
            let cells: Vec<Vec<StyledLine>> = widths
                .iter()
                .enumerate()
                .map(|(i, w)| {
                    let mut cell = row.get(i).cloned().unwrap_or_default();
                    if ri < table.head_rows {
                        for s in &mut cell {
                            s.style.bold = true;
                        }
                    }
                    let rows = if line_width(&cell) > *w {
                        wrap_words(&cell, *w)
                    } else {
                        vec![cell]
                    };
                    if rows.is_empty() { vec![Vec::new()] } else { rows }
                })
                .collect();
            let height = cells.iter().map(Vec::len).max().unwrap_or(1);
            for k in 0..height {
                let mut line: StyledLine = vec![StyledSpan::new("│", border)];
                for (i, w) in widths.iter().enumerate() {
                    let content = cells[i].get(k).cloned().unwrap_or_default();
                    let pad = w - line_width(&content).min(*w);
                    let (left, right) = match table.aligns.get(i) {
                        Some(Alignment::Right) => (pad, 0),
                        Some(Alignment::Center) => (pad / 2, pad - pad / 2),
                        _ => (0, pad),
                    };
                    line.push(StyledSpan::new(" ".repeat(left + 1), TokStyle::default()));
                    line.extend(content);
                    line.push(StyledSpan::new(" ".repeat(right + 1), TokStyle::default()));
                    line.push(StyledSpan::new("│", border));
                }
                let src = (k == 0).then(|| table.row_lines.get(ri).copied()).flatten();
                lines.push((line, src));
            }
            if ri + 1 == table.head_rows {
                lines.push((edge("╞", "╪", "╡", "═"), None));
            } else if ri + 1 < table.rows.len() {
                lines.push((edge("├", "┼", "┤", "─"), None));
            }
        }
        lines.push((edge("└", "┴", "┘", "─"), None));
        self.pending_line = None;
        for (l, src) in lines {
            let mut line = self.prefix(true);
            line.extend(l);
            self.emit(MdLine {
                src_line: src,
                ..MdLine::text(line)
            });
        }
    }

    fn event(&mut self, ev: Event) {
        match ev {
            Event::Start(tag) => self.start(tag),
            Event::End(tag) => self.end(tag),
            Event::Text(t) => {
                if let Some((_, code)) = self.code.as_mut() {
                    code.push_str(&t);
                } else if let Some((_, alt)) = self.image.as_mut() {
                    alt.push_str(&t);
                } else {
                    self.text(&t);
                }
            }
            Event::Code(t) => {
                let style = TokStyle {
                    fg: "syntax.code",
                    ..self.style()
                };
                self.push_style(|_| style);
                self.text(&t);
                self.pop_style();
            }
            Event::SoftBreak => self.text(" "),
            Event::HardBreak => self.flush(),
            Event::Rule => {
                self.flush();
                self.maybe_blank();
                self.block_starts();
                self.rule("─", "ui.dim");
            }
            Event::Html(t) | Event::InlineHtml(t) => {
                if self.inline.is_empty() && self.pending_line.is_none() {
                    self.block_starts();
                }
                for (i, part) in t.trim_end_matches('\n').split('\n').enumerate() {
                    if i > 0 {
                        self.flush();
                    }
                    self.push_style(|_| TokStyle::fg("ui.dim"));
                    self.text(part);
                    self.pop_style();
                }
            }
            Event::TaskListMarker(done) => {
                let (mark, token) = if done {
                    ("☑ ", "ui.success")
                } else {
                    ("☐ ", "ui.dim")
                };
                self.push_style(|_| TokStyle::fg(token));
                self.text(mark);
                self.pop_style();
            }
            Event::FootnoteReference(label) => {
                self.push_style(|_| TokStyle::fg("ui.dim"));
                self.text(&format!("[^{label}]"));
                self.pop_style();
            }
            _ => {}
        }
    }

    fn start(&mut self, tag: Tag) {
        match tag {
            Tag::Paragraph => {
                self.maybe_blank();
                self.block_starts();
            }
            Tag::Heading { level, .. } => {
                self.flush();
                self.maybe_blank();
                self.block_starts();
                self.heading = Some(level);
                let style = match level {
                    HeadingLevel::H1 | HeadingLevel::H2 | HeadingLevel::H3 => TokStyle::fg("syntax.heading").bold(),
                    _ => TokStyle::fg("syntax.text").bold(),
                };
                self.push_style(|_| style);
            }
            Tag::BlockQuote(_) => {
                self.flush();
                self.maybe_blank();
                self.containers.push(Container::Quote);
                self.push_style(|s| TokStyle { fg: "ui.dim", ..s }.italic());
            }
            Tag::CodeBlock(kind) => {
                self.flush();
                self.maybe_blank();
                // Fenced code starts on the line after the fence.
                self.code_line = self.cur_line + usize::from(matches!(kind, CodeBlockKind::Fenced(_)));
                let lang = match kind {
                    CodeBlockKind::Fenced(l) => l.to_string(),
                    CodeBlockKind::Indented => String::new(),
                };
                self.code = Some((lang, String::new()));
            }
            Tag::List(start) => {
                self.flush();
                let depth = self
                    .containers
                    .iter()
                    .filter(|c| matches!(c, Container::List { .. }))
                    .count();
                if depth == 0 {
                    self.maybe_blank();
                }
                self.containers.push(Container::List { next: start, depth });
            }
            Tag::Item => {
                self.flush();
                let marker = match self.containers.iter_mut().rev().find_map(|c| match c {
                    Container::List { next, depth } => Some((next, *depth)),
                    _ => None,
                }) {
                    Some((Some(n), _)) => {
                        let m = format!("{n}. ");
                        *n += 1;
                        m
                    }
                    Some((None, depth)) => BULLETS[depth % BULLETS.len()].to_string(),
                    None => BULLETS[0].to_string(),
                };
                self.containers.push(Container::Item { marker, used: false });
                self.block_starts();
            }
            Tag::Emphasis => self.push_style(|s| s.italic()),
            Tag::Strong => self.push_style(|s| s.bold()),
            Tag::Strikethrough => self.push_style(|mut s| {
                s.strike = true;
                s
            }),
            Tag::Link { .. } => self.push_style(|s| TokStyle { fg: "syntax.link", ..s }.underline()),
            Tag::Image { dest_url, .. } => self.image = Some((dest_url.to_string(), String::new())),
            Tag::Table(aligns) => {
                self.flush();
                self.maybe_blank();
                self.table = Some(Table {
                    aligns,
                    rows: Vec::new(),
                    row_lines: Vec::new(),
                    head_rows: 0,
                });
            }
            Tag::TableHead | Tag::TableRow => {
                let line = self.cur_line;
                if let Some(t) = self.table.as_mut() {
                    t.rows.push(Vec::new());
                    t.row_lines.push(line);
                }
            }
            Tag::TableCell => {
                if let Some(row) = self.table.as_mut().and_then(|t| t.rows.last_mut()) {
                    row.push(Vec::new());
                }
            }
            _ => {}
        }
    }

    fn end(&mut self, tag: TagEnd) {
        match tag {
            TagEnd::Paragraph => self.flush(),
            TagEnd::Heading(level) => {
                self.flush();
                self.pop_style();
                self.heading = None;
                match level {
                    HeadingLevel::H1 => self.rule("━", "syntax.heading"),
                    HeadingLevel::H2 => self.rule("─", "ui.dim"),
                    _ => {}
                }
            }
            TagEnd::BlockQuote(_) => {
                self.flush();
                self.containers.pop();
                self.pop_style();
            }
            TagEnd::CodeBlock => {
                if let Some((lang, code)) = self.code.take() {
                    if crate::mermaid::is_mermaid(&lang) {
                        self.mermaid_block(&code);
                    } else {
                        self.code_block(&lang, &code);
                    }
                }
            }
            TagEnd::List(_) => {
                self.flush();
                self.containers.pop();
            }
            TagEnd::Item => {
                self.flush();
                if self.in_item_unused() {
                    // Empty item: still show its marker.
                    let line = self.prefix(true);
                    self.emit(MdLine::text(line));
                }
                self.containers.pop();
            }
            TagEnd::Emphasis | TagEnd::Strong | TagEnd::Strikethrough | TagEnd::Link => self.pop_style(),
            TagEnd::Image => {
                if let Some((src, alt)) = self.image.take() {
                    self.image_block(src, alt);
                }
            }
            TagEnd::TableHead => {
                if let Some(t) = self.table.as_mut() {
                    t.head_rows = t.rows.len();
                }
            }
            TagEnd::Table => self.table_end(),
            _ => {}
        }
    }
}

/// Wraps a line of code into rows of at most `width` columns: at spaces,
/// keeping its indentation, the rows after the first lined up under its
/// first character. With `marker` (a diff), the first column is the `+` /
/// `-` and the indentation is what follows it. (Words that don't fit are
/// split.)
pub fn wrap_code(line: &StyledLine, width: usize, marker: bool) -> Vec<StyledLine> {
    let total = line_width(line);
    if total <= width || width == 0 {
        return vec![line.clone()];
    }
    let text: String = line.iter().map(|s| s.text.as_str()).collect();
    let skip = usize::from(marker && !text.is_empty());
    let indent = skip + text.chars().skip(skip).take_while(|c| *c == ' ').count();
    // Deep indentation leaves no room to line up under: just split.
    if indent * 2 > width {
        return (0..total)
            .step_by(width)
            .map(|start| slice_cols(line, start, width))
            .collect();
    }
    let head = slice_cols(line, 0, indent);
    let body = slice_cols(line, indent, total - indent);
    let pad = vec![StyledSpan::new(" ".repeat(indent), TokStyle::default())];
    wrap_words(&body, width - indent)
        .into_iter()
        .enumerate()
        .map(|(i, row)| {
            let mut out = if i == 0 { head.clone() } else { pad.clone() };
            out.retain(|s| !s.text.is_empty());
            out.extend(row);
            out
        })
        .collect()
}

/// Word-wraps spans into rows of at most `width` columns, keeping styles.
pub fn wrap_words(spans: &StyledLine, width: usize) -> Vec<StyledLine> {
    let mut rows: Vec<StyledLine> = Vec::new();
    let mut row: StyledLine = Vec::new();
    let mut row_w = 0;
    // Split into words and runs of spaces, keeping each piece's style.
    let mut pieces: Vec<(String, crate::styled::TokStyle, bool)> = Vec::new();
    for s in spans {
        let mut cur = String::new();
        let mut cur_space = None;
        for c in s.text.chars() {
            let is_space = c == ' ';
            if cur_space.is_some_and(|sp| sp != is_space) {
                pieces.push((std::mem::take(&mut cur), s.style, cur_space.unwrap_or(false)));
            }
            cur.push(c);
            cur_space = Some(is_space);
        }
        if !cur.is_empty() {
            pieces.push((cur, s.style, cur_space.unwrap_or(false)));
        }
    }
    let push = |row: &mut StyledLine, text: &str, style: TokStyle| match row.last_mut() {
        Some(last) if last.style == style => last.text.push_str(text),
        _ => row.push(StyledSpan::new(text, style)),
    };
    for (text, style, is_space) in pieces {
        let w = text.width();
        if is_space {
            if row_w > 0 && row_w + w <= width {
                push(&mut row, &text, style);
                row_w += w;
            } else if row_w > 0 {
                rows.push(std::mem::take(&mut row));
                row_w = 0;
            }
            continue;
        }
        if row_w + w > width && row_w > 0 {
            // Drop trailing spaces before breaking.
            if let Some(last) = row.last_mut() {
                let trimmed = last.text.trim_end().to_string();
                last.text = trimmed;
            }
            rows.push(std::mem::take(&mut row));
            row_w = 0;
        }
        if w > width {
            // A word longer than the line: hard-split it.
            let word = vec![StyledSpan::new(text.clone(), style)];
            let mut start = 0;
            while start < w {
                let part = slice_cols(&word, start, width - row_w);
                let pw = line_width(&part);
                for s in part {
                    push(&mut row, &s.text, s.style);
                }
                row_w += pw;
                start += pw.max(1);
                if row_w >= width {
                    rows.push(std::mem::take(&mut row));
                    row_w = 0;
                }
            }
            continue;
        }
        push(&mut row, &text, style);
        row_w += w;
    }
    if !row.is_empty() || rows.is_empty() {
        rows.push(row);
    }
    rows
}

/// Renders Markdown to lines no wider than `width`. `image_rows(src)` says how
/// many rows to reserve for an image (`None` shows a text placeholder).
pub fn render(src: &str, width: usize, hl: &Highlighter, image_rows: &dyn Fn(&str) -> Option<u16>) -> Vec<MdLine> {
    render_with(src, width, Tables::Wrap, hl, image_rows)
}

/// [`render`] with a table layout; `Tables::Wide` tables may exceed `width`.
pub fn render_with(
    src: &str,
    width: usize,
    tables: Tables,
    hl: &Highlighter,
    image_rows: &dyn Fn(&str) -> Option<u16>,
) -> Vec<MdLine> {
    let mut opts = Options::empty();
    opts.insert(Options::ENABLE_TABLES);
    opts.insert(Options::ENABLE_TASKLISTS);
    opts.insert(Options::ENABLE_STRIKETHROUGH);
    opts.insert(Options::ENABLE_FOOTNOTES);
    let mut r = Renderer {
        width: width.max(10),
        tables,
        hl,
        image_rows,
        out: Vec::new(),
        inline: Vec::new(),
        styles: Vec::new(),
        containers: Vec::new(),
        heading: None,
        code: None,
        table: None,
        image: None,
        line_starts: std::iter::once(0)
            .chain(src.match_indices('\n').map(|(i, _)| i + 1))
            .collect(),
        cur_line: 1,
        pending_line: None,
        code_line: 1,
    };
    for (ev, range) in Parser::new_ext(src, opts).into_offset_iter() {
        r.cur_line = r.line_starts.partition_point(|&s| s <= range.start);
        r.event(ev);
    }
    r.flush();
    r.out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::styled::line_text;

    fn md(src: &str, width: usize) -> Vec<MdLine> {
        render(src, width, Highlighter::global(), &|_| None)
    }

    fn texts(lines: &[MdLine]) -> Vec<String> {
        lines
            .iter()
            .map(|l| line_text(&l.line).trim_end().to_string())
            .collect()
    }

    #[test]
    fn headings_paragraphs_and_rules() {
        let out = md("# Title\n\nSome *em* and **strong** text.\n\n## Sub\n\n---\n", 20);
        let t = texts(&out);
        assert_eq!(t[0], "Title");
        assert_eq!(t[1], "━".repeat(20));
        assert_eq!(t[3], "Some em and strong");
        assert_eq!(t[4], "text.");
        assert!(out[0].line[0].style.bold && out[0].line[0].style.fg == "syntax.heading");
        let em = out[3].line.iter().find(|s| s.text == "em").unwrap();
        assert!(em.style.italic);
        let strong = out[3].line.iter().find(|s| s.text == "strong").unwrap();
        assert!(strong.style.bold);
        assert!(t.contains(&"Sub".to_string()));
        assert_eq!(t.last().unwrap(), &"─".repeat(20));
    }

    #[test]
    fn lists_nested_ordered_and_tasks() {
        let out = md(
            "- one\n- two\n  - inner\n1. first\n2. second\n\n- [x] done\n- [ ] todo\n",
            40,
        );
        let t = texts(&out);
        assert!(t.contains(&"• one".to_string()), "{t:?}");
        assert!(t.contains(&"  ◦ inner".to_string()), "{t:?}");
        assert!(t.contains(&"1. first".to_string()), "{t:?}");
        assert!(t.contains(&"2. second".to_string()), "{t:?}");
        assert!(t.contains(&"• ☑ done".to_string()), "{t:?}");
        assert!(t.contains(&"• ☐ todo".to_string()), "{t:?}");
    }

    #[test]
    fn list_items_wrap_under_their_text() {
        let out = md("- alpha beta gamma delta\n", 14);
        let t = texts(&out);
        assert_eq!(t, vec!["• alpha beta", "  gamma delta"]);
    }

    #[test]
    fn quotes_and_code_blocks() {
        let out = md("> quoted text\n\n```rust\nfn main() {}\n```\n", 30);
        let t = texts(&out);
        assert_eq!(t[0], "│ quoted text");
        let code = out.iter().find(|l| line_text(&l.line).contains("fn main")).unwrap();
        assert_eq!(code.fill, Some("ui.panel.alt"));
        assert!(
            code.line
                .iter()
                .any(|s| s.text == "fn" && s.style.fg == "syntax.keyword")
        );
    }

    const TABLE: &str = "| Name | Size |\n|:-----|-----:|\n| a.rs | 10 |\n| a very long file name.txt | 2000 |\n";

    #[test]
    fn tables_are_boxed_and_fit() {
        let out = md(TABLE, 24);
        let t = texts(&out);
        assert!(t[0].starts_with('┌') && t[0].ends_with('┐'));
        assert!(t[1].contains("Name") && t[1].contains("Size"));
        assert!(t[2].starts_with('╞'), "header rule: {t:#?}");
        assert!(t.iter().all(|l| l.chars().count() <= 24), "{t:#?}");
        assert!(t.last().unwrap().starts_with('└'));
        let header = out[1].line.iter().find(|s| s.text.contains("Name")).unwrap();
        assert!(header.style.bold);
    }

    #[test]
    fn long_cells_wrap_with_rules_between_rows() {
        let t = texts(&md(TABLE, 24));
        assert!(!t.iter().any(|l| l.contains('…')), "nothing is cut: {t:#?}");
        // Every word of the long cell is shown, spread over several rows.
        let body = t.join(" ");
        for word in ["a", "very", "long", "file", "name.txt", "2000"] {
            assert!(body.contains(word), "{word} missing: {t:#?}");
        }
        let cell_rows = t.iter().filter(|l| l.starts_with('│')).count();
        assert!(cell_rows > 3, "the long row wraps: {t:#?}");
        // A rule between the two body rows.
        assert_eq!(t.iter().filter(|l| l.starts_with('├')).count(), 1, "{t:#?}");
        // The right-aligned column stays right-aligned.
        let row = t.iter().find(|l| l.contains("2000")).unwrap();
        assert!(row.trim_end().ends_with("2000 │"), "{row}");
    }

    #[test]
    fn wide_tables_keep_cells_whole() {
        let out = render_with(TABLE, 24, Tables::Wide, Highlighter::global(), &|_| None);
        let t = texts(&out);
        assert!(t.iter().any(|l| l.contains("a very long file name.txt")), "{t:#?}");
        assert!(t.iter().any(|l| l.chars().count() > 24), "wider than the view: {t:#?}");
        let widths: Vec<usize> = t.iter().map(|l| l.chars().count()).collect();
        assert!(widths.iter().all(|w| *w == widths[0]), "all rows line up: {t:#?}");
    }

    #[test]
    fn rows_carry_the_source_line_of_their_block() {
        let src = "# Title\n\nA paragraph that is long enough to wrap onto a second row here.\n\n- one\n- two\n\n```rust\nfn a() {}\nfn b() {}\n```\n\n| K | V |\n|---|---|\n| a | 1 |\n| b | 2 |\n";
        let out = md(src, 30);
        let by_line = |n: usize| {
            out.iter()
                .find(|l| l.src_line == Some(n))
                .map(|l| line_text(&l.line))
                .unwrap_or_default()
        };
        assert!(by_line(1).contains("Title"), "{:?}", by_line(1));
        assert!(by_line(3).contains("A paragraph"));
        assert!(by_line(5).contains("one"));
        assert!(by_line(6).contains("two"));
        assert!(by_line(9).contains("fn a"));
        assert!(by_line(10).contains("fn b"));
        assert!(by_line(13).contains('K'));
        assert!(by_line(15).contains('a'));
        assert!(by_line(16).contains('b'));
        // Wrapped rows and spacing rows have no number; each number appears once.
        let numbered: Vec<usize> = out.iter().filter_map(|l| l.src_line).collect();
        let mut dedup = numbered.clone();
        dedup.dedup();
        assert_eq!(numbered, dedup, "{numbered:?}");
        assert!(
            out.iter()
                .any(|l| l.src_line.is_none() && line_text(&l.line).contains("second"))
        );
    }

    #[test]
    fn links_inline_code_and_html() {
        let out = md("See [docs](https://x.y) and `code`.\n\n<div>raw</div>\n", 60);
        let docs = out[0].line.iter().find(|s| s.text == "docs").unwrap();
        assert_eq!(docs.style.fg, "syntax.link");
        assert!(docs.style.underline);
        let code = out[0].line.iter().find(|s| s.text == "code").unwrap();
        assert_eq!(code.style.fg, "syntax.code");
        assert!(texts(&out).contains(&"<div>raw</div>".to_string()));
    }

    #[test]
    fn images_reserve_rows_or_fall_back_to_text() {
        let src = "Before\n\n![a cat](cat.png)\n\nAfter\n";
        let with = render(src, 40, Highlighter::global(), &|s| (s == "cat.png").then_some(3));
        let slots: Vec<_> = with.iter().filter_map(|l| l.image.as_ref()).collect();
        assert_eq!(slots.len(), 3);
        assert_eq!(slots[0].row, 0);
        assert_eq!(slots[0].src, "cat.png");
        assert_eq!(slots[0].alt, "a cat");
        let without = md(src, 40);
        assert!(texts(&without).contains(&"[image: a cat]".to_string()));
    }

    #[test]
    fn long_words_are_split() {
        let rows = wrap_words(&crate::styled::plain("aaaaaaaaaaaaaaa bb"), 6);
        let t: Vec<String> = rows.iter().map(line_text).collect();
        assert_eq!(t, vec!["aaaaaa", "aaaaaa", "aaa bb"]);
    }

    #[test]
    fn never_exceeds_width() {
        let src = "# A very long heading that needs wrapping across lines\n\n> a quote with many many words that wrap\n\n- item with **bold** words that go on and on\n";
        for w in [10usize, 17, 33] {
            for l in md(src, w) {
                assert!(line_width(&l.line) <= w.max(10), "width {w}: {:?}", line_text(&l.line));
            }
        }
    }

    const SEQUENCE: &str = "# Flow\n\n```mermaid\nsequenceDiagram\n  User->>vista: open README.md\n```\n\nafter\n";

    #[test]
    fn mermaid_blocks_draw_as_text_until_there_is_a_picture() {
        let lines = md(SEQUENCE, 60);
        let t = texts(&lines);
        let first = lines.iter().position(|l| l.diagram.is_some()).unwrap();
        assert!(t[first].contains('┌'), "{t:?}");
        assert!(t.iter().any(|l| l.contains("open README.md")), "{t:?}");
        assert!(
            !t.iter().any(|l| l.contains("sequenceDiagram")),
            "the source isn't shown: {t:?}"
        );
        assert_eq!(lines[first].src_line, Some(4), "the line after the fence");
        assert_eq!(
            lines[first].diagram.as_deref(),
            Some("sequenceDiagram\n  User->>vista: open README.md\n")
        );
        assert!(t.contains(&"after".to_string()));
    }

    #[test]
    fn mermaid_blocks_take_the_picture_once_it_is_made() {
        let key = crate::mermaid::key("sequenceDiagram\n  User->>vista: open README.md\n");
        let rows = |src: &str| (src == key).then_some(6);
        let lines = render(SEQUENCE, 60, Highlighter::global(), &rows);
        let slots: Vec<&ImageSlot> = lines.iter().filter_map(|l| l.image.as_ref()).collect();
        assert_eq!(slots.len(), 6);
        assert!(slots.iter().all(|s| s.src == key && s.rows == 6));
        assert!(lines.iter().all(|l| l.diagram.is_none()), "nothing left to make");
        let first = lines.iter().position(|l| l.image.is_some()).unwrap();
        assert_eq!(lines[first].src_line, Some(4));
    }

    #[test]
    fn mermaid_without_a_text_form_shows_its_source() {
        let lines = md("```mermaid\npie title Pets\n  \"Dogs\" : 3\n```\n", 60);
        let t = texts(&lines);
        assert!(t[0].contains("pie title Pets"), "{t:?}");
        assert_eq!(lines[0].fill, Some("ui.panel.alt"), "as a code block");
        assert!(lines[0].diagram.is_some(), "a picture can still be made");
    }

    const DIFF: &str = "Before\n\n```diff\n-- Amazon Web Services (Frankfurt, UE): serwery, baza danych, pliki, logowanie\n+    indented line that is also far too long to fit in the narrow view\n short\n```\n";

    #[test]
    fn long_code_lines_wrap_with_the_text() {
        let lines = md(DIFF, 40);
        let code: Vec<&MdLine> = lines.iter().filter(|l| l.fill == Some("ui.panel.alt")).collect();
        let t: Vec<String> = code.iter().map(|l| line_text(&l.line)).collect();
        assert!(code.len() > 3, "wrapped onto more rows: {t:?}");
        assert!(code.iter().all(|l| line_width(&l.line) <= 40), "{t:?}");
        // Nothing is lost: every word of the first line is there.
        let all = t.join(" ");
        for w in ["Amazon", "Frankfurt,", "pliki,", "logowanie"] {
            assert!(all.contains(w), "{w} missing: {t:?}");
        }
        // A line number on each source line's first row only.
        let numbered: Vec<usize> = code.iter().filter_map(|l| l.src_line).collect();
        assert_eq!(numbered, vec![4, 5, 6]);
        // The indented line's later rows line up under its first character.
        let first = t.iter().position(|l| l.contains("+    indented")).unwrap();
        assert!(t[first + 1].starts_with("       "), "{:?}", t[first + 1]);
        assert_eq!(t.last().unwrap().trim_end(), "   short");
    }

    #[test]
    fn wide_layout_keeps_code_lines_whole_to_scroll_sideways() {
        let lines = render_with(DIFF, 40, Tables::Wide, Highlighter::global(), &|_| None);
        let code: Vec<String> = lines
            .iter()
            .filter(|l| l.fill == Some("ui.panel.alt"))
            .map(|l| line_text(&l.line))
            .collect();
        assert_eq!(code.len(), 3, "one row each: {code:?}");
        assert!(code[0].trim_end().ends_with("pliki, logowanie"), "{:?}", code[0]);
        assert!(line_width(&lines.iter().find(|l| l.fill.is_some()).unwrap().line) > 40);
    }

    #[test]
    fn wrapping_code_keeps_indentation_or_splits_deep_lines() {
        let line = vec![StyledSpan::new("  let x = alpha + beta + gamma;", TokStyle::default())];
        let rows: Vec<String> = wrap_code(&line, 16, false).iter().map(line_text).collect();
        assert_eq!(rows, vec!["  let x = alpha", "  + beta +", "  gamma;"]);
        let deep = vec![StyledSpan::new("          abcdefghij", TokStyle::default())];
        let rows: Vec<String> = wrap_code(&deep, 8, false).iter().map(line_text).collect();
        assert_eq!(rows.concat(), "          abcdefghij", "split, nothing dropped");
    }
}
