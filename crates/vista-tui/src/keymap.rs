//! Actions, key chords and the configurable keymap.

use std::collections::{BTreeMap, HashMap};
use std::fmt;

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

/// Everything a key can be bound to globally.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Action {
    Quit,
    Help,
    FocusNext,
    FocusPrev,
    ToggleProject,
    ToggleCommit,
    ToggleFind,
    GotoFile,
    FindInFiles,
    RecentFiles,
    Edit,
    LeaveEditor,
    CloseTab,
    NextTab,
    PrevTab,
    NextChange,
    PrevChange,
    Commit,
    Refresh,
    ToggleIgnored,
    ToggleLineNumbers,
    Projects,
    NextProject,
    PrevProject,
    RevealFile,
    CollapseAll,
    CycleTheme,
    NextOccurrence,
    PrevOccurrence,
}

impl Action {
    pub const ALL: &'static [Action] = &[
        Action::Quit,
        Action::Help,
        Action::FocusNext,
        Action::FocusPrev,
        Action::ToggleProject,
        Action::ToggleCommit,
        Action::ToggleFind,
        Action::GotoFile,
        Action::FindInFiles,
        Action::RecentFiles,
        Action::Edit,
        Action::LeaveEditor,
        Action::CloseTab,
        Action::NextTab,
        Action::PrevTab,
        Action::NextChange,
        Action::PrevChange,
        Action::Commit,
        Action::Refresh,
        Action::ToggleIgnored,
        Action::ToggleLineNumbers,
        Action::Projects,
        Action::NextProject,
        Action::PrevProject,
        Action::RevealFile,
        Action::CollapseAll,
        Action::CycleTheme,
        Action::NextOccurrence,
        Action::PrevOccurrence,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Action::Quit => "quit",
            Action::Help => "help",
            Action::FocusNext => "focus-next",
            Action::FocusPrev => "focus-prev",
            Action::ToggleProject => "project",
            Action::ToggleCommit => "commit-window",
            Action::ToggleFind => "find-window",
            Action::GotoFile => "goto-file",
            Action::FindInFiles => "find-in-files",
            Action::RecentFiles => "recent-files",
            Action::Edit => "edit",
            Action::LeaveEditor => "leave-editor",
            Action::CloseTab => "close-tab",
            Action::NextTab => "next-tab",
            Action::PrevTab => "prev-tab",
            Action::NextChange => "next-change",
            Action::PrevChange => "prev-change",
            Action::Commit => "commit",
            Action::Refresh => "refresh",
            Action::ToggleIgnored => "toggle-ignored",
            Action::ToggleLineNumbers => "line-numbers",
            Action::Projects => "projects",
            Action::NextProject => "next-project",
            Action::PrevProject => "prev-project",
            Action::RevealFile => "reveal-file",
            Action::CollapseAll => "collapse-all",
            Action::CycleTheme => "cycle-theme",
            Action::NextOccurrence => "next-occurrence",
            Action::PrevOccurrence => "prev-occurrence",
        }
    }

    pub fn from_name(name: &str) -> Option<Action> {
        Action::ALL.iter().copied().find(|a| a.name() == name)
    }

    pub fn description(self) -> &'static str {
        match self {
            Action::Quit => "Quit",
            Action::Help => "Show this help",
            Action::FocusNext => "Focus next panel",
            Action::FocusPrev => "Focus previous panel",
            Action::ToggleProject => "Project tool window",
            Action::ToggleCommit => "Commit tool window",
            Action::ToggleFind => "Find tool window",
            Action::GotoFile => "Go to File",
            Action::FindInFiles => "Find in Files",
            Action::RecentFiles => "Recent files",
            Action::Edit => "Edit file in Neovim",
            Action::LeaveEditor => "Leave the editor",
            Action::CloseTab => "Close tab",
            Action::NextTab => "Next tab",
            Action::PrevTab => "Previous tab",
            Action::NextChange => "Next change",
            Action::PrevChange => "Previous change",
            Action::Commit => "Commit",
            Action::Refresh => "Refresh",
            Action::ToggleIgnored => "Show/hide ignored files",
            Action::ToggleLineNumbers => "Show/hide line numbers",
            Action::Projects => "Projects: switch, open, add",
            Action::NextProject => "Next project",
            Action::PrevProject => "Previous project",
            Action::RevealFile => "Reveal current file in tree",
            Action::CollapseAll => "Collapse all folders",
            Action::CycleTheme => "Switch to next look",
            Action::NextOccurrence => "Next Find result",
            Action::PrevOccurrence => "Previous Find result",
        }
    }
}

/// A key plus modifiers, normalised so the same physical chord compares equal
/// whether it arrived via legacy encoding or the kitty keyboard protocol.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct KeyChord {
    pub code: KeyCode,
    pub mods: KeyModifiers,
}

impl KeyChord {
    pub fn new(code: KeyCode, mods: KeyModifiers) -> KeyChord {
        let relevant = KeyModifiers::CONTROL | KeyModifiers::ALT | KeyModifiers::SHIFT;
        let mut mods = mods & relevant;
        let mut code = code;
        match code {
            // Legacy terminals send Ctrl-\ as 0x1C, which crossterm reports as Ctrl-4.
            KeyCode::Char('4') if mods.contains(KeyModifiers::CONTROL) => code = KeyCode::Char('\\'),
            KeyCode::Char(c) if c.is_ascii_alphabetic() => {
                if c.is_ascii_uppercase() || mods.contains(KeyModifiers::SHIFT) {
                    code = KeyCode::Char(c.to_ascii_uppercase());
                    mods |= KeyModifiers::SHIFT;
                }
            }
            // Shift is already folded into symbols like `?`.
            KeyCode::Char(_) => mods.remove(KeyModifiers::SHIFT),
            KeyCode::BackTab => mods.remove(KeyModifiers::SHIFT),
            _ => {}
        }
        KeyChord { code, mods }
    }

    pub fn from_event(ev: &KeyEvent) -> KeyChord {
        KeyChord::new(ev.code, ev.modifiers)
    }

    /// Parses `ctrl+shift+f`, `alt+1`, `f7`, `shift+f7`, `?`, `tab`, `space`, …
    pub fn parse(s: &str) -> Option<KeyChord> {
        // A capital letter key (`L`, `ctrl+K`) means Shift with that letter.
        let capital = s
            .trim()
            .rsplit('+')
            .next()
            .is_some_and(|k| k.len() == 1 && k.chars().all(|c| c.is_ascii_uppercase()));
        let s = s.trim().to_ascii_lowercase();
        if s.is_empty() {
            return None;
        }
        let mut mods = if capital {
            KeyModifiers::SHIFT
        } else {
            KeyModifiers::NONE
        };
        let mut parts: Vec<&str> = s.split('+').collect();
        // `ctrl++` style: a trailing empty part means the key is '+'.
        let key = if s.ends_with("++") {
            parts.truncate(parts.len().saturating_sub(2));
            "+"
        } else {
            parts.pop()?
        };
        for m in parts {
            match m {
                "ctrl" | "control" | "c" => mods |= KeyModifiers::CONTROL,
                "alt" | "option" | "meta" | "a" => mods |= KeyModifiers::ALT,
                "shift" | "s" => mods |= KeyModifiers::SHIFT,
                _ => return None,
            }
        }
        let code = match key {
            "enter" | "return" => KeyCode::Enter,
            "esc" | "escape" => KeyCode::Esc,
            "tab" => KeyCode::Tab,
            "backtab" => KeyCode::BackTab,
            "backspace" => KeyCode::Backspace,
            "delete" | "del" => KeyCode::Delete,
            "space" => KeyCode::Char(' '),
            "up" => KeyCode::Up,
            "down" => KeyCode::Down,
            "left" => KeyCode::Left,
            "right" => KeyCode::Right,
            "home" => KeyCode::Home,
            "end" => KeyCode::End,
            "pageup" => KeyCode::PageUp,
            "pagedown" => KeyCode::PageDown,
            k if k.len() > 1 && k.starts_with('f') && k[1..].parse::<u8>().is_ok() => KeyCode::F(k[1..].parse().ok()?),
            k if k.chars().count() == 1 => KeyCode::Char(k.chars().next()?),
            _ => return None,
        };
        if code == KeyCode::Tab && mods.contains(KeyModifiers::SHIFT) {
            return Some(KeyChord::new(KeyCode::BackTab, KeyModifiers::NONE));
        }
        Some(KeyChord::new(code, mods))
    }
}

impl fmt::Display for KeyChord {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.mods.contains(KeyModifiers::CONTROL) {
            f.write_str("Ctrl-")?;
        }
        if self.mods.contains(KeyModifiers::ALT) {
            f.write_str("Alt-")?;
        }
        if self.mods.contains(KeyModifiers::SHIFT) {
            f.write_str("Shift-")?;
        }
        match self.code {
            KeyCode::Char(' ') => f.write_str("Space"),
            KeyCode::Char(c) if self.mods.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) => {
                write!(f, "{}", c.to_ascii_uppercase())
            }
            KeyCode::Char(c) => write!(f, "{c}"),
            KeyCode::F(n) => write!(f, "F{n}"),
            KeyCode::Enter => f.write_str("Enter"),
            KeyCode::Esc => f.write_str("Esc"),
            KeyCode::Tab => f.write_str("Tab"),
            KeyCode::BackTab => f.write_str("Shift-Tab"),
            KeyCode::Left => f.write_str("←"),
            KeyCode::Right => f.write_str("→"),
            KeyCode::Up => f.write_str("↑"),
            KeyCode::Down => f.write_str("↓"),
            other => write!(f, "{other:?}"),
        }
    }
}

/// Default bindings. Several chords may map to one action; legacy terminals
/// can't tell Ctrl-Shift-F from Ctrl-F, so both are bound.
pub const DEFAULT_BINDINGS: &[(&str, Action)] = &[
    ("q", Action::Quit),
    ("ctrl+q", Action::Quit),
    ("?", Action::Help),
    ("f1", Action::Help),
    ("tab", Action::FocusNext),
    ("shift+tab", Action::FocusPrev),
    ("alt+1", Action::ToggleProject),
    ("alt+0", Action::ToggleCommit),
    ("alt+3", Action::ToggleFind),
    ("ctrl+p", Action::GotoFile),
    ("ctrl+shift+n", Action::GotoFile),
    ("ctrl+f", Action::FindInFiles),
    ("ctrl+shift+f", Action::FindInFiles),
    ("ctrl+e", Action::RecentFiles),
    ("e", Action::Edit),
    ("ctrl+\\", Action::LeaveEditor),
    ("ctrl+w", Action::CloseTab),
    ("alt+right", Action::NextTab),
    ("alt+left", Action::PrevTab),
    // Mac keyboards make F-keys awkward: every F-key action has another key.
    ("]", Action::NextChange),
    ("[", Action::PrevChange),
    ("f7", Action::NextChange),
    ("shift+f7", Action::PrevChange),
    ("ctrl+k", Action::Commit),
    ("ctrl+r", Action::Refresh),
    ("f5", Action::Refresh),
    ("i", Action::ToggleIgnored),
    ("L", Action::ToggleLineNumbers),
    ("ctrl+o", Action::Projects),
    // Not Alt-[ / Alt-]: as ESC [ / ESC ] they start terminal escape sequences.
    ("alt+.", Action::NextProject),
    ("alt+,", Action::PrevProject),
    ("ctrl+l", Action::RevealFile),
    ("alt+f1", Action::RevealFile),
    ("-", Action::CollapseAll),
    ("ctrl+t", Action::CycleTheme),
    ("ctrl+alt+down", Action::NextOccurrence),
    ("ctrl+alt+up", Action::PrevOccurrence),
];

#[derive(Debug, Clone)]
pub struct Keymap {
    map: HashMap<KeyChord, Action>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeymapWarning(pub String);

impl Keymap {
    pub fn defaults() -> Keymap {
        let map = DEFAULT_BINDINGS
            .iter()
            .map(|(k, a)| (KeyChord::parse(k).expect("valid default chord"), *a))
            .collect();
        Keymap { map }
    }

    /// Defaults plus user overrides (`"ctrl+o" = "goto-file"`, `"q" = "none"`).
    /// Invalid entries are skipped and reported.
    pub fn with_overrides(overrides: &BTreeMap<String, String>) -> (Keymap, Vec<KeymapWarning>) {
        let mut km = Keymap::defaults();
        let mut warnings = Vec::new();
        for (chord, action) in overrides {
            let Some(c) = KeyChord::parse(chord) else {
                warnings.push(KeymapWarning(format!("unknown key `{chord}`")));
                continue;
            };
            if action == "none" {
                km.map.remove(&c);
                continue;
            }
            match Action::from_name(action) {
                Some(a) => {
                    km.map.insert(c, a);
                }
                None => warnings.push(KeymapWarning(format!("unknown action `{action}`"))),
            }
        }
        (km, warnings)
    }

    pub fn action_for(&self, ev: &KeyEvent) -> Option<Action> {
        self.map.get(&KeyChord::from_event(ev)).copied()
    }

    /// All chords bound to an action, sorted for display.
    pub fn chords_for(&self, action: Action) -> Vec<KeyChord> {
        let mut v: Vec<KeyChord> = self
            .map
            .iter()
            .filter(|(_, a)| **a == action)
            .map(|(c, _)| *c)
            .collect();
        v.sort_by_key(|c| (c.mods.bits(), c.to_string()));
        v
    }

    /// The shortest chord for an action, for hints in the UI.
    pub fn hint(&self, action: Action) -> String {
        self.chords_for(action)
            .into_iter()
            .min_by_key(|c| c.to_string().len())
            .map(|c| c.to_string())
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::crossterm::event::KeyEventKind;

    fn ev(code: KeyCode, mods: KeyModifiers) -> KeyEvent {
        KeyEvent::new_with_kind(code, mods, KeyEventKind::Press)
    }

    #[test]
    fn parses_chords() {
        assert_eq!(
            KeyChord::parse("ctrl+shift+f"),
            Some(KeyChord::new(
                KeyCode::Char('F'),
                KeyModifiers::CONTROL | KeyModifiers::SHIFT
            ))
        );
        assert_eq!(KeyChord::parse("alt+1").unwrap().mods, KeyModifiers::ALT);
        assert_eq!(KeyChord::parse("shift+f7").unwrap().code, KeyCode::F(7));
        assert_eq!(KeyChord::parse("shift+tab").unwrap().code, KeyCode::BackTab);
        assert_eq!(KeyChord::parse("space").unwrap().code, KeyCode::Char(' '));
        assert_eq!(KeyChord::parse("hyper+x"), None);
        assert_eq!(KeyChord::parse(""), None);
    }

    #[test]
    fn normalises_terminal_variants() {
        // Legacy Ctrl-\ arrives as Ctrl-4.
        assert_eq!(
            KeyChord::from_event(&ev(KeyCode::Char('4'), KeyModifiers::CONTROL)),
            KeyChord::parse("ctrl+\\").unwrap()
        );
        // kitty protocol may send lowercase + SHIFT, legacy sends uppercase.
        assert_eq!(
            KeyChord::from_event(&ev(KeyCode::Char('f'), KeyModifiers::CONTROL | KeyModifiers::SHIFT)),
            KeyChord::from_event(&ev(KeyCode::Char('F'), KeyModifiers::CONTROL))
        );
        // `?` with or without SHIFT is the same key.
        assert_eq!(
            KeyChord::from_event(&ev(KeyCode::Char('?'), KeyModifiers::SHIFT)),
            KeyChord::parse("?").unwrap()
        );
    }

    #[test]
    fn default_keymap_resolves_actions() {
        let km = Keymap::defaults();
        assert_eq!(
            km.action_for(&ev(KeyCode::Char('q'), KeyModifiers::NONE)),
            Some(Action::Quit)
        );
        assert_eq!(
            km.action_for(&ev(KeyCode::Char('p'), KeyModifiers::CONTROL)),
            Some(Action::GotoFile)
        );
        assert_eq!(
            km.action_for(&ev(KeyCode::F(7), KeyModifiers::SHIFT)),
            Some(Action::PrevChange)
        );
        assert_eq!(
            km.action_for(&ev(KeyCode::Char('b'), KeyModifiers::CONTROL)),
            None,
            "herdr prefix stays free"
        );
    }

    #[test]
    fn overrides_rebind_unbind_and_warn() {
        let mut o = BTreeMap::new();
        o.insert("ctrl+o".to_string(), "goto-file".to_string());
        o.insert("q".to_string(), "none".to_string());
        o.insert("ctrl+y".to_string(), "fly".to_string());
        o.insert("hyper+z".to_string(), "quit".to_string());
        let (km, warnings) = Keymap::with_overrides(&o);
        assert_eq!(
            km.action_for(&ev(KeyCode::Char('o'), KeyModifiers::CONTROL)),
            Some(Action::GotoFile)
        );
        assert_eq!(km.action_for(&ev(KeyCode::Char('q'), KeyModifiers::NONE)), None);
        assert_eq!(warnings.len(), 2);
    }

    #[test]
    fn every_action_has_a_unique_name() {
        let mut names: Vec<_> = Action::ALL.iter().map(|a| a.name()).collect();
        names.sort();
        names.dedup();
        assert_eq!(names.len(), Action::ALL.len());
        for a in Action::ALL {
            assert_eq!(Action::from_name(a.name()), Some(*a));
        }
    }

    #[test]
    fn displays_chords_for_hints() {
        assert_eq!(KeyChord::parse("ctrl+shift+f").unwrap().to_string(), "Ctrl-Shift-F");
        assert_eq!(Keymap::defaults().hint(Action::GotoFile), "Ctrl-P");
        assert_eq!(Keymap::defaults().hint(Action::Edit), "e");
    }

    #[test]
    fn capital_letters_mean_shift() {
        let shift_l = KeyChord::from_event(&ev(KeyCode::Char('L'), KeyModifiers::SHIFT));
        assert_eq!(KeyChord::parse("L"), Some(shift_l));
        assert_eq!(KeyChord::parse("shift+l"), Some(shift_l));
        assert_ne!(KeyChord::parse("l"), Some(shift_l));
        assert_eq!(
            Keymap::defaults().action_for(&ev(KeyCode::Char('L'), KeyModifiers::SHIFT)),
            Some(Action::ToggleLineNumbers)
        );
        assert_eq!(
            Keymap::defaults().action_for(&ev(KeyCode::Char('l'), KeyModifiers::NONE)),
            None
        );
    }
}
