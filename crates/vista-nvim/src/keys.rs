//! Translating terminal keys into Neovim's key notation (`<C-x>`, `<CR>`, …).

/// A key independent of the UI library, so this crate needs no crossterm.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    Char(char),
    Enter,
    Esc,
    Tab,
    BackTab,
    Backspace,
    Delete,
    Insert,
    Up,
    Down,
    Left,
    Right,
    Home,
    End,
    PageUp,
    PageDown,
    F(u8),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Mods {
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
}

/// Neovim notation for one key press, ready for `nvim_input`.
pub fn notation(key: Key, mods: Mods) -> String {
    let name: String = match key {
        Key::Char(c) => {
            let plain = !mods.ctrl && !mods.alt;
            match c {
                '<' if plain => return "<lt>".into(),
                '<' => "lt".into(),
                '\\' => "Bslash".into(),
                '|' => "Bar".into(),
                ' ' => "Space".into(),
                c if plain => return c.to_string(),
                c => c.to_string(),
            }
        }
        Key::Enter => "CR".into(),
        Key::Esc => "Esc".into(),
        Key::Tab => "Tab".into(),
        Key::BackTab => return "<S-Tab>".into(),
        Key::Backspace => "BS".into(),
        Key::Delete => "Del".into(),
        Key::Insert => "Insert".into(),
        Key::Up => "Up".into(),
        Key::Down => "Down".into(),
        Key::Left => "Left".into(),
        Key::Right => "Right".into(),
        Key::Home => "Home".into(),
        Key::End => "End".into(),
        Key::PageUp => "PageUp".into(),
        Key::PageDown => "PageDown".into(),
        Key::F(n) => format!("F{n}"),
    };
    let mut prefix = String::new();
    if mods.ctrl {
        prefix.push_str("C-");
    }
    if mods.alt {
        prefix.push_str("M-");
    }
    // Shift is already part of characters ('A', '?'); keep it for special keys.
    if mods.shift && !matches!(key, Key::Char(_)) {
        prefix.push_str("S-");
    }
    if prefix.is_empty() && matches!(key, Key::Char(_)) {
        // Space, Bslash, Bar without modifiers.
        return format!("<{name}>");
    }
    format!("<{prefix}{name}>")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn m(ctrl: bool, alt: bool, shift: bool) -> Mods {
        Mods { ctrl, alt, shift }
    }

    #[test]
    fn plain_and_special_keys() {
        assert_eq!(notation(Key::Char('a'), Mods::default()), "a");
        assert_eq!(notation(Key::Char('A'), m(false, false, true)), "A");
        assert_eq!(notation(Key::Char('<'), Mods::default()), "<lt>");
        assert_eq!(notation(Key::Char(' '), Mods::default()), "<Space>");
        assert_eq!(notation(Key::Char('\\'), Mods::default()), "<Bslash>");
        assert_eq!(notation(Key::Enter, Mods::default()), "<CR>");
        assert_eq!(notation(Key::Esc, Mods::default()), "<Esc>");
        assert_eq!(notation(Key::BackTab, Mods::default()), "<S-Tab>");
        assert_eq!(notation(Key::F(5), Mods::default()), "<F5>");
    }

    #[test]
    fn modifiers() {
        assert_eq!(notation(Key::Char('w'), m(true, false, false)), "<C-w>");
        assert_eq!(notation(Key::Char('x'), m(false, true, false)), "<M-x>");
        assert_eq!(notation(Key::Up, m(true, false, true)), "<C-S-Up>");
        assert_eq!(notation(Key::Char('<'), m(true, false, false)), "<C-lt>");
        assert_eq!(notation(Key::Enter, m(false, false, true)), "<S-CR>");
    }
}
