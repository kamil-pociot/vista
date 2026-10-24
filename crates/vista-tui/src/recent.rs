//! Recently opened files (persisted per project) and the Ctrl-E popup.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

const MAX_RECENT: usize = 50;

#[derive(Debug, Default)]
pub struct Recent {
    pub list: Vec<PathBuf>,
    file: Option<PathBuf>,
}

impl Recent {
    /// Loads the list for `root` from `cache_dir/recent/<hash>.txt`.
    pub fn load(cache_dir: &Path, root: &Path) -> Recent {
        let mut h = DefaultHasher::new();
        root.hash(&mut h);
        let file = cache_dir.join("recent").join(format!("{:016x}.txt", h.finish()));
        let list = std::fs::read_to_string(&file)
            .map(|s| {
                s.lines()
                    .map(PathBuf::from)
                    .filter(|p| p.starts_with(root) && p.is_file())
                    .collect()
            })
            .unwrap_or_default();
        Recent { list, file: Some(file) }
    }

    /// Moves `path` to the front.
    pub fn push(&mut self, path: &Path) {
        self.list.retain(|p| p != path);
        self.list.insert(0, path.to_path_buf());
        self.list.truncate(MAX_RECENT);
        self.save();
    }

    fn save(&self) {
        let Some(file) = &self.file else { return };
        if let Some(dir) = file.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let text: Vec<String> = self.list.iter().map(|p| p.display().to_string()).collect();
        let _ = std::fs::write(file, text.join("\n"));
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PopupOutcome {
    Handled,
    Close,
    Open(PathBuf),
}

#[derive(Debug, Default)]
pub struct RecentPopup {
    pub filter: String,
    pub selected: usize,
}

impl RecentPopup {
    /// Like IntelliJ, the previous file is preselected so Ctrl-E Enter toggles
    /// between the last two files.
    pub fn new(list_len: usize) -> RecentPopup {
        RecentPopup {
            filter: String::new(),
            selected: usize::from(list_len > 1),
        }
    }

    pub fn visible(&self, list: &[PathBuf]) -> Vec<PathBuf> {
        let f = self.filter.to_lowercase();
        list.iter()
            .filter(|p| f.is_empty() || p.to_string_lossy().to_lowercase().contains(&f))
            .cloned()
            .collect()
    }

    pub fn handle_key(&mut self, key: &KeyEvent, list: &[PathBuf]) -> PopupOutcome {
        let items = self.visible(list);
        match key.code {
            KeyCode::Esc => return PopupOutcome::Close,
            KeyCode::Enter => {
                return match items.get(self.selected) {
                    Some(p) => PopupOutcome::Open(p.clone()),
                    None => PopupOutcome::Close,
                };
            }
            KeyCode::Up => self.selected = self.selected.saturating_sub(1),
            KeyCode::Down => self.selected = (self.selected + 1).min(items.len().saturating_sub(1)),
            // Repeated Ctrl-E moves down the list.
            KeyCode::Char('e') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.selected = (self.selected + 1) % items.len().max(1);
            }
            KeyCode::Backspace => {
                self.filter.pop();
                self.selected = 0;
            }
            KeyCode::Char(c) if !key.modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) => {
                self.filter.push(c);
                self.selected = 0;
            }
            _ => {}
        }
        PopupOutcome::Handled
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recent_list_persists_and_dedupes() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("proj");
        std::fs::create_dir_all(&root).unwrap();
        let (a, b) = (root.join("a.txt"), root.join("b.txt"));
        std::fs::write(&a, "").unwrap();
        std::fs::write(&b, "").unwrap();
        let cache = dir.path().join("cache");
        let mut r = Recent::load(&cache, &root);
        r.push(&a);
        r.push(&b);
        r.push(&a);
        assert_eq!(r.list, vec![a.clone(), b.clone()]);
        let again = Recent::load(&cache, &root);
        assert_eq!(again.list, vec![a.clone(), b.clone()]);
        std::fs::remove_file(&b).unwrap();
        assert_eq!(Recent::load(&cache, &root).list, vec![a], "missing files are dropped");
    }

    #[test]
    fn popup_preselects_previous_and_filters() {
        let list = vec![
            PathBuf::from("/p/a.rs"),
            PathBuf::from("/p/b.md"),
            PathBuf::from("/p/c.rs"),
        ];
        let mut p = RecentPopup::new(list.len());
        assert_eq!(p.selected, 1);
        assert_eq!(
            p.handle_key(&KeyEvent::from(KeyCode::Enter), &list),
            PopupOutcome::Open(PathBuf::from("/p/b.md"))
        );
        p.handle_key(&KeyEvent::from(KeyCode::Char('r')), &list);
        p.handle_key(&KeyEvent::from(KeyCode::Char('s')), &list);
        assert_eq!(p.visible(&list).len(), 2);
        p.handle_key(&KeyEvent::from(KeyCode::Down), &list);
        assert_eq!(
            p.handle_key(&KeyEvent::from(KeyCode::Enter), &list),
            PopupOutcome::Open(PathBuf::from("/p/c.rs"))
        );
        assert_eq!(p.handle_key(&KeyEvent::from(KeyCode::Esc), &list), PopupOutcome::Close);
    }
}
