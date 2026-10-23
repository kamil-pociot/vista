//! Neovim embedded as vista's editor.
//!
//! One headless `nvim --embed` process per vista session. vista attaches as a
//! UI with `ext_linegrid`, renders the grid Neovim draws, forwards keys, and
//! listens for writes so the tree and status refresh immediately.

pub mod grid;
pub mod keys;
pub mod rpc;

use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::Duration;

use rmpv::Value;

pub use grid::{Cell, Grid, HlAttr};
pub use keys::{Key, Mods, notation};
pub use rpc::{Notification, Rpc, RpcError};

/// What Neovim tells vista.
#[derive(Debug, Clone, PartialEq)]
pub enum NvimEvent {
    /// A batch of UI events for the grid.
    Redraw(Vec<Value>),
    /// A buffer was written to disk.
    Written(PathBuf),
    /// A buffer's 'modified' flag changed.
    Modified(PathBuf, bool),
    /// The process ended (`:q`, crash, …).
    Exited,
    /// The side-by-side diff editor was closed from inside Neovim (`:q`).
    DiffClosed,
    /// `wrap` was switched in a window showing `path` (`diff`: in the
    /// side-by-side diff editor).
    Wrap { path: PathBuf, wrap: bool, diff: bool },
    /// Folding of unchanged lines was switched in the diff editor (`zi`).
    Collapse(bool),
}

#[derive(Debug, thiserror::Error)]
pub enum NvimError {
    #[error("cannot start `{bin}`: {source}")]
    Spawn {
        bin: String,
        #[source]
        source: std::io::Error,
    },
    #[error("`{bin}` is Neovim {version}; vista needs {}.{} or newer", MIN_VERSION.0, MIN_VERSION.1)]
    TooOld { bin: String, version: String },
    #[error(transparent)]
    Rpc(#[from] RpcError),
}

#[derive(Debug, Clone)]
pub struct SpawnOptions {
    pub bin: String,
    /// `--clean`: no user config or plugins.
    pub minimal: bool,
    pub cwd: PathBuf,
    pub width: u16,
    pub height: u16,
}

/// A highlight group definition for `nvim_set_hl` (colours as 0xRRGGBB).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct HlSpec {
    pub fg: Option<u32>,
    pub bg: Option<u32>,
    pub bold: bool,
    pub italic: bool,
}

const TIMEOUT: Duration = Duration::from_secs(10);

const AUTOCMDS: &str = r#"
augroup vista
  autocmd!
  autocmd BufWritePost * call rpcnotify(0, 'vista_written', expand('<afile>:p'))
  autocmd BufModifiedSet * call rpcnotify(0, 'vista_modified', expand('<afile>:p'), &modified)
  autocmd OptionSet wrap call rpcnotify(0, 'vista_wrap', expand('%:p'), &l:wrap ? v:true : v:false, get(t:, 'vista_diff', v:false) ? v:true : v:false)
augroup END
"#;

/// Lua that defines `close()` (ends vista's diff tab page, if any).
const CLOSE_DIFF: &str = include_str!("close_diff.lua");

/// Lua that opens the side-by-side diff editor (after [`CLOSE_DIFF`]).
const OPEN_DIFF: &str = include_str!("open_diff.lua");

const MINIMAL: &str = "set number cursorline noswapfile hidden shortmess+=I laststatus=2";

/// Where and how the side-by-side diff editor opens.
#[derive(Debug, Clone, Copy, Default)]
pub struct DiffView {
    /// Cursor line (1-based).
    pub line: Option<usize>,
    /// First visible line (1-based).
    pub top: Option<usize>,
    /// First visible line on the HEAD side instead (1-based), when the view
    /// starts on lines only HEAD has.
    pub top_head: Option<usize>,
    pub leftcol: usize,
    pub wrap: bool,
    /// Fold unchanged lines.
    pub collapse: bool,
}

pub struct Nvim {
    rpc: Rpc,
    child: Child,
    pub size: (u16, u16),
}

impl Nvim {
    /// Starts Neovim and attaches as a UI. Events arrive via `on_event`
    /// (from a background thread).
    pub fn spawn(opts: &SpawnOptions, on_event: impl Fn(NvimEvent) + Send + Sync + 'static) -> Result<Nvim, NvimError> {
        // An older Neovim starts but misbehaves (no change reports, diff
        // layout): better the $EDITOR fallback with a reason.
        if let Some(v) = version(&opts.bin)
            && (v.0, v.1) < MIN_VERSION
        {
            return Err(NvimError::TooOld {
                bin: opts.bin.clone(),
                version: format!("{}.{}.{}", v.0, v.1, v.2),
            });
        }
        let mut cmd = Command::new(&opts.bin);
        cmd.arg("--embed").arg("-n");
        if opts.minimal {
            cmd.arg("--clean");
        }
        cmd.current_dir(&opts.cwd)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        let mut child = cmd.spawn().map_err(|source| NvimError::Spawn {
            bin: opts.bin.clone(),
            source,
        })?;
        let stdin = child.stdin.take().expect("piped stdin");
        let stdout = child.stdout.take().expect("piped stdout");
        let on_event = std::sync::Arc::new(on_event);
        let notify = on_event.clone();
        let rpc = Rpc::start(
            stdin,
            stdout,
            move |n| {
                let ev = match n.method.as_str() {
                    "redraw" => NvimEvent::Redraw(n.params),
                    "vista_written" => match n.params.first().and_then(Value::as_str) {
                        Some(p) => NvimEvent::Written(PathBuf::from(p)),
                        None => return,
                    },
                    "vista_modified" => {
                        let path = n.params.first().and_then(Value::as_str).unwrap_or_default();
                        let modified = n
                            .params
                            .get(1)
                            .and_then(|v| v.as_bool().or_else(|| v.as_i64().map(|i| i != 0)))
                            .unwrap_or(false);
                        NvimEvent::Modified(PathBuf::from(path), modified)
                    }
                    "vista_diff_closed" => NvimEvent::DiffClosed,
                    "vista_collapse" => NvimEvent::Collapse(
                        n.params
                            .first()
                            .and_then(|v| v.as_bool().or_else(|| v.as_i64().map(|i| i != 0)))
                            .unwrap_or(false),
                    ),
                    "vista_wrap" => {
                        let flag = |i: usize| {
                            n.params
                                .get(i)
                                .and_then(|v| v.as_bool().or_else(|| v.as_i64().map(|i| i != 0)))
                                .unwrap_or(false)
                        };
                        NvimEvent::Wrap {
                            path: PathBuf::from(n.params.first().and_then(Value::as_str).unwrap_or_default()),
                            wrap: flag(1),
                            diff: flag(2),
                        }
                    }
                    _ => return,
                };
                notify(ev);
            },
            move || on_event(NvimEvent::Exited),
        );
        let nvim = Nvim {
            rpc,
            child,
            size: (opts.width, opts.height),
        };
        let attach_opts = Value::Map(vec![
            (Value::from("rgb"), Value::from(true)),
            (Value::from("ext_linegrid"), Value::from(true)),
        ]);
        nvim.rpc.request(
            "nvim_ui_attach",
            vec![Value::from(opts.width), Value::from(opts.height), attach_opts],
            TIMEOUT,
        )?;
        nvim.exec(AUTOCMDS)?;
        if opts.minimal {
            nvim.exec(MINIMAL)?;
        }
        Ok(nvim)
    }

    /// Runs Vimscript.
    pub fn exec(&self, src: &str) -> Result<(), RpcError> {
        self.rpc.request(
            "nvim_exec2",
            vec![
                Value::from(src),
                Value::Map(vec![(Value::from("output"), Value::from(false))]),
            ],
            TIMEOUT,
        )?;
        Ok(())
    }

    /// Runs Lua and returns its result.
    pub fn lua(&self, code: &str, args: Vec<Value>) -> Result<Value, RpcError> {
        self.rpc
            .request("nvim_exec_lua", vec![Value::from(code), Value::Array(args)], TIMEOUT)
    }

    /// Edits `path` (switching to its buffer if already loaded), optionally at
    /// a 1-based line, with the window's `wrap` and `number` set when given.
    pub fn open(
        &self,
        path: &Path,
        line: Option<usize>,
        wrap: Option<bool>,
        number: Option<bool>,
    ) -> Result<(), RpcError> {
        let line = line.map(|l| Value::from(l as u64)).unwrap_or(Value::Nil);
        let wrap = wrap.map(Value::from).unwrap_or(Value::Nil);
        let number = number.map(Value::from).unwrap_or(Value::Nil);
        self.lua(
            &format!(
                "{CLOSE_DIFF}
             close()
             local p, l, w, nu = ...
             -- Missing values arrive as vim.NIL, which is truthy.
             if l == vim.NIL then l = nil end
             if w == vim.NIL then w = nil end
             if nu == vim.NIL then nu = nil end
             vim.cmd.edit(vim.fn.fnameescape(p))
             if w ~= nil then vim.wo.wrap = w end
             if nu ~= nil then vim.wo.number = nu end
             if l then pcall(vim.api.nvim_win_set_cursor, 0, {{ l, 0 }}) end"
            ),
            vec![Value::from(path.to_string_lossy().as_ref()), line, wrap, number],
        )?;
        Ok(())
    }

    /// Sends keys (Neovim notation). Doesn't wait.
    pub fn input(&self, keys: &str) -> Result<(), RpcError> {
        self.rpc.request_async("nvim_input", vec![Value::from(keys)])
    }

    pub fn paste(&self, text: &str) -> Result<(), RpcError> {
        self.rpc.request_async(
            "nvim_paste",
            vec![Value::from(text), Value::from(false), Value::from(-1)],
        )
    }

    /// Mouse input: button `left|right|middle|wheel`, action `press|drag|release|up|down`.
    pub fn mouse(&self, button: &str, action: &str, mods: &str, row: u16, col: u16) -> Result<(), RpcError> {
        self.rpc.request_async(
            "nvim_input_mouse",
            vec![
                Value::from(button),
                Value::from(action),
                Value::from(mods),
                Value::from(0),
                Value::from(row),
                Value::from(col),
            ],
        )
    }

    pub fn resize(&mut self, width: u16, height: u16) -> Result<(), RpcError> {
        if (width, height) == self.size || width == 0 || height == 0 {
            return Ok(());
        }
        self.size = (width, height);
        self.rpc
            .request_async("nvim_ui_try_resize", vec![Value::from(width), Value::from(height)])
    }

    /// Defines highlight groups (theme sync).
    pub fn set_highlights(&self, groups: &[(&str, HlSpec)]) -> Result<(), RpcError> {
        for (name, spec) in groups {
            let mut m = Vec::new();
            if let Some(fg) = spec.fg {
                m.push((Value::from("fg"), Value::from(fg)));
            }
            if let Some(bg) = spec.bg {
                m.push((Value::from("bg"), Value::from(bg)));
            }
            if spec.bold {
                m.push((Value::from("bold"), Value::from(true)));
            }
            if spec.italic {
                m.push((Value::from("italic"), Value::from(true)));
            }
            self.rpc
                .request_async("nvim_set_hl", vec![Value::from(0), Value::from(*name), Value::Map(m)])?;
        }
        Ok(())
    }

    /// Opens `path` side by side with its HEAD version (`head`, read-only,
    /// named `label`) in Neovim's diff mode, on a tab page of its own,
    /// re-diffed as you type. vista aligns the sides itself, so wrapped lines
    /// line up too (blank rows on the shorter side, ╱ rows for missing lines).
    /// `view` places it like vista's diff was: 1-based cursor and top line,
    /// and left column. `:q` on either side closes both and sends
    /// [`NvimEvent::DiffClosed`].
    pub fn open_diff(&self, path: &Path, head: &str, label: &str, view: DiffView) -> Result<(), RpcError> {
        let opt = |v: Option<usize>| v.map(|l| Value::from(l as u64)).unwrap_or(Value::Nil);
        self.lua(
            &format!("{CLOSE_DIFF}\n{OPEN_DIFF}"),
            vec![
                Value::from(path.to_string_lossy().as_ref()),
                Value::from(head),
                Value::from(label),
                opt(view.line),
                opt(view.top),
                opt(view.top_head),
                Value::from(view.leftcol as u64),
                Value::from(view.wrap),
                Value::from(view.collapse),
            ],
        )?;
        Ok(())
    }

    /// Closes the side-by-side diff editor, if open.
    pub fn close_diff(&self) -> Result<(), RpcError> {
        self.lua(&format!("{CLOSE_DIFF}\nclose()"), vec![])?;
        Ok(())
    }

    /// Paths of buffers with unsaved changes.
    pub fn modified_buffers(&self) -> Result<Vec<PathBuf>, RpcError> {
        let v = self.lua(
            "local out = {}
             for _, b in ipairs(vim.api.nvim_list_bufs()) do
               if vim.bo[b].modified and vim.api.nvim_buf_get_name(b) ~= '' then
                 table.insert(out, vim.api.nvim_buf_get_name(b))
               end
             end
             return out",
            vec![],
        )?;
        Ok(v.as_array()
            .map(|a| a.iter().filter_map(|x| x.as_str()).map(PathBuf::from).collect())
            .unwrap_or_default())
    }

    /// Asks Neovim to quit, discarding changes, and reaps the process.
    pub fn quit(&mut self) {
        let _ = self.rpc.request_async("nvim_command", vec![Value::from("qa!")]);
        for _ in 0..50 {
            if let Ok(Some(_)) = self.child.try_wait() {
                return;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl Drop for Nvim {
    fn drop(&mut self) {
        if let Ok(None) = self.child.try_wait() {
            self.quit();
        }
    }
}

/// The oldest Neovim vista works with.
pub const MIN_VERSION: (u32, u32) = (0, 10);

/// Is a Neovim vista can use available (it runs, and is new enough)?
pub fn available(bin: &str) -> bool {
    version(bin).is_some_and(|v| (v.0, v.1) >= MIN_VERSION)
}

/// The version `bin --version` reports, if it runs and says one.
pub fn version(bin: &str) -> Option<(u32, u32, u32)> {
    let out = Command::new(bin)
        .arg("--version")
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()
        .filter(|o| o.status.success())?;
    parse_version(String::from_utf8_lossy(&out.stdout).lines().next()?)
}

/// `NVIM v0.11.4` (or `NVIM v0.12.0-dev-1234+gabc`) → (0, 11, 4).
fn parse_version(line: &str) -> Option<(u32, u32, u32)> {
    let v = line.trim().strip_prefix("NVIM v")?;
    let mut parts = v.split(|c: char| !c.is_ascii_digit()).map(|p| p.parse::<u32>().ok());
    Some((parts.next()??, parts.next()??, parts.next()??))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;

    #[test]
    fn reads_neovims_version_line() {
        assert_eq!(parse_version("NVIM v0.11.4"), Some((0, 11, 4)));
        assert_eq!(parse_version("NVIM v0.9.5"), Some((0, 9, 5)));
        assert_eq!(parse_version("NVIM v0.12.0-dev-1234+gabcdef"), Some((0, 12, 0)));
        assert_eq!(parse_version("vim 9.1"), None);
        assert!((0, 9) < MIN_VERSION && (0, 10) >= MIN_VERSION && (1, 0) >= MIN_VERSION);
        assert!(!available("/nonexistent/nvim"));
    }

    /// Talks to a real Neovim when one is installed.
    #[test]
    fn spawns_attaches_edits_and_reports_writes() {
        if !available("nvim") {
            eprintln!("nvim not installed; skipping");
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().canonicalize().unwrap().join("a.txt");
        std::fs::write(&file, "hello\n").unwrap();
        let (tx, rx) = mpsc::channel();
        let mut nvim = Nvim::spawn(
            &SpawnOptions {
                bin: "nvim".into(),
                minimal: true,
                cwd: dir.path().to_path_buf(),
                width: 40,
                height: 10,
            },
            move |e| {
                let _ = tx.send(e);
            },
        )
        .unwrap();
        nvim.open(&file, Some(1), None, None).unwrap();
        nvim.input("A world<Esc>").unwrap();
        let mut grid = Grid::default();
        let mut modified = false;
        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        while std::time::Instant::now() < deadline && !(grid.text().contains("hello world") && modified) {
            match rx.recv_timeout(Duration::from_millis(200)) {
                Ok(NvimEvent::Redraw(p)) => grid.apply_redraw(&p),
                Ok(NvimEvent::Modified(_, m)) => modified = m,
                _ => {}
            }
        }
        assert!(grid.text().contains("hello world"), "grid:\n{}", grid.text());
        assert!(modified);
        assert_eq!(nvim.modified_buffers().unwrap(), vec![file.clone()]);
        nvim.input(":w<CR>").unwrap();
        let written = loop {
            match rx.recv_timeout(Duration::from_secs(10)).expect("event") {
                NvimEvent::Written(p) => break p,
                NvimEvent::Redraw(p) => grid.apply_redraw(&p),
                _ => {}
            }
        };
        assert_eq!(written, file);
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "hello world\n");
        nvim.quit();
    }
}
