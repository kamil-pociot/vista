//! Debounced, recursive file watching.

use std::path::{Path, PathBuf};
use std::time::Duration;

use notify_debouncer_mini::notify::RecursiveMode;
use notify_debouncer_mini::{DebounceEventResult, Debouncer, new_debouncer};

/// A batch of changed paths, split by whether they are repository metadata.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FsChange {
    /// Changed files and folders in the project.
    pub paths: Vec<PathBuf>,
    /// Something in `.git` that affects status changed (index, HEAD, refs).
    pub repo_meta: bool,
}

/// Splits raw event paths into project paths and a repository-metadata flag.
/// Noise inside `.git` (objects, logs, lock files) is dropped.
pub fn classify(paths: impl IntoIterator<Item = PathBuf>) -> FsChange {
    let mut change = FsChange::default();
    for p in paths {
        let comps: Vec<String> = p
            .components()
            .map(|c| c.as_os_str().to_string_lossy().into_owned())
            .collect();
        if let Some(i) = comps.iter().position(|c| c == ".git") {
            let rest = &comps[i + 1..];
            let meaningful = matches!(
                rest.first().map(String::as_str),
                Some("index") | Some("HEAD") | Some("refs") | Some("MERGE_HEAD") | Some("packed-refs")
            );
            if meaningful {
                change.repo_meta = true;
            }
            continue;
        }
        if !change.paths.contains(&p) {
            change.paths.push(p);
        }
    }
    change
}

/// Keeps the watcher alive; dropping it stops watching.
pub struct Watcher {
    _debouncer: Debouncer<notify_debouncer_mini::notify::RecommendedWatcher>,
}

impl Watcher {
    /// Watches `root` recursively and calls `on_change` with debounced batches.
    pub fn new(
        root: &Path,
        debounce: Duration,
        on_change: impl Fn(FsChange) + Send + 'static,
    ) -> Result<Watcher, notify_debouncer_mini::notify::Error> {
        let mut debouncer = new_debouncer(debounce, move |res: DebounceEventResult| {
            if let Ok(events) = res {
                let change = classify(events.into_iter().map(|e| e.path));
                if !change.paths.is_empty() || change.repo_meta {
                    on_change(change);
                }
            }
        })?;
        debouncer.watcher().watch(root, RecursiveMode::Recursive)?;
        Ok(Watcher { _debouncer: debouncer })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;

    #[test]
    fn classifies_repository_metadata() {
        let c = classify(vec![
            PathBuf::from("/p/src/a.rs"),
            PathBuf::from("/p/.git/index"),
            PathBuf::from("/p/.git/objects/ab/cdef"),
            PathBuf::from("/p/src/a.rs"),
        ]);
        assert_eq!(c.paths, vec![PathBuf::from("/p/src/a.rs")]);
        assert!(c.repo_meta);
        let c = classify(vec![
            PathBuf::from("/p/.git/objects/x"),
            PathBuf::from("/p/.git/index.lock"),
        ]);
        assert!(!c.repo_meta && c.paths.is_empty());
    }

    #[test]
    fn reports_file_changes() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().canonicalize().unwrap();
        let (tx, rx) = mpsc::channel();
        let _w = Watcher::new(&root, Duration::from_millis(50), move |c| {
            let _ = tx.send(c);
        })
        .unwrap();
        std::thread::sleep(Duration::from_millis(100));
        std::fs::write(root.join("hello.txt"), "hi").unwrap();
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        let mut seen = false;
        while std::time::Instant::now() < deadline && !seen {
            if let Ok(c) = rx.recv_timeout(Duration::from_millis(200)) {
                seen = c.paths.iter().any(|p| p.ends_with("hello.txt"));
            }
        }
        assert!(seen, "watcher reported hello.txt");
    }
}
