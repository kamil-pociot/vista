//! Terminal tabs: a program (`claude agents`, a shell, …) running in a
//! pseudo-terminal, shown full screen. Output goes through a vt100 parser
//! (on a reader thread); keys, pastes and mouse events are encoded the way a
//! terminal would send them.

use std::io::{Read, Write};
use std::num::NonZeroU16;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

use anyhow::{Context, Result};
use portable_pty::{Child, CommandBuilder, MasterPty, PtySize, native_pty_system};
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};

use crate::event::{AppEvent, EventTx, ProjectId};

pub struct TermTab {
    pub id: ProjectId,
    /// The command, as shown on the tab.
    pub title: String,
    parser: Arc<Mutex<vt100::Parser>>,
    master: Box<dyn MasterPty + Send>,
    writer: Box<dyn Write + Send>,
    child: Box<dyn Child + Send + Sync>,
    /// Set once the program ended (its exit status, for the tab).
    pub exited: Option<String>,
    size: (u16, u16),
}

fn nz(n: u16) -> NonZeroU16 {
    NonZeroU16::new(n.max(1)).expect("non-zero")
}

impl TermTab {
    /// Runs `command` through the login shell (so `~/.zshrc`'s PATH applies)
    /// in `cwd`, `cols`×`rows`; an empty command is the interactive shell. Output wakes the app with
    /// [`AppEvent::TermOutput`]; the end with [`AppEvent::TermExited`].
    pub fn spawn(id: ProjectId, command: &str, cwd: &Path, size: (u16, u16), tx: EventTx) -> Result<TermTab> {
        let (cols, rows) = (size.0.max(2), size.1.max(2));
        let pair = native_pty_system()
            .openpty(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .context("cannot open a pseudo-terminal")?;
        let shell = std::env::var("SHELL")
            .ok()
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "/bin/sh".into());
        let mut cmd = CommandBuilder::new(&shell);
        if command.trim().is_empty() {
            cmd.args(["-l", "-i"]);
        } else {
            cmd.args(["-l", "-i", "-c", command]);
        }
        cmd.cwd(cwd);
        cmd.env("TERM", "xterm-256color");
        cmd.env("COLORTERM", "truecolor");
        let child = pair
            .slave
            .spawn_command(cmd)
            .with_context(|| format!("cannot run {command}"))?;
        drop(pair.slave);
        let parser = Arc::new(Mutex::new(vt100::Parser::new(nz(rows), nz(cols), 2000)));
        let mut reader = pair.master.try_clone_reader().context("pty reader")?;
        let writer = pair.master.take_writer().context("pty writer")?;
        let p = parser.clone();
        // One wake-up at a time: the next is sent after the app drew.
        let pending = Arc::new(AtomicBool::new(false));
        std::thread::Builder::new()
            .name(format!("term-{id}"))
            .spawn(move || {
                let mut buf = [0u8; 16 * 1024];
                loop {
                    match reader.read(&mut buf) {
                        Ok(0) | Err(_) => break,
                        Ok(n) => {
                            p.lock().expect("terminal parser").process(&buf[..n]);
                            if !pending.swap(true, Ordering::SeqCst) {
                                let _ = tx.send(AppEvent::TermOutput(pending.clone()));
                            }
                        }
                    }
                }
                let _ = tx.send(AppEvent::TermExited);
            })
            .context("terminal reader thread")?;
        let title = if command.trim().is_empty() {
            Path::new(&shell)
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| "shell".into())
        } else {
            command.to_string()
        };
        Ok(TermTab {
            id,
            title,
            parser,
            master: pair.master,
            writer,
            child,
            exited: None,
            size: (cols, rows),
        })
    }

    pub fn screen(&self) -> MutexGuard<'_, vt100::Parser> {
        self.parser.lock().expect("terminal parser")
    }

    pub fn write(&mut self, bytes: &[u8]) {
        if self.exited.is_none() {
            let _ = self.writer.write_all(bytes);
            let _ = self.writer.flush();
        }
    }

    /// Keeps the program's terminal the size of the area it's drawn in.
    pub fn resize(&mut self, cols: u16, rows: u16) {
        if (cols, rows) == self.size || cols < 2 || rows < 2 {
            return;
        }
        self.size = (cols, rows);
        let _ = self.master.resize(PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        });
        self.screen().screen_mut().set_size(nz(rows), nz(cols));
    }

    /// Records how the program ended.
    pub fn mark_exited(&mut self) {
        if self.exited.is_none() {
            let status = self
                .child
                .try_wait()
                .ok()
                .flatten()
                .map(|s| {
                    if s.success() {
                        "exited".to_string()
                    } else {
                        format!("exited with status {}", s.exit_code())
                    }
                })
                .unwrap_or_else(|| "exited".into());
            self.exited = Some(status);
        }
    }

    pub fn kill(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }

    /// Sends a key as the terminal would.
    pub fn key(&mut self, key: &KeyEvent) {
        let app_cursor = self.screen().screen().application_cursor();
        if let Some(bytes) = key_bytes(key, app_cursor) {
            self.write(&bytes);
        }
    }

    /// Sends pasted text (bracketed when the program asked for it).
    pub fn paste(&mut self, text: &str) {
        let bracketed = self.screen().screen().bracketed_paste();
        let mut bytes = Vec::new();
        if bracketed {
            bytes.extend_from_slice(b"\x1b[200~");
        }
        bytes.extend_from_slice(text.replace("\r\n", "\r").replace('\n', "\r").as_bytes());
        if bracketed {
            bytes.extend_from_slice(b"\x1b[201~");
        }
        self.write(&bytes);
    }

    /// Forwards a mouse event at (`col`, `row`) of the terminal, if the
    /// program asked for mouse reporting.
    pub fn mouse(&mut self, m: &MouseEvent, col: u16, row: u16) {
        let (mode, encoding) = {
            let p = self.screen();
            (p.screen().mouse_protocol_mode(), p.screen().mouse_protocol_encoding())
        };
        if let Some(bytes) = mouse_bytes(m, col, row, mode, encoding) {
            self.write(&bytes);
        }
    }
}

impl Drop for TermTab {
    fn drop(&mut self) {
        if self.exited.is_none() {
            self.kill();
        }
    }
}

/// The bytes a terminal sends for `key` (xterm style).
pub fn key_bytes(key: &KeyEvent, app_cursor: bool) -> Option<Vec<u8>> {
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    let alt = key.modifiers.contains(KeyModifiers::ALT);
    let shift = key.modifiers.contains(KeyModifiers::SHIFT);
    // xterm modifier parameter: 1 + shift + 2·alt + 4·ctrl.
    let m = 1 + u8::from(shift) + 2 * u8::from(alt) + 4 * u8::from(ctrl);
    let csi = |end: char| -> Vec<u8> {
        if m > 1 {
            format!("\x1b[1;{m}{end}").into_bytes()
        } else if app_cursor && "ABCDHF".contains(end) {
            format!("\x1bO{end}").into_bytes()
        } else {
            format!("\x1b[{end}").into_bytes()
        }
    };
    let tilde = |n: u8| -> Vec<u8> {
        if m > 1 {
            format!("\x1b[{n};{m}~").into_bytes()
        } else {
            format!("\x1b[{n}~").into_bytes()
        }
    };
    let mut out = match key.code {
        KeyCode::Char(c) if ctrl => {
            let b = match c.to_ascii_lowercase() {
                c @ 'a'..='z' => c as u8 - b'a' + 1,
                '@' | ' ' | '2' => 0,
                '[' | '3' => 0x1b,
                '\\' | '4' => 0x1c,
                ']' | '5' => 0x1d,
                '^' | '6' => 0x1e,
                '_' | '/' | '7' => 0x1f,
                _ => return None,
            };
            vec![b]
        }
        KeyCode::Char(c) => c.to_string().into_bytes(),
        KeyCode::Enter => vec![b'\r'],
        KeyCode::Tab => vec![b'\t'],
        KeyCode::BackTab => b"\x1b[Z".to_vec(),
        KeyCode::Backspace => vec![if ctrl { 0x08 } else { 0x7f }],
        KeyCode::Esc => vec![0x1b],
        KeyCode::Up => csi('A'),
        KeyCode::Down => csi('B'),
        KeyCode::Right => csi('C'),
        KeyCode::Left => csi('D'),
        KeyCode::Home => csi('H'),
        KeyCode::End => csi('F'),
        KeyCode::Insert => tilde(2),
        KeyCode::Delete => tilde(3),
        KeyCode::PageUp => tilde(5),
        KeyCode::PageDown => tilde(6),
        KeyCode::F(n @ 1..=4) => {
            let end = b"PQRS"[usize::from(n - 1)] as char;
            if m > 1 {
                format!("\x1b[1;{m}{end}").into_bytes()
            } else {
                format!("\x1bO{end}").into_bytes()
            }
        }
        KeyCode::F(n @ 5..=12) => tilde([15, 17, 18, 19, 20, 21, 23, 24][usize::from(n - 5)]),
        _ => return None,
    };
    // Alt prefixes ESC to plain keys (the CSI forms carry it already).
    if alt
        && matches!(
            key.code,
            KeyCode::Char(_) | KeyCode::Enter | KeyCode::Backspace | KeyCode::Esc | KeyCode::Tab
        )
    {
        out.insert(0, 0x1b);
    }
    Some(out)
}

/// The bytes for a mouse event, per the program's requested mode.
pub fn mouse_bytes(
    m: &MouseEvent,
    col: u16,
    row: u16,
    mode: vt100::MouseProtocolMode,
    encoding: vt100::MouseProtocolEncoding,
) -> Option<Vec<u8>> {
    use vt100::MouseProtocolMode as Mode;
    let button = |b: MouseButton| match b {
        MouseButton::Left => 0u8,
        MouseButton::Middle => 1,
        MouseButton::Right => 2,
    };
    let (code, release) = match (m.kind, mode) {
        (_, Mode::None) => return None,
        (MouseEventKind::Down(b), _) => (button(b), false),
        (MouseEventKind::Up(b), Mode::PressRelease | Mode::ButtonMotion | Mode::AnyMotion) => (button(b), true),
        (MouseEventKind::Drag(b), Mode::ButtonMotion | Mode::AnyMotion) => (button(b) + 32, false),
        (MouseEventKind::Moved, Mode::AnyMotion) => (35, false),
        (MouseEventKind::ScrollUp, _) => (64, false),
        (MouseEventKind::ScrollDown, _) => (65, false),
        _ => return None,
    };
    let (x, y) = (col + 1, row + 1);
    Some(match encoding {
        vt100::MouseProtocolEncoding::Sgr => {
            format!("\x1b[<{code};{x};{y}{}", if release { 'm' } else { 'M' }).into_bytes()
        }
        _ => {
            // Legacy: release is button 3; coordinates + 32, capped.
            let code = if release { 3 } else { code };
            let enc = |v: u16| (v.min(223) + 32) as u8;
            vec![0x1b, b'[', b'M', code + 32, enc(x), enc(y)]
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn k(code: KeyCode, mods: KeyModifiers) -> KeyEvent {
        KeyEvent::new(code, mods)
    }

    #[test]
    fn keys_encode_like_xterm() {
        let none = KeyModifiers::NONE;
        let b = |code, mods, app| key_bytes(&k(code, mods), app).unwrap();
        assert_eq!(b(KeyCode::Char('a'), none, false), b"a");
        assert_eq!(b(KeyCode::Char('ł'), none, false), "ł".as_bytes());
        assert_eq!(b(KeyCode::Char('c'), KeyModifiers::CONTROL, false), [0x03]);
        assert_eq!(b(KeyCode::Char('x'), KeyModifiers::ALT, false), b"\x1bx");
        assert_eq!(b(KeyCode::Enter, none, false), b"\r");
        assert_eq!(b(KeyCode::Backspace, none, false), [0x7f]);
        assert_eq!(b(KeyCode::Esc, none, false), [0x1b]);
        assert_eq!(b(KeyCode::Up, none, false), b"\x1b[A");
        assert_eq!(b(KeyCode::Up, none, true), b"\x1bOA", "application cursor mode");
        assert_eq!(b(KeyCode::Left, KeyModifiers::CONTROL, false), b"\x1b[1;5D");
        assert_eq!(b(KeyCode::BackTab, KeyModifiers::SHIFT, false), b"\x1b[Z");
        assert_eq!(b(KeyCode::Delete, none, false), b"\x1b[3~");
        assert_eq!(b(KeyCode::PageDown, none, false), b"\x1b[6~");
        assert_eq!(b(KeyCode::F(1), none, false), b"\x1bOP");
        assert_eq!(b(KeyCode::F(5), none, false), b"\x1b[15~");
    }

    #[test]
    fn mouse_follows_the_programs_mode() {
        use vt100::{MouseProtocolEncoding as E, MouseProtocolMode as M};
        let ev = |kind| MouseEvent {
            kind,
            column: 0,
            row: 0,
            modifiers: KeyModifiers::NONE,
        };
        let down = ev(MouseEventKind::Down(MouseButton::Left));
        assert_eq!(mouse_bytes(&down, 4, 2, M::None, E::Sgr), None, "not asked for");
        assert_eq!(
            mouse_bytes(&down, 4, 2, M::PressRelease, E::Sgr).unwrap(),
            b"\x1b[<0;5;3M"
        );
        let up = ev(MouseEventKind::Up(MouseButton::Left));
        assert_eq!(
            mouse_bytes(&up, 4, 2, M::PressRelease, E::Sgr).unwrap(),
            b"\x1b[<0;5;3m"
        );
        assert_eq!(mouse_bytes(&up, 4, 2, M::Press, E::Sgr), None);
        let wheel = ev(MouseEventKind::ScrollDown);
        assert_eq!(
            mouse_bytes(&wheel, 0, 0, M::Press, E::Default).unwrap(),
            [0x1b, b'[', b'M', 97, 33, 33]
        );
    }

    #[test]
    fn runs_a_program_and_round_trips_input() {
        let dir = tempfile::tempdir().unwrap();
        let (tx, rx) = crate::event::channel(7);
        let mut t = TermTab::spawn(7, "cat", dir.path(), (40, 10), tx).unwrap();
        t.write(b"hello\r");
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while std::time::Instant::now() < deadline {
            if let Ok((_, AppEvent::TermOutput(pending))) = rx.recv_timeout(std::time::Duration::from_millis(200)) {
                pending.store(false, Ordering::SeqCst);
            }
            // The tty echoes the line and cat prints it again.
            if t.screen().screen().contents().matches("hello").count() >= 2 {
                break;
            }
        }
        assert!(
            t.screen().screen().contents().matches("hello").count() >= 2,
            "{}",
            t.screen().screen().contents()
        );
        t.resize(60, 12);
        assert_eq!(t.screen().screen().size(), (nz(12), nz(60)));
        t.kill();
        // The reader sees the end.
        let ended = (0..50).any(|_| {
            matches!(
                rx.recv_timeout(std::time::Duration::from_millis(100)),
                Ok((_, AppEvent::TermExited))
            )
        });
        assert!(ended);
    }
}
