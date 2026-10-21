//! Playwright-style end-to-end tests for vista.
//!
//! The real `vista` binary runs in a pseudo-terminal. Tests send keys and
//! mouse events, and read the screen back through a terminal emulator
//! (`vt100`). Every `expect_*` call auto-waits until the screen matches or a
//! timeout passes, so there are no sleeps. When a test fails, an HTML trace of
//! every step is written to `target/e2e-traces/<test>.html`.
//!
//! ```ignore
//! use vista_e2e::{Fixture, Vista};
//! let repo = Fixture::rust_project().build();
//! let mut vista = Vista::launch(env!("CARGO_BIN_EXE_vista"), &repo).start();
//! vista.expect_text("status.rs").to_have_fg("git.modified");
//! vista.press("q");
//! vista.expect_exit();
//! ```

pub mod fixture;
pub mod keys;
pub mod trace;

use std::io::{Read, Write};
use std::num::NonZeroU16;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

pub use fixture::{Fixture, Repo};
use portable_pty::{Child, CommandBuilder, MasterPty, PtySize, native_pty_system};
use tempfile::TempDir;
use vista_theme::{Rgb, Theme, ThemeLoader};
pub use vt100;

fn default_timeout() -> Duration {
    std::env::var("VISTA_E2E_TIMEOUT_MS")
        .ok()
        .and_then(|v| v.parse().ok())
        .map(Duration::from_millis)
        .unwrap_or(Duration::from_secs(8))
}

/// Where vista's config, cache and log go during a test.
enum Home<'a> {
    Borrowed(&'a Path),
    Owned(TempDir),
}

impl Home<'_> {
    fn path(&self) -> &Path {
        match self {
            Home::Borrowed(p) => p,
            Home::Owned(t) => t.path(),
        }
    }
}

/// Configures and starts a vista process.
pub struct Launcher<'a> {
    bin: PathBuf,
    dir: PathBuf,
    home: Home<'a>,
    cols: u16,
    rows: u16,
    theme: String,
    env: Vec<(String, String)>,
    args: Vec<String>,
    config: Option<String>,
    files: Vec<(String, String)>,
    name: Option<String>,
    open: Option<String>,
}

impl<'a> Launcher<'a> {
    pub fn size(mut self, cols: u16, rows: u16) -> Self {
        self.cols = cols;
        self.rows = rows;
        self
    }

    pub fn theme(mut self, id: &str) -> Self {
        self.theme = id.to_string();
        self
    }

    pub fn env(mut self, k: &str, v: &str) -> Self {
        self.env.push((k.to_string(), v.to_string()));
        self
    }

    pub fn arg(mut self, a: &str) -> Self {
        self.args.push(a.to_string());
        self
    }

    /// Contents of `~/.config/vista/config.toml` for this run.
    pub fn config(mut self, toml: &str) -> Self {
        self.config = Some(toml.to_string());
        self
    }

    /// Writes a file relative to vista's config directory (e.g. `themes/x.toml`).
    pub fn config_file(mut self, rel: &str, content: &str) -> Self {
        self.files.push((rel.to_string(), content.to_string()));
        self
    }

    /// Name used for the trace file (defaults to the test's thread name).
    /// Launch as `vista <dir>/<rel>` (opens a file) instead of `vista <dir>`.
    pub fn open(mut self, rel: &str) -> Self {
        self.open = Some(rel.to_string());
        self
    }

    pub fn name(mut self, n: &str) -> Self {
        self.name = Some(n.to_string());
        self
    }

    pub fn config_dir(&self) -> PathBuf {
        self.home.path().join(".config").join("vista")
    }

    pub fn start(self) -> Vista {
        let config_dir = self.config_dir();
        std::fs::create_dir_all(config_dir.join("themes")).expect("config dir");
        if let Some(c) = &self.config {
            std::fs::write(config_dir.join("config.toml"), c).expect("write config");
        }
        for (rel, content) in &self.files {
            let p = config_dir.join(rel);
            std::fs::create_dir_all(p.parent().expect("parent")).expect("mkdir");
            std::fs::write(p, content).expect("write config file");
        }
        let home = self.home.path().to_path_buf();
        let log_file = home.join("vista.log");

        let pty = native_pty_system();
        let pair = pty
            .openpty(PtySize {
                rows: self.rows,
                cols: self.cols,
                pixel_width: self.cols * 8,
                pixel_height: self.rows * 16,
            })
            .expect("open pty");
        let mut cmd = CommandBuilder::new(&self.bin);
        cmd.cwd(&self.dir);
        match &self.open {
            Some(rel) => cmd.arg(self.dir.join(rel)),
            None => cmd.arg(&self.dir),
        }
        for a in &self.args {
            cmd.arg(a);
        }
        let s = |p: &Path| p.to_string_lossy().into_owned();
        let mut env: Vec<(String, String)> = fixture::git_env(&home);
        env.extend([
            ("TERM".into(), "xterm-256color".into()),
            ("COLORTERM".into(), "truecolor".into()),
            ("LANG".into(), "en_US.UTF-8".into()),
            ("XDG_CONFIG_HOME".into(), s(&home.join(".config"))),
            ("XDG_DATA_HOME".into(), s(&home.join(".local/share"))),
            ("XDG_STATE_HOME".into(), s(&home.join(".local/state"))),
            ("XDG_CACHE_HOME".into(), s(&home.join(".cache"))),
            ("VISTA_CONFIG_DIR".into(), s(&config_dir)),
            ("VISTA_CACHE_DIR".into(), s(&home.join(".cache/vista"))),
            ("VISTA_LOG_FILE".into(), s(&log_file)),
            ("VISTA_LOG".into(), "debug".into()),
            ("VISTA_KEYBOARD".into(), "legacy".into()),
            ("VISTA_IMAGE_PROTOCOL".into(), "halfblocks".into()),
            ("VISTA_THEME".into(), self.theme.clone()),
            ("EDITOR".into(), "true".into()),
        ]);
        env.extend(self.env.iter().cloned());
        for (k, v) in &env {
            cmd.env(k, v);
        }
        let child = pair.slave.spawn_command(cmd).expect("spawn vista");
        drop(pair.slave);

        let parser = Arc::new(Mutex::new(vt100::Parser::new(nz(self.rows), nz(self.cols), 0)));
        let raw = Arc::new(Mutex::new(Vec::new()));
        let eof = Arc::new(AtomicBool::new(false));
        let mut reader = pair.master.try_clone_reader().expect("pty reader");
        {
            let (parser, raw, eof) = (parser.clone(), raw.clone(), eof.clone());
            thread::spawn(move || {
                let mut buf = [0u8; 16 * 1024];
                loop {
                    match reader.read(&mut buf) {
                        Ok(0) | Err(_) => break,
                        Ok(n) => {
                            parser.lock().expect("parser").process(&buf[..n]);
                            raw.lock().expect("raw").extend_from_slice(&buf[..n]);
                        }
                    }
                }
                eof.store(true, Ordering::SeqCst);
            });
        }
        let writer = pair.master.take_writer().expect("pty writer");
        let theme = ThemeLoader::new(Some(config_dir.join("themes")))
            .load(&self.theme)
            .unwrap_or_else(|_| Theme::default_theme());
        let name = self
            .name
            .or_else(|| thread::current().name().map(str::to_string))
            .unwrap_or_else(|| "vista-e2e".into());
        let home_guard = match self.home {
            Home::Owned(t) => Some(t),
            Home::Borrowed(_) => None,
        };
        Vista {
            child,
            master: pair.master,
            writer,
            parser,
            raw,
            eof,
            theme,
            steps: Vec::new(),
            name,
            timeout: default_timeout(),
            log_file,
            config_dir,
            _home: home_guard,
            exited: false,
        }
    }
}

fn nz(n: u16) -> NonZeroU16 {
    NonZeroU16::new(n.max(1)).expect("non-zero")
}

/// A running vista under test.
pub struct Vista {
    child: Box<dyn Child + Send + Sync>,
    master: Box<dyn MasterPty + Send>,
    writer: Box<dyn Write + Send>,
    parser: Arc<Mutex<vt100::Parser>>,
    raw: Arc<Mutex<Vec<u8>>>,
    eof: Arc<AtomicBool>,
    theme: Theme,
    steps: Vec<trace::Step>,
    name: String,
    timeout: Duration,
    log_file: PathBuf,
    config_dir: PathBuf,
    _home: Option<TempDir>,
    exited: bool,
}

impl Vista {
    /// Launch vista on a fixture repo (shares the repo's private home).
    pub fn launch<'a>(bin: impl AsRef<Path>, repo: &'a Repo) -> Launcher<'a> {
        Launcher {
            bin: bin.as_ref().to_path_buf(),
            dir: repo.path().to_path_buf(),
            home: Home::Borrowed(repo.home()),
            cols: 120,
            rows: 36,
            theme: "darcula".into(),
            env: Vec::new(),
            args: Vec::new(),
            config: None,
            files: Vec::new(),
            name: None,
            open: None,
        }
    }

    /// Launch vista on any directory, with a fresh private home.
    pub fn launch_dir(bin: impl AsRef<Path>, dir: &Path) -> Launcher<'static> {
        Launcher {
            bin: bin.as_ref().to_path_buf(),
            dir: dir.to_path_buf(),
            home: Home::Owned(tempfile::tempdir().expect("home")),
            cols: 120,
            rows: 36,
            theme: "darcula".into(),
            env: Vec::new(),
            args: Vec::new(),
            config: None,
            files: Vec::new(),
            name: None,
            open: None,
        }
    }

    /// Process id of the running vista.
    pub fn pid(&self) -> Option<u32> {
        self.child.process_id()
    }

    pub fn theme(&self) -> &Theme {
        &self.theme
    }

    pub fn config_dir(&self) -> &Path {
        &self.config_dir
    }

    pub fn set_timeout(&mut self, t: Duration) {
        self.timeout = t;
    }

    fn record(&mut self, label: impl Into<String>) {
        let html = trace::screen_html(self.parser.lock().expect("parser").screen());
        self.steps.push(trace::Step {
            label: label.into(),
            html,
        });
    }

    // ----- input -------------------------------------------------------

    pub fn send_raw(&mut self, bytes: &[u8]) {
        self.writer.write_all(bytes).expect("write to pty");
        self.writer.flush().expect("flush pty");
    }

    /// Presses one key chord, e.g. `ctrl+p`, `enter`, `shift+f7`.
    pub fn press(&mut self, key: &str) {
        self.record(format!("press {key}"));
        let bytes = keys::encode(key);
        self.send_raw(&bytes);
        // A lone ESC is ambiguous; give the app a moment to see it as a key.
        if bytes == b"\x1b" {
            thread::sleep(Duration::from_millis(60));
        }
    }

    /// Presses several chords in order.
    pub fn press_all(&mut self, keys: &[&str]) {
        for k in keys {
            self.press(k);
        }
    }

    /// Types text as if from the keyboard.
    pub fn type_text(&mut self, text: &str) {
        self.record(format!("type {text:?}"));
        self.send_raw(text.as_bytes());
    }

    /// Pastes text (bracketed paste).
    pub fn paste(&mut self, text: &str) {
        self.record(format!("paste {text:?}"));
        let mut bytes = b"\x1b[200~".to_vec();
        bytes.extend_from_slice(text.as_bytes());
        bytes.extend_from_slice(b"\x1b[201~");
        self.send_raw(&bytes);
    }

    pub fn click(&mut self, col: u16, row: u16) {
        self.record(format!("click ({col}, {row})"));
        self.send_raw(&keys::mouse(0, col, row, true));
        self.send_raw(&keys::mouse(0, col, row, false));
    }

    pub fn double_click(&mut self, col: u16, row: u16) {
        self.record(format!("double-click ({col}, {row})"));
        for _ in 0..2 {
            self.send_raw(&keys::mouse(0, col, row, true));
            self.send_raw(&keys::mouse(0, col, row, false));
        }
    }

    pub fn scroll_down(&mut self, col: u16, row: u16) {
        self.record(format!("scroll down ({col}, {row})"));
        self.send_raw(&keys::mouse(65, col, row, true));
    }

    pub fn scroll_up(&mut self, col: u16, row: u16) {
        self.record(format!("scroll up ({col}, {row})"));
        self.send_raw(&keys::mouse(64, col, row, true));
    }

    /// Clicks the first occurrence of `text` on screen (waits for it).
    pub fn click_text(&mut self, text: &str) {
        let (row, col) = self.wait_for_pos(text);
        self.click(col, row);
    }

    pub fn resize(&mut self, cols: u16, rows: u16) {
        self.record(format!("resize {cols}x{rows}"));
        self.parser.lock().expect("parser").set_size(nz(rows), nz(cols));
        self.master
            .resize(PtySize {
                rows,
                cols,
                pixel_width: cols * 8,
                pixel_height: rows * 16,
            })
            .expect("resize pty");
    }

    // ----- reading the screen -----------------------------------------

    /// The whole screen as text (rows joined with `\n`).
    pub fn screen_text(&self) -> String {
        self.parser.lock().expect("parser").screen().contents()
    }

    /// Text of one row, with wide-character continuations skipped.
    pub fn row_text(&self, row: u16) -> String {
        row_string(self.parser.lock().expect("parser").screen(), row).0
    }

    pub fn size(&self) -> (u16, u16) {
        let p = self.parser.lock().expect("parser");
        let (r, c) = p.screen().size();
        (c.get(), r.get())
    }

    /// Everything the app has written so far, escape sequences included.
    pub fn raw_output(&self) -> Vec<u8> {
        self.raw.lock().expect("raw").clone()
    }

    pub fn clear_raw_output(&mut self) {
        self.raw.lock().expect("raw").clear();
    }

    /// Foreground colour of the cell at (col, row), if it's 24-bit.
    pub fn fg_at(&self, col: u16, row: u16) -> Option<Rgb> {
        let p = self.parser.lock().expect("parser");
        p.screen().cell(row, col).and_then(|c| rgb(c.fgcolor()))
    }

    pub fn bg_at(&self, col: u16, row: u16) -> Option<Rgb> {
        let p = self.parser.lock().expect("parser");
        p.screen().cell(row, col).and_then(|c| rgb(c.bgcolor()))
    }

    // ----- waiting & assertions ---------------------------------------

    fn fail(&mut self, msg: &str) -> ! {
        self.record(format!("FAILED: {msg}"));
        let screen = self.screen_text();
        let trace = self.write_trace(msg);
        panic!(
            "{msg}\n--- screen ---\n{screen}\n--- end screen ---\ntrace: {}\nlog:   {}",
            trace.map(|p| p.display().to_string()).unwrap_or_else(|| "-".into()),
            self.log_file.display()
        );
    }

    /// Polls until `pred(screen)` holds or the timeout passes.
    pub fn wait_until(&mut self, desc: &str, pred: impl Fn(&vt100::Screen) -> bool) {
        let start = Instant::now();
        loop {
            {
                let p = self.parser.lock().expect("parser");
                if pred(p.screen()) {
                    break;
                }
            }
            if start.elapsed() > self.timeout {
                self.fail(&format!("timed out after {:?} waiting for {desc}", self.timeout));
            }
            if self.eof.load(Ordering::SeqCst) && !self.exited {
                // One last look: the final frame may already satisfy pred.
                let ok = pred(self.parser.lock().expect("parser").screen());
                if ok {
                    break;
                }
                self.fail(&format!("vista exited while waiting for {desc}"));
            }
            thread::sleep(Duration::from_millis(15));
        }
        self.record(format!("✓ {desc}"));
    }

    fn wait_for_pos(&mut self, text: &str) -> (u16, u16) {
        let t = text.to_string();
        self.wait_until(&format!("text {text:?}"), move |s| find(s, &t).is_some());
        let p = self.parser.lock().expect("parser");
        find(p.screen(), text).expect("found")
    }

    /// Waits for `text` to be visible; returns a locator for further checks.
    pub fn expect_text(&mut self, text: &str) -> Locator<'_> {
        self.wait_for_pos(text);
        Locator {
            vista: self,
            text: text.to_string(),
            cols: 0..u16::MAX,
        }
    }

    /// Like [`expect_text`](Self::expect_text) but only matches text that
    /// starts within `cols` (e.g. inside the tree panel).
    pub fn expect_text_in(&mut self, text: &str, cols: std::ops::Range<u16>) -> Locator<'_> {
        let (t, c) = (text.to_string(), cols.clone());
        self.wait_until(&format!("text {text:?} in columns {cols:?}"), move |s| {
            find_in(s, &t, c.clone()).is_some()
        });
        Locator {
            vista: self,
            text: text.to_string(),
            cols,
        }
    }

    /// Waits until `text` is no longer visible.
    pub fn expect_no_text(&mut self, text: &str) {
        let t = text.to_string();
        self.wait_until(&format!("no text {text:?}"), move |s| find(s, &t).is_none());
    }

    /// Waits for `text` on a specific row.
    pub fn expect_row(&mut self, row: u16, text: &str) {
        let t = text.to_string();
        self.wait_until(&format!("row {row} to contain {text:?}"), move |s| {
            row_string(s, row).0.contains(&t)
        });
    }

    /// Waits for raw output to contain `needle` (e.g. an image escape).
    pub fn expect_raw(&mut self, needle: &[u8], desc: &str) {
        let start = Instant::now();
        loop {
            if contains(&self.raw.lock().expect("raw"), needle) {
                self.record(format!("✓ raw output contains {desc}"));
                return;
            }
            if start.elapsed() > self.timeout {
                self.fail(&format!("raw output never contained {desc}"));
            }
            thread::sleep(Duration::from_millis(15));
        }
    }

    /// Waits for the process to exit and returns its exit code.
    pub fn expect_exit(&mut self) -> u32 {
        let start = Instant::now();
        loop {
            if let Ok(Some(status)) = self.child.try_wait() {
                self.exited = true;
                self.record("✓ exited");
                return status.exit_code();
            }
            if start.elapsed() > self.timeout {
                self.fail("vista did not exit");
            }
            thread::sleep(Duration::from_millis(15));
        }
    }

    /// Lets the app settle: waits until the screen stops changing for `quiet`.
    pub fn settle(&mut self, quiet: Duration) {
        let mut last = self.screen_text();
        let mut since = Instant::now();
        let start = Instant::now();
        while since.elapsed() < quiet && start.elapsed() < self.timeout {
            thread::sleep(Duration::from_millis(15));
            let now = self.screen_text();
            if now != last {
                last = now;
                since = Instant::now();
            }
        }
    }

    /// The vista log so far (handy in assertion messages).
    pub fn log(&self) -> String {
        std::fs::read_to_string(&self.log_file).unwrap_or_default()
    }

    fn write_trace(&self, extra: &str) -> Option<PathBuf> {
        let dir = std::env::var_os("VISTA_E2E_TRACE_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| {
                // CARGO_TARGET_TMPDIR isn't available to libraries; walk up to `target`.
                let exe = std::env::current_exe().unwrap_or_default();
                exe.ancestors()
                    .find(|p| p.file_name().is_some_and(|n| n == "target"))
                    .map(|t| t.join("e2e-traces"))
                    .unwrap_or_else(|| std::env::temp_dir().join("vista-e2e-traces"))
            });
        let mut extra = extra.to_string();
        let log = self.log();
        if !log.is_empty() {
            extra.push_str("\n\n--- vista log ---\n");
            extra.push_str(&log);
        }
        trace::write(&dir, &self.name, &self.steps, &extra).ok()
    }
}

impl Drop for Vista {
    fn drop(&mut self) {
        if thread::panicking() && !self.steps.iter().any(|s| s.label.starts_with("FAILED")) {
            self.record("FAILED (panic in test)");
            let _ = self.write_trace("test panicked");
        }
        if !self.exited {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }
}

/// A piece of text on screen, re-located on every check (the screen may redraw).
pub struct Locator<'a> {
    vista: &'a mut Vista,
    text: String,
    /// Only match text starting in these columns.
    cols: std::ops::Range<u16>,
}

impl Locator<'_> {
    fn check(self, desc: &str, pred: impl Fn(&vt100::Cell) -> bool) {
        let text = self.text.clone();
        let cols = self.cols.clone();
        self.vista.wait_until(&format!("{text:?} {desc}"), move |s| {
            find_in(s, &text, cols.clone())
                .and_then(|(r, c)| s.cell(r, c))
                .is_some_and(&pred)
        });
    }

    /// Waits until the first character of the text has the token's foreground.
    pub fn to_have_fg(self, token: &str) {
        let want = self.vista.theme.color(token);
        self.check(&format!("to have fg {token} ({want})"), move |c| {
            rgb(c.fgcolor()) == Some(want)
        });
    }

    pub fn to_have_bg(self, token: &str) {
        let want = self.vista.theme.color(token);
        self.check(&format!("to have bg {token} ({want})"), move |c| {
            rgb(c.bgcolor()) == Some(want)
        });
    }

    pub fn to_be_bold(self) {
        self.check("to be bold", |c| c.bold());
    }

    /// (row, col) of the text right now.
    pub fn position(self) -> (u16, u16) {
        let p = self.vista.parser.lock().expect("parser");
        find_in(p.screen(), &self.text, self.cols.clone()).expect("text vanished")
    }
}

fn rgb(c: vt100::Color) -> Option<Rgb> {
    match c {
        vt100::Color::Rgb(r, g, b) => Some(Rgb(r, g, b)),
        _ => None,
    }
}

fn contains(hay: &[u8], needle: &[u8]) -> bool {
    needle.is_empty() || hay.windows(needle.len()).any(|w| w == needle)
}

/// A row's text plus, for each char index, the screen column it came from.
fn row_string(screen: &vt100::Screen, row: u16) -> (String, Vec<u16>) {
    let (_, cols) = screen.size();
    let mut s = String::new();
    let mut map = Vec::new();
    for c in 0..cols.get() {
        let Some(cell) = screen.cell(row, c) else { continue };
        if cell.is_wide_continuation() {
            continue;
        }
        let text = if cell.contents().is_empty() {
            " "
        } else {
            cell.contents()
        };
        for ch in text.chars() {
            s.push(ch);
            map.push(c);
        }
    }
    (s, map)
}

/// First (row, col) where `text` appears.
pub fn find(screen: &vt100::Screen, text: &str) -> Option<(u16, u16)> {
    find_in(screen, text, 0..u16::MAX)
}

/// First (row, col) where `text` appears starting within `cols`.
pub fn find_in(screen: &vt100::Screen, text: &str, cols: std::ops::Range<u16>) -> Option<(u16, u16)> {
    let (rows, _) = screen.size();
    for r in 0..rows.get() {
        let (s, map) = row_string(screen, r);
        for (byte_idx, _) in s.match_indices(text) {
            let col = map[s[..byte_idx].chars().count()];
            if cols.contains(&col) {
                return Some((r, col));
            }
        }
    }
    None
}
