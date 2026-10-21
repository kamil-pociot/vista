//! Theme model for vista.
//!
//! A *look* is one TOML file with two parts:
//! - `[palette]`: every colour the UI uses, addressed by a semantic token
//!   (`git.modified`, `syntax.keyword`, `diff.word.inserted`, …). Widgets never
//!   hold colour literals; they ask the theme for a token.
//! - `[chrome]`: layout and decoration choices (borders, panel headers, tabs,
//!   tool-window bar, …), so a new design is a new file rather than new code.
//!
//! A look may `inherits = "<other look>"` and override only what differs.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::path::{Path, PathBuf};

use serde::Deserialize;

/// A 24-bit colour.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Rgb(pub u8, pub u8, pub u8);

impl Rgb {
    /// Parses `#RRGGBB` (the leading `#` is optional).
    pub fn parse(s: &str) -> Option<Rgb> {
        let hex = s.trim().strip_prefix('#').unwrap_or(s.trim());
        if hex.len() != 6 || !hex.chars().all(|c| c.is_ascii_hexdigit()) {
            return None;
        }
        let byte = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).ok();
        Some(Rgb(byte(0)?, byte(2)?, byte(4)?))
    }

    pub fn to_hex(self) -> String {
        format!("#{:02X}{:02X}{:02X}", self.0, self.1, self.2)
    }
}

impl fmt::Display for Rgb {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_hex())
    }
}

/// Every token a complete theme must define, in a stable order.
pub const TOKENS: &[&str] = &[
    // chrome
    "ui.bg",
    "ui.panel",
    "ui.panel.alt",
    "ui.header",
    "ui.border",
    "ui.border.focused",
    "ui.text",
    "ui.text.strong",
    "ui.dim",
    "ui.selection",
    "ui.selection.inactive",
    "ui.accent",
    "ui.cursorline",
    "ui.linenr",
    "ui.popup",
    "ui.popup.border",
    "ui.input",
    "ui.match",
    "ui.match.text",
    "ui.button",
    "ui.button.text",
    "ui.error",
    "ui.warning",
    "ui.success",
    "ui.filler",
    // git status
    "git.modified",
    "git.added",
    "git.unversioned",
    "git.deleted",
    "git.ignored",
    "git.conflict",
    "git.renamed",
    // syntax
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
    // diff
    "diff.inserted",
    "diff.modified",
    "diff.deleted",
    "diff.word.inserted",
    "diff.word.modified",
    "diff.word.deleted",
    "gutter.added",
    "gutter.modified",
    "gutter.deleted",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Borders {
    Plain,
    Rounded,
    None,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PanelHeaders {
    /// A full-width header row on a panel background (IntelliJ classic).
    TitledBar,
    /// A dim, lower-case label with no background.
    Inline,
    /// The title sits in the panel's border.
    BorderTitle,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Tabs {
    Underline,
    Text,
    Pill,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ToolWindows {
    BottomBar,
    IconRail,
    None,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub enum GitIndicator {
    #[serde(rename = "colour")]
    Colour,
    #[serde(rename = "colour+letter")]
    ColourLetter,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Spacing {
    Compact,
    Comfortable,
}

/// Layout and decoration options of a look.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Chrome {
    pub borders: Borders,
    pub panel_headers: PanelHeaders,
    pub tabs: Tabs,
    pub tool_windows: ToolWindows,
    pub git_indicator: GitIndicator,
    pub keyhints: bool,
    pub spacing: Spacing,
}

#[derive(Debug, Default, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawChrome {
    borders: Option<Borders>,
    panel_headers: Option<PanelHeaders>,
    tabs: Option<Tabs>,
    tool_windows: Option<ToolWindows>,
    git_indicator: Option<GitIndicator>,
    keyhints: Option<bool>,
    spacing: Option<Spacing>,
}

impl RawChrome {
    fn overlay(self, child: RawChrome) -> RawChrome {
        RawChrome {
            borders: child.borders.or(self.borders),
            panel_headers: child.panel_headers.or(self.panel_headers),
            tabs: child.tabs.or(self.tabs),
            tool_windows: child.tool_windows.or(self.tool_windows),
            git_indicator: child.git_indicator.or(self.git_indicator),
            keyhints: child.keyhints.or(self.keyhints),
            spacing: child.spacing.or(self.spacing),
        }
    }

    fn finish(self, id: &str) -> Result<Chrome, ThemeError> {
        let missing = |field: &str| ThemeError::MissingChrome {
            id: id.to_string(),
            field: field.to_string(),
        };
        Ok(Chrome {
            borders: self.borders.ok_or_else(|| missing("borders"))?,
            panel_headers: self.panel_headers.ok_or_else(|| missing("panel_headers"))?,
            tabs: self.tabs.ok_or_else(|| missing("tabs"))?,
            tool_windows: self.tool_windows.ok_or_else(|| missing("tool_windows"))?,
            git_indicator: self.git_indicator.ok_or_else(|| missing("git_indicator"))?,
            keyhints: self.keyhints.ok_or_else(|| missing("keyhints"))?,
            spacing: self.spacing.ok_or_else(|| missing("spacing"))?,
        })
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawTheme {
    name: Option<String>,
    inherits: Option<String>,
    #[serde(default)]
    palette: BTreeMap<String, String>,
    #[serde(default)]
    chrome: RawChrome,
}

/// A fully resolved look.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Theme {
    pub id: String,
    pub name: String,
    pub chrome: Chrome,
    palette: BTreeMap<String, Rgb>,
}

impl Rgb {
    /// WCAG relative luminance.
    pub fn luminance(self) -> f64 {
        let lin = |c: u8| {
            let c = f64::from(c) / 255.0;
            if c <= 0.03928 {
                c / 12.92
            } else {
                ((c + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * lin(self.0) + 0.7152 * lin(self.1) + 0.0722 * lin(self.2)
    }

    /// WCAG contrast ratio between two colours (1.0 – 21.0).
    pub fn contrast(self, other: Rgb) -> f64 {
        let (a, b) = (self.luminance(), other.luminance());
        let (hi, lo) = if a > b { (a, b) } else { (b, a) };
        (hi + 0.05) / (lo + 0.05)
    }
}

/// Returned for tokens a theme doesn't define, so a bug is visible instead of invisible.
pub const MISSING_COLOUR: Rgb = Rgb(0xFF, 0x00, 0xFF);

impl Theme {
    /// Colour of a token. Unknown tokens are a programming error: they assert
    /// in debug builds and render magenta in release builds.
    pub fn color(&self, token: &str) -> Rgb {
        match self.palette.get(token) {
            Some(c) => *c,
            None => {
                debug_assert!(false, "unknown theme token `{token}`");
                MISSING_COLOUR
            }
        }
    }

    pub fn try_color(&self, token: &str) -> Option<Rgb> {
        self.palette.get(token).copied()
    }

    /// The first token (in [`TOKENS`] order) whose colour is `rgb`.
    pub fn token_for(&self, rgb: Rgb) -> Option<&'static str> {
        TOKENS.iter().copied().find(|t| self.palette.get(*t) == Some(&rgb))
    }

    pub fn tokens(&self) -> impl Iterator<Item = (&str, Rgb)> {
        self.palette.iter().map(|(k, v)| (k.as_str(), *v))
    }

    /// Built-in default look.
    pub fn default_theme() -> Theme {
        ThemeLoader::new(None)
            .load("darcula")
            .expect("built-in darcula theme is valid")
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ThemeError {
    #[error("theme `{0}` not found")]
    NotFound(String),
    #[error("theme `{id}`: {source}")]
    Parse {
        id: String,
        #[source]
        source: toml::de::Error,
    },
    #[error("cannot read theme file {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("theme inheritance cycle: {}", .0.join(" -> "))]
    Cycle(Vec<String>),
    #[error("theme `{id}` is missing tokens: {}", .tokens.join(", "))]
    MissingTokens { id: String, tokens: Vec<String> },
    #[error("theme `{id}`: `{token}` = `{value}` is not a #RRGGBB colour")]
    BadColour { id: String, token: String, value: String },
    #[error("theme `{id}`: unknown token `{token}`")]
    UnknownToken { id: String, token: String },
    #[error("theme `{id}`: chrome option `{field}` is not set")]
    MissingChrome { id: String, field: String },
}

/// Built-in looks, embedded in the binary.
pub const BUILTIN: &[(&str, &str)] = &[
    ("darcula", include_str!("../themes/darcula.toml")),
    ("quiet", include_str!("../themes/quiet.toml")),
    ("paper", include_str!("../themes/paper.toml")),
];

/// Finds and resolves looks from the built-in set and an optional user directory.
/// A user file with the same id as a built-in look replaces it.
#[derive(Debug, Clone, Default)]
pub struct ThemeLoader {
    user_dir: Option<PathBuf>,
}

impl ThemeLoader {
    pub fn new(user_dir: Option<PathBuf>) -> Self {
        Self { user_dir }
    }

    /// Ids of all available looks, sorted.
    pub fn available(&self) -> Vec<String> {
        let mut ids: BTreeSet<String> = BUILTIN.iter().map(|(id, _)| id.to_string()).collect();
        if let Some(dir) = &self.user_dir
            && let Ok(entries) = std::fs::read_dir(dir)
        {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().is_some_and(|e| e == "toml")
                    && let Some(stem) = path.file_stem().and_then(|s| s.to_str())
                {
                    ids.insert(stem.to_string());
                }
            }
        }
        ids.into_iter().collect()
    }

    /// The user file backing a look, if there is one (used for hot reload).
    pub fn user_path(&self, id: &str) -> Option<PathBuf> {
        let path = self.user_dir.as_ref()?.join(format!("{id}.toml"));
        path.is_file().then_some(path)
    }

    fn source(&self, id: &str) -> Result<String, ThemeError> {
        if let Some(path) = self.user_path(id) {
            return std::fs::read_to_string(&path).map_err(|source| ThemeError::Io { path, source });
        }
        BUILTIN
            .iter()
            .find(|(bid, _)| *bid == id)
            .map(|(_, src)| src.to_string())
            .ok_or_else(|| ThemeError::NotFound(id.to_string()))
    }

    /// Loads a look by id, resolving `inherits` and validating the result.
    pub fn load(&self, id: &str) -> Result<Theme, ThemeError> {
        let mut chain = Vec::new();
        let (name, palette, chrome) = self.resolve(id, &mut chain)?;
        let mut colours = BTreeMap::new();
        for (token, value) in palette {
            let rgb = Rgb::parse(&value).ok_or_else(|| ThemeError::BadColour {
                id: id.to_string(),
                token: token.clone(),
                value: value.clone(),
            })?;
            colours.insert(token, rgb);
        }
        let missing: Vec<String> = TOKENS
            .iter()
            .filter(|t| !colours.contains_key(**t))
            .map(|t| t.to_string())
            .collect();
        if !missing.is_empty() {
            return Err(ThemeError::MissingTokens {
                id: id.to_string(),
                tokens: missing,
            });
        }
        Ok(Theme {
            id: id.to_string(),
            name: name.unwrap_or_else(|| id.to_string()),
            chrome: chrome.finish(id)?,
            palette: colours,
        })
    }

    #[allow(clippy::type_complexity)]
    fn resolve(
        &self,
        id: &str,
        chain: &mut Vec<String>,
    ) -> Result<(Option<String>, BTreeMap<String, String>, RawChrome), ThemeError> {
        if chain.iter().any(|c| c == id) {
            chain.push(id.to_string());
            return Err(ThemeError::Cycle(chain.clone()));
        }
        chain.push(id.to_string());
        let raw: RawTheme = toml::from_str(&self.source(id)?).map_err(|source| ThemeError::Parse {
            id: id.to_string(),
            source,
        })?;
        for token in raw.palette.keys() {
            if !TOKENS.contains(&token.as_str()) {
                return Err(ThemeError::UnknownToken {
                    id: id.to_string(),
                    token: token.clone(),
                });
            }
        }
        let (parent_name, mut palette, parent_chrome) = match &raw.inherits {
            Some(parent) => self.resolve(parent, chain)?,
            None => (None, BTreeMap::new(), RawChrome::default()),
        };
        palette.extend(raw.palette);
        Ok((raw.name.or(parent_name), palette, parent_chrome.overlay(raw.chrome)))
    }
}

/// Convenience for tests and tools: load a look from a directory of TOML files only.
pub fn load_from_dir(dir: &Path, id: &str) -> Result<Theme, ThemeError> {
    ThemeLoader::new(Some(dir.to_path_buf())).load(id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn user_dir(files: &[(&str, &str)]) -> tempfile_lite::Dir {
        let dir = tempfile_lite::Dir::new();
        for (name, content) in files {
            fs::write(dir.path().join(name), content).unwrap();
        }
        dir
    }

    #[test]
    fn parses_hex_colours() {
        assert_eq!(Rgb::parse("#2B2B2B"), Some(Rgb(0x2B, 0x2B, 0x2B)));
        assert_eq!(Rgb::parse("ffc66d"), Some(Rgb(0xFF, 0xC6, 0x6D)));
        assert_eq!(Rgb::parse("#FFF"), None);
        assert_eq!(Rgb::parse("#GG0000"), None);
        assert_eq!(Rgb(1, 2, 255).to_hex(), "#0102FF");
    }

    #[test]
    fn every_builtin_look_is_complete() {
        let loader = ThemeLoader::new(None);
        for (id, _) in BUILTIN {
            let theme = loader.load(id).unwrap_or_else(|e| panic!("{e}"));
            for token in TOKENS {
                assert!(theme.try_color(token).is_some(), "{id} lacks {token}");
            }
        }
    }

    /// Text must stay readable in every built-in look (WCAG AA for body text,
    /// 3:1 for secondary text, status colours and syntax).
    #[test]
    fn builtin_looks_have_readable_contrast() {
        let loader = ThemeLoader::new(None);
        for (id, _) in BUILTIN {
            let t = loader.load(id).unwrap();
            let check = |fg: &str, bg: &str, min: f64| {
                let c = t.color(fg).contrast(t.color(bg));
                assert!(c >= min, "{id}: {fg} on {bg} = {c:.2} < {min}");
            };
            check("ui.text", "ui.bg", 4.5);
            check("syntax.text", "ui.bg", 4.5);
            check("ui.text", "ui.panel", 4.5);
            check("ui.dim", "ui.bg", 3.0);
            check("ui.linenr", "ui.bg", 3.0);
            check("ui.text", "ui.selection", 4.5);
            check("ui.button.text", "ui.button", 4.5);
            check("ui.match.text", "ui.match", 4.5);
            for g in ["git.modified", "git.added", "git.unversioned", "git.conflict"] {
                check(g, "ui.panel.alt", 3.0);
                check(g, "ui.bg", 3.0);
            }
            for s in [
                "syntax.keyword",
                "syntax.string",
                "syntax.function",
                "syntax.type",
                "syntax.comment",
                "syntax.number",
            ] {
                check(s, "ui.bg", 3.0);
            }
            for d in ["diff.inserted", "diff.modified", "diff.deleted"] {
                check("syntax.text", d, 4.5);
            }
            // Word highlights must stand out from the block colour, so they
            // only need 3:1 (they mark short spans).
            for d in ["diff.word.inserted", "diff.word.modified", "diff.word.deleted"] {
                check("syntax.text", d, 3.0);
            }
        }
    }

    #[test]
    fn contrast_math() {
        assert!((Rgb(0, 0, 0).contrast(Rgb(255, 255, 255)) - 21.0).abs() < 0.01);
        assert!((Rgb(10, 10, 10).contrast(Rgb(10, 10, 10)) - 1.0).abs() < 0.001);
    }

    #[test]
    fn darcula_matches_design_a() {
        let t = Theme::default_theme();
        assert_eq!(t.name, "Darcula Classic");
        assert_eq!(t.color("ui.bg"), Rgb(0x2B, 0x2B, 0x2B));
        assert_eq!(t.color("git.modified"), Rgb(0x72, 0xA0, 0xC4));
        assert_eq!(t.chrome.panel_headers, PanelHeaders::TitledBar);
        assert_eq!(t.chrome.tool_windows, ToolWindows::BottomBar);
    }

    #[test]
    fn inherits_overrides_only_what_differs() {
        let dir = user_dir(&[(
            "mine.toml",
            "name = \"Mine\"\ninherits = \"darcula\"\n[palette]\n\"git.modified\" = \"#010203\"\n[chrome]\nborders = \"rounded\"\n",
        )]);
        let t = load_from_dir(dir.path(), "mine").unwrap();
        assert_eq!(t.name, "Mine");
        assert_eq!(t.color("git.modified"), Rgb(1, 2, 3));
        assert_eq!(t.color("ui.bg"), Theme::default_theme().color("ui.bg"));
        assert_eq!(t.chrome.borders, Borders::Rounded);
        assert_eq!(t.chrome.tabs, Theme::default_theme().chrome.tabs);
    }

    #[test]
    fn user_file_replaces_builtin_with_same_id() {
        let src = BUILTIN[0].1.replace("Darcula Classic", "Patched");
        let dir = user_dir(&[("darcula.toml", &src)]);
        let t = load_from_dir(dir.path(), "darcula").unwrap();
        assert_eq!(t.name, "Patched");
    }

    #[test]
    fn reports_missing_tokens() {
        let dir = user_dir(&[("bare.toml", "[palette]\n\"ui.bg\" = \"#000000\"\n")]);
        match load_from_dir(dir.path(), "bare") {
            Err(ThemeError::MissingTokens { tokens, .. }) => {
                assert!(tokens.contains(&"ui.text".to_string()));
                assert!(!tokens.contains(&"ui.bg".to_string()));
            }
            other => panic!("expected MissingTokens, got {other:?}"),
        }
    }

    #[test]
    fn rejects_unknown_tokens_bad_colours_and_cycles() {
        let dir = user_dir(&[
            (
                "typo.toml",
                "inherits = \"darcula\"\n[palette]\n\"git.modifed\" = \"#000000\"\n",
            ),
            ("bad.toml", "inherits = \"darcula\"\n[palette]\n\"ui.bg\" = \"red\"\n"),
            ("a.toml", "inherits = \"b\"\n"),
            ("b.toml", "inherits = \"a\"\n"),
            (
                "chromeless.toml",
                "inherits = \"darcula\"\n[chrome]\nborders = \"wavy\"\n",
            ),
        ]);
        let loader = ThemeLoader::new(Some(dir.path().to_path_buf()));
        assert!(matches!(loader.load("typo"), Err(ThemeError::UnknownToken { .. })));
        assert!(matches!(loader.load("bad"), Err(ThemeError::BadColour { .. })));
        assert!(matches!(loader.load("a"), Err(ThemeError::Cycle(_))));
        assert!(matches!(loader.load("chromeless"), Err(ThemeError::Parse { .. })));
        assert!(matches!(loader.load("nope"), Err(ThemeError::NotFound(_))));
    }

    #[test]
    fn lists_builtin_and_user_looks() {
        let dir = user_dir(&[("zz.toml", "inherits = \"darcula\"\n"), ("notes.txt", "")]);
        let ids = ThemeLoader::new(Some(dir.path().to_path_buf())).available();
        assert!(ids.contains(&"darcula".to_string()));
        assert!(ids.contains(&"zz".to_string()));
        assert!(!ids.iter().any(|i| i == "notes"));
    }

    #[test]
    fn reverse_lookup_finds_first_token() {
        let t = Theme::default_theme();
        assert_eq!(t.token_for(t.color("ui.bg")), Some("ui.bg"));
    }

    /// Tiny temp-dir helper so the theme crate needs no dev-dependencies.
    mod tempfile_lite {
        use std::path::{Path, PathBuf};
        use std::sync::atomic::{AtomicUsize, Ordering};

        static N: AtomicUsize = AtomicUsize::new(0);

        pub struct Dir(PathBuf);
        impl Dir {
            pub fn new() -> Dir {
                let p = std::env::temp_dir().join(format!(
                    "vista-theme-test-{}-{}",
                    std::process::id(),
                    N.fetch_add(1, Ordering::SeqCst)
                ));
                std::fs::create_dir_all(&p).unwrap();
                Dir(p)
            }
            pub fn path(&self) -> &Path {
                &self.0
            }
        }
        impl Drop for Dir {
            fn drop(&mut self) {
                let _ = std::fs::remove_dir_all(&self.0);
            }
        }
    }
}
