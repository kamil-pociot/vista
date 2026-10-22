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
}

impl MdLine {
    fn text(line: StyledLine) -> MdLine {
        MdLine {
            line,
            fill: None,
            image: None,
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
            self.out.push(MdLine::text(line));
        }
    }

    fn rule(&mut self, ch: &str, token: &'static str) {
        let mut line = self.prefix(false);
        let w = self.width.saturating_sub(line_width(&line));
        line.push(StyledSpan::new(ch.repeat(w), TokStyle::fg(token)));
        self.out.push(MdLine::text(line));
    }

    fn code_block(&mut self, lang: &str, code: &str) {
        let lines = if lang.is_empty() {
            crate::highlight::plain_lines(code)
        } else {
            self.hl.highlight_lang(lang, code)
        };
        for l in lines {
            let mut line = self.prefix(true);
            line.push(StyledSpan::new("  ", TokStyle::default()));
            let avail = self.width.saturating_sub(line_width(&line));
            line.extend(slice_cols(&l, 0, avail));
            self.out.push(MdLine {
                line,
                fill: Some("ui.panel.alt"),
                image: None,
            });
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
                    self.out.push(MdLine {
                        line,
                        fill: None,
                        image: Some(ImageSlot {
                            src: src.clone(),
                            alt: alt.clone(),
                            row,
                            rows,
                        }),
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
        let mut lines: Vec<StyledLine> = vec![edge("┌", "┬", "┐", "─")];
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
                lines.push(line);
            }
            if ri + 1 == table.head_rows {
                lines.push(edge("╞", "╪", "╡", "═"));
            } else if ri + 1 < table.rows.len() {
                lines.push(edge("├", "┼", "┤", "─"));
            }
        }
        lines.push(edge("└", "┴", "┘", "─"));
        for l in lines {
            let mut line = self.prefix(true);
            line.extend(l);
            self.out.push(MdLine::text(line));
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
                self.rule("─", "ui.dim");
            }
            Event::Html(t) | Event::InlineHtml(t) => {
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
            Tag::Paragraph => self.maybe_blank(),
            Tag::Heading { level, .. } => {
                self.flush();
                self.maybe_blank();
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
                    head_rows: 0,
                });
            }
            Tag::TableHead | Tag::TableRow => {
                if let Some(t) = self.table.as_mut() {
                    t.rows.push(Vec::new());
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
                    self.code_block(&lang, &code);
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
                    self.out.push(MdLine::text(line));
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
    };
    for ev in Parser::new_ext(src, opts) {
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
}
