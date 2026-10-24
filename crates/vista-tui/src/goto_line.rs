//! Go to Line (`Ctrl-G`): IntelliJ's Line:Column popup. The input starts
//! as the current position, selected, so typing replaces it.

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

/// Where to go: a 1-based line (`None`: stay on the current one) and an
/// optional 1-based column.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Target {
    pub line: Option<usize>,
    pub column: Option<usize>,
}

/// Reads `42`, `42:7` and `:7` (a column on the current line). Line and
/// column 0 count as 1, as IntelliJ takes them.
pub fn parse(input: &str) -> Option<Target> {
    let input = input.trim();
    let (line, column) = match input.split_once(':') {
        Some((l, c)) => (l.trim(), Some(c.trim())),
        None => (input, None),
    };
    let number = |s: &str| s.parse::<usize>().ok().map(|n| n.max(1));
    let line = if line.is_empty() { None } else { Some(number(line)?) };
    let column = match column {
        Some("") | None => None,
        Some(c) => Some(number(c)?),
    };
    if line.is_none() && column.is_none() {
        return None;
    }
    Some(Target { line, column })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GotoLine {
    pub input: String,
    /// The input is still the prefilled position: typing replaces it.
    pub fresh: bool,
    /// Lines in the file, for the hint.
    pub lines: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GotoLineOutcome {
    Handled,
    Close,
    Go(Target),
}

impl GotoLine {
    /// Starts at the current position (1-based).
    pub fn new(line: usize, column: usize, lines: usize) -> GotoLine {
        GotoLine {
            input: format!("{line}:{column}"),
            fresh: true,
            lines,
        }
    }

    pub fn handle_key(&mut self, key: &KeyEvent) -> GotoLineOutcome {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        match key.code {
            KeyCode::Esc => return GotoLineOutcome::Close,
            KeyCode::Enter => {
                return match parse(&self.input) {
                    Some(t) => GotoLineOutcome::Go(t),
                    None => GotoLineOutcome::Close,
                };
            }
            KeyCode::Char('u') if ctrl => self.input.clear(),
            KeyCode::Backspace if self.fresh => self.input.clear(),
            KeyCode::Backspace => {
                self.input.pop();
            }
            // ←/→ keep the prefilled text to edit instead of replacing it.
            KeyCode::Left | KeyCode::Right | KeyCode::End => {}
            KeyCode::Char(c) if !ctrl && (c.is_ascii_digit() || c == ':') => {
                if self.fresh {
                    self.input.clear();
                }
                if c != ':' || !self.input.contains(':') {
                    self.input.push(c);
                }
            }
            _ => return GotoLineOutcome::Handled,
        }
        self.fresh = false;
        GotoLineOutcome::Handled
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn target(line: Option<usize>, column: Option<usize>) -> Option<Target> {
        Some(Target { line, column })
    }

    #[test]
    fn parses_line_and_column() {
        assert_eq!(parse("42"), target(Some(42), None));
        assert_eq!(parse(" 42:7 "), target(Some(42), Some(7)));
        assert_eq!(parse("42:"), target(Some(42), None));
        assert_eq!(parse(":7"), target(None, Some(7)));
        assert_eq!(parse("0:0"), target(Some(1), Some(1)));
        assert_eq!(parse(""), None);
        assert_eq!(parse(":"), None);
        assert_eq!(parse("4x"), None);
        assert_eq!(parse("1:2:3"), None);
    }

    #[test]
    fn typing_replaces_the_prefilled_position() {
        let key = |c| KeyEvent::from(KeyCode::Char(c));
        let mut g = GotoLine::new(12, 1, 200);
        assert_eq!(g.input, "12:1");
        g.handle_key(&key('1'));
        g.handle_key(&key('5'));
        assert_eq!(g.input, "15");
        g.handle_key(&key(':'));
        g.handle_key(&key(':'));
        g.handle_key(&key('3'));
        g.handle_key(&key('x'));
        assert_eq!(g.input, "15:3", "one colon, digits only");
        g.handle_key(&KeyEvent::from(KeyCode::Backspace));
        assert_eq!(g.input, "15:");
        assert_eq!(
            g.handle_key(&KeyEvent::from(KeyCode::Enter)),
            GotoLineOutcome::Go(Target {
                line: Some(15),
                column: None
            })
        );
        assert_eq!(g.handle_key(&KeyEvent::from(KeyCode::Esc)), GotoLineOutcome::Close);
    }

    #[test]
    fn editing_keeps_the_prefilled_position() {
        let mut g = GotoLine::new(12, 1, 200);
        g.handle_key(&KeyEvent::from(KeyCode::End));
        g.handle_key(&KeyEvent::from(KeyCode::Char('0')));
        assert_eq!(g.input, "12:10");
        let mut g = GotoLine::new(12, 1, 200);
        g.handle_key(&KeyEvent::from(KeyCode::Backspace));
        assert_eq!(g.input, "", "Backspace clears the selected text");
        assert_eq!(g.handle_key(&KeyEvent::from(KeyCode::Enter)), GotoLineOutcome::Close);
    }
}
