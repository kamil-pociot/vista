//! View settings vista remembers between runs (`state.toml` in the config
//! dir): wrap, line numbers, diff and Commit-window options, the look, ….
//!
//! Every value is optional: unset means "use the default" (from
//! `config.toml` where it has one, otherwise vista's built-in). Changing a
//! setting in the app writes it here.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::config::Paths;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct UiState {
    /// The look picked with Ctrl-T.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub look: Option<String>,
    #[serde(skip_serializing_if = "ViewerState::is_empty")]
    pub viewer: ViewerState,
    #[serde(skip_serializing_if = "DiffState::is_empty")]
    pub diff: DiffState,
    #[serde(skip_serializing_if = "CommitState::is_empty")]
    pub commit: CommitState,
    #[serde(skip_serializing_if = "TreeState::is_empty")]
    pub tree: TreeState,
    #[serde(skip_serializing_if = "FindState::is_empty")]
    pub find: FindState,
    #[serde(skip_serializing_if = "ProjectsState::is_empty")]
    pub projects: ProjectsState,
    #[serde(skip_serializing_if = "LayoutState::is_empty")]
    pub layout: LayoutState,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct LayoutState {
    /// Width of the tree / Commit panel in columns (dragged or Ctrl-Shift-←/→).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub left_width: Option<u16>,
    /// The sidebar with projects and Claude sessions (Alt-B).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sidebar: Option<bool>,
    /// Its width in columns (dragged).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sidebar_width: Option<u16>,
}

/// Projects open at exit (restored on start) and opened before.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ProjectsState {
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub open: Vec<PathBuf>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub active: Option<PathBuf>,
    /// Most recent first.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub recent: Vec<PathBuf>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ViewerState {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line_numbers: Option<bool>,
    /// Long lines wrap in code (and other text) files.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wrap_code: Option<bool>,
    /// Long lines (and tables) wrap in Markdown.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wrap_markdown: Option<bool>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct DiffState {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unified: Option<bool>,
    /// Unchanged runs folded.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub collapse: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ignore_whitespace: Option<bool>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct CommitState {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub group_by_folder: Option<bool>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct TreeState {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub show_ignored: Option<bool>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct FindState {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub case: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub words: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub regex: Option<bool>,
}

macro_rules! is_empty {
    ($($t:ty),*) => {$(
        impl $t {
            fn is_empty(&self) -> bool {
                *self == <$t>::default()
            }
        }
    )*};
}
is_empty!(
    ViewerState,
    DiffState,
    CommitState,
    TreeState,
    FindState,
    ProjectsState,
    LayoutState
);

impl UiState {
    /// `state.toml` next to `config.toml`.
    pub fn path(paths: &Paths) -> PathBuf {
        paths.config_dir.join("state.toml")
    }

    /// Reads the settings; a missing file is empty, an unreadable one is
    /// empty plus a message saying why.
    pub fn load(path: &Path) -> (UiState, Option<String>) {
        match std::fs::read_to_string(path) {
            Ok(text) => match toml::from_str(&text) {
                Ok(s) => (s, None),
                Err(e) => (
                    UiState::default(),
                    Some(format!("{}: {e}; using defaults", path.display())),
                ),
            },
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => (UiState::default(), None),
            Err(e) => (
                UiState::default(),
                Some(format!("{}: {e}; using defaults", path.display())),
            ),
        }
    }

    /// Writes the settings (atomically: a temp file, then a rename).
    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let text = format!(
            "# View settings vista remembers (written by vista; delete to reset).\n{}",
            toml::to_string(self).map_err(std::io::Error::other)?
        );
        let tmp = path.with_extension("toml.tmp");
        std::fs::write(&tmp, text)?;
        std::fs::rename(&tmp, path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_file_is_empty() {
        let dir = tempfile::tempdir().unwrap();
        let (s, warn) = UiState::load(&dir.path().join("state.toml"));
        assert_eq!(s, UiState::default());
        assert!(warn.is_none());
    }

    #[test]
    fn round_trips_and_writes_only_what_was_set() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sub/state.toml");
        let mut s = UiState::default();
        s.viewer.line_numbers = Some(false);
        s.viewer.wrap_markdown = Some(true);
        s.diff.collapse = Some(true);
        s.commit.group_by_folder = Some(false);
        s.look = Some("paper".into());
        s.projects.open = vec![PathBuf::from("/a"), PathBuf::from("/b")];
        s.projects.active = Some(PathBuf::from("/b"));
        s.save(&path).unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("line_numbers = false"), "{text}");
        assert!(!text.contains("wrap_code"), "unset values stay out: {text}");
        assert!(!text.contains("[tree]"), "empty sections stay out: {text}");
        assert_eq!(UiState::load(&path), (s, None));
    }

    #[test]
    fn a_broken_file_falls_back_to_defaults_with_a_message() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("state.toml");
        std::fs::write(&path, "viewer = 3\n").unwrap();
        let (s, warn) = UiState::load(&path);
        assert_eq!(s, UiState::default());
        assert!(warn.unwrap().contains("using defaults"));
    }

    #[test]
    fn unknown_keys_are_ignored() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("state.toml");
        std::fs::write(&path, "future = 1\n[viewer]\nline_numbers = false\nlater = true\n").unwrap();
        let (s, warn) = UiState::load(&path);
        assert!(warn.is_none());
        assert_eq!(s.viewer.line_numbers, Some(false));
    }
}
