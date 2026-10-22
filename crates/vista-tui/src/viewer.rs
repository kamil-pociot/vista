//! Open files: tabs, loading, scrolling, search. Drawing is in `ui::viewer`.

use std::collections::HashMap;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::mpsc::Sender;

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui_image::protocol::StatefulProtocol;
use vista_diff::{Markers, line_markers};
use vista_git::Git;
use vista_render::filetype::{self, FileKind};
use vista_render::highlight::plain_lines;
use vista_render::markdown::MdLine;
use vista_render::styled::line_text;
use vista_render::{Highlighter, StyledLine};

use crate::diffview::{DiffDoc, DiffKey, DiffSpec};
use crate::event::AppEvent;

/// At most this many tabs stay open; the least recently used one closes.
pub const MAX_TABS: usize = 12;

pub enum ImageState {
    Loading,
    Ready(Box<StatefulProtocol>),
    Failed(String),
    /// Images are off (`[images] protocol = "off"`).
    Disabled,
}

pub enum Content {
    Text {
        lines: Vec<StyledLine>,
        markers: Markers,
        highlighted: bool,
    },
    Markdown {
        source: String,
        lines: Vec<StyledLine>,
        markers: Markers,
        raw: bool,
        /// Rendered for a width and table mode: (width, wide tables, lines).
        rendered: Option<(u16, bool, Vec<MdLine>)>,
    },
    Image {
        info: String,
        state: ImageState,
    },
    Binary {
        size: u64,
        dump: Vec<String>,
    },
    TooLarge {
        size: u64,
    },
    /// HEAD ⇄ working tree.
    Diff(Box<DiffDoc>),
    DiffLoading,
    Error(String),
}

impl Content {
    pub fn is_textual(&self) -> bool {
        matches!(self, Content::Text { .. } | Content::Markdown { .. })
    }
}

#[derive(Debug, Clone, Default)]
pub struct Search {
    pub query: String,
    /// The query is being typed.
    pub editing: bool,
    /// (line, start char, end char)
    pub matches: Vec<(usize, usize, usize)>,
    pub current: usize,
}

pub struct Tab {
    pub id: u64,
    pub path: PathBuf,
    pub content: Content,
    pub generation: u64,
    pub language: String,
    /// First visible line (rendered row for Markdown).
    pub scroll: usize,
    /// Cursor line.
    pub cursor: usize,
    pub hscroll: usize,
    /// Long lines wrap (`w`); otherwise they scroll sideways. In rendered
    /// Markdown this also decides whether tables wrap or keep full width.
    pub wrap: bool,
    /// After the next load, put the cursor on the first (true) or last change.
    pub land: Option<bool>,
    /// Scroll limit while wrapping, where lines take several rows (set when
    /// drawing; `None` means one row per line).
    pub wrapped_max: Option<usize>,
    /// The view was scrolled (wheel) rather than the cursor moved: the
    /// cursor follows the view instead of the view following the cursor.
    pub scrolled: bool,
    pub search: Option<Search>,
    pub view_height: usize,
    pub view_width: usize,
    pub last_used: u64,
    /// Shown in the embedded editor instead of the read-only viewer.
    pub editing: bool,
    /// This tab shows a diff of the file.
    pub diff: Option<DiffSpec>,
}

/// What a key in the viewer asks the app to do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ViewerOutcome {
    Handled,
    Unhandled,
    /// Ask for a line number.
    GotoLinePrompt,
    /// Open the working file at a 0-based line (from a diff).
    OpenAt(usize),
    /// Show the diff of the next/previous changed file.
    DiffFile(bool),
    /// Show the diff of this file against HEAD.
    ShowDiff,
    /// Tick/untick a change of this file for the commit.
    ToggleHunk(u64, Vec<u64>),
    Info(&'static str),
}

/// Result of a background load.
pub struct Loaded {
    pub lines: Vec<StyledLine>,
    pub markers: Markers,
    pub language: String,
}

pub struct Viewer {
    pub tabs: Vec<Tab>,
    pub active: Option<usize>,
    next_id: u64,
    clock: u64,
}

impl Default for Viewer {
    fn default() -> Self {
        Self::new()
    }
}

/// How many text lines a tab shows (for scroll limits).
pub fn line_count(content: &Content) -> usize {
    match content {
        Content::Text { lines, .. } => lines.len(),
        Content::Markdown { raw: true, lines, .. } => lines.len(),
        Content::Markdown {
            rendered: Some((_, _, r)),
            ..
        } => r.len(),
        Content::Markdown { lines, .. } => lines.len(),
        Content::Binary { dump, .. } => dump.len(),
        Content::Diff(d) => d.len(),
        _ => 0,
    }
}

impl Viewer {
    pub fn new() -> Viewer {
        Viewer {
            tabs: Vec::new(),
            active: None,
            next_id: 1,
            clock: 0,
        }
    }

    pub fn active_tab(&self) -> Option<&Tab> {
        self.active.and_then(|i| self.tabs.get(i))
    }

    pub fn active_tab_mut(&mut self) -> Option<&mut Tab> {
        self.active.and_then(|i| self.tabs.get_mut(i))
    }

    pub fn tab_by_id_mut(&mut self, id: u64) -> Option<&mut Tab> {
        self.tabs.iter_mut().find(|t| t.id == id)
    }

    /// The (non-diff) tab showing `path`.
    pub fn position(&self, path: &Path) -> Option<usize> {
        self.tabs.iter().position(|t| t.path == path && t.diff.is_none())
    }

    pub fn diff_position(&self, path: &Path) -> Option<usize> {
        self.tabs.iter().position(|t| t.path == path && t.diff.is_some())
    }

    /// Opens (or reuses) the diff tab for `path`.
    pub fn open_diff(&mut self, path: &Path, spec: DiffSpec, ctx: &LoadCtx) {
        if let Some(i) = self.diff_position(path) {
            self.activate(i);
            let t = &mut self.tabs[i];
            t.diff = Some(spec);
            load(t, ctx);
            return;
        }
        // Reuse the active diff tab when stepping between files, like
        // IntelliJ's diff window. Opening first as a plain tab also picks the
        // long-line default: Markdown (prose) wraps, code scrolls sideways.
        let id = self.open(path, ctx);
        if let Some(t) = self.tab_by_id_mut(id) {
            t.diff = Some(spec);
            t.scroll = 0;
            t.cursor = 0;
            load(t, ctx);
        }
    }

    /// Points the active diff tab at another file.
    pub fn retarget_diff(&mut self, path: &Path, spec: DiffSpec, ctx: &LoadCtx) {
        match self.active_tab_mut() {
            Some(t) if t.diff.is_some() => {
                t.path = path.to_path_buf();
                t.diff = Some(spec);
                t.scroll = 0;
                t.cursor = 0;
                load(t, ctx);
            }
            _ => self.open_diff(path, spec, ctx),
        }
    }

    pub fn activate(&mut self, i: usize) {
        if i < self.tabs.len() {
            self.clock += 1;
            self.tabs[i].last_used = self.clock;
            self.active = Some(i);
        }
    }

    pub fn close(&mut self, i: usize) {
        if i >= self.tabs.len() {
            return;
        }
        self.tabs.remove(i);
        self.active = if self.tabs.is_empty() {
            None
        } else {
            Some(i.min(self.tabs.len() - 1))
        };
    }

    pub fn cycle(&mut self, delta: isize) {
        if let Some(a) = self.active {
            let n = self.tabs.len() as isize;
            let next = ((a as isize + delta) % n + n) % n;
            self.activate(next as usize);
        }
    }

    /// Opens (or activates) a tab for `path` and starts loading it.
    /// Returns the tab id.
    pub fn open(&mut self, path: &Path, ctx: &LoadCtx) -> u64 {
        if let Some(i) = self.position(path) {
            self.activate(i);
            return self.tabs[i].id;
        }
        // `open` is also used by `open_diff` for a fresh tab; the caller sets
        // `diff` right after.
        if self.tabs.len() >= MAX_TABS
            && let Some(lru) = (0..self.tabs.len()).min_by_key(|i| self.tabs[*i].last_used)
        {
            self.tabs.remove(lru);
        }
        let id = self.next_id;
        self.next_id += 1;
        let mut tab = Tab {
            id,
            path: path.to_path_buf(),
            content: Content::Error(String::new()),
            generation: 0,
            language: String::new(),
            scroll: 0,
            cursor: 0,
            hscroll: 0,
            wrap: false,
            land: None,
            wrapped_max: None,
            scrolled: false,
            search: None,
            view_height: 20,
            view_width: 80,
            last_used: 0,
            editing: false,
            diff: None,
        };
        load(&mut tab, ctx);
        self.tabs.push(tab);
        let i = self.tabs.len() - 1;
        self.activate(i);
        id
    }

    /// Reloads tabs whose file changed on disk, keeping the scroll position.
    pub fn reload_paths(&mut self, paths: &[PathBuf], ctx: &LoadCtx) {
        for tab in &mut self.tabs {
            if paths.contains(&tab.path) {
                load(tab, ctx);
            }
        }
    }

    /// Recomputes gutter markers for all text tabs (HEAD moved).
    pub fn reload_all(&mut self, ctx: &LoadCtx) {
        for tab in &mut self.tabs {
            if tab.content.is_textual() || tab.diff.is_some() {
                load(tab, ctx);
            }
        }
    }
}

/// Scroll limit for `lines` lines in a view `height` rows tall: the last
/// line can rise to about 30% above the bottom edge.
pub fn max_scroll(lines: usize, height: usize) -> usize {
    let h = height.max(1);
    (lines + h * 3 / 10).saturating_sub(h)
}

/// [`max_scroll`] for lines of different heights: the first line from which
/// the rest fills at most ~70% of the view.
pub fn wrapped_max_scroll(heights: &[usize], height: usize) -> usize {
    let h = height.max(1);
    let room = h - h * 3 / 10;
    let mut used = 0;
    let mut s = heights.len();
    while s > 0 && used + heights[s - 1] <= room {
        used += heights[s - 1];
        s -= 1;
    }
    // A last line taller than the room still gets shown from its top.
    s.min(heights.len().saturating_sub(1))
}

/// Screen rows a line of `width` columns takes when wrapped to `cols`.
pub fn wrapped_rows(width: usize, cols: usize) -> usize {
    if cols == 0 { 1 } else { width.div_ceil(cols).max(1) }
}

/// What loading needs from the app.
pub struct LoadCtx {
    pub tx: Sender<AppEvent>,
    pub git: Option<Git>,
    pub images_enabled: bool,
}

fn read_head(path: &Path, n: usize) -> std::io::Result<Vec<u8>> {
    let mut f = std::fs::File::open(path)?;
    let mut buf = vec![0u8; n];
    let mut read = 0;
    while read < n {
        let k = f.read(&mut buf[read..])?;
        if k == 0 {
            break;
        }
        read += k;
    }
    buf.truncate(read);
    Ok(buf)
}

/// (Re)loads a tab: cheap work now, highlighting/decoding in the background.
pub fn load(tab: &mut Tab, ctx: &LoadCtx) {
    tab.generation += 1;
    let path = tab.path.clone();
    if let Some(spec) = tab.diff.clone() {
        if !matches!(tab.content, Content::Diff(_)) {
            tab.content = Content::DiffLoading;
        }
        tab.language = "Diff".into();
        let (tx, git, id, generation, images) = (
            ctx.tx.clone(),
            ctx.git.clone(),
            tab.id,
            tab.generation,
            ctx.images_enabled,
        );
        std::thread::spawn(move || {
            let load = crate::diffview::load(git.as_ref(), &path, &spec, images);
            let _ = tx.send(AppEvent::DiffLoaded {
                tab: id,
                generation,
                load: Box::new(load),
            });
        });
        return;
    }
    let meta = match std::fs::metadata(&path) {
        Ok(m) => m,
        Err(e) => {
            tab.content = Content::Error(format!("cannot open {}: {e}", path.display()));
            return;
        }
    };
    let head = read_head(&path, 8192).unwrap_or_default();
    let kind = filetype::classify(&path, meta.len(), &head);
    let keep_raw = matches!(tab.content, Content::Markdown { raw: true, .. });
    match kind {
        FileKind::Text | FileKind::Markdown => {
            let bytes = std::fs::read(&path).unwrap_or_default();
            let text = String::from_utf8_lossy(&bytes).into_owned();
            let lines = plain_lines(&text);
            let (old_markers, old_rendered) = match &tab.content {
                Content::Text { markers, .. } => (markers.clone(), None),
                Content::Markdown { markers, rendered, .. } => (markers.clone(), rendered.clone()),
                _ => (Markers::default(), None),
            };
            if kind == FileKind::Markdown && !matches!(tab.content, Content::Markdown { .. }) {
                tab.wrap = true;
            }
            tab.content = if kind == FileKind::Markdown {
                Content::Markdown {
                    source: text.clone(),
                    lines,
                    markers: old_markers,
                    raw: keep_raw,
                    rendered: old_rendered.filter(|_| false),
                }
            } else {
                Content::Text {
                    lines,
                    markers: old_markers,
                    highlighted: false,
                }
            };
            let (tx, git, id, generation) = (ctx.tx.clone(), ctx.git.clone(), tab.id, tab.generation);
            std::thread::spawn(move || {
                let hl = Highlighter::global();
                let lines = hl.highlight_file(&path, &text);
                let language = hl.language(&path, text.lines().next().unwrap_or(""));
                let markers = git
                    .as_ref()
                    .map(|g| {
                        let old = g
                            .head_content(&path)
                            .map(|b| String::from_utf8_lossy(&b).into_owned())
                            .unwrap_or_default();
                        // Untracked files have no HEAD content: everything is new.
                        line_markers(&old, &text)
                    })
                    .unwrap_or_default();
                let _ = tx.send(AppEvent::Loaded {
                    tab: id,
                    generation,
                    loaded: Box::new(Loaded {
                        lines,
                        markers,
                        language,
                    }),
                });
            });
        }
        FileKind::Image => {
            let fmt = path
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("")
                .to_ascii_uppercase();
            let dims = image::image_dimensions(&path)
                .map(|(w, h)| format!("{w}×{h}"))
                .unwrap_or_else(|_| "?".into());
            tab.language = "Image".into();
            tab.content = Content::Image {
                info: format!("{fmt} {dims} · {}", filetype::human_size(meta.len())),
                state: if ctx.images_enabled {
                    ImageState::Loading
                } else {
                    ImageState::Disabled
                },
            };
            if ctx.images_enabled {
                let (tx, id, generation) = (ctx.tx.clone(), tab.id, tab.generation);
                std::thread::spawn(move || {
                    let res = image::open(&path).map_err(|e| e.to_string());
                    let _ = tx.send(AppEvent::ImageDecoded {
                        tab: id,
                        generation,
                        image: res.map(Box::new),
                    });
                });
            }
        }
        FileKind::Binary => {
            let head = read_head(&path, 4096).unwrap_or_default();
            tab.language = "Binary".into();
            tab.content = Content::Binary {
                size: meta.len(),
                dump: filetype::hex_dump(&head, 256),
            };
        }
        FileKind::TooLarge => {
            tab.language = "Text".into();
            tab.content = Content::TooLarge { size: meta.len() };
        }
    }
    let n = line_count(&tab.content);
    tab.cursor = tab.cursor.min(n.saturating_sub(1));
    tab.scroll = tab.scroll.min(n.saturating_sub(1));
}

/// Applies a finished background load (ignores stale generations).
pub fn apply_loaded(tab: &mut Tab, generation: u64, loaded: Loaded) {
    if generation != tab.generation {
        return;
    }
    tab.language = loaded.language;
    match &mut tab.content {
        Content::Text {
            lines,
            markers,
            highlighted,
        } => {
            *lines = loaded.lines;
            *markers = loaded.markers;
            *highlighted = true;
        }
        Content::Markdown { lines, markers, .. } => {
            *lines = loaded.lines;
            *markers = loaded.markers;
        }
        _ => {}
    }
    refresh_search(tab);
    if let Some(first) = tab.land.take() {
        tab.land_on_change(first);
    }
}

fn find_matches(lines: &[StyledLine], query: &str) -> Vec<(usize, usize, usize)> {
    if query.is_empty() {
        return Vec::new();
    }
    // Smart case: lowercase queries match case-insensitively.
    let insensitive = !query.chars().any(char::is_uppercase);
    let q: Vec<char> = if insensitive {
        query.to_lowercase().chars().collect()
    } else {
        query.chars().collect()
    };
    let mut out = Vec::new();
    for (li, line) in lines.iter().enumerate() {
        let text: Vec<char> = if insensitive {
            line_text(line).to_lowercase().chars().collect()
        } else {
            line_text(line).chars().collect()
        };
        if text.len() < q.len() {
            continue;
        }
        let mut i = 0;
        while i + q.len() <= text.len() {
            if text[i..i + q.len()] == q[..] {
                out.push((li, i, i + q.len()));
                i += q.len().max(1);
            } else {
                i += 1;
            }
        }
    }
    out
}

/// The lines search runs over (source lines; rendered rows for Markdown).
pub fn searchable_lines(tab: &Tab) -> Vec<StyledLine> {
    match &tab.content {
        Content::Text { lines, .. } => lines.clone(),
        Content::Markdown {
            raw: false,
            rendered: Some((_, _, r)),
            ..
        } => r.iter().map(|l| l.line.clone()).collect(),
        Content::Markdown { lines, .. } => lines.clone(),
        _ => Vec::new(),
    }
}

pub fn refresh_search(tab: &mut Tab) {
    let lines = searchable_lines(tab);
    if let Some(s) = tab.search.as_mut() {
        s.matches = find_matches(&lines, &s.query);
        s.current = s.current.min(s.matches.len().saturating_sub(1));
    }
}

impl Tab {
    pub fn total_lines(&self) -> usize {
        line_count(&self.content)
    }

    /// Markdown in rendered mode scrolls by rows and has no cursor.
    pub fn has_cursor(&self) -> bool {
        match &self.content {
            Content::Text { .. } | Content::Diff(_) => true,
            Content::Markdown { raw, .. } => *raw,
            _ => false,
        }
    }

    pub fn scroll_into_view(&mut self) {
        let h = self.view_height.max(1);
        if self.cursor < self.scroll {
            self.scroll = self.cursor;
        } else if self.cursor >= self.scroll + h {
            self.scroll = self.cursor + 1 - h;
        }
    }

    pub fn set_cursor(&mut self, line: usize) {
        self.scrolled = false;
        let n = self.total_lines();
        self.cursor = line.min(n.saturating_sub(1));
        if self.has_cursor() {
            self.scroll_into_view();
        } else {
            self.scroll = self.cursor.min(self.max_scroll());
        }
    }

    /// How far the view can scroll: past the end, until the last line sits
    /// about 30% above the bottom edge (like an IDE's "scroll past end").
    pub fn max_scroll(&self) -> usize {
        self.wrapped_max
            .unwrap_or_else(|| max_scroll(self.total_lines(), self.view_height))
    }

    /// Scroll limits for wrapped lines, `heights[i]` screen rows each: keeps
    /// the whole cursor line on screen and lets the end rise like
    /// [`max_scroll`]. Call while drawing, after `view_height` is set.
    pub fn fit_wrapped(&mut self, heights: &[usize]) {
        let h = self.view_height.max(1);
        let Some(last) = heights.len().checked_sub(1) else {
            self.wrapped_max = Some(0);
            self.scroll = 0;
            return;
        };
        let max = wrapped_max_scroll(heights, h);
        self.wrapped_max = Some(max);
        if !self.has_cursor() {
            self.scroll = self.scroll.min(max);
            return;
        }
        let cursor = self.cursor.min(last);
        if self.scrolled {
            // Wheel: keep the view, pull the cursor onto its last full line.
            self.scroll = self.scroll.min(max).min(last);
            let mut used = 0;
            let mut bottom = self.scroll;
            for (i, rows) in heights.iter().enumerate().skip(self.scroll) {
                used += rows;
                if used > h {
                    break;
                }
                bottom = i;
            }
            self.cursor = cursor.clamp(self.scroll, bottom);
        } else {
            // Cursor moved: scroll until its whole line is on screen.
            if cursor >= self.scroll {
                let mut used: usize = heights[self.scroll..=cursor].iter().sum();
                while used > h && self.scroll < cursor {
                    used -= heights[self.scroll];
                    self.scroll += 1;
                }
            }
            self.scroll = self.scroll.min(max);
        }
    }

    pub fn scroll_by(&mut self, delta: isize) {
        self.scrolled = true;
        let n = self.total_lines();
        let max = self.max_scroll();
        self.scroll = (self.scroll as isize + delta).clamp(0, max as isize) as usize;
        if self.has_cursor() {
            let h = self.view_height.max(1);
            self.cursor = self
                .cursor
                .clamp(self.scroll, (self.scroll + h - 1).min(n.saturating_sub(1)));
        } else {
            self.cursor = self.scroll;
        }
    }

    /// Scrolls sideways (no-op while wrapping); drawing clamps the far end.
    pub fn hscroll_by(&mut self, delta: isize) {
        if !self.wrap {
            self.hscroll = (self.hscroll as isize + delta).max(0) as usize;
        }
    }

    /// Moves the cursor to the first or last change, if there is one.
    pub fn land_on_change(&mut self, first: bool) -> bool {
        let starts = match &self.content {
            Content::Diff(d) => d.hunk_starts(),
            Content::Text { markers, .. } | Content::Markdown { markers, raw: true, .. } => markers.change_starts(),
            _ => return false,
        };
        let target = if first { starts.first() } else { starts.last() };
        match target.copied() {
            Some(t) => {
                self.set_cursor(t);
                self.scroll = t.saturating_sub(3);
                true
            }
            None => false,
        }
    }

    fn move_cursor(&mut self, delta: isize) {
        if self.has_cursor() {
            let target = (self.cursor as isize + delta).max(0) as usize;
            self.set_cursor(target);
        } else {
            self.scroll_by(delta);
        }
    }

    fn jump_to_match(&mut self, forward: bool) {
        let Some(s) = self.search.as_mut() else { return };
        if s.matches.is_empty() {
            return;
        }
        let cur_line = self.cursor;
        let idx = if forward {
            s.matches
                .iter()
                .position(|m| m.0 > cur_line || (m.0 == cur_line && Some(m) > s.matches.get(s.current)))
                .unwrap_or(0)
        } else {
            s.matches
                .iter()
                .rposition(|m| m.0 < cur_line || (m.0 == cur_line && Some(m) < s.matches.get(s.current)))
                .unwrap_or(s.matches.len() - 1)
        };
        s.current = idx;
        let line = s.matches[idx].0;
        self.set_cursor(line);
        // Center the match when it was off-screen.
        let h = self.view_height.max(1);
        if line < self.scroll || line >= self.scroll + h {
            self.scroll = line.saturating_sub(h / 2);
        }
    }

    /// Jumps to the next/previous block of changed lines.
    pub fn jump_to_change(&mut self, forward: bool) -> bool {
        if let Content::Diff(d) = &self.content {
            let starts = d.hunk_starts();
            let target = if forward {
                starts.iter().copied().find(|s| *s > self.cursor)
            } else {
                starts.iter().rev().copied().find(|s| *s < self.cursor)
            };
            return match target {
                Some(t) => {
                    self.set_cursor(t);
                    // Show some context above the change.
                    self.scroll = t.saturating_sub(3);
                    true
                }
                None => false,
            };
        }
        let markers = match &self.content {
            Content::Text { markers, .. } => markers,
            Content::Markdown { markers, raw: true, .. } => markers,
            _ => return false,
        };
        let starts = markers.change_starts();
        let target = if forward {
            starts.iter().copied().find(|s| *s > self.cursor)
        } else {
            starts.iter().rev().copied().find(|s| *s < self.cursor)
        };
        match target {
            Some(t) => {
                self.set_cursor(t);
                true
            }
            None => false,
        }
    }

    /// Keys while the search query is being typed.
    fn search_key(&mut self, key: &KeyEvent) -> ViewerOutcome {
        let Some(s) = self.search.as_mut() else {
            return ViewerOutcome::Unhandled;
        };
        match key.code {
            KeyCode::Esc => {
                self.search = None;
            }
            KeyCode::Enter => {
                s.editing = false;
                self.jump_to_match(true);
            }
            KeyCode::Backspace => {
                s.query.pop();
                refresh_search(self);
            }
            KeyCode::Char(c) if !key.modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) => {
                s.query.push(c);
                refresh_search(self);
                // Incremental: show the first match at or after the cursor.
                if let Some(s) = self.search.as_mut()
                    && let Some(i) = s.matches.iter().position(|m| m.0 >= self.cursor)
                {
                    s.current = i;
                    let line = s.matches[i].0;
                    self.set_cursor(line);
                }
            }
            _ => {}
        }
        ViewerOutcome::Handled
    }

    pub fn handle_key(&mut self, key: &KeyEvent) -> ViewerOutcome {
        if self.search.as_ref().is_some_and(|s| s.editing) {
            return self.search_key(key);
        }
        if let Content::Diff(d) = &mut self.content {
            match crate::diffview::handle_key(d, key, self.cursor) {
                DiffKey::Handled => {
                    let n = d.len();
                    self.cursor = self.cursor.min(n.saturating_sub(1));
                    self.scroll = self.scroll.min(n.saturating_sub(1));
                    return ViewerOutcome::Handled;
                }
                DiffKey::OpenAt(l) => return ViewerOutcome::OpenAt(l),
                DiffKey::File(f) => return ViewerOutcome::DiffFile(f),
                DiffKey::ToggleHunk(id, all) => return ViewerOutcome::ToggleHunk(id, all),
                DiffKey::Info(m) => return ViewerOutcome::Info(m),
                DiffKey::Unhandled => {}
            }
        } else if key.code == KeyCode::Char('d')
            && key.modifiers.is_empty()
            && matches!(self.content, Content::Text { .. } | Content::Markdown { .. })
        {
            return ViewerOutcome::ShowDiff;
        }
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        if key.modifiers.contains(KeyModifiers::ALT) {
            return ViewerOutcome::Unhandled;
        }
        let page = self.view_height.max(1) as isize;
        match key.code {
            KeyCode::Up | KeyCode::Char('k') if !ctrl => self.move_cursor(-1),
            KeyCode::Down | KeyCode::Char('j') if !ctrl => self.move_cursor(1),
            KeyCode::PageUp => self.move_cursor(-page),
            KeyCode::PageDown => self.move_cursor(page),
            KeyCode::Char('u') if ctrl => self.move_cursor(-page / 2),
            KeyCode::Char('d') if ctrl => self.move_cursor(page / 2),
            KeyCode::Char('g') if ctrl => return ViewerOutcome::GotoLinePrompt,
            KeyCode::Home | KeyCode::Char('g') if !ctrl => self.set_cursor(0),
            KeyCode::End | KeyCode::Char('G') if !ctrl => self.set_cursor(usize::MAX / 2),
            KeyCode::Left | KeyCode::Char('h') if !ctrl && !self.wrap => self.hscroll_by(-4),
            KeyCode::Right | KeyCode::Char('l') if !ctrl && !self.wrap => self.hscroll_by(4),
            KeyCode::Char('w') if !ctrl => {
                self.wrap = !self.wrap;
                self.hscroll = 0;
                return ViewerOutcome::Info(if self.wrap {
                    "Long lines: wrap (w to scroll sideways)"
                } else {
                    "Long lines: scroll sideways with ←/→, shift+wheel or the trackpad (w to wrap)"
                });
            }
            KeyCode::Char('m') if !ctrl => {
                if let Content::Markdown { raw, .. } = &mut self.content {
                    *raw = !*raw;
                    self.scroll = 0;
                    self.cursor = 0;
                    refresh_search(self);
                } else {
                    return ViewerOutcome::Unhandled;
                }
            }
            KeyCode::Char('/') if !ctrl => {
                if !self.content.is_textual() {
                    return ViewerOutcome::Unhandled;
                }
                self.search = Some(Search {
                    editing: true,
                    ..Search::default()
                });
            }
            KeyCode::Char('n') if !ctrl => self.jump_to_match(true),
            KeyCode::Char('N') if !ctrl => self.jump_to_match(false),
            KeyCode::Esc if self.search.is_some() => self.search = None,
            _ => return ViewerOutcome::Unhandled,
        }
        ViewerOutcome::Handled
    }
}

/// Decoded Markdown images, shared by all Markdown tabs.
#[derive(Default)]
pub struct MdImages {
    pub entries: HashMap<PathBuf, ImageState>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;

    fn ctx() -> (LoadCtx, mpsc::Receiver<AppEvent>) {
        let (tx, rx) = mpsc::channel();
        (
            LoadCtx {
                tx,
                git: None,
                images_enabled: false,
            },
            rx,
        )
    }

    fn finish(viewer: &mut Viewer, rx: &mpsc::Receiver<AppEvent>) {
        while let Ok(ev) = rx.recv_timeout(std::time::Duration::from_secs(5)) {
            if let AppEvent::Loaded {
                tab,
                generation,
                loaded,
            } = ev
                && let Some(t) = viewer.tab_by_id_mut(tab)
            {
                apply_loaded(t, generation, *loaded);
                return;
            }
        }
        panic!("no load result");
    }

    fn key(c: KeyCode) -> KeyEvent {
        KeyEvent::from(c)
    }

    #[test]
    fn opens_text_and_navigates() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("a.rs");
        let src: String = (1..=100).map(|i| format!("line {i}\n")).collect();
        std::fs::write(&p, &src).unwrap();
        let (ctx, rx) = ctx();
        let mut v = Viewer::new();
        v.open(&p, &ctx);
        finish(&mut v, &rx);
        let t = v.active_tab_mut().unwrap();
        assert_eq!(t.language, "Rust");
        assert_eq!(t.total_lines(), 100);
        t.view_height = 10;
        t.handle_key(&key(KeyCode::PageDown));
        assert_eq!(t.cursor, 10);
        t.handle_key(&key(KeyCode::End));
        assert_eq!((t.cursor, t.scroll), (99, 90));
        t.handle_key(&key(KeyCode::Char('g')));
        assert_eq!((t.cursor, t.scroll), (0, 0));
        assert_eq!(
            t.handle_key(&KeyEvent::new(KeyCode::Char('g'), KeyModifiers::CONTROL)),
            ViewerOutcome::GotoLinePrompt
        );
    }

    #[test]
    fn incremental_search_with_smart_case() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("a.txt");
        std::fs::write(&p, "alpha\nBeta\nbeta gamma beta\n").unwrap();
        let (ctx, rx) = ctx();
        let mut v = Viewer::new();
        v.open(&p, &ctx);
        finish(&mut v, &rx);
        let t = v.active_tab_mut().unwrap();
        t.handle_key(&key(KeyCode::Char('/')));
        for c in "beta".chars() {
            t.handle_key(&key(KeyCode::Char(c)));
        }
        assert_eq!(
            t.search.as_ref().unwrap().matches.len(),
            3,
            "lowercase is case-insensitive"
        );
        assert_eq!(t.cursor, 1);
        t.handle_key(&key(KeyCode::Enter));
        t.handle_key(&key(KeyCode::Char('n')));
        assert_eq!(t.cursor, 2);
        t.handle_key(&key(KeyCode::Char('/')));
        for c in "Beta".chars() {
            t.handle_key(&key(KeyCode::Char(c)));
        }
        assert_eq!(
            t.search.as_ref().unwrap().matches.len(),
            1,
            "uppercase is case-sensitive"
        );
        t.handle_key(&key(KeyCode::Esc));
        assert!(t.search.is_none());
    }

    #[test]
    fn tabs_reuse_cycle_close_and_evict() {
        let dir = tempfile::tempdir().unwrap();
        let (ctx, _rx) = ctx();
        let mut v = Viewer::new();
        let paths: Vec<PathBuf> = (0..MAX_TABS + 2)
            .map(|i| {
                let p = dir.path().join(format!("f{i}.txt"));
                std::fs::write(&p, "x").unwrap();
                p
            })
            .collect();
        let id0 = v.open(&paths[0], &ctx);
        v.open(&paths[1], &ctx);
        assert_eq!(v.open(&paths[0], &ctx), id0, "re-opening activates");
        assert_eq!(v.tabs.len(), 2);
        v.cycle(1);
        assert_eq!(v.active_tab().unwrap().path, paths[1]);
        for p in &paths[2..] {
            v.open(p, &ctx);
        }
        assert_eq!(v.tabs.len(), MAX_TABS);
        assert!(
            v.position(&paths[0]).is_none() || v.position(&paths[1]).is_none(),
            "LRU evicted"
        );
        let n = v.tabs.len();
        v.close(n - 1);
        assert_eq!(v.active, Some(n - 2));
    }

    #[test]
    fn classifies_binary_large_and_missing() {
        let dir = tempfile::tempdir().unwrap();
        let bin = dir.path().join("app");
        std::fs::write(&bin, b"\x7fELF\0\0\x01").unwrap();
        let (ctx, _rx) = ctx();
        let mut v = Viewer::new();
        v.open(&bin, &ctx);
        assert!(matches!(v.active_tab().unwrap().content, Content::Binary { .. }));
        v.open(&dir.path().join("missing.txt"), &ctx);
        assert!(matches!(v.active_tab().unwrap().content, Content::Error(_)));
    }

    #[test]
    fn stale_loads_are_ignored() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("a.txt");
        std::fs::write(&p, "one\n").unwrap();
        let (ctx, _rx) = ctx();
        let mut v = Viewer::new();
        v.open(&p, &ctx);
        let t = v.active_tab_mut().unwrap();
        let old_gen = t.generation;
        load(t, &ctx);
        apply_loaded(
            t,
            old_gen,
            Loaded {
                lines: plain_lines("stale\n"),
                markers: Markers::default(),
                language: "X".into(),
            },
        );
        assert_ne!(t.language, "X");
    }

    #[test]
    fn scrolls_past_the_end_until_the_last_line_is_30_percent_up() {
        // 100 lines in a 20-row view: the last line can rise 6 rows.
        assert_eq!(max_scroll(100, 20), 86);
        assert_eq!(max_scroll(100, 10), 93);
        // Files shorter than the view never scroll.
        assert_eq!(max_scroll(5, 20), 0);
        assert_eq!(max_scroll(14, 20), 0);
        assert_eq!(max_scroll(0, 0), 0);
    }

    #[test]
    fn wheel_and_sideways_scrolling_respect_limits_and_wrap() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("a.txt");
        std::fs::write(&p, "x\n".repeat(100)).unwrap();
        let (ctx, _rx) = ctx();
        let mut v = Viewer::new();
        v.open(&p, &ctx);
        let t = v.active_tab_mut().unwrap();
        t.view_height = 20;
        t.scroll_by(1000);
        assert_eq!(t.scroll, 86);
        assert!(t.cursor >= 86 && t.cursor < 100, "cursor stays on a real line");
        t.hscroll_by(-5);
        assert_eq!(t.hscroll, 0);
        t.hscroll_by(12);
        assert_eq!(t.hscroll, 12);
        t.wrap = true;
        t.hscroll = 0;
        t.hscroll_by(12);
        assert_eq!(t.hscroll, 0, "no sideways scrolling while wrapping");
    }

    #[test]
    fn diff_tabs_land_on_the_first_or_last_change() {
        let old: String = (0..40).map(|i| format!("l{i}\n")).collect();
        let new = old.replace("l5\n", "five\n").replace("l30\n", "thirty\n");
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("a.txt");
        std::fs::write(&p, &new).unwrap();
        let (ctx, _rx) = ctx();
        let mut v = Viewer::new();
        v.open(&p, &ctx);
        let t = v.active_tab_mut().unwrap();
        t.content = Content::Diff(Box::new(DiffDoc::text(old, new, &p, "abc".into())));
        assert!(t.land_on_change(false));
        assert_eq!(t.cursor, 30);
        assert!(t.land_on_change(true));
        assert_eq!(t.cursor, 5);
        assert!(!t.jump_to_change(false), "nothing before the first change");
    }

    #[test]
    fn markdown_opens_wrapped() {
        let dir = tempfile::tempdir().unwrap();
        let (ctx, _rx) = ctx();
        let mut v = Viewer::new();
        let md = dir.path().join("a.md");
        std::fs::write(&md, "# x\n").unwrap();
        v.open(&md, &ctx);
        assert!(v.active_tab().unwrap().wrap);
        let rs = dir.path().join("a.rs");
        std::fs::write(&rs, "fn x() {}\n").unwrap();
        v.open(&rs, &ctx);
        assert!(!v.active_tab().unwrap().wrap);
    }

    #[test]
    fn wrapped_scroll_limit_counts_rows() {
        // 10 lines of 5 rows in a 20-row view: the tail may fill 14 rows,
        // so two lines (10 rows) fit after the max scroll, not 14 lines.
        assert_eq!(wrapped_max_scroll(&[5; 10], 20), 8);
        // One row per line matches the plain limit.
        assert_eq!(wrapped_max_scroll(&[1; 100], 20), max_scroll(100, 20));
        // A last line taller than the view still gets reached.
        assert_eq!(wrapped_max_scroll(&[1, 1, 50], 20), 2);
        assert_eq!(wrapped_max_scroll(&[], 20), 0);
        assert_eq!(wrapped_rows(0, 40), 1);
        assert_eq!(wrapped_rows(81, 40), 3);
    }

    #[test]
    fn fit_wrapped_keeps_the_cursor_line_on_screen() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("a.txt");
        std::fs::write(&p, "x\n".repeat(30)).unwrap();
        let (ctx, _rx) = ctx();
        let mut v = Viewer::new();
        v.open(&p, &ctx);
        let t = v.active_tab_mut().unwrap();
        t.view_height = 20;
        let heights = [4usize; 30];
        // Plain scrolling thinks line 19 is visible from 0; wrapped, it needs
        // rows 76..80, so the view must scroll until lines s..=19 fit.
        t.set_cursor(19);
        t.fit_wrapped(&heights);
        assert_eq!(t.scroll, 15, "lines 15..=19 fill the 20 rows");
        // The limit is the wrapped one: the tail fills at most 14 rows.
        assert_eq!(t.max_scroll(), 27);
        t.scroll_by(1000);
        assert_eq!(t.scroll, 27);
    }

    #[test]
    fn wheel_scrolls_up_through_wrapped_lines() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("a.txt");
        std::fs::write(&p, "x\n".repeat(30)).unwrap();
        let (ctx, _rx) = ctx();
        let mut v = Viewer::new();
        v.open(&p, &ctx);
        let t = v.active_tab_mut().unwrap();
        t.view_height = 20;
        let heights = [4usize; 30];
        // At the end of the file…
        t.set_cursor(29);
        t.fit_wrapped(&heights);
        let end = t.scroll;
        assert!(end > 20);
        // …each wheel step up sticks, and the cursor comes along.
        for _ in 0..20 {
            t.scroll_by(-3);
            t.fit_wrapped(&heights);
        }
        assert_eq!(t.scroll, 0, "reached the top");
        assert!(t.cursor <= 4, "cursor on a visible line: {}", t.cursor);
        // Moving the cursor afterwards follows it again.
        t.set_cursor(29);
        t.fit_wrapped(&heights);
        assert_eq!(t.scroll, end);
    }
}
