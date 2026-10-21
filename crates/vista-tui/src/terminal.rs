//! Terminal setup and guaranteed restore (also on panic).

use std::io::{self, Stdout};
use std::sync::atomic::{AtomicBool, Ordering};

use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use ratatui::crossterm::cursor::Show;
use ratatui::crossterm::event::{
    DisableBracketedPaste, DisableMouseCapture, EnableBracketedPaste, EnableMouseCapture, KeyboardEnhancementFlags,
    PopKeyboardEnhancementFlags, PushKeyboardEnhancementFlags,
};
use ratatui::crossterm::execute;
use ratatui::crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, SetTitle, disable_raw_mode, enable_raw_mode,
    supports_keyboard_enhancement,
};

pub type Term = Terminal<CrosstermBackend<Stdout>>;

static KITTY_KEYBOARD: AtomicBool = AtomicBool::new(false);
static RESTORED: AtomicBool = AtomicBool::new(false);

/// Restores the terminal when dropped.
pub struct TermGuard;

impl Drop for TermGuard {
    fn drop(&mut self) {
        restore();
    }
}

/// `VISTA_KEYBOARD=legacy` skips the kitty keyboard protocol (and its query).
pub fn keyboard_enhancement_wanted() -> bool {
    std::env::var("VISTA_KEYBOARD").map(|v| v != "legacy").unwrap_or(true)
}

pub fn kitty_keyboard_active() -> bool {
    KITTY_KEYBOARD.load(Ordering::SeqCst)
}

pub fn setup(title: &str) -> io::Result<(Term, TermGuard)> {
    enable_raw_mode()?;
    let kitty = keyboard_enhancement_wanted() && supports_keyboard_enhancement().unwrap_or(false);
    let mut out = io::stdout();
    execute!(
        out,
        EnterAlternateScreen,
        EnableMouseCapture,
        EnableBracketedPaste,
        SetTitle(title)
    )?;
    if kitty {
        execute!(
            out,
            PushKeyboardEnhancementFlags(KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES)
        )?;
    }
    KITTY_KEYBOARD.store(kitty, Ordering::SeqCst);
    RESTORED.store(false, Ordering::SeqCst);
    install_panic_hook();
    let term = Terminal::new(CrosstermBackend::new(out))?;
    Ok((term, TermGuard))
}

/// Leaves the alternate screen and raw mode. Safe to call more than once.
pub fn restore() {
    if RESTORED.swap(true, Ordering::SeqCst) {
        return;
    }
    let mut out = io::stdout();
    if KITTY_KEYBOARD.load(Ordering::SeqCst) {
        let _ = execute!(out, PopKeyboardEnhancementFlags);
    }
    let _ = execute!(
        out,
        DisableBracketedPaste,
        DisableMouseCapture,
        LeaveAlternateScreen,
        Show
    );
    let _ = disable_raw_mode();
}

fn install_panic_hook() {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        restore();
        previous(info);
    }));
}
