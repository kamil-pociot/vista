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
    Clear, ClearType, EnterAlternateScreen, LeaveAlternateScreen, SetTitle, disable_raw_mode, enable_raw_mode,
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

/// Sets the terminal window's title (the active project).
pub fn set_title(title: &str) {
    let _ = execute!(io::stdout(), SetTitle(title));
}

/// Passes on a desktop notification a terminal tab's program (in tab or
/// project `from`) asked for: its escape sequence to the terminal vista runs
/// in and, inside herdr (which doesn't pass those on), to `herdr notification`.
pub fn notify(n: &crate::term_tab::Notification, from: &str) {
    tracing::debug!("notification from {from}: {:?} {:?}", n.title, n.body);
    pass_through(&n.seq);
    if let Some((bin, args)) = herdr_command(n, from, |k| std::env::var(k).ok()) {
        // Off the UI thread; waited for so it doesn't linger as a zombie.
        std::thread::spawn(move || {
            let child = std::process::Command::new(bin)
                .args(args)
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .spawn();
            if let Ok(mut child) = child {
                let _ = child.wait();
            }
        });
    }
}

/// The `herdr notification show` command for `n` when vista runs in a herdr
/// pane. herdr shows notifications for the agents it sees in its panes; one
/// running inside vista it can't see.
fn herdr_command(
    n: &crate::term_tab::Notification,
    from: &str,
    env: impl Fn(&str) -> Option<String>,
) -> Option<(String, Vec<String>)> {
    let set = |k: &str| env(k).filter(|v| !v.is_empty());
    if set("HERDR_ENV").as_deref() != Some("1") || set("HERDR_PANE_ID").is_none() || n.partial {
        return None;
    }
    let bin = set("HERDR_BIN_PATH").unwrap_or_else(|| "herdr".into());
    let title = if n.title.is_empty() {
        from.to_string()
    } else {
        format!("{} · {from}", n.title)
    };
    let mut args = vec!["notification".to_string(), "show".into(), title];
    if !n.body.is_empty() {
        args.extend(["--body".into(), n.body.clone()]);
    }
    args.extend(["--sound".into(), "request".into()]);
    Some((bin, args))
}

/// Writes an escape sequence a terminal tab's program meant for the real
/// terminal. Inside tmux it's wrapped so tmux hands it on (that needs
/// `set -g allow-passthrough on`).
fn pass_through(seq: &[u8]) {
    use std::io::Write;
    let mut out = io::stdout();
    let _ = if std::env::var_os("TMUX").is_some_and(|v| !v.is_empty()) {
        let mut wrapped = b"\x1bPtmux;".to_vec();
        for &b in seq {
            wrapped.extend_from_slice(if b == 0x1b {
                b"\x1b\x1b"
            } else {
                std::slice::from_ref(&b)
            });
        }
        wrapped.extend_from_slice(b"\x1b\\");
        out.write_all(&wrapped)
    } else {
        out.write_all(seq)
    };
    let _ = out.flush();
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

/// Temporarily gives the terminal back (to run an external program).
pub fn suspend() {
    restore();
}

/// Takes the terminal again after [`suspend`].
pub fn resume() -> io::Result<()> {
    enable_raw_mode()?;
    let mut out = io::stdout();
    execute!(
        out,
        EnterAlternateScreen,
        Clear(ClearType::All),
        EnableMouseCapture,
        EnableBracketedPaste
    )?;
    if KITTY_KEYBOARD.load(Ordering::SeqCst) {
        execute!(
            out,
            PushKeyboardEnhancementFlags(KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES)
        )?;
    }
    RESTORED.store(false, Ordering::SeqCst);
    Ok(())
}

fn install_panic_hook() {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        restore();
        previous(info);
    }));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::term_tab::Notification;

    fn note(title: &str, body: &str, partial: bool) -> Notification {
        Notification {
            seq: Vec::new(),
            title: title.into(),
            body: body.into(),
            partial,
        }
    }

    #[test]
    fn inside_herdr_a_notification_goes_to_herdr_too() {
        let herdr = |k: &str| match k {
            "HERDR_ENV" => Some("1".to_string()),
            "HERDR_PANE_ID" => Some("w1:p2".into()),
            "HERDR_BIN_PATH" => Some("/bin/herdr".into()),
            _ => None,
        };
        let (bin, args) = herdr_command(&note("Claude Code", "waiting", false), "claude agents", herdr).unwrap();
        assert_eq!(bin, "/bin/herdr");
        assert_eq!(
            args,
            [
                "notification",
                "show",
                "Claude Code · claude agents",
                "--body",
                "waiting",
                "--sound",
                "request"
            ]
        );
        // No title (OSC 9): the tab's; no body: no --body.
        let (_, args) = herdr_command(&note("", "", false), "vista", herdr).unwrap();
        assert_eq!(args, ["notification", "show", "vista", "--sound", "request"]);
        // Kitty's first parts wait for the last.
        assert_eq!(herdr_command(&note("", "t", true), "x", herdr), None);
        // Outside herdr (or with its variables emptied): only the escape sequence.
        assert_eq!(herdr_command(&note("a", "b", false), "x", |_| None), None);
        let emptied = |k: &str| (k == "HERDR_ENV").then(|| "1".to_string()).or(Some(String::new()));
        assert_eq!(herdr_command(&note("a", "b", false), "x", emptied), None);
    }
}
