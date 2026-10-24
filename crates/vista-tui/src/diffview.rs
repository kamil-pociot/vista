//! Diff tabs: HEAD ⇄ working tree, side by side or unified.

use std::path::{Path, PathBuf};

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use vista_diff::{DiffOptions, DisplayRow, SideBySide, Unified, fold, side_by_side, unified};
use vista_render::{Highlighter, StyledLine};

use crate::viewer::ImageState;

/// Which file a diff tab compares.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DiffSpec {
    /// HEAD path for renames (the parent's path, for history diffs).
    pub orig_path: Option<PathBuf>,
    /// A commit from the history (read-only); `None` is HEAD ⇄ working tree.
    pub revs: Option<Revs>,
    /// A review's base ⇄ the working tree (read-only).
    pub base: Option<Base>,
}

/// A review diff: this revision ⇄ the working tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Base {
    pub hash: String,
    pub short: String,
}

impl DiffSpec {
    /// History and review diffs can't be ticked for a commit or rolled back.
    pub fn read_only(&self) -> bool {
        self.revs.is_some() || self.base.is_some()
    }

    /// Whether `other` compares the same revisions (the same tab).
    pub fn same_kind(&self, other: &DiffSpec) -> bool {
        self.revs == other.revs && self.base == other.base
    }
}

/// A history diff: the commit's parent ⇄ the commit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Revs {
    /// First parent (none for a root commit).
    pub parent: Option<String>,
    pub commit: String,
    pub short: String,
    pub subject: String,
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
    /// Column headers: (left, right).
    pub sides: (String, String),
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
            sides: (format!("HEAD · {head} (read-only)"), "Working tree".into()),
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

    /// Screen rows each display row takes when wrapped: side by side the
    /// taller side wins (`left`/`right` text columns); unified uses `cols`.
    pub fn wrapped_heights(&self, left: usize, right: usize, cols: usize) -> Vec<usize> {
        use crate::viewer::wrapped_rows;
        let width = |lines: &[StyledLine], i: Option<usize>| {
            i.and_then(|i| lines.get(i))
                .map(vista_render::styled::line_width)
                .unwrap_or(0)
        };
        if self.unified {
            self.udisplay
                .iter()
                .map(|u| match *u {
                    Unified::Fold { .. } => 1,
                    Unified::Old(r) => wrapped_rows(width(&self.old_hl, self.sbs.rows[r].left), cols),
                    Unified::Both(r) | Unified::New(r) => {
                        wrapped_rows(width(&self.new_hl, self.sbs.rows[r].right), cols)
                    }
                })
                .collect()
        } else {
            self.display
                .iter()
                .map(|d| match *d {
                    DisplayRow::Fold { .. } => 1,
                    DisplayRow::Row(r) => {
                        let row = &self.sbs.rows[r];
                        wrapped_rows(width(&self.old_hl, row.left), left)
                            .max(wrapped_rows(width(&self.new_hl, row.right), right))
                    }
                })
                .collect()
        }
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

    /// Which side Go to Line counts lines on, and how many it has: the
    /// working tree's, or HEAD's when the file is gone.
    pub fn side_for_lines(&self) -> (bool, usize) {
        let (old, new) = (self.old_hl.len(), self.new_hl.len());
        if new == 0 && old > 0 { (false, old) } else { (true, new) }
    }

    /// The HEAD line (0-based) for a display row, if any.
    pub fn old_line_at(&self, i: usize) -> Option<usize> {
        let r = self.row_of(i)?;
        self.sbs.rows[r..].iter().find_map(|x| x.left)
    }

    /// The display row showing 0-based `line` of the working tree (`new`)
    /// or of HEAD; `None` when it is folded away.
    pub fn display_row_of(&self, line: usize, new: bool) -> Option<usize> {
        let side = |r: usize| {
            let row = &self.sbs.rows[r];
            if new { row.right } else { row.left }
        };
        (0..self.len()).find(|&i| {
            if self.unified {
                // A changed line shows twice: HEAD's (Old) and the new one.
                match self.udisplay[i] {
                    Unified::Both(r) => side(r) == Some(line),
                    Unified::New(r) => new && side(r) == Some(line),
                    Unified::Old(r) => !new && side(r) == Some(line),
                    Unified::Fold { .. } => false,
                }
            } else {
                self.row_of(i).is_some_and(|r| side(r) == Some(line))
            }
        })
    }

    /// The HEAD line (0-based) of a display row that exists only in HEAD.
    pub fn head_only_line_at(&self, i: usize) -> Option<usize> {
        let row = &self.sbs.rows[self.row_of(i)?];
        if row.right.is_none() { row.left } else { None }
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
    let (old, new, head, sides) = match (&spec.revs, &spec.base) {
        // A review: the base (fixed) ⇄ the working tree as it is now.
        (_, Some(b)) => {
            let sides = (
                format!("{} · review base (read-only)", b.short),
                "Working tree".to_string(),
            );
            (
                git.content_at(&b.hash, head_path),
                std::fs::read(path).ok(),
                b.short.clone(),
                sides,
            )
        }
        // From the history: the commit's parent ⇄ the commit, both from git.
        (Some(r), None) => {
            let old = r.parent.as_deref().and_then(|p| git.content_at(p, head_path));
            let new = git.content_at(&r.commit, path);
            let parent = r.parent.as_deref().map(|p| &p[..p.len().min(7)]).unwrap_or("(none)");
            let sides = (
                format!("{parent} · parent (read-only)"),
                format!("{} · {} (read-only)", r.short, r.subject),
            );
            (old, new, r.short.clone(), sides)
        }
        (None, None) => {
            let head = git
                .run(&["rev-parse", "--short", "HEAD"])
                .map(|s| s.trim().to_string())
                .unwrap_or_else(|_| "(no commits)".into());
            let sides = (format!("HEAD · {head} (read-only)"), "Working tree".to_string());
            (git.head_content(head_path), std::fs::read(path).ok(), head, sides)
        }
    };
    let mut load = build(path, old, new, head, images_enabled);
    if let Some(doc) = load.doc.as_mut() {
        doc.sides = sides;
    }
    load
}

/// The diff of `old` ⇄ `new` (text, image or binary).
fn build(path: &Path, old: Option<Vec<u8>>, new: Option<Vec<u8>>, head: String, images_enabled: bool) -> DiffLoad {
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
    fn a_review_diff_is_the_base_against_the_working_tree() {
        let repo = vista_e2e::Fixture::git()
            .file("a.txt", "one\n")
            .file("b.txt", "bee\n")
            .file("gone.txt", "bye\n")
            .file("bin.dat", [0u8, 1, 2])
            .commit("base")
            .run(&["mv", "b.txt", "c.txt"])
            .delete("gone.txt")
            .commit("later")
            .file("a.txt", "one\ntwo\n")
            .file("new.txt", "hi\n")
            .file("bin.dat", [0u8, 1, 2, 3])
            .build();
        // Canonical, as the app's paths are (/var is /private/var on macOS).
        let root = repo.path().canonicalize().unwrap();
        let git = vista_git::Git::discover(&root).unwrap();
        let hash = vista_git::review::resolve(&git, "HEAD~1").unwrap();
        let spec = |orig: Option<&str>| DiffSpec {
            orig_path: orig.map(|o| root.join(o)),
            revs: None,
            base: Some(Base {
                short: hash[..7].to_string(),
                hash: hash.clone(),
            }),
        };
        let text = |rel: &str, orig: Option<&str>| {
            let doc = load(Some(&git), &root.join(rel), &spec(orig), false).doc.unwrap();
            (doc.old_text, doc.new_text, doc.sides.0)
        };
        // Modified (uncommitted), renamed (committed), added, deleted.
        let (old, new, left) = text("a.txt", None);
        assert_eq!((old.as_str(), new.as_str()), ("one\n", "one\ntwo\n"));
        assert!(left.contains("review base"), "{left}");
        assert_eq!(text("c.txt", Some("b.txt")).0, "bee\n");
        assert_eq!(text("new.txt", None).0, "");
        assert_eq!(text("gone.txt", None), ("bye\n".into(), String::new(), left.clone()));
        let bin = load(Some(&git), &root.join("bin.dat"), &spec(None), false).doc.unwrap();
        assert!(matches!(
            bin.body,
            DiffBody::Binary {
                old: Some(3),
                new: Some(4)
            }
        ));
        assert!(spec(None).read_only() && !spec(None).same_kind(&DiffSpec::default()));
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
    fn finds_the_display_row_of_a_file_line() {
        // Line 2 changed, a line inserted after line 5.
        let old: String = (0..10).map(|i| format!("l{i}\n")).collect();
        let new = old.replace("l2\n", "two\n").replace("l5\n", "l5\nadded\n");
        let mut d = doc(&old, &new);
        assert_eq!(d.side_for_lines(), (true, 11));
        assert_eq!(d.display_row_of(2, true), Some(2));
        assert_eq!(d.display_row_of(6, true), Some(6), "the inserted line");
        assert_eq!(d.display_row_of(7, true), Some(7));
        assert_eq!(d.display_row_of(6, false), Some(7), "HEAD's l6 sits beside the new l6");
        assert_eq!(d.old_line_at(6), Some(6), "the next HEAD line below an insertion");
        // Unified: the changed line shows twice, HEAD's first.
        d.unified = true;
        assert_eq!(d.display_row_of(2, false), Some(2));
        assert_eq!(d.display_row_of(2, true), Some(3));
        // Folded away: not shown.
        d.collapse = true;
        d.rebuild();
        assert_eq!(d.display_row_of(10, true).map(|i| d.new_line_at(i)), Some(Some(10)));
        let far: String = (0..60).map(|i| format!("l{i}\n")).collect();
        let mut d = doc(&far, &far.replace("l1\n", "one\n"));
        d.collapse = true;
        d.rebuild();
        assert_eq!(d.display_row_of(40, true), None);
        // A deleted file counts HEAD's lines.
        assert_eq!(doc("a\nb\n", "").side_for_lines(), (false, 2));
    }

    #[test]
    fn whitespace_toggle_hides_indent_only_changes() {
        let mut d = doc("a\n  b\n", "a\n    b\n");
        assert_eq!(d.hunk_starts().len(), 1);
        handle_key(&mut d, &KeyEvent::from(KeyCode::Char('i')), 0);
        assert!(d.hunk_starts().is_empty());
    }
}
