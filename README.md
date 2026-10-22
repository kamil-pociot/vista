# vista

A lean, keyboard-driven terminal replacement for the parts of IntelliJ IDEA you use every day: a project tree coloured by git status, a viewer for code / Markdown / images, Go to File, Find in Files, a commit window and an IntelliJ-style diff viewer — with Neovim embedded for editing.

```
vista [PATH]          open a folder (default: .)
vista path/to/file    open a file, with its folder as the project
vista --theme quiet   pick a look (darcula, quiet, paper, or your own)
vista --probe         check what your terminal supports (images, keyboard)
```

## Install

```sh
cargo install --path crates/vista-tui      # from a checkout
```

Requirements: `git` on `PATH`. Optional: `nvim` 0.10+ for editing (otherwise `$EDITOR` is used). Release binaries for macOS and Linux are built by `.github/workflows/release.yml` on `v*` tags; `packaging/homebrew/vista.rb` is a formula template for a tap.

Terminal: any truecolor terminal works. Images use the kitty graphics protocol (Ghostty, kitty, WezTerm) and fall back to coloured half-blocks. On macOS, set Ghostty's `macos-option-as-alt = true` so `Alt-…` shortcuts reach vista.

## Keys

| Where | Keys |
|---|---|
| Anywhere | `Ctrl-O` projects · `Alt-.`/`Alt-,` next/previous project · `Ctrl-P` Go to File · `Ctrl-F` Find in Files · `Ctrl-E` recent files · `Alt-1` Project · `Alt-0` Commit · `Alt-3` Find results · `Ctrl-K` commit · `Tab` next panel · `?` all shortcuts · `q` quit |
| Project tree | `⌖` in the header (or `Ctrl-L`) shows the opened file in the tree · arrows / `hjkl`, `Enter` or double-click open, `e` edit in Neovim, `i` show/hide ignored, `-` collapse all, `Ctrl-R` refresh |
| Viewer | `/` find in file (`n`/`N`), `Ctrl-G` go to line, `w` wrap ⇄ scroll sideways (`←`/`→`, shift+wheel, trackpad; in Markdown: wrapped ⇄ full-width tables), `m` Markdown raw/rendered, `d` diff vs HEAD, `]`/`[` next/previous change (at the last one, press again for the next changed file), `e` edit, `Ctrl-L` reveal in tree |
| Editor (Neovim) | everything goes to Neovim; `Ctrl-\` back to the tree; `:w` saves and refreshes git colours; long lines wrap if the viewer wrapped them, and `:set wrap` / `:set nowrap` carry back to the viewer |
| Go to File | type a few letters (`FoBaCo`, `stat`, `name:42`), `Enter` view, `Alt-Enter` edit, `Tab` scopes, `Alt-I` include ignored |
| Find in Files | `Alt-C` case · `Alt-W` words · `Alt-X` regex · `Alt-S` scope · `Tab` file mask (`*.md, !archive/**`) · `Alt-↑↓` history · `Enter` open · `Alt-O` to the Find window · `Ctrl-Alt-↓/↑` next/previous result |
| Commit | `Space` tick · `Tab` message · `Ctrl-K` commit · `m` amend · `P` commit & push · `r` rollback · `a` add to VCS · `g` group by folder · `Enter` or double-click diff |
| Diff | `]`/`[` changes (past the last one: press again for the next file; it stops, without wrapping, at the end of the last changed file) · `Space` tick/untick the change for the commit (partial commits) · `J`/`K` next/previous file · `←`/`→`, shift+wheel or trackpad scroll sideways · `w` wrap · `u` unified · `i` ignore whitespace · `z` collapse unchanged · `Enter` open · `e` edit side by side (Neovim diff mode: HEAD read-only left, working file right, re-diffed as you type; opens where the diff was, with line numbers; the sides stay aligned row by row — wrapped lines included — with blank rows on the shorter side and ╱ rows for missing lines, and scroll together; `:q` returns to the diff; it follows the diff's wrap and collapse settings, and `:set wrap` / `zi` there carry back) |

`Shift-L` shows/hides line numbers (code, and source lines of each block in rendered Markdown). Every shortcut can be changed in the config. No action needs an F-key: `F7`, `F1`, `F5` and `Alt-F1` still work, alongside `]`, `?`, `Ctrl-R` and `Ctrl-L`. Views scroll a little past the end of a file, so the last line can sit about 30% above the bottom edge, and show a scrollbar measured in screen rows (wrapped lines count once per row). Markdown files (and their diffs) open wrapped; code opens scrolling sideways.

## Configuration

`~/.config/vista/config.toml` (or `$VISTA_CONFIG_DIR`):

```toml
theme = "darcula"            # or quiet, paper, or a file in ~/.config/vista/themes/

[images]
protocol = "auto"            # auto | kitty | halfblocks | off

[editor]
config = "user"              # user = your init.lua, minimal = nvim --clean
theme_sync = true            # paint Neovim with the active look
nvim = "nvim"

[tree]
show_ignored = true
folder_colours = true

[keys]
"ctrl+o" = "goto-file"       # bind
"q" = "none"                 # unbind
```

### Projects

Several projects can be open at once, as tabs at the left of the top bar: click one to switch, its `×` to close it, `+` (or `Ctrl-O`) to open the Projects popup. There, type a few letters: open and recent projects match, and so do folders next to them (`lms` finds `~/Repositories/lms-context` when another project lives in `~/Repositories`). Or type part of a path (`~/Repositories/lm`): the folders there that match are listed, `Tab` completes the selected one to go deeper. `Enter` switches to, reopens or adds the selected folder; `Ctrl-W` closes the selected open project. `Alt-.` / `Alt-,` step through open projects. Each project keeps its own tree, tabs, Commit window, git status, file watcher and editor; settings and the look are shared. The projects open when vista quits are reopened next time (alongside the one you start it in).

### Remembered settings

View settings you change in the app are saved to `state.toml` next to `config.toml` and restored on the next run: line numbers, wrapping (separately for Markdown and code), the diff's unified/collapse/whitespace options, Commit-window grouping, showing ignored files, Find in Files case/words/regex, and the look picked with `Ctrl-T`. Until you change one, the default comes from `config.toml` (or vista's built-in). `--theme` and `VISTA_THEME` still win over the remembered look. Delete `state.toml` to reset.

## Looks

A look is one TOML file: a `[palette]` of semantic colour tokens (`git.modified`, `syntax.keyword`, `diff.word.inserted`, …) and `[chrome]` layout options (borders, panel headers, tabs, tool-window bar, git indicators, key hints, spacing). Built in: **darcula** (IntelliJ classic), **quiet** (borderless, modal), **paper** (light, rounded). Make your own:

```toml
# ~/.config/vista/themes/mine.toml
name = "Mine"
inherits = "darcula"
[palette]
"git.modified" = "#5FA8FF"
[chrome]
borders = "rounded"
```

Theme files reload live while vista runs; `Ctrl-T` cycles looks. See `crates/vista-theme/src/lib.rs` for every token.

## Development

```sh
cargo test --workspace                       # unit, snapshot, integration and e2e tests
cargo clippy --workspace --all-targets -- -D warnings
VISTA_PERF_REPO=~/some/repo cargo test --release --test perf -- --ignored --nocapture
```

End-to-end tests run the real binary in a pseudo-terminal (`crates/vista-e2e`): they type keys, click, and assert on the screen with auto-waiting locators. A failing test writes an HTML trace of every step to `target/e2e-traces/`. Snapshot tests use `insta` (`cargo insta review`). See `ROADMAP.md` for the plan and what each milestone covers.

| Crate | Role |
|---|---|
| `vista-core` | config, file tree, file watching |
| `vista-theme` | looks: palette tokens + chrome |
| `vista-git` | status, HEAD content, commit/amend/rollback/push (git CLI) |
| `vista-diff` | gutter markers, aligned side-by-side diffs, hunks |
| `vista-search` | Go to File index, Find in Files engine |
| `vista-render` | syntax highlighting, Markdown, file types |
| `vista-nvim` | embedded Neovim (msgpack-RPC, line grid) |
| `vista-tui` | the app (`vista` binary) |
| `vista-e2e` | PTY test harness and git fixtures |
