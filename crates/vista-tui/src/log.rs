//! The Git tool window (Alt-9): the commit log, or one file's history
//! (Alt-H), with the files each commit changed; Enter shows a file's change
//! as a read-only diff.

use std::collections::HashMap;
use std::path::PathBuf;

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use vista_git::{Commit, CommitFile};

/// Commits loaded at a time.
pub const MAX_COMMITS: usize = 1000;

#[derive(Default)]
pub struct LogWindow {
    pub visible: bool,
    /// Only commits touching this file (file history).
    pub path: Option<PathBuf>,
    pub commits: Vec<Commit>,
    pub selected: usize,
    pub scroll: usize,
    /// Changed files per commit hash (loaded on demand).
    pub files: HashMap<String, Result<Vec<CommitFile>, String>>,
    pub file_selected: usize,
    pub file_scroll: usize,
    /// Keys move in the file list (else the commit list).
    pub on_files: bool,
    pub loading: bool,
    pub error: Option<String>,
    /// Bumped per load; stale results are dropped.
    pub generation: u64,
    pub view_height: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LogOutcome {
    Handled,
    Unhandled,
    /// Hide the window.
    Close,
    /// Load the changed files of this commit.
    NeedFiles(Commit),
    /// Show this file's change in this commit.
    Open(Commit, CommitFile),
    /// Load the log again.
    Reload,
}

impl LogWindow {
    pub fn selected_commit(&self) -> Option<&Commit> {
        self.commits.get(self.selected)
    }

    /// The selected commit's files (empty while loading).
    pub fn current_files(&self) -> &[CommitFile] {
        match self.selected_commit().and_then(|c| self.files.get(&c.hash)) {
            Some(Ok(f)) => f,
            _ => &[],
        }
    }

    /// The changed files of commit `hash`, if loaded.
    pub fn files_of(&self, hash: &str) -> Option<&[CommitFile]> {
        match self.files.get(hash) {
            Some(Ok(f)) => Some(f),
            _ => None,
        }
    }

    /// New log results; keeps the selection on the same commit if it's
    /// still there.
    pub fn set_commits(&mut self, commits: Vec<Commit>) {
        let keep = self.selected_commit().map(|c| c.hash.clone());
        self.commits = commits;
        self.loading = false;
        self.error = None;
        self.selected = keep
            .and_then(|h| self.commits.iter().position(|c| c.hash == h))
            .unwrap_or(0);
        self.file_selected = 0;
        self.file_scroll = 0;
    }

    /// Asks for the selected commit's files unless already loaded.
    pub fn need_files(&self) -> LogOutcome {
        match self.selected_commit() {
            Some(c) if !self.files.contains_key(&c.hash) => LogOutcome::NeedFiles(c.clone()),
            _ => LogOutcome::Handled,
        }
    }

    pub fn select_commit(&mut self, i: usize) -> LogOutcome {
        if self.commits.is_empty() {
            return LogOutcome::Handled;
        }
        self.selected = i.min(self.commits.len() - 1);
        self.file_selected = 0;
        self.file_scroll = 0;
        self.keep_visible();
        self.need_files()
    }

    fn keep_visible(&mut self) {
        let h = self.view_height.max(1);
        let fit = |sel: usize, scroll: &mut usize| {
            if sel < *scroll {
                *scroll = sel;
            } else if sel >= *scroll + h {
                *scroll = sel + 1 - h;
            }
        };
        fit(self.selected, &mut self.scroll);
        fit(self.file_selected, &mut self.file_scroll);
    }

    /// Opens the selected file of the selected commit.
    pub fn open_selected(&self) -> LogOutcome {
        match (self.selected_commit(), self.current_files().get(self.file_selected)) {
            (Some(c), Some(f)) => LogOutcome::Open(c.clone(), f.clone()),
            _ => LogOutcome::Handled,
        }
    }

    pub fn handle_key(&mut self, key: &KeyEvent) -> LogOutcome {
        if key.modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) {
            return LogOutcome::Unhandled;
        }
        let page = self.view_height.max(1) as isize;
        let step = |n: usize, len: usize, d: isize| -> usize {
            if len == 0 {
                0
            } else {
                (n as isize + d).clamp(0, len as isize - 1) as usize
            }
        };
        let delta = match key.code {
            KeyCode::Up | KeyCode::Char('k') => Some(-1),
            KeyCode::Down | KeyCode::Char('j') => Some(1),
            KeyCode::PageUp => Some(-page),
            KeyCode::PageDown => Some(page),
            KeyCode::Home => Some(isize::MIN / 2),
            KeyCode::End => Some(isize::MAX / 2),
            _ => None,
        };
        if let Some(d) = delta {
            if self.on_files {
                self.file_selected = step(self.file_selected, self.current_files().len(), d);
                self.keep_visible();
                return LogOutcome::Handled;
            }
            let i = step(self.selected, self.commits.len(), d);
            return self.select_commit(i);
        }
        match key.code {
            KeyCode::Tab | KeyCode::Right | KeyCode::Char('l') if !self.on_files => {
                self.on_files = true;
                LogOutcome::Handled
            }
            KeyCode::BackTab | KeyCode::Left | KeyCode::Char('h') if self.on_files => {
                self.on_files = false;
                LogOutcome::Handled
            }
            KeyCode::Enter if !self.on_files => {
                self.on_files = true;
                LogOutcome::Handled
            }
            KeyCode::Enter => self.open_selected(),
            KeyCode::Char('r') => LogOutcome::Reload,
            // In a file's history, `a` shows all commits again.
            KeyCode::Char('a') if self.path.is_some() => {
                self.path = None;
                LogOutcome::Reload
            }
            KeyCode::Esc => LogOutcome::Close,
            _ => LogOutcome::Unhandled,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn commit(h: &str) -> Commit {
        Commit {
            hash: h.repeat(8),
            short: h.into(),
            parents: vec![],
            author: "A".into(),
            date: "2026-10-01".into(),
            refs: String::new(),
            subject: format!("subject {h}"),
        }
    }

    fn file(p: &str) -> CommitFile {
        CommitFile {
            status: 'M',
            path: Path::new("/r").join(p),
            old_path: None,
        }
    }

    fn key(c: KeyCode) -> KeyEvent {
        KeyEvent::from(c)
    }

    #[test]
    fn moving_through_commits_asks_for_their_files_once() {
        let mut w = LogWindow {
            view_height: 5,
            ..LogWindow::default()
        };
        w.set_commits(vec![commit("a"), commit("b"), commit("c")]);
        assert_eq!(w.handle_key(&key(KeyCode::Down)), LogOutcome::NeedFiles(commit("b")));
        w.files.insert(commit("b").hash, Ok(vec![file("x.rs"), file("y.rs")]));
        assert_eq!(w.handle_key(&key(KeyCode::Up)), LogOutcome::NeedFiles(commit("a")));
        assert_eq!(
            w.handle_key(&key(KeyCode::Down)),
            LogOutcome::Handled,
            "b's files are cached"
        );
        // Enter goes to the files; Enter there opens one.
        assert_eq!(w.handle_key(&key(KeyCode::Enter)), LogOutcome::Handled);
        assert!(w.on_files);
        w.handle_key(&key(KeyCode::Down));
        assert_eq!(
            w.handle_key(&key(KeyCode::Enter)),
            LogOutcome::Open(commit("b"), file("y.rs"))
        );
        assert_eq!(w.handle_key(&key(KeyCode::Left)), LogOutcome::Handled);
        assert!(!w.on_files);
        assert_eq!(w.handle_key(&key(KeyCode::Esc)), LogOutcome::Close);
    }

    #[test]
    fn reloading_keeps_the_selected_commit() {
        let mut w = LogWindow::default();
        w.set_commits(vec![commit("a"), commit("b")]);
        w.select_commit(1);
        w.set_commits(vec![commit("new"), commit("a"), commit("b")]);
        assert_eq!(w.selected_commit().unwrap().short, "b");
    }

    #[test]
    fn file_history_can_widen_to_all_commits() {
        let mut w = LogWindow {
            path: Some(PathBuf::from("/r/x.rs")),
            ..LogWindow::default()
        };
        assert_eq!(w.handle_key(&key(KeyCode::Char('a'))), LogOutcome::Reload);
        assert!(w.path.is_none());
        assert_eq!(w.handle_key(&key(KeyCode::Char('a'))), LogOutcome::Unhandled);
    }
}
