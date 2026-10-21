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
| 5 | Partial commits | Not now, but **planned** (M9). The diff model keeps stable hunk identities from day one so it's an add-on, not a rewrite. |
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

**Done when:** any file is a few keystrokes away, each keystroke under 16 ms. Measured: ranking 50k paths < 16 ms (release unit test); keystroke-to-screen 2.3 ms worst case on a 50k-file project (e2e).
**Tests:** unit: ranking table (`stat` → `status.rs` first, camel-hump cases). E2e: `Ctrl-P`, type, `Enter` opens; `name:42` lands on line 42; keystroke latency budget on the 50k-file fixture.

### M5 — Find in Files (IntelliJ-style)
Layout per mockup **A · Find in Files**. Behaviour mirrors IntelliJ's `Ctrl+Shift+F`:
- [ ] **Popup:** query field (prefilled from the selection or the word under the cursor; `↑`/`↓` search history; multi-line queries) with toggles **Match case** `Alt-C`, **Words** `Alt-O`, **Regex** `Alt-X`; **File mask** with several patterns and exclusions (`*.md, !archive/**`).
- [ ] **Scopes:** `In Project` · `Directory` (defaults to the folder selected in the tree; recursive toggle) · `Changed files` · `Open files`.
- [ ] **Live results** while typing: one row per match with the line text and match highlighted, file name and line number right-aligned; status-coloured file names; counter "`254 matches in 38 files`"; results capped at 1,000 with a "show more" option.
- [ ] **Preview pane** under the results, the file scrolled to the match with every occurrence highlighted.
- [ ] `Enter` opens the file at the match (viewer, or Neovim when editing); `Ctrl-Enter` **"Open in Find tool window"**.
- [ ] **Find tool window** (`3: Find`), docked at the bottom: persistent results tree grouped by directory → file → match, several result tabs, and next/previous occurrence across files (`Ctrl-Alt-↓/↑`) while the window is closed.
- [ ] Respects `.gitignore` and excludes (ignored files opt-in), skips binaries, Markdown searched as raw text.
- [ ] Search runs off the UI thread, streaming; each keystroke cancels the previous search.
- [ ] *(Later)* **Replace in Files** (`Ctrl-Shift-R`) with per-match preview, applied through Neovim buffers so it can be undone.

**Done when:** it feels like IntelliJ's Find in Files and results match `rg` on `lms-context`.
**Tests:** integration: results vs `rg --json` across literal / regex / case / word / mask / scope combinations; cancellation; result cap. E2e: `Ctrl-Shift-F` → type → toggles change counts → preview follows selection → `Enter` opens at the line; `Ctrl-Enter` → tool window → `Ctrl-Alt-↓` walks matches; history recall. Latency budget: first results < 100 ms on `lms-context`.

### M6 — Commit view
- [ ] `0: Commit` tool window: **Changes** and **Unversioned Files** groups, flat or by directory.
- [ ] File-level checkboxes, commit message editor (single-line fast path; full editor via Neovim), **Commit**, **Amend**, optional **Commit & Push**. All writes go through the `git` CLI.
- [ ] Rollback (confirm), add unversioned to VCS, refresh.

**Done when:** a normal commit workflow happens without leaving vista.
**Tests:** e2e: groups and counts; untick a file → commit → `git log` and `git status` confirm only the ticked files went in; amend; failing pre-commit hook shows its output; rollback asks for confirmation.

### M7 — IntelliJ-style diff viewer
- [ ] Side-by-side HEAD ⇄ working tree, synced scroll, aligned blocks with hatched filler.
- [ ] Block colours (inserted / modified / deleted) and word-level highlights from theme tokens; centre gutter.
- [ ] Syntax highlighting both sides; `F7`/`Shift-F7` change navigation; next/prev file.
- [ ] Unified toggle, ignore-whitespace toggle, collapse unchanged.
- [ ] New/deleted/binary/image files handled.
- [ ] Hunks carry stable ids (groundwork for M9).

**Done when:** reviewing changes is as comfortable as in IntelliJ.
**Tests:** proptest: aligned rows reconstruct both sides exactly. Snapshots: the mockup-A diff (`status.rs`) as a golden screen. E2e: `F7` / `Shift-F7` navigation, unified toggle, whitespace toggle, new/deleted/binary files.

### M8 — Polish, more looks, distribution
- [ ] Ship looks **B "Quiet Modal"** and **C "Paper Light"** as theme files, proving §2.1. Any gap found in the chrome options gets fixed here.
- [ ] Performance pass (startup, memory, status refresh).
- [ ] Release binaries, Homebrew tap, `cargo install`.

### M9 — Partial commits
- [ ] Per-hunk (then per-line) checkboxes in the diff gutter, IntelliJ-style.
- [ ] Commit only the selected hunks: build a patch and `git apply --cached`, then commit; working tree untouched.
- [ ] Tests: e2e: tick 1 of 3 hunks → commit → `git show` holds exactly that hunk; working tree still has the other two.

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
