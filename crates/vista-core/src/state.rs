//! View settings vista remembers between runs (`state.toml` in the config
//! dir): wrap, line numbers, diff and Commit-window options, the look, ….
//!
//! Every value is optional: unset means "use the default" (from
//! `config.toml` where it has one, otherwise vista's built-in). Changing a
//! setting in the app writes it here.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::config::Paths;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct UiState {
    /// The look picked with Ctrl-T or in Preferences; `herdr` follows herdr.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub look: Option<String>,
    #[serde(skip_serializing_if = "LooksState::is_empty")]
    pub looks: LooksState,
    #[serde(skip_serializing_if = "MouseState::is_empty")]
    pub mouse: MouseState,
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
    #[serde(skip_serializing_if = "TabsState::is_empty")]
    pub tabs: TabsState,
    /// Key bindings changed on the Keymap page, on top of config.toml's
    /// `[keys]` and in the same form (`"ctrl+g" = "goto-line"`, `"q" = "none"`).
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub keys: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct LooksState {
    /// Paint the look's own background where it would use the terminal's
    /// (looks with `"terminal"` colours, such as herdr's).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fill_background: Option<bool>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct MouseState {
    /// Overrides `[mouse] copy_on_select`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub copy_on_select: Option<bool>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct LayoutState {
    /// Width of the tree / Commit panel in columns (dragged or Ctrl-Shift-←/→).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub left_width: Option<u16>,
    /// Height of the bottom tool window (Terminal, Find, Git) in rows
    /// (its title row dragged, or Ctrl-Shift-↑/↓).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bottom_height: Option<u16>,
}

/// File tabs.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct TabsState {
    /// Tabs that don't fit wrap onto more rows; otherwise the row scrolls.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub multiple_rows: Option<bool>,
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
    LooksState,
    MouseState,
    ViewerState,
    DiffState,
    CommitState,
    TreeState,
    FindState,
    ProjectsState,
    LayoutState,
    TabsState
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

    /// Applies `change` to the file as it is now and writes it back, under an
    /// exclusive lock on `state.toml.lock`: several vistas share the file,
    /// and saving a whole in-memory copy would undo what the others saved.
    /// Returns the merged settings (this instance's change plus everyone
    /// else's).
    pub fn update(path: &Path, change: impl FnOnce(&mut UiState)) -> std::io::Result<UiState> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let mut lock_path = path.as_os_str().to_owned();
        lock_path.push(".lock");
        let lock = std::fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(&lock_path)?;
        // Released when `lock` is dropped (also if writing fails).
        lock.lock()?;
        let mut state = UiState::load(path).0;
        change(&mut state);
        state.write(path)?;
        Ok(state)
    }

    /// Replaces the whole file with these settings (tests, a reset); the app
    /// saves with [`UiState::update`].
    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        Self::update(path, |s| *s = self.clone()).map(|_| ())
    }

    /// Writes atomically: a temp file of this process, then a rename.
    fn write(&self, path: &Path) -> std::io::Result<()> {
        let text = format!(
            "# View settings vista remembers (written by vista; delete to reset).\n{}",
            toml::to_string(self).map_err(std::io::Error::other)?
        );
        let mut tmp = path.as_os_str().to_owned();
        tmp.push(format!(".{}.tmp", std::process::id()));
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
        s.keys.insert("ctrl+g".into(), "none".into());
        s.save(&path).unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("line_numbers = false"), "{text}");
        assert!(text.contains("[keys]\n\"ctrl+g\" = \"none\""), "{text}");
        assert!(!text.contains("wrap_code"), "unset values stay out: {text}");
        assert!(!text.contains("[tree]"), "empty sections stay out: {text}");
        assert_eq!(UiState::load(&path), (s, None));
    }

    #[test]
    fn updates_keep_what_other_instances_saved() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("state.toml");
        // Two vistas loaded the same file, then each changes one setting.
        let (a, _) = UiState::load(&path);
        let (b, _) = UiState::load(&path);
        assert_eq!(a, b);
        let after_a = UiState::update(&path, |s| s.viewer.line_numbers = Some(false)).unwrap();
        let after_b = UiState::update(&path, |s| s.diff.unified = Some(true)).unwrap();
        assert_eq!(after_a.viewer.line_numbers, Some(false));
        // The second writer sees (and keeps) the first one's setting.
        assert_eq!(after_b.viewer.line_numbers, Some(false));
        assert_eq!(after_b.diff.unified, Some(true));
        assert_eq!(UiState::load(&path), (after_b, None));
        // Recent projects from two writers, most recent first.
        for p in ["/one", "/two"] {
            UiState::update(&path, |s| s.projects.recent.insert(0, PathBuf::from(p))).unwrap();
        }
        let recent = UiState::load(&path).0.projects.recent;
        assert_eq!(recent, [PathBuf::from("/two"), PathBuf::from("/one")]);
        // Each process writes its own temp file, and leaves none behind.
        let left: Vec<_> = std::fs::read_dir(dir.path())
            .unwrap()
            .map(|e| e.unwrap().file_name().into_string().unwrap())
            .filter(|n| n.ends_with(".tmp"))
            .collect();
        assert!(left.is_empty(), "{left:?}");
    }

    #[test]
    fn many_writers_at_once_lose_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("state.toml");
        std::thread::scope(|scope| {
            for i in 0..16 {
                let path = &path;
                scope.spawn(move || {
                    for k in 0..10 {
                        let p = PathBuf::from(format!("/w{i}/{k}"));
                        UiState::update(path, |s| s.projects.recent.push(p)).unwrap();
                        // The file parses at every moment.
                        assert_eq!(UiState::load(path).1, None);
                    }
                });
            }
        });
        assert_eq!(UiState::load(&path).0.projects.recent.len(), 160, "no update lost");
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
