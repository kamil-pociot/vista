//! Text selection in the viewer (mouse drag, double-click word, Shift-↑↓
//! lines, Ctrl-A) and copying it to the clipboard.

use unicode_width::UnicodeWidthChar;

/// A selection between two positions: (line, char index). The head moves;
/// the anchor stays where it began. `usize::MAX` as a char index means the
/// end of the line (line-wise selection).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Selection {
    pub anchor: (usize, usize),
    pub head: (usize, usize),
}

impl Selection {
    pub fn at(p: (usize, usize)) -> Selection {
        Selection { anchor: p, head: p }
    }

    /// (start, end), in reading order.
    pub fn ordered(&self) -> ((usize, usize), (usize, usize)) {
        if self.anchor <= self.head {
            (self.anchor, self.head)
        } else {
            (self.head, self.anchor)
        }
    }

    pub fn is_empty(&self) -> bool {
        self.anchor == self.head
    }

    /// The selected char range of line `line` (`len` chars long), if any.
    pub fn range_on(&self, line: usize, len: usize) -> Option<(usize, usize)> {
        let ((l0, c0), (l1, c1)) = self.ordered();
        if line < l0 || line > l1 || self.is_empty() {
            return None;
        }
        let start = if line == l0 { c0.min(len) } else { 0 };
        let end = if line == l1 { c1.min(len) } else { len };
        (start < end).then_some((start, end))
    }

    /// The selected text of `lines`, joined with newlines.
    pub fn text(&self, lines: &[String]) -> String {
        let ((l0, _), (l1, _)) = self.ordered();
        let mut out = Vec::new();
        for (i, line) in lines.iter().enumerate().take(l1 + 1).skip(l0) {
            let len = line.chars().count();
            let (s, e) = self.range_on(i, len).unwrap_or((0, 0));
            out.push(line.chars().skip(s).take(e - s).collect::<String>());
        }
        out.join("\n")
    }
}

/// The char index at display column `col` of `text` (past the end: the
/// length).
pub fn char_at_col(text: &str, col: usize) -> usize {
    let mut w = 0;
    for (i, c) in text.chars().enumerate() {
        let cw = c.width().unwrap_or(0);
        if col < w + cw.max(1) {
            return i;
        }
        w += cw;
    }
    text.chars().count()
}

/// The word around char `at`: (start, end) chars. Word characters are
/// letters, digits and `_`; anything else selects just itself.
pub fn word_at(text: &str, at: usize) -> (usize, usize) {
    let chars: Vec<char> = text.chars().collect();
    if at >= chars.len() {
        return (chars.len(), chars.len());
    }
    let word = |c: char| c.is_alphanumeric() || c == '_';
    if !word(chars[at]) {
        return (at, at + 1);
    }
    let mut s = at;
    while s > 0 && word(chars[s - 1]) {
        s -= 1;
    }
    let mut e = at;
    while e < chars.len() && word(chars[e]) {
        e += 1;
    }
    (s, e)
}

/// Copies `text` to the system clipboard; returns how. Tests set
/// `VISTA_CLIPBOARD_FILE` to capture it instead.
pub fn copy(text: &str) -> Result<&'static str, String> {
    use std::io::Write;
    if let Some(path) = std::env::var_os("VISTA_CLIPBOARD_FILE") {
        std::fs::write(path, text).map_err(|e| e.to_string())?;
        return Ok("file");
    }
    // A clipboard program, if there is one…
    for (prog, args) in [
        ("pbcopy", &[][..]),
        ("wl-copy", &[][..]),
        ("xclip", &["-selection", "clipboard"][..]),
    ] {
        let child = std::process::Command::new(prog)
            .args(args)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn();
        if let Ok(mut child) = child {
            if let Some(mut stdin) = child.stdin.take() {
                let _ = stdin.write_all(text.as_bytes());
            }
            if child.wait().is_ok_and(|s| s.success()) {
                return Ok(prog);
            }
        }
    }
    // …or ask the terminal (OSC 52; works over ssh in most terminals).
    let mut out = std::io::stdout();
    write!(out, "\x1b]52;c;{}\x07", base64(text.as_bytes())).map_err(|e| e.to_string())?;
    out.flush().map_err(|e| e.to_string())?;
    Ok("the terminal")
}

fn base64(bytes: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for chunk in bytes.chunks(3) {
        let b = [chunk[0], *chunk.get(1).unwrap_or(&0), *chunk.get(2).unwrap_or(&0)];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(T[((n >> (18 - 6 * i)) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lines(s: &[&str]) -> Vec<String> {
        s.iter().map(|l| l.to_string()).collect()
    }

    #[test]
    fn selects_across_lines_either_way() {
        let l = lines(&["hello world", "second line", "third"]);
        let s = Selection {
            anchor: (0, 6),
            head: (2, 3),
        };
        assert_eq!(s.text(&l), "world\nsecond line\nthi");
        let back = Selection {
            anchor: (2, 3),
            head: (0, 6),
        };
        assert_eq!(back.text(&l), "world\nsecond line\nthi");
        assert_eq!(s.range_on(1, 11), Some((0, 11)));
        assert_eq!(s.range_on(3, 5), None);
        // Line-wise (to the end of the line).
        let lw = Selection {
            anchor: (1, 0),
            head: (1, usize::MAX),
        };
        assert_eq!(lw.text(&l), "second line");
        assert!(Selection::at((1, 2)).range_on(1, 11).is_none(), "empty");
    }

    #[test]
    fn columns_and_words() {
        assert_eq!(char_at_col("abc", 1), 1);
        assert_eq!(char_at_col("abc", 9), 3);
        // A wide character takes two columns.
        assert_eq!(char_at_col("日本x", 1), 0);
        assert_eq!(char_at_col("日本x", 2), 1);
        assert_eq!(char_at_col("日本x", 4), 2);
        assert_eq!(word_at("let foo_bar = 1;", 6), (4, 11));
        assert_eq!(word_at("let foo_bar = 1;", 12), (12, 13), "punctuation alone");
    }

    #[test]
    fn base64_encodes() {
        assert_eq!(base64(b"hi"), "aGk=");
        assert_eq!(base64(b"vista"), "dmlzdGE=");
        assert_eq!(base64(b"abc"), "YWJj");
    }
}
