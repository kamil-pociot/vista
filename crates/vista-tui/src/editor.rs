//! Glue between the app and the embedded Neovim: spawning, theme sync, key
//! and mouse translation, and the `$EDITOR` fallback.

use std::path::Path;
use std::sync::mpsc::Sender;

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use vista_nvim::{HlSpec, Key, Mods, Nvim, NvimError, SpawnOptions, notation};
use vista_theme::{Rgb, Theme};

use crate::event::AppEvent;

fn hex(rgb: Rgb) -> u32 {
    (u32::from(rgb.0) << 16) | (u32::from(rgb.1) << 8) | u32::from(rgb.2)
}

/// Neovim highlight groups expressed in vista theme tokens, so the editor
/// matches the active look.
pub fn highlight_groups(t: &Theme) -> Vec<(&'static str, HlSpec)> {
    let c = |tok: &str| Some(hex(t.color(tok)));
    let fg = |tok: &str| HlSpec {
        fg: c(tok),
        ..HlSpec::default()
    };
    let fg_bg = |f: &str, b: &str| HlSpec {
        fg: c(f),
        bg: c(b),
        ..HlSpec::default()
    };
    let bg = |b: &str| HlSpec {
        bg: c(b),
        ..HlSpec::default()
    };
    vec![
        ("Normal", fg_bg("syntax.text", "ui.bg")),
        ("NormalNC", fg_bg("syntax.text", "ui.bg")),
        ("NormalFloat", fg_bg("ui.text", "ui.popup")),
        ("FloatBorder", fg_bg("ui.popup.border", "ui.popup")),
        ("LineNr", fg("ui.linenr")),
        (
            "CursorLineNr",
            HlSpec {
                bold: true,
                ..fg("ui.text.strong")
            },
        ),
        ("CursorLine", bg("ui.cursorline")),
        ("SignColumn", bg("ui.bg")),
        ("Visual", bg("ui.selection")),
        ("Search", fg_bg("ui.match.text", "ui.match")),
        ("IncSearch", fg_bg("ui.match.text", "ui.accent")),
        ("CurSearch", fg_bg("ui.match.text", "ui.accent")),
        ("StatusLine", fg_bg("ui.text", "ui.panel")),
        ("StatusLineNC", fg_bg("ui.dim", "ui.panel")),
        ("WinSeparator", fg("ui.border")),
        ("VertSplit", fg("ui.border")),
        ("Pmenu", fg_bg("ui.text", "ui.popup")),
        ("PmenuSel", fg_bg("ui.text.strong", "ui.selection")),
        ("NonText", fg("ui.dim")),
        ("EndOfBuffer", fg("ui.dim")),
        (
            "Comment",
            HlSpec {
                italic: true,
                ..fg("syntax.comment")
            },
        ),
        ("String", fg("syntax.string")),
        ("Character", fg("syntax.string")),
        ("Number", fg("syntax.number")),
        ("Boolean", fg("syntax.constant")),
        ("Constant", fg("syntax.constant")),
        ("Identifier", fg("syntax.text")),
        ("Function", fg("syntax.function")),
        ("Statement", fg("syntax.keyword")),
        ("Keyword", fg("syntax.keyword")),
        ("Conditional", fg("syntax.keyword")),
        ("Repeat", fg("syntax.keyword")),
        ("Operator", fg("syntax.operator")),
        ("Type", fg("syntax.type")),
        ("StorageClass", fg("syntax.keyword")),
        ("PreProc", fg("syntax.attribute")),
        ("Special", fg("syntax.constant")),
        ("Delimiter", fg("syntax.punctuation")),
        ("Tag", fg("syntax.tag")),
        (
            "Title",
            HlSpec {
                bold: true,
                ..fg("syntax.heading")
            },
        ),
        ("Underlined", fg("syntax.link")),
        ("ErrorMsg", fg("ui.error")),
        ("WarningMsg", fg("ui.warning")),
        (
            "ModeMsg",
            HlSpec {
                bold: true,
                ..fg("ui.text.strong")
            },
        ),
        (
            "MatchParen",
            HlSpec {
                bold: true,
                ..bg("ui.selection.inactive")
            },
        ),
        ("DiffAdd", bg("diff.inserted")),
        ("DiffChange", bg("diff.modified")),
        ("DiffDelete", bg("diff.deleted")),
        ("DiffText", bg("diff.word.modified")),
    ]
}

/// Starts Neovim for the project; events go to the app's channel.
pub fn spawn(bin: &str, minimal: bool, cwd: &Path, size: (u16, u16), tx: Sender<AppEvent>) -> Result<Nvim, NvimError> {
    Nvim::spawn(
        &SpawnOptions {
            bin: bin.to_string(),
            minimal,
            cwd: cwd.to_path_buf(),
            width: size.0.max(10),
            height: size.1.max(3),
        },
        move |e| {
            let _ = tx.send(AppEvent::Nvim(e));
        },
    )
}

/// Neovim notation for a key event (`None` for keys Neovim can't take).
pub fn key_notation(key: &KeyEvent) -> Option<String> {
    let k = match key.code {
        KeyCode::Char(c) => Key::Char(c),
        KeyCode::Enter => Key::Enter,
        KeyCode::Esc => Key::Esc,
        KeyCode::Tab => Key::Tab,
        KeyCode::BackTab => Key::BackTab,
        KeyCode::Backspace => Key::Backspace,
        KeyCode::Delete => Key::Delete,
        KeyCode::Insert => Key::Insert,
        KeyCode::Up => Key::Up,
        KeyCode::Down => Key::Down,
        KeyCode::Left => Key::Left,
        KeyCode::Right => Key::Right,
        KeyCode::Home => Key::Home,
        KeyCode::End => Key::End,
        KeyCode::PageUp => Key::PageUp,
        KeyCode::PageDown => Key::PageDown,
        KeyCode::F(n) => Key::F(n),
        _ => return None,
    };
    let mods = Mods {
        ctrl: key.modifiers.contains(KeyModifiers::CONTROL),
        alt: key.modifiers.contains(KeyModifiers::ALT),
        shift: key.modifiers.contains(KeyModifiers::SHIFT),
    };
    Some(notation(k, mods))
}

/// Forwards a mouse event inside the editor area (cell coordinates relative to it).
pub fn send_mouse(nvim: &Nvim, m: &MouseEvent, row: u16, col: u16) {
    let mods = {
        let mut s = String::new();
        if m.modifiers.contains(KeyModifiers::SHIFT) {
            s.push('S');
        }
        if m.modifiers.contains(KeyModifiers::CONTROL) {
            s.push('C');
        }
        if m.modifiers.contains(KeyModifiers::ALT) {
            s.push('A');
        }
        s
    };
    let (button, action) = match m.kind {
        MouseEventKind::Down(b) => (button_name(b), "press"),
        MouseEventKind::Drag(b) => (button_name(b), "drag"),
        MouseEventKind::Up(b) => (button_name(b), "release"),
        MouseEventKind::ScrollUp => ("wheel", "up"),
        MouseEventKind::ScrollDown => ("wheel", "down"),
        _ => return,
    };
    let _ = nvim.mouse(button, action, &mods, row, col);
}

fn button_name(b: MouseButton) -> &'static str {
    match b {
        MouseButton::Left => "left",
        MouseButton::Right => "right",
        MouseButton::Middle => "middle",
    }
}

/// The external editor command for the fallback (`$VISUAL`, `$EDITOR`, `vi`).
pub fn external_editor() -> String {
    std::env::var("VISUAL")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .or_else(|| std::env::var("EDITOR").ok().filter(|s| !s.trim().is_empty()))
        .unwrap_or_else(|| "vi".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn translates_keys() {
        assert_eq!(key_notation(&KeyEvent::from(KeyCode::Char('x'))).as_deref(), Some("x"));
        assert_eq!(
            key_notation(&KeyEvent::new(KeyCode::Char('w'), KeyModifiers::CONTROL)).as_deref(),
            Some("<C-w>")
        );
        assert_eq!(key_notation(&KeyEvent::from(KeyCode::Esc)).as_deref(), Some("<Esc>"));
        assert_eq!(
            key_notation(&KeyEvent::from(KeyCode::Char('<'))).as_deref(),
            Some("<lt>")
        );
    }

    #[test]
    fn every_group_uses_existing_tokens() {
        let t = Theme::default_theme();
        let groups = highlight_groups(&t);
        assert!(groups.iter().any(|(n, _)| *n == "Normal"));
        let normal = &groups[0].1;
        assert_eq!(normal.bg, Some(0x2B2B2B));
    }
}
