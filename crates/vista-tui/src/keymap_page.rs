//! The Keymap page of Preferences: every action with its keys, IntelliJ's
//! Settings › Keymap. A key is added by pressing it, removed, or reset to
//! the default; changes apply at once and are remembered in `state.toml`.

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::keymap::{Action, KeyChord, Keymap};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Mode {
    #[default]
    Browse,
    /// The next key becomes a binding of the selected action.
    Capture,
    /// The key pressed runs another action: take it, or keep it there.
    Conflict(KeyChord, Action),
    /// Reset every key (`R`): asks first.
    ConfirmResetAll,
}

/// What a key on the page asks for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeymapOutcome {
    Handled,
    Close,
    /// Over to the Settings page.
    SwitchPage,
    Bind(Action, KeyChord),
    Unbind(KeyChord),
    Reset(Action),
    ResetAll,
}

#[derive(Debug, Clone, Default)]
pub struct KeymapPage {
    /// Index into [`KeymapPage::rows`].
    pub selected: usize,
    /// The selected key of the selected action.
    pub chip: usize,
    pub filter: String,
    /// The filter is being typed (`/`).
    pub filtering: bool,
    pub mode: Mode,
    /// Said under the list until the next key.
    pub note: Option<String>,
}

impl KeymapPage {
    /// The actions shown: all of them, or those whose description, name
    /// or keys contain the filter.
    pub fn rows(&self, km: &Keymap) -> Vec<Action> {
        let f = self.filter.to_lowercase();
        Action::ALL
            .iter()
            .copied()
            .filter(|a| {
                f.is_empty()
                    || a.description().to_lowercase().contains(&f)
                    || a.name().contains(&f)
                    || km
                        .chords_for(*a)
                        .iter()
                        .any(|c| c.to_string().to_lowercase().contains(&f))
            })
            .collect()
    }

    pub fn selected_action(&self, km: &Keymap) -> Option<Action> {
        let rows = self.rows(km);
        rows.get(self.selected.min(rows.len().saturating_sub(1))).copied()
    }

    /// The selected action's selected key.
    pub fn selected_chord(&self, km: &Keymap) -> Option<KeyChord> {
        let chords = km.chords_for(self.selected_action(km)?);
        chords.get(self.chip.min(chords.len().saturating_sub(1))).copied()
    }

    /// Selects `chord` among the action's keys (after binding it).
    pub fn select_chord(&mut self, km: &Keymap, action: Action, chord: KeyChord) {
        if let Some(i) = self.rows(km).iter().position(|a| *a == action) {
            self.selected = i;
        }
        self.chip = km.chords_for(action).iter().position(|c| *c == chord).unwrap_or(0);
    }

    pub fn handle_key(&mut self, key: &KeyEvent, km: &Keymap) -> KeymapOutcome {
        self.note = None;
        match self.mode {
            Mode::Capture => return self.capture(key, km),
            Mode::Conflict(chord, _) => {
                self.mode = Mode::Browse;
                if matches!(key.code, KeyCode::Enter | KeyCode::Char('y')) {
                    return self
                        .selected_action(km)
                        .map_or(KeymapOutcome::Handled, |a| KeymapOutcome::Bind(a, chord));
                }
                return KeymapOutcome::Handled;
            }
            Mode::ConfirmResetAll => {
                self.mode = Mode::Browse;
                if matches!(key.code, KeyCode::Enter | KeyCode::Char('y')) {
                    return KeymapOutcome::ResetAll;
                }
                return KeymapOutcome::Handled;
            }
            Mode::Browse => {}
        }
        let n = self.rows(km).len();
        if self.filtering {
            match key.code {
                KeyCode::Esc => {
                    self.filtering = false;
                    self.filter.clear();
                }
                KeyCode::Enter | KeyCode::Down | KeyCode::Up => self.filtering = false,
                KeyCode::Backspace => {
                    self.filter.pop();
                }
                KeyCode::Char(c) if !key.modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) => {
                    self.filter.push(c);
                    self.selected = 0;
                    self.chip = 0;
                }
                _ => {}
            }
            return KeymapOutcome::Handled;
        }
        let chips = self.selected_action(km).map_or(0, |a| km.chords_for(a).len());
        match key.code {
            KeyCode::Esc if !self.filter.is_empty() => {
                self.filter.clear();
                self.selected = 0;
            }
            KeyCode::Esc | KeyCode::Char('q') => return KeymapOutcome::Close,
            KeyCode::Tab | KeyCode::BackTab => return KeymapOutcome::SwitchPage,
            KeyCode::Up | KeyCode::Char('k') if n > 0 => {
                self.selected = (self.selected.min(n - 1) + n - 1) % n;
                self.chip = 0;
            }
            KeyCode::Down | KeyCode::Char('j') if n > 0 => {
                self.selected = (self.selected.min(n - 1) + 1) % n;
                self.chip = 0;
            }
            KeyCode::PageUp => {
                self.selected = self.selected.saturating_sub(10);
                self.chip = 0;
            }
            KeyCode::PageDown => {
                self.selected = (self.selected + 10).min(n.saturating_sub(1));
                self.chip = 0;
            }
            KeyCode::Home => self.selected = 0,
            KeyCode::End => self.selected = n.saturating_sub(1),
            KeyCode::Left | KeyCode::Char('h') => self.chip = self.chip.min(chips.saturating_sub(1)).saturating_sub(1),
            KeyCode::Right | KeyCode::Char('l') => self.chip = (self.chip + 1).min(chips.saturating_sub(1)),
            KeyCode::Char('/') => self.filtering = true,
            KeyCode::Enter | KeyCode::Char('a') if n > 0 => self.mode = Mode::Capture,
            KeyCode::Delete | KeyCode::Backspace | KeyCode::Char('d') => {
                if let Some(c) = self.selected_chord(km) {
                    self.chip = self.chip.min(chips.saturating_sub(2));
                    return KeymapOutcome::Unbind(c);
                }
            }
            KeyCode::Char('r') => {
                if let Some(a) = self.selected_action(km) {
                    self.chip = 0;
                    return KeymapOutcome::Reset(a);
                }
            }
            KeyCode::Char('R') => self.mode = Mode::ConfirmResetAll,
            _ => {}
        }
        KeymapOutcome::Handled
    }

    /// The key pressed while waiting for one.
    fn capture(&mut self, key: &KeyEvent, km: &Keymap) -> KeymapOutcome {
        self.mode = Mode::Browse;
        // Modifier keys on their own (kitty keyboard protocol) aren't a chord yet.
        if let KeyCode::Modifier(_) = key.code {
            self.mode = Mode::Capture;
            return KeymapOutcome::Handled;
        }
        if key.code == KeyCode::Esc && key.modifiers.is_empty() {
            return KeymapOutcome::Handled;
        }
        let Some(action) = self.selected_action(km) else {
            return KeymapOutcome::Handled;
        };
        let chord = KeyChord::from_event(key);
        if chord.config_name().is_none() {
            self.note = Some(format!("{chord} can't be bound"));
            return KeymapOutcome::Handled;
        }
        match km.action_of(chord) {
            Some(a) if a == action => {
                self.note = Some(format!("{chord} already runs {}", action.description()));
                KeymapOutcome::Handled
            }
            Some(other) => {
                self.mode = Mode::Conflict(chord, other);
                KeymapOutcome::Handled
            }
            None => KeymapOutcome::Bind(action, chord),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(c: KeyCode) -> KeyEvent {
        KeyEvent::from(c)
    }

    fn page_at(km: &Keymap, action: Action) -> KeymapPage {
        let mut p = KeymapPage::default();
        p.selected = p.rows(km).iter().position(|a| *a == action).unwrap();
        p
    }

    #[test]
    fn lists_every_action_and_filters() {
        let km = Keymap::defaults();
        let mut p = KeymapPage::default();
        assert_eq!(p.rows(&km).len(), Action::ALL.len());
        p.handle_key(&key(KeyCode::Char('/')), &km);
        for c in "go to".chars() {
            p.handle_key(&key(KeyCode::Char(c)), &km);
        }
        assert_eq!(p.rows(&km), vec![Action::GotoFile, Action::GotoLine]);
        // Keys match too.
        p.filter = "ctrl-g".into();
        assert_eq!(p.rows(&km), vec![Action::GotoLine]);
        // Esc while typing clears the filter; q then closes the page.
        p.handle_key(&key(KeyCode::Esc), &km);
        assert!(p.filter.is_empty() && !p.filtering);
        assert_eq!(p.handle_key(&key(KeyCode::Char('q')), &km), KeymapOutcome::Close);
    }

    #[test]
    fn a_pressed_key_is_bound_or_asks_about_a_conflict() {
        let km = Keymap::defaults();
        let mut p = page_at(&km, Action::GotoLine);
        let alt_g = KeyEvent::new(KeyCode::Char('g'), KeyModifiers::ALT);
        assert_eq!(p.handle_key(&key(KeyCode::Enter), &km), KeymapOutcome::Handled);
        assert_eq!(p.mode, Mode::Capture);
        assert_eq!(
            p.handle_key(&alt_g, &km),
            KeymapOutcome::Bind(Action::GotoLine, KeyChord::parse("alt+g").unwrap())
        );
        // Ctrl-P is Go to File's: asked, then taken on Enter.
        let ctrl_p = KeyEvent::new(KeyCode::Char('p'), KeyModifiers::CONTROL);
        p.handle_key(&key(KeyCode::Char('a')), &km);
        assert_eq!(p.handle_key(&ctrl_p, &km), KeymapOutcome::Handled);
        let chord = KeyChord::parse("ctrl+p").unwrap();
        assert_eq!(p.mode, Mode::Conflict(chord, Action::GotoFile));
        assert_eq!(
            p.handle_key(&key(KeyCode::Enter), &km),
            KeymapOutcome::Bind(Action::GotoLine, chord)
        );
        // Esc while waiting cancels; its own key again is just a note.
        p.handle_key(&key(KeyCode::Enter), &km);
        assert_eq!(p.handle_key(&key(KeyCode::Esc), &km), KeymapOutcome::Handled);
        assert_eq!(p.mode, Mode::Browse);
        p.handle_key(&key(KeyCode::Enter), &km);
        let ctrl_g = KeyEvent::new(KeyCode::Char('g'), KeyModifiers::CONTROL);
        assert_eq!(p.handle_key(&ctrl_g, &km), KeymapOutcome::Handled);
        assert!(p.note.as_deref().unwrap().contains("already"));
    }

    #[test]
    fn keys_are_picked_with_arrows_removed_and_reset() {
        let km = Keymap::defaults();
        let mut p = page_at(&km, Action::GotoFile);
        // Ctrl-P, then Ctrl-Shift-N.
        assert_eq!(p.selected_chord(&km), KeyChord::parse("ctrl+p"));
        p.handle_key(&key(KeyCode::Right), &km);
        p.handle_key(&key(KeyCode::Right), &km);
        assert_eq!(p.selected_chord(&km), KeyChord::parse("ctrl+shift+n"));
        assert_eq!(
            p.handle_key(&key(KeyCode::Delete), &km),
            KeymapOutcome::Unbind(KeyChord::parse("ctrl+shift+n").unwrap())
        );
        assert_eq!(p.chip, 0, "the key before it is selected");
        assert_eq!(
            p.handle_key(&key(KeyCode::Char('r')), &km),
            KeymapOutcome::Reset(Action::GotoFile)
        );
        // Reset all asks first.
        assert_eq!(p.handle_key(&key(KeyCode::Char('R')), &km), KeymapOutcome::Handled);
        assert_eq!(p.handle_key(&key(KeyCode::Enter), &km), KeymapOutcome::ResetAll);
        p.handle_key(&key(KeyCode::Char('R')), &km);
        assert_eq!(p.handle_key(&key(KeyCode::Esc), &km), KeymapOutcome::Handled);
        assert_eq!(p.handle_key(&key(KeyCode::Tab), &km), KeymapOutcome::SwitchPage);
    }
}
