//! Diffs for vista: gutter markers for the viewer, and (from M7) the aligned
//! side-by-side model for the diff viewer.

use similar::{DiffOp, TextDiff};

/// How a line of the new text differs from the old text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LineMark {
    #[default]
    None,
    Added,
    Modified,
}

/// Per-line gutter information for the new text.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Markers {
    /// One entry per line of the new text.
    pub lines: Vec<LineMark>,
    /// `deleted_before[i]`: lines were removed just above new line `i`
    /// (index `lines.len()` means at the end of the file).
    pub deleted_before: Vec<bool>,
}

impl Markers {
    pub fn mark(&self, line: usize) -> LineMark {
        self.lines.get(line).copied().unwrap_or_default()
    }

    pub fn deleted_above(&self, line: usize) -> bool {
        self.deleted_before.get(line).copied().unwrap_or(false)
    }

    pub fn is_clean(&self) -> bool {
        self.lines.iter().all(|m| *m == LineMark::None) && !self.deleted_before.iter().any(|d| *d)
    }

    /// Line indices where a change block starts (for next/previous change).
    pub fn change_starts(&self) -> Vec<usize> {
        let mut out = Vec::new();
        let mut prev = LineMark::None;
        for (i, m) in self.lines.iter().enumerate() {
            let starts_block = *m != LineMark::None && prev == LineMark::None;
            if starts_block || (self.deleted_above(i) && *m == LineMark::None) {
                out.push(i);
            }
            prev = *m;
        }
        if self.deleted_above(self.lines.len()) {
            out.push(self.lines.len().saturating_sub(1));
        }
        out.dedup();
        out
    }
}

/// Gutter markers for `new` compared with `old` (both full file texts).
pub fn line_markers(old: &str, new: &str) -> Markers {
    let diff = TextDiff::from_lines(old, new);
    let n = new.lines().count().max(if new.is_empty() { 0 } else { 1 });
    let mut m = Markers {
        lines: vec![LineMark::None; n],
        deleted_before: vec![false; n + 1],
    };
    for op in diff.ops() {
        match *op {
            DiffOp::Equal { .. } => {}
            DiffOp::Insert { new_index, new_len, .. } => {
                for i in new_index..(new_index + new_len).min(n) {
                    m.lines[i] = LineMark::Added;
                }
            }
            DiffOp::Delete { new_index, .. } => {
                m.deleted_before[new_index.min(n)] = true;
            }
            DiffOp::Replace { new_index, new_len, .. } => {
                for i in new_index..(new_index + new_len).min(n) {
                    m.lines[i] = LineMark::Modified;
                }
            }
        }
    }
    m
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn marks_added_modified_and_deleted_lines() {
        let old = "a\nb\nc\nd\n";
        let new = "a\nB\nc\nnew\n";
        let m = line_markers(old, new);
        assert_eq!(
            m.lines,
            vec![LineMark::None, LineMark::Modified, LineMark::None, LineMark::Modified]
        );
        let m = line_markers("a\nb\nc\n", "a\nx\nb\nc\n");
        assert_eq!(m.lines[1], LineMark::Added);
        let m = line_markers("a\nb\nc\n", "a\nc\n");
        assert!(m.deleted_above(1));
        assert_eq!(m.lines, vec![LineMark::None, LineMark::None]);
    }

    #[test]
    fn clean_and_new_files() {
        assert!(line_markers("x\n", "x\n").is_clean());
        let m = line_markers("", "a\nb\n");
        assert_eq!(m.lines, vec![LineMark::Added, LineMark::Added]);
    }

    #[test]
    fn change_starts_for_navigation() {
        let old = "1\n2\n3\n4\n5\n6\n";
        let new = "1\nX\n3\n4\nY\nZ\n";
        let m = line_markers(old, new);
        assert_eq!(m.change_starts(), vec![1, 4]);
    }
}
