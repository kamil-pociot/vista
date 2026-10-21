//! Text styled with theme tokens (not colours), plus column-based slicing.

use unicode_width::UnicodeWidthChar;

/// A style expressed in theme tokens.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TokStyle {
    pub fg: &'static str,
    pub bg: Option<&'static str>,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub strike: bool,
}

impl Default for TokStyle {
    fn default() -> Self {
        TokStyle::fg("syntax.text")
    }
}

impl TokStyle {
    pub const fn fg(token: &'static str) -> TokStyle {
        TokStyle {
            fg: token,
            bg: None,
            bold: false,
            italic: false,
            underline: false,
            strike: false,
        }
    }
    pub const fn bold(mut self) -> Self {
        self.bold = true;
        self
    }
    pub const fn italic(mut self) -> Self {
        self.italic = true;
        self
    }
    pub const fn underline(mut self) -> Self {
        self.underline = true;
        self
    }
    pub const fn on(mut self, bg: &'static str) -> Self {
        self.bg = Some(bg);
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StyledSpan {
    pub text: String,
    pub style: TokStyle,
}

impl StyledSpan {
    pub fn new(text: impl Into<String>, style: TokStyle) -> StyledSpan {
        StyledSpan {
            text: text.into(),
            style,
        }
    }
}

pub type StyledLine = Vec<StyledSpan>;

/// A single unstyled line.
pub fn plain(text: &str) -> StyledLine {
    vec![StyledSpan::new(text, TokStyle::default())]
}

pub fn char_width(c: char) -> usize {
    if c == '\t' { 1 } else { c.width().unwrap_or(0) }
}

/// Display width of a line.
pub fn line_width(line: &StyledLine) -> usize {
    line.iter().flat_map(|s| s.text.chars()).map(char_width).sum()
}

/// Concatenated text of a line.
pub fn line_text(line: &StyledLine) -> String {
    line.iter().map(|s| s.text.as_str()).collect()
}

/// Replaces tabs with spaces up to the next tab stop.
pub fn expand_tabs(s: &str, tab: usize) -> String {
    if !s.contains('\t') {
        return s.to_string();
    }
    let mut out = String::with_capacity(s.len() + 8);
    let mut col = 0;
    for c in s.chars() {
        if c == '\t' {
            let n = tab - (col % tab);
            out.extend(std::iter::repeat_n(' ', n));
            col += n;
        } else {
            out.push(c);
            col += char_width(c);
        }
    }
    out
}

/// The part of `line` between display columns `start` and `start + width`.
/// A wide character cut by either edge becomes a space.
pub fn slice_cols(line: &StyledLine, start: usize, width: usize) -> StyledLine {
    let end = start + width;
    let mut out: StyledLine = Vec::new();
    let mut col = 0;
    for span in line {
        let mut text = String::new();
        for c in span.text.chars() {
            let w = char_width(c);
            let (c0, c1) = (col, col + w);
            col = c1;
            if c1 <= start || c0 >= end {
                continue;
            }
            if c0 < start || c1 > end {
                // Partially visible wide char.
                text.push(' ');
            } else {
                text.push(c);
            }
        }
        if !text.is_empty() {
            push_merged(&mut out, StyledSpan::new(text, span.style));
        }
        if col >= end {
            break;
        }
    }
    out
}

fn push_merged(out: &mut StyledLine, span: StyledSpan) {
    match out.last_mut() {
        Some(last) if last.style == span.style => last.text.push_str(&span.text),
        _ => out.push(span),
    }
}

/// Applies `f` to the style of every character whose char index is inside
/// one of `ranges` (half-open, char indices into the line's text).
pub fn overlay(line: &StyledLine, ranges: &[(usize, usize)], f: impl Fn(&mut TokStyle)) -> StyledLine {
    if ranges.is_empty() {
        return line.clone();
    }
    let mut out: StyledLine = Vec::new();
    let mut idx = 0;
    for span in line {
        for c in span.text.chars() {
            let mut style = span.style;
            if ranges.iter().any(|(a, b)| (*a..*b).contains(&idx)) {
                f(&mut style);
            }
            push_merged(&mut out, StyledSpan::new(c.to_string(), style));
            idx += 1;
        }
    }
    out
}

/// Splits a line into rows of at most `width` columns.
pub fn wrap_cols(line: &StyledLine, width: usize) -> Vec<StyledLine> {
    let total = line_width(line);
    if width == 0 || total <= width {
        return vec![line.clone()];
    }
    (0..total).step_by(width).map(|s| slice_cols(line, s, width)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    fn line(parts: &[(&str, &'static str)]) -> StyledLine {
        parts
            .iter()
            .map(|(t, tok)| StyledSpan::new(*t, TokStyle::fg(tok)))
            .collect()
    }

    #[test]
    fn slices_across_spans() {
        let l = line(&[("let ", "syntax.keyword"), ("x = 1;", "syntax.text")]);
        assert_eq!(line_text(&slice_cols(&l, 2, 5)), "t x =");
        let s = slice_cols(&l, 2, 5);
        assert_eq!(s[0].style.fg, "syntax.keyword");
        assert_eq!(s[1].style.fg, "syntax.text");
        assert!(slice_cols(&l, 50, 5).is_empty());
    }

    #[test]
    fn wide_chars_cut_by_an_edge_become_spaces() {
        let l = plain("a漢字b");
        assert_eq!(line_width(&l), 6);
        assert_eq!(
            line_text(&slice_cols(&l, 2, 2)),
            " 字".chars().take(2).collect::<String>().replace('字', " ")
        );
        assert_eq!(line_text(&slice_cols(&l, 1, 4)), "漢字");
    }

    #[test]
    fn expands_tabs_to_stops() {
        assert_eq!(expand_tabs("a\tb", 4), "a   b");
        assert_eq!(expand_tabs("\tx", 4), "    x");
        assert_eq!(expand_tabs("abcd\te", 4), "abcd    e");
    }

    #[test]
    fn overlays_ranges() {
        let l = plain("hello world");
        let o = overlay(&l, &[(6, 11)], |s| s.bg = Some("ui.match"));
        assert_eq!(o.len(), 2);
        assert_eq!(o[1].text, "world");
        assert_eq!(o[1].style.bg, Some("ui.match"));
    }

    #[test]
    fn wraps_by_columns() {
        let rows = wrap_cols(&plain("abcdefghij"), 4);
        let texts: Vec<String> = rows.iter().map(line_text).collect();
        assert_eq!(texts, vec!["abcd", "efgh", "ij"]);
    }

    proptest! {
        #[test]
        fn slicing_then_joining_reconstructs_ascii(s in "[ -~]{0,80}", w in 1usize..20) {
            let l = plain(&s);
            let rows = wrap_cols(&l, w);
            let joined: String = rows.iter().map(line_text).collect();
            prop_assert_eq!(joined, s);
        }
    }
}
