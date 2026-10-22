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

/// Lua that defines `close()`: ends vista's diff tab page, if any, without
/// reporting it (the caller already knows).
const CLOSE_DIFF: &str = r#"
local function close()
  pcall(vim.api.nvim_del_augroup_by_name, 'vista_diff')
  for _, tp in ipairs(vim.api.nvim_list_tabpages()) do
    local ok, v = pcall(vim.api.nvim_tabpage_get_var, tp, 'vista_diff')
    local okb, b = pcall(vim.api.nvim_tabpage_get_var, tp, 'vista_diff_buf')
    if okb and vim.api.nvim_buf_is_valid(b) then
      for _, k in ipairs({ 'zi', 'zn', 'zN' }) do
        pcall(vim.keymap.del, 'n', k, { buffer = b })
      end
    end
    if ok and v then
      vim.api.nvim_set_current_tabpage(tp)
      vim.cmd('diffoff!')
      pcall(vim.cmd, 'tabclose')
    end
  end
  if vim.g.vista_stal ~= nil then
    vim.o.showtabline = vim.g.vista_stal
    vim.g.vista_stal = nil
  end
end
"#;

const MINIMAL: &str = "set number cursorline noswapfile hidden shortmess+=I laststatus=2";

pub struct Nvim {
    rpc: Rpc,
    child: Child,
    pub size: (u16, u16),
}

impl Nvim {
    /// Starts Neovim and attaches as a UI. Events arrive via `on_event`
    /// (from a background thread).
    pub fn spawn(opts: &SpawnOptions, on_event: impl Fn(NvimEvent) + Send + Sync + 'static) -> Result<Nvim, NvimError> {
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
    /// a 1-based line, with the window's `wrap` set when given.
    pub fn open(&self, path: &Path, line: Option<usize>, wrap: Option<bool>) -> Result<(), RpcError> {
        let line = line.map(|l| Value::from(l as u64)).unwrap_or(Value::Nil);
        let wrap = wrap.map(Value::from).unwrap_or(Value::Nil);
        self.lua(
            &format!(
                "{CLOSE_DIFF}
             close()
             local p, l, w = ...
             vim.cmd.edit(vim.fn.fnameescape(p))
             if w ~= nil then vim.wo.wrap = w end
             if l then pcall(vim.api.nvim_win_set_cursor, 0, {{ l, 0 }}) end"
            ),
            vec![Value::from(path.to_string_lossy().as_ref()), line, wrap],
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
    /// named `label`) in Neovim's diff mode, on a tab page of its own:
    /// aligned, scroll-bound, and re-diffed as you type. `:q` on either side
    /// closes both and sends [`NvimEvent::DiffClosed`].
    pub fn open_diff(
        &self,
        path: &Path,
        head: &str,
        label: &str,
        line: Option<usize>,
        wrap: bool,
        collapse: bool,
    ) -> Result<(), RpcError> {
        let line = line.map(|l| Value::from(l as u64)).unwrap_or(Value::Nil);
        let code = format!(
            "{CLOSE_DIFF}
local p, head, label, l, wrap, collapse = ...
close()
vim.g.vista_stal = vim.o.showtabline
vim.o.showtabline = 0
vim.cmd.tabedit(vim.fn.fnameescape(p))
vim.t.vista_diff = true
local work = vim.api.nvim_get_current_win()
vim.cmd('leftabove vnew')
local head_win = vim.api.nvim_get_current_win()
local buf = vim.api.nvim_get_current_buf()
local lines = vim.split(head, '\\n', {{ plain = true }})
if lines[#lines] == '' then table.remove(lines) end
vim.api.nvim_buf_set_lines(buf, 0, -1, false, lines)
vim.bo[buf].buftype = 'nofile'
vim.bo[buf].bufhidden = 'wipe'
vim.bo[buf].swapfile = false
vim.bo[buf].modifiable = false
vim.bo[buf].readonly = true
pcall(vim.api.nvim_buf_set_name, buf, label)
local ft = vim.filetype.match({{ filename = p }})
if ft then vim.bo[buf].filetype = ft end
-- Lines only in HEAD read as deleted (IntelliJ), not as added.
vim.wo[head_win].winhighlight = 'DiffAdd:VistaDiffDeleted'
for _, w in ipairs({{ head_win, work }}) do
  vim.wo[w].fillchars = 'diff:╱'
end
vim.cmd('diffthis')
vim.api.nvim_set_current_win(work)
vim.cmd('diffthis')
-- Diff mode turns wrapping off and folds unchanged lines; follow the
-- viewer's wrap and collapse settings instead.
for _, w in ipairs({{ head_win, work }}) do
  vim.wo[w].wrap = wrap
  vim.wo[w].foldenable = collapse
end
if l then pcall(vim.api.nvim_win_set_cursor, work, {{ l, 0 }}) end
local g = vim.api.nvim_create_augroup('vista_diff', {{ clear = true }})
-- `:set wrap` on one side applies to both, so they stay aligned.
vim.api.nvim_create_autocmd('OptionSet', {{
  group = g,
  pattern = 'wrap',
  callback = function()
    local v = vim.wo.wrap
    for _, w in ipairs({{ head_win, work }}) do
      if vim.api.nvim_win_is_valid(w) then vim.wo[w].wrap = v end
    end
  end,
}})
-- `zi`/`zn`/`zN` don't fire OptionSet: map them (diff buffers only) so they
-- also reach the other side and vista.
local work_buf = vim.api.nvim_win_get_buf(work)
vim.t.vista_diff_buf = work_buf
for _, b in ipairs({{ buf, work_buf }}) do
  for _, k in ipairs({{ 'zi', 'zn', 'zN' }}) do
    vim.keymap.set('n', k, function()
      vim.cmd('normal! ' .. k)
      local v = vim.wo.foldenable
      for _, w in ipairs({{ head_win, work }}) do
        if vim.api.nvim_win_is_valid(w) then vim.wo[w].foldenable = v end
      end
      vim.rpcnotify(0, 'vista_collapse', v)
    end, {{ buffer = b, desc = 'vista: fold unchanged lines' }})
  end
end
-- `:set foldenable` on one side applies to both, and to vista's collapse.
vim.api.nvim_create_autocmd('OptionSet', {{
  group = g,
  pattern = 'foldenable',
  callback = function()
    local v = vim.wo.foldenable
    for _, w in ipairs({{ head_win, work }}) do
      if vim.api.nvim_win_is_valid(w) then vim.wo[w].foldenable = v end
    end
    vim.rpcnotify(0, 'vista_collapse', v)
  end,
}})
vim.api.nvim_create_autocmd('WinClosed', {{
  group = g,
  pattern = {{ tostring(work), tostring(head_win) }},
  callback = function()
    vim.schedule(function()
      close()
      vim.rpcnotify(0, 'vista_diff_closed')
    end)
  end,
}})"
        );
        self.lua(
            &code,
            vec![
                Value::from(path.to_string_lossy().as_ref()),
                Value::from(head),
                Value::from(label),
                line,
                Value::from(wrap),
                Value::from(collapse),
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

/// Is a Neovim binary available?
pub fn available(bin: &str) -> bool {
    Command::new(bin)
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;

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
        nvim.open(&file, Some(1), None).unwrap();
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
