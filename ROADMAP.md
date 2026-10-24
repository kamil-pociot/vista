# vista — Research & Roadmap

A lean, keyboard-driven replacement for the parts of IntelliJ IDEA that are actually used day to day:

1. **Project view** — file tree with expandable folders, file names coloured by git status.
2. **Viewer** — images, rendered Markdown, source code with syntax highlighting.
3. **Editing** — through an embedded Neovim, when needed.
4. **Find file** — IntelliJ-style "Go to File" (type a few letters, fuzzy / camel-hump matching).
5. **Find in Files** — IntelliJ-style full-text search inside files (text, regex, case, words, mask, scope, preview).
6. **Commit view** — changed files (tracked + unversioned) with an IntelliJ-like side-by-side diff against `HEAD`, and committing.

Non-goals: language intelligence (LSP) beyond what your Neovim config brings, run configurations, plugins, debugger.

> Status: **approved.** Decisions are recorded in §0. Next step: M0.
> Design mockups: <https://claude.ai/artifact/UJ6tw99LFSzjDsXjW276Dr> (A was chosen).

---

## 0. Decisions (2026-10-21)

| # | Topic | Decision |
|---|---|---|
| 1 | UI design | **Design A — Darcula Classic** is the default look. The theming system must make further looks (B "Quiet Modal" and C "Paper Light" from the mockups, or new ones) cheap to add: see §2.1. |
| 2 | Editing | **Embedded Neovim** is part of the core plan (M3), not an optional extra. |
| 3 | Terminal | **Ghostty**, usually inside **herdr**; plain Ghostty is fine where herdr gets in the way. Target: kitty graphics + kitty keyboard protocols. See §1.6. |
| 4 | Commits | The commit view **makes commits** (commit, amend; push as an option). |
| 5 | Partial commits | Not now, but **planned** (M9, now done per change). The diff model keeps stable hunk identities from day one so it's an add-on, not a rewrite. |
| 6 | Repo size | Reference repo `lms-context`: **~4,000 files** (4,004 tracked, 4,083 not ignored, 4,266 total, 203 MB; 3,881 of them Markdown, 25.7 MB of text). |
| 7 | Name | **`vista`** (binary `vista`, crates `vista-*`, config `~/.config/vista/`). Not on Homebrew. On crates.io `vista` is taken, which only matters for `cargo install`, so we'd publish as `vista-tui`. |
| 8 | Search in files | Works **like IntelliJ's Find in Files** (M5, §1.3). Measured on `lms-context`: a full scan takes **~38 ms** for literal, word, case-insensitive and regex queries, so v1 needs **no index**. The engine is pluggable, and a trigram index (M10) is **deferred**: built only if a repo outgrows scanning. |
| 9 | Opening files | `Enter` opens the **read-only viewer** (fast; renders images and Markdown). Editing is optional: `e` hands the pane to embedded Neovim. |

---

## 1. Research summary

### 1.1 Terminal or desktop app?

**Conclusion: a terminal (TUI) app can deliver everything on the list.** The few compromises are cosmetic.

| Requirement | Terminal feasibility | Notes |
|---|---|---|
| File tree, expand/collapse, mouse | ✅ Full | Standard TUI territory (yazi, gitui, lazygit prove it). |
| Git-status colours | ✅ Full | Truecolor everywhere. |
| Code highlighting | ✅ Full | Same TextMate/Sublime grammars that `bat` and `delta` use. |
| Images | ✅ Good in Ghostty | Kitty graphics protocol. Inside herdr: works, with known rough edges (§1.6). Half-block fallback elsewhere. |
| Rendered Markdown | 🟡 Good, not "browser-pretty" | Bold/italic/colour, highlighted code blocks, tables, lists, links, inline images. **Not possible:** proportional fonts, real heading sizes, HTML blocks, Mermaid. |
| IntelliJ-like diff | 🟡 Very close | Aligned side-by-side, word-level highlights, gutter markers, next/prev change. **Not possible:** the curved connector ribbons, which are approximated by a coloured centre gutter (see mockup A). |
| Editing | ✅ Full | Embedded Neovim (§1.5). |
| Full-text search, fuzzy find | ✅ Full | Backend concerns; UI is lists + previews. |
| Keybindings | 🟡 Mostly | `Cmd` isn't available; we use `Ctrl`/`Alt`. Must not clash with herdr's prefix `Ctrl-b`. |

The architecture keeps a desktop front-end possible later: all logic lives in UI-agnostic crates.

### 1.2 Language: Rust + Ratatui

| Option | Verdict | Why |
|---|---|---|
| **Rust + Ratatui** | ✅ **Chosen** | Best ecosystem for exactly this app. Single static binary, instant startup, low memory, real parallelism. |
| Go + Bubble Tea | Runner-up | Nice TUI framework. Weaker image support and highlighting, and no ripgrep-library equivalent. |
| TypeScript (Bun) + OpenTUI/Ink | Possible | Heavier runtime and startup; thinner image and diff tooling. |
| Python + Textual | No | Slow startup, high memory on large repos. |

| Concern | Crate | Proven in |
|---|---|---|
| TUI framework | `ratatui` + `crossterm` | gitui, yazi, television |
| Images in terminal | `ratatui-image` (kitty / Sixel / iTerm2 / half-block) | — |
| Syntax highlighting | `syntect` + `two-face` | bat, delta |
| Markdown parsing | `pulldown-cmark` | mdBook, rustdoc |
| Fuzzy matching | `nucleo` | Helix, television |
| Directory walking / `.gitignore` | `ignore` | ripgrep, fd |
| Full-text search | `grep-searcher` + `grep-regex` | ripgrep |
| Git reads (status, blobs, HEAD tree) | `git` CLI (`status --porcelain=v2 -z`, `cat-file`) | *Changed from `gix` during M1: one code path for reads and writes, no heavy dependency; status of `lms-context` takes well under 100 ms.* |
| Git writes (stage, commit, rollback, push) | `git` CLI subprocess | Keeps hooks, signing and credential helpers working |
| Line/word diff | `imara-diff` + `similar` (intra-line) | gix, Helix |
| File watching | `notify` (+ debouncer) | Zed, watchexec |
| Neovim RPC | `nvim-rs` (msgpack-RPC, tokio) | Neovim GUI clients |
| Themes / config | `serde` + `toml` | — |

### 1.3 Full-text search strategy

**Goal:** IntelliJ's *Find in Files* behaviour: exact text or regex, case and whole-word toggles, file mask, scope, live results with preview.

**How IntelliJ does it:** a persistent **trigram index**, a map from every 3-character sequence to the files containing it. A query's trigrams narrow the search to a few candidate files, and only those are scanned to confirm matches. IntelliJ needs this because it's a heavyweight JVM IDE indexing huge projects anyway.

**Measured on `lms-context`** (whole repo, ripgrep engine, process start-up included, 2026-10-21):

| Query | Files matched | Time |
|---|---|---|
| `deploy` (case-insensitive) | 254 | 38 ms (80 ms on a cold cache) |
| `StatusMap` (literal) | 0 | 37 ms |
| `agent` (whole word, case-insensitive) | 124 | 38 ms |
| `TODO\|FIXME` (regex) | 27 | 38 ms |
| `retry.*backoff` (regex, case-insensitive) | 4 | 38 ms |

Anything under ~50 ms feels instant, and inside vista there's no process start-up. So:

- **v1 (M5), scan engine:** parallel, streaming search with the ripgrep libraries, respecting `.gitignore`, debounced and cancelled per keystroke. Always fresh, nothing on disk.
- **Pluggable engine:** `SearchEngine` trait with `Scan` and `TrigramIndex` implementations, and identical results (enforced by tests that compare both engines' output).
- **v2 (M10), trigram index:** built in the background and updated incrementally by the file watcher, so it's never stale. It narrows candidates; the ripgrep engine still confirms every match, so results never differ. `auto` mode switches to it when a full scan exceeds ~100 ms or a repo has 50k+ files. It's stored **outside the repo** in `~/Library/Caches/vista/<repo-id>/`, so nothing needs gitignoring. Expected size for `lms-context`: a few MB.

### 1.4 Prerequisites on this machine

- **Rust:** installed 2026-10-21 via `brew install rustup` + `rustup default stable` (rustc 1.99.1). Homebrew's rustup is keg-only, so add `export PATH="/opt/homebrew/opt/rustup/bin:$PATH"` to `~/.zshrc` to get `cargo` in your shell.
- **Neovim:** installed 2026-10-21 via `brew install neovim` (v0.12.6).
- Already present: git 2.54, fd, Homebrew, herdr 0.9.5. Standalone `ripgrep` isn't installed (the `rg` in Claude Code sessions is bundled); vista links the ripgrep libraries, so it isn't needed.

### 1.5 Editing: embedded Neovim

Building a pure Neovim *client* was rejected: the tree, git colours, search, commit view and diff aren't part of Neovim's protocol, so they'd still be our code. Instead, **Neovim is embedded as the editor inside vista's viewer pane**:

- One headless `nvim --embed` process per vista session, attached over msgpack-RPC with the **`ext_linegrid`** UI extension. Without `ext_multigrid`, Neovim composites floating windows (completion, telescope, etc.) into a single grid itself, so vista only renders one grid: cells, highlight attributes, cursor, mode.
- Files open as Neovim buffers (`:edit`), so undo history survives switching files.
- **Theme sync:** vista pushes its theme into Neovim (`nvim_set_hl`) so the editor matches Darcula exactly; or it keeps your colourscheme (config option).
- **Events:** an autocmd → `rpcnotify` on `BufWritePost` makes vista refresh git status, tree colours and gutter markers immediately.
- **Config:** uses your `init.lua` by default; `--clean`-style minimal profile available (`[editor] config = "user" | "minimal"`).
- **Focus model:** Viewer is read-only and instant (and renders images/Markdown). `Enter` opens files in the viewer; `e` hands the pane to Neovim; `Ctrl-\ Ctrl-n`-style escape chord or a configurable key returns focus to vista's tree/panels.
- **Fallback:** if `nvim` isn't on `PATH`, `e` opens `$EDITOR` full-screen (suspend/resume).

### 1.6 Ghostty + herdr

- **Ghostty** supports the kitty graphics protocol and the kitty keyboard protocol: both are first-class targets.
- **herdr** (0.9.5 installed) is a tmux-style multiplexer with its own terminal emulation. It implements kitty graphics, but its tracker shows open issues: images missing after re-attach (#5408), slow large images (#5442), and an unanswered case where the support query says OK but nothing renders (#5468).
- **Plan:** a protocol spike in M0 runs inside herdr *and* plain Ghostty. vista auto-detects support; `[images] protocol = "auto" | "kitty" | "halfblocks" | "off"` lets you force a mode. If herdr misbehaves, images degrade to half-blocks there and stay pixel-perfect in plain Ghostty. Everything else (colours, mouse, keys) is unaffected by herdr.
- vista never binds `Ctrl-b` (herdr's prefix).

---

## 2. Architecture

```
vista/                         (cargo workspace)
├── crates/
│   ├── vista-core/     workspace model, file tree, fs watcher, config, events
│   ├── vista-theme/    theme + chrome model, loading, built-in looks (themes/*.toml)
│   ├── vista-git/      status, HEAD blobs, change list, write ops via git CLI
│   ├── vista-diff/     line + word diff, side-by-side alignment, hunk ids
│   ├── vista-search/   fuzzy file finder (nucleo), Find in Files engine (grep-*)
│   ├── vista-index/    trigram index for Find in Files (M10)
│   ├── vista-render/   syntax highlighting, markdown → styled lines, image decoding
│   ├── vista-nvim/     embedded Neovim: RPC, linegrid model, theme sync
│   ├── vista-tui/      ratatui front-end (binary: `vista`)
│   └── vista-e2e/      PTY test harness + git fixtures (dev-only)
└── ROADMAP.md
```

- **UI-agnostic core.** `vista-tui` consumes view-models (tree rows, styled lines, diff rows), never git or fs directly.
- **Never block the UI thread.** Status, search, highlighting, image decoding and Neovim RPC run on workers and post results through channels.
- **Swappable viewer.** The viewer pane is a trait with implementations: code, markdown, image, diff, Neovim editor.
- **Lazy everything.** Tree nodes load on expand; highlighting covers the viewport (+ margin); big/binary files are detected.

### 2.1 Theming: making new designs cheap

The three mockups differ in two independent ways, and the system mirrors that split:

1. **Palette (theme)**: every colour in the UI is a *semantic token*, never a literal in widget code:
   `ui.bg`, `ui.panel`, `ui.border`, `ui.text`, `ui.dim`, `ui.selection`, `ui.accent`;
   `git.modified`, `git.added`, `git.unversioned`, `git.deleted`, `git.ignored`, `git.conflict`;
   `syntax.keyword`, `syntax.string`, `syntax.function`, `syntax.type`, `syntax.comment`, …;
   `diff.inserted`, `diff.modified`, `diff.deleted`, `diff.word.*`, `diff.filler`, `gutter.*`.
   Syntax scopes from syntect map onto these tokens, so **one file styles both the chrome and the code**; Neovim gets the same palette via theme sync.
2. **Chrome (layout & decoration)**: border style (`plain` / `rounded` / `none`), panel headers (`titled-bar` / `inline` / `none`), tab style (`underline` / `pill` / `text`), status-bar layout and segments, key-hint line on/off, tool-window strip (`bottom-bar` / `icon-rail` / `none`), git indicators (`colour` / `colour+letter`), tree glyphs, spacing (`compact` / `comfortable`).

A **look** = one TOML file with `[palette]` + `[chrome]` (+ optional `inherits = "darcula"`). Adding a design means writing a TOML file, not touching widget code. Built-in looks are embedded in the binary; user looks live in `~/.config/vista/themes/`. Hot reload while editing a theme file; `vista --theme quiet` or `:theme` to switch. A CI check fails if a widget uses a colour literal.

```toml
# themes/darcula.toml (excerpt)
name = "Darcula Classic"
[palette]
"ui.bg" = "#2B2B2B"
"ui.panel" = "#3C3F41"
"ui.border" = "#4A4D50"
"git.modified" = "#72A0C4"
"git.added" = "#6FAA61"
"git.unversioned" = "#E07B6E"
"syntax.keyword" = "#D58840"
"diff.modified" = "#2C3E55"
"diff.word.modified" = "#44679A"

[chrome]
borders = "plain"
panel_headers = "titled-bar"
tabs = "underline"
tool_windows = "bottom-bar"
git_indicator = "colour"
keyhints = false
```

### 2.2 Git status colours (design A defaults)

| Status | Colour |
|---|---|
| Modified / Renamed | blue `#72A0C4` |
| Added | green `#6FAA61` |
| Unversioned | red `#E07B6E` |
| Deleted | grey |
| Ignored | olive `#9C9D2C` |
| Conflicted | bright red |

Folders inherit a colour when they contain changes (toggleable).

### 2.3 Testing strategy

Terminal apps can be tested end to end much like Playwright tests a browser. The real `vista` binary runs inside a **pseudo-terminal (PTY)**, the test sends keystrokes, and a terminal-emulator library turns vista's output back into a grid of cells: text, colours, cursor. Assertions **auto-wait** for the screen to reach the expected state, like Playwright's `expect(...).toBeVisible()`. No sleeps, no flaky timing.

Five layers, fastest first:

| Layer | What it covers | Tools | When |
|---|---|---|---|
| 1. Unit | Pure logic: tree model, status mapping, fuzzy ranking, diff alignment, theme parsing, Neovim grid model | `cargo nextest`, `proptest` (e.g. "aligned diff rows always reconstruct both files") | every change, milliseconds |
| 2. Render snapshots | Each widget drawn into an in-memory terminal (`ratatui::TestBackend`); text **and colours** snapshotted, for every built-in look | `insta` + `cargo insta review` | every change, milliseconds |
| 3. Integration | Core crates against real git repos built by a fixture builder in a temp dir. Real `git status --porcelain=v2` and `rg --json` act as the **oracle**: our results must match theirs. | `tempfile`, `git`, `rg` | every change, seconds |
| 4. End-to-end | Real `vista` binary in a PTY, driven by Playwright-style scripts | in-repo `vista-e2e` harness (`portable-pty` + `vt100`) | every change, seconds |
| 5. Real-terminal smoke | Pixel images and keyboard protocol in Ghostty and Ghostty+herdr | short manual checklist | per release |

**What an e2e test looks like:**

```rust
#[test]
fn modified_file_turns_blue_and_commits() -> Result<()> {
    let repo = Fixture::rust_project().modify("src/status.rs", "// edit\n").build()?;
    let mut vista = Vista::launch(&repo).size(160, 45).theme("darcula").start()?;

    vista.expect_text("status.rs").to_have_fg(token("git.modified"))?;
    vista.press("alt+0")?;                                  // Commit tool window
    vista.expect_text("Changes  1 file").visible()?;
    vista.type_text("Fix status refresh")?;
    vista.press("ctrl+k")?;
    vista.expect_text("Committed 1 file").visible()?;

    assert_eq!(repo.git(["log", "-1", "--format=%s"])?, "Fix status refresh");
    Ok(())
}
```

**Harness features (`crates/vista-e2e`):**
- **Fixtures:** builder for git repos (files, commits, modifications, untracked, ignored, conflicts). Git runs hermetically (temp `HOME`, no global config, fixed author and dates), so screens are deterministic.
- **Locators + auto-wait:** `expect_text`, `expect_row(n)`, `to_have_fg/bg(token(..))`. Colours are asserted by **theme token**, not hex, so tests survive palette changes.
- **Input:** keys (including kitty-keyboard encoding), mouse clicks and scrolling at cells, paste, terminal resize.
- **Whole-screen golden snapshots** via `insta`.
- **Failure traces**, like Playwright's trace viewer: every step's screen is saved as a coloured HTML page with the action taken. Kept locally on failure and uploaded as a CI artifact.
- **Images:** the harness records vista's raw output. Tests assert that a kitty graphics sequence (`ESC _ G …`) is sent with the right size and placement, and that `protocol = "halfblocks"` draws block characters instead. Whether the pixels look right is the terminal's job, which is why layer 5 exists.
- **Neovim:** scenarios run with a pinned minimal profile (no user config) for determinism.
- **Performance budgets:** on a generated 50k-file fixture, first frame < 150 ms and Go to File keystroke-to-frame < 16 ms are asserted in e2e. `criterion` benches cover the hot paths.

**Why a small in-repo harness?** The low-level pieces are mature: `portable-pty` (from WezTerm) and the `vt100` emulator. The Playwright-like layer on top is a few hundred lines, and owning it lets us capture kitty graphics, assert theme tokens and produce HTML traces. Existing options get a look in M0 before we write our own:
- Microsoft's **tui-test**: closest to Playwright's API, but TypeScript, so it adds a Node toolchain.
- Rust crates **termlens**, **ratatui-testlib**, **testty**: young; we adopt one if it holds up.

A later upgrade could swap `vt100` for **libghostty-vt**, Ghostty's own parser, so e2e tests see exactly what Ghostty sees, including kitty graphics state.

**Rules**
- Every milestone ships with tests at layers 1–4. Each milestone below lists its e2e scenarios.
- Every bug fix starts with a failing test.
- Snapshot changes are reviewed, never blindly accepted.
- CI runs on macOS + Linux: `fmt`, `clippy`, `nextest`, snapshots, e2e, with traces uploaded on failure. The workflow file ships in M0 and goes live once the repo has a remote.

---

## 3. Roadmap

Each milestone ends in something usable. Design A is built first; B and C come in M8.

### M0 — Foundations ✅
- [x] Install Rust + Neovim; cargo workspace with the crates above.
- [x] App shell: alternate screen, event loop, panic hook restoring the terminal, `tracing` to a log file.
- [x] **Theme engine** (`vista-theme`): semantic tokens, chrome options, TOML loading, `inherits`, hot reload; `darcula.toml` built in.
- [x] Pane/focus model, configurable keymap (avoiding `Ctrl-b`), status bar, help overlay (`?`).
- [x] Config at `~/.config/vista/config.toml`.
- [x] **Terminal spike:** `vista --probe` (and `--probe-image`, which draws a test gradient) reports kitty graphics, kitty keyboard and cell size. **To record:** results from running it in herdr and in plain Ghostty.
- [x] CI: `fmt`, `clippy`, tests, "no colour literals in widgets" check (`.github/workflows/ci.yml`, live once the repo has a remote).
- [x] **Test infrastructure:** own `vista-e2e` harness (`portable-pty` + `vt100`, locators, auto-wait, HTML traces in `target/e2e-traces/`), repository `Fixture` builder, `insta` snapshots with colours as theme tokens. Plain `cargo test` runs everything, so nextest isn't required.

**Done when:** `vista [path]` opens design A's split layout and quits cleanly, in herdr and in plain Ghostty.
**Tests:** e2e: launch → first frame shows the layout → `q` quits and restores the terminal; resize redraws. Snapshot: empty layout in darcula. Unit: theme loading, `inherits`, invalid theme errors.

### M1 — Project tree with git colours ✅
- [x] Lazy tree, keyboard + mouse (click, arrow click, double-click, wheel); folders first, natural sort.
- [x] `.gitignore`-aware, ignored shown dimmed/olive italic (toggle `i`, or `[tree] show_ignored`), `.git` hidden.
- [x] Git status: file colours + folder roll-up; branch with ahead/behind in the top bar; change counts in the status bar.
- [x] File watcher: tree and status refresh on disk changes, including commits made outside vista (debounced).
- [x] Collapse all (`-`), refresh (`F5`/`Ctrl-R`). *(Reveal current file arrives with open files in M2.)*

**Done when:** `lms-context` browses instantly and colours match `git status`. Measured: first frame and branch+status both under 0.4 s on `lms-context` (debug build, `tests/perf.rs`).
**Tests:** integration: status of every fixture state vs `git status --porcelain=v1` (incl. 24 random edit sequences and merge conflicts). E2e: expand/collapse via keys and mouse; colours per status; an external file change shows up without a keypress; ignored toggle.

### M2 — Viewer (code / Markdown / images) ✅
- [x] Code: syntect (bat's grammars) mapped to theme tokens, line numbers, scroll, go-to-line (`Ctrl-G`), wrap toggle (`w`), horizontal scroll (`h`/`l`).
- [x] Git change markers in the gutter (added / modified / deleted vs HEAD); `F7`/`Shift-F7` jump between changes.
- [x] Large/binary file safety: >20 MB not loaded, binaries as a hex dump. Highlighting runs off the UI thread for the whole file (plain text above 2 MB); viewport-lazy highlighting wasn't needed at these sizes.
- [x] Markdown rendered view (headings, emphasis, lists, task lists, tables, quotes, highlighted fenced code, links, inline images) + raw toggle (`m`).
- [x] Images: png/jpg/gif/webp/bmp/ico/tiff via `ratatui-image` (kitty with unicode placeholders, half-block fallback, or off), fit-to-pane. SVG is shown as source.
- [x] Tabs (LRU limit 12, `Alt-←/→`, `Ctrl-W`, click), recent files (`Ctrl-E`, persisted per project), find in file (`/`, `n`/`N`, smart case), reveal in tree (`Alt-F1`), `vista <file>` opens a file.

**Done when:** opening any file in the tree shows the right view.
**Tests:** snapshots: highlighted Rust/TS/JSON, rendered Markdown, gutter markers. E2e: open code / Markdown / image (kitty sequence emitted; half-blocks when forced) / binary / huge file.

### M3 — Editing with embedded Neovim ✅
- [x] `vista-nvim`: spawn `nvim --embed`, own msgpack-RPC client, `ext_linegrid` grid model (cells, hl attrs, cursor, mode, scroll, resize).
- [x] Render the grid in the viewer pane; forward keys, paste and mouse while focused; `Ctrl-\` returns focus to the tree (tool-window keys still work).
- [x] `e` opens the selected tree file or the current viewer file (at the cursor line); switching tabs switches buffers; `●` unsaved marker in tabs and tree; `:q` closes the editor and returns to the viewer.
- [x] Theme sync (`nvim_set_hl` for ~50 groups from the active look) or keep the user colourscheme (`[editor] theme_sync = false`).
- [x] `BufWritePost` → `rpcnotify` → refresh status, tree colours, gutter; quitting with unsaved changes asks for a second `q`.
- [x] Config modes `user` / `minimal`; `$VISUAL`/`$EDITOR` fallback (terminal suspended and resumed) when `nvim` is missing.

**Done when:** a file can be opened, edited, saved, and its tree colour flips to "modified" right away.
**Tests:** unit: grid model fed recorded `redraw` events, key notation, a real-Neovim RPC round trip. E2e (real `nvim`): `e` → insert text → `:w` → file on disk changed, tree colour now `git.modified`; focus returns to tree; unsaved-changes marker and quit confirmation; theme sync; missing `nvim` falls back to `$EDITOR`.

### M4 — Go to File (fuzzy) ✅
- [x] Popup per mockup A: tabs `All / Files / Text / Recent` (`Tab`/`Shift-Tab`; `Text` hands the query to Find in Files, M5); `nucleo` over non-ignored paths, rebuilt in the background when files change; "include ignored" toggle (`Alt-I`).
- [x] IntelliJ-like ranking: camel-hump/word-start hits (nucleo bonuses), file-name matches over folder matches, exact names first, recent files boosted.
- [x] `name:42` jumps to a line; `Enter` opens in the viewer, `Alt-Enter` (or Shift/Ctrl-Enter with the kitty keyboard protocol) straight in Neovim; matched characters highlighted; preview of the selected file; mouse click/wheel.

**Done when:** any file is a few keystrokes away, each keystroke under 16 ms. Measured: ranking 50k paths < 16 ms (release unit test); keystroke → updated top result 22–27 ms through the PTY harness on a 50k-file project (the harness itself polls every 15 ms).
**Tests:** unit: ranking table (`stat` → `status.rs` first, camel-hump cases). E2e: `Ctrl-P`, type, `Enter` opens; `name:42` lands on line 42; keystroke latency budget on the 50k-file fixture.

### M5 — Find in Files (IntelliJ-style) ✅
Layout per mockup **A · Find in Files**. Behaviour mirrors IntelliJ's `Ctrl+Shift+F`:
- [x] **Popup:** query field prefilled from the viewer's find query or the last search (selected, so typing replaces it); `Alt-↑`/`Alt-↓` history (persisted); toggles **Match case** `Alt-C`, **Words** `Alt-W`, **Regex** `Alt-X`; **File mask** (`Tab` to edit) with several patterns and exclusions (`*.md, !archive/**`). *Not done: multi-line queries; prefill from a selection (the viewer has no text selection).*
- [x] **Scopes** (`Alt-S`): `In Project` · `Directory` (the folder selected in the tree) · `Changed files` (local changes, like IntelliJ) · `Open files`.
- [x] **Live results** while typing: line text with matches highlighted, file name (status-coloured) and line number right-aligned; "`N matches in M files`"; capped at 1,000.
- [x] **Preview pane** under the results: the file around the match, syntax-highlighted once the search finishes, match marked.
- [x] `Enter` opens the file at the match; `Alt-Enter` opens it in Neovim; `Alt-O` (or `Ctrl-Enter` with the kitty keyboard protocol) **"Open in Find tool window"**.
- [x] **Find tool window** (`3: Find`, `Alt-3`), docked at the bottom: results grouped by file, several result tabs (`←/→`, `Ctrl-W` closes), next/previous occurrence across files (`Ctrl-Alt-↓/↑`) even when the window is closed.
- [x] Respects `.gitignore` (hidden files included, `.git` never), skips binaries.
- [x] Search runs off the UI thread, streaming; a burst of keystrokes cancels and restarts it once.
- [ ] *(Later)* **Replace in Files** (`Ctrl-Shift-R`) with per-match preview, applied through Neovim buffers so it can be undone.

**Done when:** it feels like IntelliJ's Find in Files and results match `rg` on `lms-context`. Measured on `lms-context` (release): first results on screen in 22–32 ms, complete in 53–118 ms; engine alone ~30 ms per search (ripgrep CLI: 38 ms).
**Tests:** integration: results vs `git grep` (standalone ripgrep isn't installed everywhere; `git grep` is an independent implementation) across literal / case / word / regex / mask / directory-scope combinations, plus engine unit tests for scopes, cancellation and the result cap. E2e: popup, toggles, mask, regex errors, scopes, preview, open at line, Find tool window, `Ctrl-Alt-↓`, history, hand-over from Go to File. Latency: opt-in perf test on `lms-context`.

### M6 — Commit view ✅
- [x] `0: Commit` tool window (`Alt-0`, `Ctrl-K` from anywhere): **Changes** and **Unversioned Files** groups with counts, flat or by directory (`g`), collapsible.
- [x] File/folder/group checkboxes (`Space`, click; tracked changes ticked by default, unversioned not), commit message editor (`Tab`), **Commit** (`Ctrl-K`), **Amend** (`m`, prefills the last message), **Commit & Push** (`P`). Writes go through the `git` CLI, built in a temporary index so only ticked files go in and unticked staged files stay staged.
- [x] Rollback (`r`, with confirmation; added files are unstaged and kept), add unversioned to VCS (`a`), refresh. Hook output and errors are shown in the window.

**Done when:** a normal commit workflow happens without leaving vista.
**Tests:** e2e: groups and counts; untick a file → commit → `git log` and `git status` confirm only the ticked files went in; amend; failing pre-commit hook shows its output; rollback asks for confirmation; add to VCS; empty-message guard; commit & push to a remote. Integration: commit subsets, deletions, renames, partial content, amend, first commit, hooks, rollback, add, push.

### M7 — IntelliJ-style diff viewer ✅
- [x] Side-by-side: HEAD (left, read-only) vs working tree (right), one scroll position, **blocks aligned** with hatched filler where one side has no lines.
- [x] Block backgrounds: green (inserted), blue (modified), grey (deleted). **Word-level highlights** inside modified blocks (identifier-level tokens).
- [x] Centre gutter coloured per change (stand-in for IntelliJ's connector ribbons), cursor marker.
- [x] Syntax highlighting on both sides.
- [x] `F7`/`Shift-F7` between changes (opens on the first change, "change k of n"), `J`/`K` next/previous file in the Commit list, `Enter` opens the file at the line, `e` edits there.
- [x] Unified view (`u`), ignore whitespace (`w`), collapse unchanged regions (`z`).
- [x] New/unversioned files as all-added; deleted files as all-removed; binaries as a size summary; images side by side. Renames compare against the old path. Diffs follow edits on disk.
- [x] Hunks carry stable, content-based ids (groundwork for M9).

**Done when:** reviewing changes is as comfortable as in IntelliJ.
**Tests:** proptest: aligned rows reconstruct both sides exactly, applying all hunks yields the new text, folding never loses rows. Snapshot: the mockup-A diff (`status.rs`) as a golden screen. E2e: `F7` / `Shift-F7` navigation, word highlights, unified/whitespace/collapse toggles, new/deleted/image files, next file, live update, open at line.

### M8 — Polish, more looks, distribution ✅
- [x] Looks **B "Quiet Modal"** and **C "Paper Light"** ship as theme files only, proving §2.1. Gap found and fixed: boxed looks now frame the editor area too. A contrast test (WCAG) guards every built-in look (it caught Darcula's selection colour, now fixed).
- [x] Performance pass on `lms-context` (release): ready to draw ~2 ms after `main` (repository discovery no longer spawns git: 68 ms → 0.05 ms), first frame drawn in ~1 ms, git status and branch right after, ~17 MiB resident. Find in Files first results 22–32 ms.
- [x] Release binaries via `.github/workflows/release.yml` (macOS arm64/x86_64, Linux x86_64 on `v*` tags), Homebrew formula in `Formula/vista.rb` (written by `packaging/update-formula.sh` from a release; tap with `brew tap kamil-pociot/vista https://github.com/kamil-pociot/vista`), `cargo install --path crates/vista-tui` (crates.io name `vista-tui`; `vista` is taken). README with keys, config and looks.

### M9 — Partial commits ✅ (per change; per line still open)
- [x] Per-change checkboxes in the diff gutter, IntelliJ-style: ☑/☐ at the start of every change (side-by-side and unified), `Space` ticks/unticks the change under the cursor, the header shows "2 of 3 in commit". The Commit list shows `[~]` for a partly included file. Ticking a change of an unticked file includes only that change; unticking every change leaves the file out. Picking is off while ignore-whitespace is on, because hunks differ then.
- [ ] Per-line checkboxes inside a change: not done yet. The data model (excluded change ids per file) leaves room for it.
- [x] Commit only the selected changes, working tree untouched. Instead of a patch and `apply --cached`, vista rebuilds the file from HEAD plus the ticked changes (`vista_diff::partial_content`), stores it with `hash-object -w` and puts that blob in the temporary commit index. No patch context can go stale or fail to apply. Exclusions are stable content-hash ids, so they survive edits elsewhere in the file, and they reset after a successful commit.
- [x] Tests: proptest: no exclusions gives the new text, all excluded gives the old text, and excluding a change keeps exactly its old lines. E2e: tick 1 of 3 changes, commit, and `show HEAD:` holds exactly that change while the working tree still has all three and the diff shows "2 of 2". Also: tick a single change of an unticked file, and picking is blocked while ignoring whitespace.

### Feedback round 1 ✅
- [x] Long lines: `w` toggles wrap ⇄ scroll sideways in the viewer **and** the diff (both diff sides scroll together; wrapped diff rows stay aligned). Sideways: `←`/`→`, shift+wheel, trackpad. Ignore-whitespace in the diff moved to `i`.
- [x] Markdown tables show every value: cells wrap to fit with a rule between rows (default), or `w` keeps full-width rows that scroll sideways. Nothing is cut to `…` any more.
- [x] Scroll past the end: the last line can rise to ~30% above the bottom edge (viewer, rendered Markdown, diff).
- [x] Double-click opens from every list (tree, Commit window, Find results, Go to File); one 500 ms detector.
- [x] IntelliJ-style next/previous change across files: at the last change a popup says "Press ] again to go to b.txt"; the second press opens it on its first change (backwards: the previous file's last change). Works in diffs and in the plain viewer. Any other key dismisses the offer.
- [x] No F-keys needed: `]`/`[` changes, `?` help, `Ctrl-R` refresh, `Ctrl-L` reveal in tree. Hints and help show the short key first.
- [x] Fixed: clicking below the last Commit-list row panicked in debug builds (overflow).
- [x] Fixed: with wrap on, long files couldn't be scrolled to the end (limits counted lines, not rows). Scroll limits and keeping the cursor on screen now use each line's wrapped height, in the diff and the code viewer.
- [x] Scrollbar on code, rendered Markdown and diffs, sized and placed by screen rows, so it stays right with wrapped lines.
- [x] Tests: Markdown table units (wrap, rules, alignment, wide), scroll-limit and landing units, `e2e_r1_feedback` (7 PTY tests: diff sideways + wrap, viewer shift-wheel, tables both modes, scroll past end, double-click, cross-file changes in diff and viewer).

### Clickable labels ✅
- [x] Every label that names a window, option or key can be clicked and does what its key does: top and bottom bars, icon rail, key hints, Find and Go to File popups (scopes, toggles, fields, tabs, footers), Projects popup footer, Git window hints, Commit window (Amend, Commit, Commit & Push), confirm dialog, diff header and hints, find-in-file bar. Popups cover what's under them.
- [x] Tests: `ui::click` units (placement for every alignment, popups), `e2e_r1_clicks` (4 PTY tests).

### Tree speed search ✅
- [x] IntelliJ's speed search in the Project tree: typing selects matching rows (substring or word starts, matched letters highlighted), `↑`/`↓` step through matches, `Enter` opens, `Esc` ends; a `⌕` bar shows the query and the match count.
- [x] Letters now search in the tree, so `Alt-Enter` (edit), `Alt-I` (ignored files) and `Alt-L` (line numbers) were added; `Ctrl-Q` quits. `?`, `[`, `]` and `-` don't start a search.
- [x] Tests: `vista_search::speed` units, tree units, `e2e_m1_tree::typing_in_the_tree_searches_it`.

### Copy on select ✅
- [x] A mouse selection in the viewer (drag or double-click) is copied when the button is released, as in herdr; `[mouse] copy_on_select = false` turns it off.
- [x] `Cmd-C` / `Cmd-A` work like `Ctrl-C` / `Ctrl-A` when the terminal passes Cmd on. Ghostty only does that with `keybind = performable:super+c=copy_to_clipboard:mixed`: its default Cmd-C binding always takes the key.
- [x] Tests: `e2e_r1_selection` (copy on release, Cmd-C as the kitty keyboard protocol sends it).

### Open Settings ✅
- [x] `Ctrl-,` or `⚙` in the top bar opens `config.toml` for editing, written with every default first (a test keeps that template equal to the defaults).
- [x] Shift-Enter reaches terminal programs as `ESC CR` (programs inside vista can't use the kitty keyboard protocol).

### Back to the editor ✅
Agents now run in herdr beside vista, so vista's agent features were removed: terminal tabs (`claude agents`, `>command` in `Ctrl-O`, `[terminal] commands`), the `Alt-B` sidebar with Claude sessions and their states, passing desktop notifications on (and the herdr handoff), notification sounds (`notify_sound`, `notify_sound_done`) and the `●` attention marks on tabs. The Terminal tool window (`Alt-T`) stays. Doc number 18 (the sidebar) is retired, not reused.

### herdr's themes ✅
- [x] herdr's 17 themes as built-in looks under herdr's names (Catppuccin, Catppuccin Latte, Tokyo Night/Day, Dracula, Nord, Gruvbox/Light, One Dark/Light, Solarized/Light, Kanagawa/Lotus, Rosé Pine/Dawn, Vesper). Chrome colours measured from herdr (editor and tree on the terminal's background, herdr's surface, accent, text, dim and selected row); syntax and git from the published palettes; all pass the contrast test.
- [x] Look colours can be `"terminal"` (the terminal's own background), with `terminal_bg` as the colour assumed for contrast and blends.
- [x] `theme = "herdr"` follows herdr's `[theme]` live (and `auto_switch` with the macOS appearance).
- [x] Tests: `vista_core::herdr` units, the built-in contrast test over every look, `e2e_m8_looks::the_look_follows_herdrs_theme`.

### Preferences ✅
- [x] `Ctrl-,` / `⚙` opens a Preferences page instead of config.toml: the look (Follow herdr first), Fill background, viewer, diff, tree, Commit-window, Find and mouse settings, changed in place (keys or clicks) and remembered in `state.toml`; the last row opens config.toml.
- [x] Fill background: herdr's looks painted with their own background instead of the terminal's (`Theme::fill_terminal`).
- [x] The look picked last (Ctrl-T or Preferences, `herdr` included) wins over `theme = …` in config.toml.
- [x] Tests: `prefs` units, `e2e_r1_prefs` (3 PTY tests), `e2e_r1_settings` (Edit config.toml…).

### Linux CI ✅
- [x] Neovim 0.10+ is required: an older one (Ubuntu's apt 0.9.5, which made the Linux CI fail: no change reports, wrong diff layout) is refused with a reason and `$EDITOR` is used; tests skip their Neovim cases. CI installs Neovim 0.11 from its release on Linux.

### herdr-vista support (0.3.0)
- [x] `vista --only [PATH]`: just that project; `projects.open` / `active` aren't touched (recent still learns it).
- [x] Several vistas share `state.toml`: every save is read-modify-write under a lock on `state.toml.lock`, with a per-process temp file; each instance takes the merged result. Rust 1.89+ (`File::lock`).
- [x] Review panel (`vista --review <rev>`, `--review-label`, `Alt-R`): everything changed since a base (committed, uncommitted, untracked), each a read-only diff base ⇄ working tree; refreshed as files change; a `HEAD` base opens the Commit window.
- [x] Control socket `<runtime dir>/<pid>.sock` (`VISTA_RUNTIME_DIR` › `$XDG_RUNTIME_DIR/vista` › `$TMPDIR/vista-<uid>`): JSON lines `status`, `open-project`, `review`, `commit`, `open-file`, applied on the UI thread before the reply; removed on quit, `SIGTERM`, `SIGHUP`; `vista --remote`.
- [x] Version 0.3.0.

### Go to Line:Column ✅
- [x] `Ctrl-G` (`goto-line`, rebindable) from any panel opens IntelliJ's Go to Line:Column popup, filled in with the cursor's position and selected: `42`, `42:7`, `:7`; past the end goes to the last line / end of line.
- [x] A column puts a caret on that character (shown in the status bar, scrolled to sideways) that carries over to `e`.
- [x] Diffs count working-tree lines (HEAD's for a deleted file) and unfold a collapsed diff to show the line; the status bar shows file lines in diffs too. Rendered Markdown scrolls the line's block to the top. In Neovim, `Ctrl-G` opens the same popup from Neovim's cursor.
- [x] Tests: `goto_line`, `viewer`, `diffview` units, `e2e_r1_goto_line` (5 PTY tests).

### Keymap page ✅
- [x] Preferences gets a second page, Keymap (`Tab`, the Keymap… row, or a click): every action and its keys; press a key to add it (a key another action has asks before taking it), `Delete` removes the selected key, `r` / `R` reset one / all, `/` filters by name, action or key.
- [x] Applied at once in every open project; saved as `[keys]` in `state.toml`, on top of config.toml's `[keys]` (the defaults Reset goes back to).
- [x] Tests: `keymap` (key names round trip, bind/unbind/reset over the config), `keymap_page`, `app` units, `e2e_r1_keymap` (3 PTY tests).
- [x] Version 0.4.0 (with Go to Line:Column).

### M10 — Trigram index for Find in Files *(deferred: only if a repo outgrows scanning)*
- [ ] `vista-index`: a map from every 3-character sequence to the files containing it, kept in `~/Library/Caches/vista/<repo-id>/`. Built in the background, updated incrementally by the file watcher and on `HEAD` changes.
- [ ] Turn literal and regex queries into required trigrams; scan only candidate files with the ripgrep engine, so results are identical to M5.
- [ ] `[search] engine = "auto" | "scan" | "index"`. `auto` picks the index when a full scan exceeds ~100 ms or a repo has 50k+ files. Status bar shows indexing progress; search keeps scanning until the index is ready.
- [ ] Tests: differential (index results = scan results, on random queries over fixtures), crash/restart recovery, incremental update after edits/checkout, size and build-time budgets on the 50k-file fixture.

### Later / ideas
- Replace in Files; search in comments/strings only (IntelliJ's context filter).
- Git log & file history, blame ("Annotate"), branch switcher.
- Compare two files/revisions; edit the right side of a diff via Neovim.
- Desktop front-end reusing the core crates.

---

## 4. Remaining open questions

None. New questions get added here as they come up during implementation.
