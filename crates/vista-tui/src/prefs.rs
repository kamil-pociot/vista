//! The Preferences page (`Ctrl-,`, `⚙`): vista's settings, changed in
//! place. A change applies at once and is remembered in `state.toml`, like
//! the toggles elsewhere; `config.toml` keeps the defaults, which the last
//! row opens. `Tab` switches to the Keymap page (`keymap_page`).

use ratatui::crossterm::event::{KeyCode, KeyEvent};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pref {
    Look,
    FillBackground,
    LineNumbers,
    WrapMarkdown,
    WrapCode,
    DiffUnified,
    DiffCollapse,
    DiffWhitespace,
    ShowIgnored,
    GroupByFolder,
    FindCase,
    FindWords,
    FindRegex,
    CopyOnSelect,
    Keymap,
    EditConfig,
}

/// The page, in order: a heading, then its settings.
pub const SECTIONS: &[(&str, &[Pref])] = &[
    ("Appearance", &[Pref::Look, Pref::FillBackground]),
    ("Viewer", &[Pref::LineNumbers, Pref::WrapMarkdown, Pref::WrapCode]),
    ("Diff", &[Pref::DiffUnified, Pref::DiffCollapse, Pref::DiffWhitespace]),
    ("Project tree", &[Pref::ShowIgnored]),
    ("Commit window", &[Pref::GroupByFolder]),
    ("Find in Files", &[Pref::FindCase, Pref::FindWords, Pref::FindRegex]),
    ("Mouse", &[Pref::CopyOnSelect]),
    ("More", &[Pref::Keymap, Pref::EditConfig]),
];

impl Pref {
    pub fn label(self) -> &'static str {
        match self {
            Pref::Look => "Look",
            Pref::FillBackground => "Fill background",
            Pref::LineNumbers => "Line numbers",
            Pref::WrapMarkdown => "Wrap Markdown",
            Pref::WrapCode => "Wrap code",
            Pref::DiffUnified => "Unified diffs",
            Pref::DiffCollapse => "Collapse unchanged lines",
            Pref::DiffWhitespace => "Ignore whitespace",
            Pref::ShowIgnored => "Show ignored files",
            Pref::GroupByFolder => "Group changes by folder",
            Pref::FindCase => "Match case",
            Pref::FindWords => "Whole words",
            Pref::FindRegex => "Regular expressions",
            Pref::CopyOnSelect => "Copy selection on release",
            Pref::Keymap => "Keymap…",
            Pref::EditConfig => "Edit config.toml…",
        }
    }

    /// What it does, shown under the list for the selected row.
    pub fn about(self) -> &'static str {
        match self {
            Pref::Look => "The colours and layout. Follow herdr uses herdr's theme and switches with it.",
            Pref::FillBackground => {
                "Paint the look's background where it would show the terminal's (herdr's themes leave it to the terminal)."
            }
            Pref::LineNumbers => "Line numbers in code, and source lines in rendered Markdown (Shift-L).",
            Pref::WrapMarkdown => "Long lines and tables wrap in Markdown (w in the viewer).",
            Pref::WrapCode => "Long lines wrap in code; off, they scroll sideways (w in the viewer).",
            Pref::DiffUnified => "One column instead of side by side, for diffs opened from now on (u in a diff).",
            Pref::DiffCollapse => "Fold runs of unchanged lines, for diffs opened from now on (z in a diff).",
            Pref::DiffWhitespace => "Changes in whitespace alone don't count (i in a diff).",
            Pref::ShowIgnored => "Git-ignored files in the tree, dimmed (Alt-I).",
            Pref::GroupByFolder => "The Commit window lists changes under their folders (g there).",
            Pref::FindCase => "Find in Files matches case (Alt-C there).",
            Pref::FindWords => "Find in Files matches whole words (Alt-W there).",
            Pref::FindRegex => "Find in Files takes a regular expression (Alt-X there).",
            Pref::CopyOnSelect => "Releasing the mouse after selecting copies; off, Ctrl-C / Cmd-C / y do.",
            Pref::Keymap => "Every action and its keys: add, remove or reset them (Tab).",
            Pref::EditConfig => {
                "Defaults for these settings and the rest, in the settings file. They apply when vista starts again."
            }
        }
    }
}

/// Every setting, in page order.
pub fn all() -> Vec<Pref> {
    SECTIONS.iter().flat_map(|(_, prefs)| prefs.iter().copied()).collect()
}

/// What a key on the page asks for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrefsOutcome {
    Handled,
    Close,
    /// Change the selected setting: to the next value (`true`) or back.
    Change(Pref, bool),
    /// Over to the Keymap page.
    SwitchPage,
}

/// The two pages of Preferences.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Page {
    #[default]
    Settings,
    Keymap,
}

#[derive(Debug, Default)]
pub struct PrefsPage {
    /// Index into [`all`].
    pub selected: usize,
    pub page: Page,
    pub keymap: crate::keymap_page::KeymapPage,
}

impl PrefsPage {
    pub fn selected_pref(&self) -> Pref {
        let all = all();
        all[self.selected.min(all.len() - 1)]
    }

    pub fn handle_key(&mut self, key: &KeyEvent) -> PrefsOutcome {
        let n = all().len();
        let pref = self.selected_pref();
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') => return PrefsOutcome::Close,
            KeyCode::Tab | KeyCode::BackTab => return PrefsOutcome::SwitchPage,
            KeyCode::Up | KeyCode::Char('k') => self.selected = (self.selected + n - 1) % n,
            KeyCode::Down | KeyCode::Char('j') => self.selected = (self.selected + 1) % n,
            KeyCode::Home => self.selected = 0,
            KeyCode::End => self.selected = n - 1,
            KeyCode::Right | KeyCode::Char('l' | ' ') | KeyCode::Enter => return PrefsOutcome::Change(pref, true),
            KeyCode::Left | KeyCode::Char('h') => return PrefsOutcome::Change(pref, false),
            _ => {}
        }
        PrefsOutcome::Handled
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_move_round_the_page_and_change_the_selected_setting() {
        let mut p = PrefsPage::default();
        assert_eq!(p.selected_pref(), Pref::Look);
        assert_eq!(p.handle_key(&KeyEvent::from(KeyCode::Up)), PrefsOutcome::Handled);
        assert_eq!(p.selected_pref(), Pref::EditConfig, "wraps round");
        p.handle_key(&KeyEvent::from(KeyCode::Down));
        p.handle_key(&KeyEvent::from(KeyCode::Down));
        assert_eq!(p.selected_pref(), Pref::FillBackground);
        assert_eq!(
            p.handle_key(&KeyEvent::from(KeyCode::Char(' '))),
            PrefsOutcome::Change(Pref::FillBackground, true)
        );
        assert_eq!(
            p.handle_key(&KeyEvent::from(KeyCode::Left)),
            PrefsOutcome::Change(Pref::FillBackground, false)
        );
        assert_eq!(p.handle_key(&KeyEvent::from(KeyCode::Esc)), PrefsOutcome::Close);
        // Every setting is on the page once.
        let all = all();
        for (i, a) in all.iter().enumerate() {
            assert!(!all[i + 1..].contains(a), "{a:?} twice");
        }
    }
}
