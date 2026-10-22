//! Diff tabs: HEAD ⇄ working tree, side by side or unified.

use std::path::{Path, PathBuf};

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use vista_diff::{DiffOptions, DisplayRow, SideBySide, Unified, fold, side_by_side, unified};
use vista_render::{Highlighter, StyledLine};

use crate::viewer::ImageState;

/// Which file a diff tab compares.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiffSpec {
    /// HEAD path for renames.
    pub orig_path: Option<PathBuf>,
}

pub enum DiffBody {
    Text,
    Binary { old: Option<u64>, new: Option<u64> },
    Image { old: ImageState, new: ImageState },
}

pub struct DiffDoc {
    pub old_text: String,
    pub new_text: String,
    pub old_hl: Vec<StyledLine>,
    pub new_hl: Vec<StyledLine>,
    pub opts: DiffOptions,
    pub unified: bool,
    pub collapse: bool,
    pub sbs: SideBySide,
    pub display: Vec<DisplayRow>,
    pub udisplay: Vec<Unified>,
    /// Short id of HEAD (column header).
    pub head: String,
    pub body: DiffBody,
}

/// Lines of context kept around changes when unchanged runs are collapsed.
pub const CONTEXT: usize = 4;

impl DiffDoc {
    pub fn text(old: String, new: String, path: &Path, head: String) -> DiffDoc {
        let hl = Highlighter::global();
        let old_hl = hl.highlight_file(path, &old);
        let new_hl = hl.highlight_file(path, &new);
        let mut d = DiffDoc {
            old_text: old,
            new_text: new,
            old_hl,
            new_hl,
            opts: DiffOptions::default(),
            unified: false,
            collapse: false,
            sbs: SideBySide::default(),
            display: Vec::new(),
            udisplay: Vec::new(),
            head,
            body: DiffBody::Text,
        };
        d.rebuild();
        d
    }

    /// Recomputes rows after an option change.
    pub fn rebuild(&mut self) {
        self.sbs = side_by_side(&self.old_text, &self.new_text, self.opts);
        self.display = if self.collapse {
            fold(&self.sbs, CONTEXT)
        } else {
            (0..self.sbs.rows.len()).map(DisplayRow::Row).collect()
        };
        self.udisplay = unified(&self.sbs, &self.display);
    }

    pub fn len(&self) -> usize {
        match self.body {
            DiffBody::Text if self.unified => self.udisplay.len(),
            DiffBody::Text => self.display.len(),
            _ => 0,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// The diff row behind a display row.
    fn row_of(&self, i: usize) -> Option<usize> {
        if self.unified {
            match self.udisplay.get(i)? {
                Unified::Both(r) | Unified::Old(r) | Unified::New(r) => Some(*r),
                Unified::Fold { .. } => None,
            }
        } else {
            match self.display.get(i)? {
                DisplayRow::Row(r) => Some(*r),
                DisplayRow::Fold { .. } => None,
            }
        }
    }

    /// Display indices where hunks start.
    pub fn hunk_starts(&self) -> Vec<usize> {
        let mut out = Vec::new();
        let mut last = None;
        for i in 0..self.len() {
            let h = self.row_of(i).and_then(|r| self.sbs.rows[r].hunk);
            if h.is_some() && h != last {
                out.push(i);
            }
            last = h;
        }
        out
    }

    /// (1-based number of the hunk at/after `cursor`, total hunks).
    pub fn hunk_position(&self, cursor: usize) -> (usize, usize) {
        let starts = self.hunk_starts();
        let n = starts.iter().take_while(|s| **s <= cursor).count();
        (n.max(1).min(starts.len()), starts.len())
    }

    /// The change (hunk id) at a display row, or the next one below it.
    pub fn hunk_at(&self, i: usize) -> Option<u64> {
        let h = self.row_of(i).and_then(|r| self.sbs.rows[r].hunk).or_else(|| {
            let starts = self.hunk_starts();
            let next = starts.iter().find(|s| **s >= i).or(starts.last())?;
            self.row_of(*next).and_then(|r| self.sbs.rows[r].hunk)
        })?;
        self.sbs.hunks.get(h).map(|x| x.id)
    }

    /// Display row index where the hunk `id` starts.
    pub fn hunk_start(&self, id: u64) -> Option<usize> {
        let idx = self.sbs.hunks.iter().position(|h| h.id == id)?;
        (0..self.len()).find(|i| self.row_of(*i).and_then(|r| self.sbs.rows[r].hunk) == Some(idx))
    }

    /// Width of the widest line on either side among `count` display rows
    /// from `from` (limits sideways scrolling).
    pub fn widest(&self, from: usize, count: usize) -> usize {
        let width = |lines: &[StyledLine], i: Option<usize>| {
            i.and_then(|i| lines.get(i))
                .map(vista_render::styled::line_width)
                .unwrap_or(0)
        };
        (from..(from + count).min(self.len()))
            .filter_map(|i| self.row_of(i))
            .map(|r| {
                let row = &self.sbs.rows[r];
                width(&self.old_hl, row.left).max(width(&self.new_hl, row.right))
            })
            .max()
            .unwrap_or(0)
    }

    /// The working-tree line (0-based) for a display row, if any.
    pub fn new_line_at(&self, i: usize) -> Option<usize> {
        let r = self.row_of(i)?;
        let row = &self.sbs.rows[r];
        row.right.or_else(|| {
            // Deleted lines: the nearest following line in the new text.
            self.sbs.rows[r..].iter().find_map(|x| x.right)
        })
    }
}

/// What a key in a diff tab asks for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiffKey {
    Handled,
    Unhandled,
    /// Open the working file at this 0-based line.
    OpenAt(usize),
    /// Show the next (true) or previous file's diff.
    File(bool),
    /// Tick/untick a change for the commit: (hunk id, all hunk ids).
    ToggleHunk(u64, Vec<u64>),
    /// Tell the user something.
    Info(&'static str),
}

/// Keys specific to diff tabs (scrolling is shared with the viewer).
pub fn handle_key(doc: &mut DiffDoc, key: &KeyEvent, cursor: usize) -> DiffKey {
    if key.modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) {
        return DiffKey::Unhandled;
    }
    match key.code {
        KeyCode::Char('u') => doc.unified = !doc.unified,
        KeyCode::Char('i') => {
            doc.opts.ignore_whitespace = !doc.opts.ignore_whitespace;
            doc.rebuild();
        }
        KeyCode::Char('z') => {
            doc.collapse = !doc.collapse;
            doc.rebuild();
        }
        KeyCode::Char(' ') => {
            if doc.opts.ignore_whitespace {
                return DiffKey::Info("Turn off ignore-whitespace (i) to pick changes");
            }
            return match doc.hunk_at(cursor) {
                Some(id) => DiffKey::ToggleHunk(id, doc.sbs.hunks.iter().map(|h| h.id).collect()),
                None => DiffKey::Info("No change here"),
            };
        }
        KeyCode::Char('J') => return DiffKey::File(true),
        KeyCode::Char('K') => return DiffKey::File(false),
        KeyCode::Enter => {
            return match doc.new_line_at(cursor) {
                Some(l) => DiffKey::OpenAt(l),
                None => DiffKey::Handled,
            };
        }
        _ => return DiffKey::Unhandled,
    }
    DiffKey::Handled
}

/// Result of loading a diff in the background.
/// Decoded (HEAD, working tree) images; `None` where the file is absent.
pub type ImagePair = (Option<Box<image::DynamicImage>>, Option<Box<image::DynamicImage>>);

pub struct DiffLoad {
    pub doc: Option<DiffDoc>,
    pub images: Option<ImagePair>,
    pub error: Option<String>,
}

/// Reads HEAD and working-tree content and builds the diff (worker thread).
pub fn load(git: Option<&vista_git::Git>, path: &Path, spec: &DiffSpec, images_enabled: bool) -> DiffLoad {
    let Some(git) = git else {
        return DiffLoad {
            doc: None,
            images: None,
            error: Some("Not a git repository".into()),
        };
    };
    let head_path = spec.orig_path.as_deref().unwrap_or(path);
    let old = git.head_content(head_path);
    let new = std::fs::read(path).ok();
    let head = git
        .run(&["rev-parse", "--short", "HEAD"])
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|_| "(no commits)".into());
    if vista_render::filetype::is_image_path(path) {
        let dec = |b: &Option<Vec<u8>>| b.as_ref().and_then(|b| image::load_from_memory(b).ok()).map(Box::new);
        let body = DiffBody::Image {
            old: if images_enabled {
                ImageState::Loading
            } else {
                ImageState::Disabled
            },
            new: if images_enabled {
                ImageState::Loading
            } else {
                ImageState::Disabled
            },
        };
        return DiffLoad {
            doc: Some(DiffDoc {
                body,
                ..DiffDoc::text(String::new(), String::new(), path, head)
            }),
            images: images_enabled.then(|| (dec(&old), dec(&new))),
            error: None,
        };
    }
    let binary = |b: &Option<Vec<u8>>| b.as_ref().is_some_and(|b| vista_render::filetype::looks_binary(b));
    if binary(&old) || binary(&new) {
        let size = |b: &Option<Vec<u8>>| b.as_ref().map(|b| b.len() as u64);
        return DiffLoad {
            doc: Some(DiffDoc {
                body: DiffBody::Binary {
                    old: size(&old),
                    new: size(&new),
                },
                ..DiffDoc::text(String::new(), String::new(), path, head)
            }),
            images: None,
            error: None,
        };
    }
    let text = |b: Option<Vec<u8>>| b.map(|b| String::from_utf8_lossy(&b).into_owned()).unwrap_or_default();
    DiffLoad {
        doc: Some(DiffDoc::text(text(old), text(new), path, head)),
        images: None,
        error: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn doc(old: &str, new: &str) -> DiffDoc {
        DiffDoc::text(old.into(), new.into(), Path::new("a.rs"), "abc1234".into())
    }

    #[test]
    fn hunk_navigation_and_positions() {
        let old: String = (0..20).map(|i| format!("line{i}\n")).collect();
        let new = old.replace("line3\n", "three\n").replace("line15\n", "fifteen\n");
        let d = doc(&old, &new);
        assert_eq!(d.hunk_starts(), vec![3, 15]);
        assert_eq!(d.hunk_position(0), (1, 2));
        assert_eq!(d.hunk_position(15), (2, 2));
        assert_eq!(d.new_line_at(3), Some(3));
    }

    #[test]
    fn toggles_rebuild_rows() {
        let old: String = (0..30).map(|i| format!("{i}\n")).collect();
        let new = old.replace("15\n", "  fifteen\n");
        let mut d = doc(&old, &new);
        let full = d.len();
        handle_key(&mut d, &KeyEvent::from(KeyCode::Char('z')), 0);
        assert!(d.len() < full, "collapsed");
        handle_key(&mut d, &KeyEvent::from(KeyCode::Char('u')), 0);
        assert!(d.unified);
        assert_eq!(d.hunk_starts().len(), 1);
        assert_eq!(
            handle_key(&mut d, &KeyEvent::from(KeyCode::Char('J')), 0),
            DiffKey::File(true)
        );
    }

    #[test]
    fn whitespace_toggle_hides_indent_only_changes() {
        let mut d = doc("a\n  b\n", "a\n    b\n");
        assert_eq!(d.hunk_starts().len(), 1);
        handle_key(&mut d, &KeyEvent::from(KeyCode::Char('i')), 0);
        assert!(d.hunk_starts().is_empty());
    }
}
