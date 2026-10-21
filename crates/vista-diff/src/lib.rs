//! Diffs for vista: gutter markers for the viewer, and the aligned
//! side-by-side model for the IntelliJ-style diff viewer.

use std::collections::HashSet;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::ops::Range;

use similar::{Algorithm, DiffOp, TextDiff, capture_diff_slices};

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

// ----- side-by-side --------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct DiffOptions {
    /// Treat lines that differ only in whitespace as equal.
    pub ignore_whitespace: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RowKind {
    Context,
    /// Only on the right.
    Added,
    /// Only on the left.
    Deleted,
    /// Both sides, different.
    Modified,
}

/// One aligned row of a side-by-side diff.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    pub kind: RowKind,
    /// 0-based line index in the old text.
    pub left: Option<usize>,
    /// 0-based line index in the new text.
    pub right: Option<usize>,
    /// Index into [`SideBySide::hunks`] for changed rows.
    pub hunk: Option<usize>,
    /// Changed words, as char ranges (Modified rows only).
    pub left_words: Vec<(usize, usize)>,
    pub right_words: Vec<(usize, usize)>,
}

/// A block of consecutive changes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hunk {
    /// Stable across recomputation while the hunk's content is unchanged.
    pub id: u64,
    pub old: Range<usize>,
    pub new: Range<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SideBySide {
    pub old_lines: Vec<String>,
    pub new_lines: Vec<String>,
    pub rows: Vec<Row>,
    pub hunks: Vec<Hunk>,
}

fn split_lines(s: &str) -> Vec<String> {
    s.lines().map(str::to_string).collect()
}

fn normalize(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn hunk_id(old: &[String], new: &[String], old_start: usize) -> u64 {
    let mut h = DefaultHasher::new();
    old_start.hash(&mut h);
    old.hash(&mut h);
    new.hash(&mut h);
    h.finish()
}

/// Changed-word ranges (char indices) between two lines.
/// Changed char ranges on the (old, new) side.
pub type WordRanges = (Vec<(usize, usize)>, Vec<(usize, usize)>);

pub fn word_ranges(old: &str, new: &str) -> WordRanges {
    let (ot, nt) = (tokens(old), tokens(new));
    let ok: Vec<&str> = ot.iter().map(|t| t.0).collect();
    let nk: Vec<&str> = nt.iter().map(|t| t.0).collect();
    let (mut l, mut r) = (Vec::new(), Vec::new());
    for op in capture_diff_slices(Algorithm::Myers, &ok, &nk) {
        let (a, b) = (op.old_range(), op.new_range());
        if matches!(op, DiffOp::Equal { .. }) {
            continue;
        }
        for t in &ot[a] {
            if !t.0.trim().is_empty() {
                l.push((t.1, t.1 + t.0.chars().count()));
            }
        }
        for t in &nt[b] {
            if !t.0.trim().is_empty() {
                r.push((t.1, t.1 + t.0.chars().count()));
            }
        }
    }
    (merge(l), merge(r))
}

/// Splits a line into identifier runs, whitespace runs and single
/// punctuation characters, with each token's char offset.
fn tokens(s: &str) -> Vec<(&str, usize)> {
    #[derive(PartialEq)]
    enum K {
        Word,
        Space,
        Punct,
    }
    let kind = |c: char| {
        if c.is_alphanumeric() || c == '_' {
            K::Word
        } else if c.is_whitespace() {
            K::Space
        } else {
            K::Punct
        }
    };
    let mut out = Vec::new();
    let mut start_byte = 0;
    let mut start_char = 0;
    let mut prev: Option<K> = None;
    for (ci, (bi, c)) in s.char_indices().enumerate() {
        let k = kind(c);
        let boundary = match &prev {
            None => false,
            Some(p) => *p != k || k == K::Punct,
        };
        if boundary {
            out.push((&s[start_byte..bi], start_char));
            start_byte = bi;
            start_char = ci;
        }
        prev = Some(k);
    }
    if start_byte < s.len() {
        out.push((&s[start_byte..], start_char));
    }
    out
}

fn merge(mut v: Vec<(usize, usize)>) -> Vec<(usize, usize)> {
    v.sort_unstable();
    let mut out: Vec<(usize, usize)> = Vec::new();
    for (a, b) in v {
        match out.last_mut() {
            Some(last) if a <= last.1 => last.1 = last.1.max(b),
            _ => out.push((a, b)),
        }
    }
    out
}

/// Aligns `old` and `new` for side-by-side display.
pub fn side_by_side(old: &str, new: &str, opts: DiffOptions) -> SideBySide {
    let old_lines = split_lines(old);
    let new_lines = split_lines(new);
    let key = |l: &String| {
        if opts.ignore_whitespace {
            normalize(l)
        } else {
            l.clone()
        }
    };
    let ok: Vec<String> = old_lines.iter().map(key).collect();
    let nk: Vec<String> = new_lines.iter().map(key).collect();
    let ops = capture_diff_slices(Algorithm::Patience, &ok, &nk);
    let mut rows = Vec::new();
    let mut hunks: Vec<Hunk> = Vec::new();
    let mut open: Option<usize> = None;
    // Running positions on both sides. `similar` reports the opposite side's
    // index of a pure delete/insert in ways that don't always mean "here", so
    // hunk ranges are derived from these counters instead.
    let (mut o_pos, mut n_pos) = (0usize, 0usize);
    let hunk_at = |hunks: &mut Vec<Hunk>, open: &mut Option<usize>, o: usize, n: usize| -> usize {
        if let Some(h) = *open {
            return h;
        }
        hunks.push(Hunk {
            id: 0,
            old: o..o,
            new: n..n,
        });
        *open = Some(hunks.len() - 1);
        hunks.len() - 1
    };
    let blank = |kind, left, right, hunk| Row {
        kind,
        left,
        right,
        hunk,
        left_words: Vec::new(),
        right_words: Vec::new(),
    };
    for op in &ops {
        match *op {
            DiffOp::Equal { len, .. } => {
                open = None;
                for k in 0..len {
                    rows.push(blank(RowKind::Context, Some(o_pos + k), Some(n_pos + k), None));
                }
                o_pos += len;
                n_pos += len;
            }
            DiffOp::Delete { old_len, .. } => {
                let h = hunk_at(&mut hunks, &mut open, o_pos, n_pos);
                for k in 0..old_len {
                    rows.push(blank(RowKind::Deleted, Some(o_pos + k), None, Some(h)));
                }
                o_pos += old_len;
                hunks[h].old.end = o_pos;
                hunks[h].new.end = n_pos;
            }
            DiffOp::Insert { new_len, .. } => {
                let h = hunk_at(&mut hunks, &mut open, o_pos, n_pos);
                for k in 0..new_len {
                    rows.push(blank(RowKind::Added, None, Some(n_pos + k), Some(h)));
                }
                n_pos += new_len;
                hunks[h].old.end = o_pos;
                hunks[h].new.end = n_pos;
            }
            DiffOp::Replace { old_len, new_len, .. } => {
                let h = hunk_at(&mut hunks, &mut open, o_pos, n_pos);
                for k in 0..old_len.max(new_len) {
                    let (l, r) = ((k < old_len).then_some(o_pos + k), (k < new_len).then_some(n_pos + k));
                    match (l, r) {
                        (Some(a), Some(b)) => {
                            let (lw, rw) = word_ranges(&old_lines[a], &new_lines[b]);
                            rows.push(Row {
                                kind: RowKind::Modified,
                                left: l,
                                right: r,
                                hunk: Some(h),
                                left_words: lw,
                                right_words: rw,
                            });
                        }
                        (Some(_), None) => rows.push(blank(RowKind::Deleted, l, None, Some(h))),
                        _ => rows.push(blank(RowKind::Added, None, r, Some(h))),
                    }
                }
                o_pos += old_len;
                n_pos += new_len;
                hunks[h].old.end = o_pos;
                hunks[h].new.end = n_pos;
            }
        }
    }
    for h in &mut hunks {
        h.id = hunk_id(&old_lines[h.old.clone()], &new_lines[h.new.clone()], h.old.start);
    }
    SideBySide {
        old_lines,
        new_lines,
        rows,
        hunks,
    }
}

impl SideBySide {
    /// Row index where each hunk starts.
    pub fn hunk_starts(&self) -> Vec<usize> {
        let mut out = Vec::new();
        let mut last = None;
        for (i, r) in self.rows.iter().enumerate() {
            if r.hunk.is_some() && r.hunk != last {
                out.push(i);
            }
            last = r.hunk;
        }
        out
    }

    /// Old lines as shown on the left, in order.
    pub fn left_lines(&self) -> Vec<&str> {
        self.rows
            .iter()
            .filter_map(|r| r.left.map(|i| self.old_lines[i].as_str()))
            .collect()
    }

    pub fn right_lines(&self) -> Vec<&str> {
        self.rows
            .iter()
            .filter_map(|r| r.right.map(|i| self.new_lines[i].as_str()))
            .collect()
    }

    /// The new text with only the `selected` hunks (by id) applied to the old
    /// text — what a partial commit records.
    pub fn apply_hunks(&self, selected: &HashSet<u64>) -> Vec<String> {
        let mut out = Vec::new();
        let mut old_pos = 0;
        for h in &self.hunks {
            out.extend(self.old_lines[old_pos..h.old.start].iter().cloned());
            if selected.contains(&h.id) {
                out.extend(self.new_lines[h.new.clone()].iter().cloned());
            } else {
                out.extend(self.old_lines[h.old.clone()].iter().cloned());
            }
            old_pos = h.old.end;
        }
        out.extend(self.old_lines[old_pos..].iter().cloned());
        out
    }
}

/// What the viewer shows: a row of the diff or a folded run of context.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DisplayRow {
    Row(usize),
    Fold { start: usize, len: usize },
}

/// Folds context runs longer than `2 * context + 1` lines.
pub fn fold(sbs: &SideBySide, context: usize) -> Vec<DisplayRow> {
    let mut out = Vec::new();
    let n = sbs.rows.len();
    let mut i = 0;
    while i < n {
        if sbs.rows[i].kind != RowKind::Context {
            out.push(DisplayRow::Row(i));
            i += 1;
            continue;
        }
        let start = i;
        while i < n && sbs.rows[i].kind == RowKind::Context {
            i += 1;
        }
        let run = i - start;
        let keep_before = if start == 0 { 0 } else { context };
        let keep_after = if i == n { 0 } else { context };
        if run > keep_before + keep_after + 1 {
            for k in start..start + keep_before {
                out.push(DisplayRow::Row(k));
            }
            out.push(DisplayRow::Fold {
                start: start + keep_before,
                len: run - keep_before - keep_after,
            });
            for k in i - keep_after..i {
                out.push(DisplayRow::Row(k));
            }
        } else {
            for k in start..i {
                out.push(DisplayRow::Row(k));
            }
        }
    }
    out
}

/// A line of the unified view.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Unified {
    Both(usize),
    Old(usize),
    New(usize),
    Fold { len: usize },
}

/// The unified view of display rows: modified rows become `-` then `+`.
pub fn unified(sbs: &SideBySide, display: &[DisplayRow]) -> Vec<Unified> {
    let mut out = Vec::new();
    let mut pending_new = Vec::new();
    let flush = |out: &mut Vec<Unified>, pending: &mut Vec<usize>| {
        out.extend(pending.drain(..).map(Unified::New));
    };
    for d in display {
        match *d {
            DisplayRow::Fold { len, .. } => {
                flush(&mut out, &mut pending_new);
                out.push(Unified::Fold { len });
            }
            DisplayRow::Row(i) => match sbs.rows[i].kind {
                RowKind::Context => {
                    flush(&mut out, &mut pending_new);
                    out.push(Unified::Both(i));
                }
                RowKind::Deleted => out.push(Unified::Old(i)),
                RowKind::Added => pending_new.push(i),
                RowKind::Modified => {
                    out.push(Unified::Old(i));
                    pending_new.push(i);
                }
            },
        }
    }
    flush(&mut out, &mut pending_new);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

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

    #[test]
    fn aligns_modified_added_and_deleted_blocks() {
        let old = "keep\nlet root = repo.work_dir();\ngone\nend\n";
        let new = "keep\nlet root = repo.workdir();\nend\nadded\n";
        let s = side_by_side(old, new, DiffOptions::default());
        let kinds: Vec<RowKind> = s.rows.iter().map(|r| r.kind).collect();
        assert_eq!(
            kinds,
            vec![
                RowKind::Context,
                RowKind::Modified,
                RowKind::Deleted,
                RowKind::Context,
                RowKind::Added
            ]
        );
        let m = &s.rows[1];
        // "work_dir" vs "workdir": the changed words are highlighted on both sides.
        assert!(!m.left_words.is_empty() && !m.right_words.is_empty());
        let marked = |line: &str, ranges: &[(usize, usize)]| -> Vec<String> {
            ranges
                .iter()
                .map(|(a, b)| line.chars().skip(*a).take(b - a).collect())
                .collect()
        };
        assert_eq!(marked(&s.old_lines[1], &m.left_words), vec!["work_dir"]);
        assert_eq!(marked(&s.new_lines[1], &m.right_words), vec!["workdir"]);
        assert_eq!(s.hunks.len(), 2);
        assert_eq!(s.hunk_starts(), vec![1, 4]);
    }

    #[test]
    fn ignore_whitespace_hides_indentation_changes() {
        let old = "fn a() {\n  x();\n}\n";
        let new = "fn a() {\n    x();\n}\n";
        assert_eq!(side_by_side(old, new, DiffOptions::default()).hunks.len(), 1);
        let s = side_by_side(
            old,
            new,
            DiffOptions {
                ignore_whitespace: true,
            },
        );
        assert!(s.hunks.is_empty());
        assert_eq!(s.right_lines()[1], "    x();", "the shown text is the real text");
    }

    #[test]
    fn hunk_ids_are_stable_when_other_hunks_change() {
        let old = "a\nb\nc\nd\ne\nf\ng\n";
        let s1 = side_by_side(old, "a\nB\nc\nd\ne\nF\ng\n", DiffOptions::default());
        let s2 = side_by_side(old, "a\nB\nc\nd\ne\nFF\ng\n", DiffOptions::default());
        assert_eq!(s1.hunks[0].id, s2.hunks[0].id);
        assert_ne!(s1.hunks[1].id, s2.hunks[1].id);
    }

    #[test]
    fn applies_selected_hunks_only() {
        let old = "a\nb\nc\nd\ne\n";
        let new = "a\nB\nc\nd\nE\nnew\n";
        let s = side_by_side(old, new, DiffOptions::default());
        let first: HashSet<u64> = [s.hunks[0].id].into();
        assert_eq!(s.apply_hunks(&first), vec!["a", "B", "c", "d", "e"]);
        let all: HashSet<u64> = s.hunks.iter().map(|h| h.id).collect();
        assert_eq!(s.apply_hunks(&all), split_lines(new));
        assert_eq!(s.apply_hunks(&HashSet::new()), split_lines(old));
    }

    #[test]
    fn folds_long_context_runs() {
        let old: String = (0..30).map(|i| format!("{i}\n")).collect();
        let new = old.replace("15\n", "fifteen\n");
        let s = side_by_side(&old, &new, DiffOptions::default());
        let d = fold(&s, 3);
        assert!(matches!(d[0], DisplayRow::Fold { start: 0, len: 12 }));
        assert!(matches!(d.last(), Some(DisplayRow::Fold { len: 11, .. })));
        assert_eq!(d.iter().filter(|r| matches!(r, DisplayRow::Row(_))).count(), 7);
    }

    #[test]
    fn unified_view_orders_old_before_new() {
        let s = side_by_side("a\nb\nc\n", "a\nB\nc\n", DiffOptions::default());
        let d: Vec<DisplayRow> = (0..s.rows.len()).map(DisplayRow::Row).collect();
        assert_eq!(
            unified(&s, &d),
            vec![Unified::Both(0), Unified::Old(1), Unified::New(1), Unified::Both(2)]
        );
    }

    proptest! {
        /// Aligned rows always reconstruct both sides exactly, and applying
        /// every hunk to the old text yields the new text.
        #[test]
        fn rows_reconstruct_both_sides(
            old in proptest::collection::vec("[abc ]{0,4}", 0..30),
            new in proptest::collection::vec("[abc ]{0,4}", 0..30),
            ws in any::<bool>(),
        ) {
            let (o, n) = (old.join("\n"), new.join("\n"));
            let s = side_by_side(&o, &n, DiffOptions { ignore_whitespace: ws });
            prop_assert_eq!(s.left_lines(), s.old_lines.iter().map(String::as_str).collect::<Vec<_>>());
            prop_assert_eq!(s.right_lines(), s.new_lines.iter().map(String::as_str).collect::<Vec<_>>());
            if !ws {
                let all: HashSet<u64> = s.hunks.iter().map(|h| h.id).collect();
                prop_assert_eq!(s.apply_hunks(&all), s.new_lines.clone());
            }
            // Folding never loses rows: shown + folded = all.
            let d = fold(&s, 2);
            let count: usize = d.iter().map(|r| match r { DisplayRow::Row(_) => 1, DisplayRow::Fold { len, .. } => *len }).sum();
            prop_assert_eq!(count, s.rows.len());
        }
    }
}
