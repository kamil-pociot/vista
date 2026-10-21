//! Syntax highlighting with syntect (bat's grammars via two-face), producing
//! theme tokens instead of colours.
//!
//! Trick: we hand syntect a theme whose "colours" encode token indices, then
//! decode each highlighted span back into a token. One look file therefore
//! styles the chrome and the code alike.

use std::path::Path;
use std::str::FromStr;
use std::sync::OnceLock;

use syntect::easy::HighlightLines;
use syntect::highlighting::{
    Color, FontStyle, ScopeSelectors, StyleModifier, Theme as SynTheme, ThemeItem, ThemeSettings,
};
use syntect::parsing::{SyntaxReference, SyntaxSet};
use syntect::util::LinesWithEndings;

use crate::styled::{StyledLine, StyledSpan, TokStyle, expand_tabs};

/// Syntax tokens, indexed by the encoded colour's red channel.
const SYNTAX_TOKENS: &[&str] = &[
    "syntax.text",
    "syntax.keyword",
    "syntax.string",
    "syntax.function",
    "syntax.type",
    "syntax.comment",
    "syntax.attribute",
    "syntax.number",
    "syntax.constant",
    "syntax.operator",
    "syntax.punctuation",
    "syntax.tag",
    "syntax.heading",
    "syntax.link",
    "syntax.code",
];

/// TextMate scope selectors → (token, bold, italic). Later entries win when
/// they match more specifically (syntect's normal rules).
const SCOPES: &[(&str, &str, bool, bool)] = &[
    ("punctuation", "syntax.punctuation", false, false),
    ("keyword.operator", "syntax.operator", false, false),
    (
        "keyword, storage, storage.type, storage.modifier, keyword.control, keyword.other, variable.language",
        "syntax.keyword",
        false,
        false,
    ),
    (
        "keyword.operator.word, keyword.operator.logical.python",
        "syntax.keyword",
        false,
        false,
    ),
    (
        "string, string.quoted, punctuation.definition.string",
        "syntax.string",
        false,
        false,
    ),
    ("constant.character.escape", "syntax.constant", false, false),
    ("constant.numeric", "syntax.number", false, false),
    (
        "constant.language, constant.other, support.constant, variable.other.constant, variable.other.enummember, entity.name.constant",
        "syntax.constant",
        false,
        false,
    ),
    (
        "entity.name.function, support.function, meta.function-call entity.name, variable.function, support.macro, entity.name.macro",
        "syntax.function",
        false,
        false,
    ),
    (
        "entity.name.type, entity.name.class, entity.name.struct, entity.name.enum, entity.name.trait, entity.name.interface, entity.name.namespace, support.type, support.class, entity.other.inherited-class, storage.type.primitive, storage.type.numeric, storage.type.builtin",
        "syntax.type",
        false,
        false,
    ),
    (
        "entity.other.attribute-name, meta.attribute, meta.annotation, punctuation.definition.annotation",
        "syntax.attribute",
        false,
        false,
    ),
    (
        "entity.name.tag, punctuation.definition.tag",
        "syntax.tag",
        false,
        false,
    ),
    (
        "markup.heading, entity.name.section, markup.heading punctuation.definition.heading",
        "syntax.heading",
        true,
        false,
    ),
    (
        "markup.underline.link, string.other.link, meta.link",
        "syntax.link",
        false,
        false,
    ),
    (
        "markup.raw, markup.inline.raw, markup.raw.block",
        "syntax.code",
        false,
        false,
    ),
    ("markup.bold", "syntax.text", true, false),
    ("markup.italic", "syntax.text", false, true),
    ("comment, punctuation.definition.comment", "syntax.comment", false, true),
];

fn encode(token: &str) -> Color {
    let i = SYNTAX_TOKENS
        .iter()
        .position(|t| *t == token)
        .expect("known syntax token");
    Color {
        r: i as u8,
        g: 0x5A,
        b: 0xC3,
        a: 0xFF,
    }
}

fn decode(c: Color) -> &'static str {
    if c.g == 0x5A && c.b == 0xC3 {
        SYNTAX_TOKENS.get(c.r as usize).copied().unwrap_or("syntax.text")
    } else {
        "syntax.text"
    }
}

fn token_theme() -> SynTheme {
    let scopes = SCOPES
        .iter()
        .map(|(sel, token, bold, italic)| {
            let mut fs = FontStyle::empty();
            if *bold {
                fs |= FontStyle::BOLD;
            }
            if *italic {
                fs |= FontStyle::ITALIC;
            }
            ThemeItem {
                scope: ScopeSelectors::from_str(sel).expect("valid selector"),
                style: StyleModifier {
                    foreground: Some(encode(token)),
                    background: None,
                    font_style: Some(fs),
                },
            }
        })
        .collect();
    SynTheme {
        name: Some("vista-tokens".into()),
        author: None,
        settings: ThemeSettings {
            foreground: Some(encode("syntax.text")),
            ..ThemeSettings::default()
        },
        scopes,
    }
}

/// Loaded grammars plus the token theme. Building one takes tens of
/// milliseconds, so use [`Highlighter::global`].
pub struct Highlighter {
    syntaxes: SyntaxSet,
    theme: SynTheme,
}

/// Files bigger than this are shown without highlighting.
pub const MAX_HIGHLIGHT_BYTES: usize = 2 * 1024 * 1024;

impl Highlighter {
    pub fn new() -> Highlighter {
        Highlighter {
            syntaxes: two_face::syntax::extra_newlines(),
            theme: token_theme(),
        }
    }

    pub fn global() -> &'static Highlighter {
        static H: OnceLock<Highlighter> = OnceLock::new();
        H.get_or_init(Highlighter::new)
    }

    /// Warms the global highlighter in the background.
    pub fn preload() {
        std::thread::spawn(|| {
            let _ = Highlighter::global();
        });
    }

    fn syntax_for(&self, path: Option<&Path>, first_line: &str, lang: Option<&str>) -> &SyntaxReference {
        let ss = &self.syntaxes;
        if let Some(lang) = lang.map(str::trim).filter(|l| !l.is_empty()) {
            let lang = lang.split([',', ' ', '{']).next().unwrap_or(lang);
            if let Some(s) = ss.find_syntax_by_token(lang) {
                return s;
            }
        }
        if let Some(p) = path {
            if let Some(name) = p.file_name().and_then(|n| n.to_str())
                && let Some(s) = ss.find_syntax_by_extension(name)
            {
                return s;
            }
            if let Some(ext) = p.extension().and_then(|e| e.to_str())
                && let Some(s) = ss
                    .find_syntax_by_extension(ext)
                    .or_else(|| ss.find_syntax_by_extension(&ext.to_ascii_lowercase()))
            {
                return s;
            }
        }
        ss.find_syntax_by_first_line(first_line)
            .unwrap_or_else(|| ss.find_syntax_plain_text())
    }

    /// Name of the language detected for a file (status bar).
    pub fn language(&self, path: &Path, first_line: &str) -> String {
        self.syntax_for(Some(path), first_line, None).name.clone()
    }

    /// Highlights a whole file. Tabs are expanded to 4 columns.
    pub fn highlight_file(&self, path: &Path, text: &str) -> Vec<StyledLine> {
        let first = text.lines().next().unwrap_or("");
        let syntax = self.syntax_for(Some(path), first, None);
        self.highlight_with(syntax, text)
    }

    /// Highlights a code snippet in a given language (Markdown fences).
    pub fn highlight_lang(&self, lang: &str, text: &str) -> Vec<StyledLine> {
        let first = text.lines().next().unwrap_or("");
        let syntax = self.syntax_for(None, first, Some(lang));
        self.highlight_with(syntax, text)
    }

    fn highlight_with(&self, syntax: &SyntaxReference, text: &str) -> Vec<StyledLine> {
        if text.len() > MAX_HIGHLIGHT_BYTES {
            return plain_lines(text);
        }
        let mut h = HighlightLines::new(syntax, &self.theme);
        let mut out = Vec::new();
        for raw in LinesWithEndings::from(text) {
            let line = expand_tabs(raw, 4);
            let regions = match h.highlight_line(&line, &self.syntaxes) {
                Ok(r) => r,
                Err(_) => {
                    out.push(vec![StyledSpan::new(
                        line.trim_end_matches(['\n', '\r']),
                        TokStyle::default(),
                    )]);
                    continue;
                }
            };
            let mut spans: StyledLine = Vec::new();
            for (style, piece) in regions {
                let piece = piece.trim_end_matches(['\n', '\r']);
                if piece.is_empty() {
                    continue;
                }
                let mut ts = TokStyle::fg(decode(style.foreground));
                ts.bold = style.font_style.contains(FontStyle::BOLD);
                ts.italic = style.font_style.contains(FontStyle::ITALIC);
                ts.underline = style.font_style.contains(FontStyle::UNDERLINE);
                match spans.last_mut() {
                    Some(last) if last.style == ts => last.text.push_str(piece),
                    _ => spans.push(StyledSpan::new(piece, ts)),
                }
            }
            out.push(spans);
        }
        out
    }
}

impl Default for Highlighter {
    fn default() -> Self {
        Self::new()
    }
}

/// Lines without highlighting (tabs expanded).
pub fn plain_lines(text: &str) -> Vec<StyledLine> {
    text.lines()
        .map(|l| vec![StyledSpan::new(expand_tabs(l, 4), TokStyle::default())])
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::styled::line_text;

    fn tokens_of<'a>(lines: &'a [StyledLine], word: &str) -> Vec<&'a str> {
        lines
            .iter()
            .flatten()
            .filter(|s| s.text.contains(word))
            .map(|s| s.style.fg)
            .collect()
    }

    #[test]
    fn highlights_rust_into_tokens() {
        let h = Highlighter::global();
        let src = "// hi\nfn main() {\n    let s = \"str\";\n    let n = 42;\n}\n";
        let lines = h.highlight_file(Path::new("main.rs"), src);
        assert_eq!(lines.len(), 5);
        assert_eq!(line_text(&lines[1]), "fn main() {");
        assert!(tokens_of(&lines, "fn").contains(&"syntax.keyword"));
        assert!(tokens_of(&lines, "main").contains(&"syntax.function"));
        assert!(tokens_of(&lines, "str").contains(&"syntax.string"));
        assert!(tokens_of(&lines, "42").contains(&"syntax.number"));
        let comment = &lines[0][0];
        assert_eq!(comment.style.fg, "syntax.comment");
        assert!(comment.style.italic);
    }

    #[test]
    fn detects_languages() {
        let h = Highlighter::global();
        assert_eq!(h.language(Path::new("a.rs"), ""), "Rust");
        assert_eq!(h.language(Path::new("x.py"), ""), "Python");
        assert_eq!(h.language(Path::new("Cargo.toml"), ""), "TOML");
        assert_eq!(h.language(Path::new("run"), "#!/bin/bash"), "Bourne Again Shell (bash)");
        assert_eq!(h.language(Path::new("app.ts"), ""), "TypeScript");
        assert_eq!(h.language(Path::new("notes.unknownext"), ""), "Plain Text");
    }

    #[test]
    fn fenced_code_languages() {
        let h = Highlighter::global();
        let lines = h.highlight_lang("python", "def f():\n    return 1\n");
        assert!(tokens_of(&lines, "def").contains(&"syntax.keyword"));
    }

    #[test]
    fn huge_input_stays_plain_and_tabs_expand() {
        let big = "x".repeat(MAX_HIGHLIGHT_BYTES + 1);
        let lines = Highlighter::global().highlight_file(Path::new("a.rs"), &big);
        assert_eq!(lines[0][0].style, TokStyle::default());
        let tabbed = Highlighter::global().highlight_file(Path::new("a.txt"), "\tx\n");
        assert_eq!(line_text(&tabbed[0]), "    x");
    }

    #[test]
    fn every_scope_maps_to_a_syntax_token() {
        for (_, token, _, _) in SCOPES {
            assert!(SYNTAX_TOKENS.contains(token));
            assert_eq!(decode(encode(token)), *token);
        }
    }
}
