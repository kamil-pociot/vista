//! Claude Code sessions for the sidebar: what `claude agents --json` lists
//! (background and interactive sessions, with their state), polled on a
//! thread while the sidebar is shown.

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use serde::Deserialize;

use crate::event::{AppEvent, EventTx};

/// How often the list is refreshed.
const POLL: Duration = Duration::from_secs(2);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionState {
    Working,
    /// Waiting for an answer or a permission.
    Blocked,
    Done,
    Failed,
    Stopped,
}

impl SessionState {
    /// From `state` (what the agent view makes of the session) and `status`
    /// (what its process is doing: `busy`, `shell`, `idle`, `waiting`). A
    /// background session that ended its turn is `idle` while its state may
    /// still read `working`.
    fn parse(state: &str, status: &str) -> SessionState {
        match (state, status) {
            ("blocked", _) | (_, "waiting") => SessionState::Blocked,
            ("failed", _) => SessionState::Failed,
            ("done" | "completed", _) | (_, "idle") => SessionState::Done,
            ("stopped", _) => SessionState::Stopped,
            _ => SessionState::Working,
        }
    }

    /// The mark in front of the session and its colour token.
    pub fn mark(self) -> (&'static str, &'static str) {
        match self {
            SessionState::Blocked => ("●", "ui.warning"),
            SessionState::Working => ("◐", "ui.accent"),
            SessionState::Done => ("✓", "git.added"),
            SessionState::Failed => ("✗", "ui.error"),
            SessionState::Stopped => ("○", "ui.dim"),
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            SessionState::Blocked => "needs input",
            SessionState::Working => "working",
            SessionState::Done => "done",
            SessionState::Failed => "failed",
            SessionState::Stopped => "stopped",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Session {
    /// Short id (`claude attach <id>`).
    pub id: String,
    pub name: String,
    pub cwd: PathBuf,
    pub state: SessionState,
}

#[derive(Deserialize)]
struct Raw {
    id: String,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    cwd: Option<PathBuf>,
    #[serde(default)]
    state: Option<String>,
    #[serde(default)]
    status: Option<String>,
}

/// Parses `claude agents --json`.
pub fn parse(json: &str) -> Result<Vec<Session>, String> {
    let raw: Vec<Raw> = serde_json::from_str(json).map_err(|e| format!("unexpected session list: {e}"))?;
    Ok(raw
        .into_iter()
        .map(|r| Session {
            name: r.name.filter(|n| !n.is_empty()).unwrap_or_else(|| r.id.clone()),
            state: SessionState::parse(r.state.as_deref().unwrap_or(""), r.status.as_deref().unwrap_or("")),
            cwd: r.cwd.unwrap_or_default(),
            id: r.id,
        })
        .collect())
}

/// What a new list says happened since the last one: the sound to play.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Change {
    /// A working session now needs input.
    NeedsInput,
    /// A working session finished (or failed).
    Finished,
}

/// The most urgent change from `before` to `now` among sessions in both:
/// needing input beats finishing. New sessions, and ones that were already
/// waiting or done, change nothing.
pub fn change(before: &[Session], now: &[Session]) -> Option<Change> {
    let mut found = None;
    for s in now {
        let Some(old) = before.iter().find(|b| b.id == s.id) else {
            continue;
        };
        if old.state != SessionState::Working {
            continue;
        }
        match s.state {
            SessionState::Blocked => return Some(Change::NeedsInput),
            SessionState::Done | SessionState::Failed => found = Some(Change::Finished),
            _ => {}
        }
    }
    found
}

/// Runs `command` (through the login shell, for `PATH`) every couple of
/// seconds while `wanted` is set and posts [`AppEvent::Sessions`].
pub fn spawn_poller(command: String, wanted: Arc<AtomicBool>, tx: EventTx) {
    let shell = std::env::var("SHELL")
        .ok()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "/bin/sh".into());
    let _ = std::thread::Builder::new().name("sessions".into()).spawn(move || {
        loop {
            if wanted.load(Ordering::SeqCst) {
                let out = std::process::Command::new(&shell)
                    .args(["-l", "-c", &command])
                    .stdin(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null())
                    .output();
                let result = match out {
                    Ok(o) if o.status.success() => parse(&String::from_utf8_lossy(&o.stdout)),
                    Ok(_) => Err(format!("`{command}` failed")),
                    Err(e) => Err(format!("cannot run `{command}`: {e}")),
                };
                if tx.send(AppEvent::Sessions(result)).is_err() {
                    return;
                }
            }
            std::thread::sleep(POLL);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_what_claude_agents_lists() {
        let json = r#"[
            {"pid": 1, "id": "74ab178b", "cwd": "/r/lms", "kind": "background", "name": "product-owner",
             "status": "busy", "state": "working"},
            {"id": "40008faf", "cwd": "/r/fb", "name": "", "state": "blocked"},
            {"id": "aa", "state": "done"}, {"id": "bb", "state": "failed"},
            {"id": "cc", "state": "stopped"}, {"id": "dd", "status": "idle"}
        ]"#;
        let s = parse(json).unwrap();
        assert_eq!(s.len(), 6);
        assert_eq!(s[0].id, "74ab178b");
        assert_eq!(s[0].name, "product-owner");
        assert_eq!(s[0].cwd, PathBuf::from("/r/lms"));
        assert_eq!(s[0].state, SessionState::Working);
        // No name: the id stands in.
        assert_eq!(s[1].name, "40008faf");
        let states: Vec<_> = s.iter().map(|s| s.state).collect();
        use SessionState::*;
        assert_eq!(states, [Working, Blocked, Done, Failed, Stopped, Done]);
        // The process status wins over a stale state: waiting, or idle (its turn ended).
        let st = |state: &str, status: &str| SessionState::parse(state, status);
        assert_eq!(st("working", "waiting"), Blocked);
        assert_eq!(st("working", "idle"), Done);
        assert_eq!(st("working", "busy"), Working);
        assert_eq!(st("working", "shell"), Working);
        assert_eq!(st("failed", "idle"), Failed);
        assert_eq!(Blocked.mark(), ("●", "ui.warning"));
        assert!(parse("not json").is_err());
        assert_eq!(parse("[]").unwrap(), vec![]);
    }

    fn s(id: &str, state: SessionState) -> Session {
        Session {
            id: id.into(),
            name: id.into(),
            cwd: PathBuf::new(),
            state,
        }
    }

    #[test]
    fn a_working_session_that_finishes_or_needs_input_is_a_change() {
        use SessionState::*;
        let before = [s("a", Working), s("b", Working), s("c", Done)];
        assert_eq!(change(&before, &before), None);
        assert_eq!(
            change(&before, &[s("a", Done), s("b", Working)]),
            Some(Change::Finished)
        );
        assert_eq!(change(&before, &[s("a", Failed)]), Some(Change::Finished));
        // Needing input wins over finishing.
        assert_eq!(
            change(&before, &[s("a", Done), s("b", Blocked)]),
            Some(Change::NeedsInput)
        );
        // Already done, or new: nothing to tell.
        assert_eq!(change(&before, &[s("c", Blocked), s("d", Blocked)]), None);
        // Gone from the list: nothing either.
        assert_eq!(change(&before, &[]), None);
    }
}
