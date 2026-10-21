# fb — Research & Roadmap

A lean, keyboard-driven replacement for the parts of IntelliJ IDEA that are actually used day to day:

1. **Project view** — file tree with expandable folders, file names coloured by git status.
2. **Viewer** — images, rendered Markdown, source code with syntax highlighting.
3. **Editing** — through an embedded Neovim, when needed.
4. **Find file** — IntelliJ-style "Go to File" (type a few letters, fuzzy / camel-hump matching).
5. **Find in Path** — full-text search across the project.
6. **Commit view** — changed files (tracked + unversioned) with an IntelliJ-like side-by-side diff against `HEAD`, and committing.

Non-goals: language intelligence (LSP) beyond what your Neovim config brings, run configurations, plugins, debugger.

> Status: **approved direction, decisions recorded in §0.** Implementation has not started.
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
| 6 | Repo size | Reference repo `lms-context`: **~4,000 files** (4,004 tracked, 4,083 not ignored, 4,266 total, 203 MB). At that size a live ripgrep-engine search takes tens of milliseconds, so **no persistent index** is planned. |
| 7 | Name | `fb` for now. Alternatives in §4. |

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
| Git reads (status, blobs, HEAD tree) | `gix` (gitoxide) | cargo, Helix, gitui |
| Git writes (stage, commit, rollback, push) | `git` CLI subprocess | Keeps hooks, signing and credential helpers working |
| Line/word diff | `imara-diff` + `similar` (intra-line) | gix, Helix |
| File watching | `notify` (+ debouncer) | Zed, watchexec |
| Neovim RPC | `nvim-rs` (msgpack-RPC, tokio) | Neovim GUI clients |
| Themes / config | `serde` + `toml` | — |

### 1.3 Full-text search strategy

Parallel, streaming search with the ripgrep engine, respecting `.gitignore`; cancelled and restarted per keystroke (debounced). At ~4k files (§0.6) this finishes in tens of milliseconds once the page cache is warm, so there's **no index to build, store or keep fresh**. A trigram index stays a documented escape hatch if repos grow by 50×+, not a planned item.

### 1.4 Prerequisites on this machine

- **Rust toolchain:** not installed. Run `brew install rustup && rustup-init` (or the rustup.rs script).
- **Neovim:** not installed. Run `brew install neovim` (≥ 0.10 needed for the UI protocol features we use).
- Already present: git 2.54, ripgrep, fd, Homebrew, herdr 0.9.5.

### 1.5 Editing: embedded Neovim

Building a pure Neovim *client* was rejected: the tree, git colours, search, commit view and diff aren't part of Neovim's protocol, so they'd still be our code. Instead, **Neovim is embedded as the editor inside fb's viewer pane**:

- One headless `nvim --embed` process per fb session, attached over msgpack-RPC with the **`ext_linegrid`** UI extension. Without `ext_multigrid`, Neovim composites floating windows (completion, telescope, etc.) into a single grid itself, so fb only renders one grid: cells, highlight attributes, cursor, mode.
- Files open as Neovim buffers (`:edit`), so undo history survives switching files.
- **Theme sync:** fb pushes its theme into Neovim (`nvim_set_hl`) so the editor matches Darcula exactly; or it keeps your colourscheme (config option).
- **Events:** an autocmd → `rpcnotify` on `BufWritePost` makes fb refresh git status, tree colours and gutter markers immediately.
- **Config:** uses your `init.lua` by default; `--clean`-style minimal profile available (`[editor] config = "user" | "minimal"`).
- **Focus model:** Viewer is read-only and instant (and renders images/Markdown). `e` (or `Enter` on a code file, configurable) hands the pane to Neovim; `Ctrl-\ Ctrl-n`-style escape chord or a configurable key returns focus to fb's tree/panels.
- **Fallback:** if `nvim` isn't on `PATH`, `e` opens `$EDITOR` full-screen (suspend/resume).

### 1.6 Ghostty + herdr

- **Ghostty** supports the kitty graphics protocol and the kitty keyboard protocol: both are first-class targets.
- **herdr** (0.9.5 installed) is a tmux-style multiplexer with its own terminal emulation. It implements kitty graphics, but its tracker shows open issues: images missing after re-attach (#5408), slow large images (#5442), and an unanswered case where the support query says OK but nothing renders (#5468).
- **Plan:** a protocol spike in M0 runs inside herdr *and* plain Ghostty. fb auto-detects support; `[images] protocol = "auto" | "kitty" | "halfblocks" | "off"` lets you force a mode. If herdr misbehaves, images degrade to half-blocks there and stay pixel-perfect in plain Ghostty. Everything else (colours, mouse, keys) is unaffected by herdr.
- fb never binds `Ctrl-b` (herdr's prefix).

---

## 2. Architecture

```
fb/                         (cargo workspace)
├── crates/
│   ├── fb-core/     workspace model, file tree, fs watcher, config, events
│   ├── fb-theme/    theme + chrome model, loading, built-in looks
│   ├── fb-git/      status, HEAD blobs, change list, write ops via git CLI
│   ├── fb-diff/     line + word diff, side-by-side alignment, hunk ids
│   ├── fb-search/   fuzzy file finder (nucleo), full-text search (grep-*)
│   ├── fb-render/   syntax highlighting, markdown → styled lines, image decoding
│   ├── fb-nvim/     embedded Neovim: RPC, linegrid model, theme sync
│   └── fb-tui/      ratatui front-end (binary: `fb`)
├── themes/          darcula.toml (A), quiet.toml (B), paper.toml (C)…
└── ROADMAP.md
```

- **UI-agnostic core.** `fb-tui` consumes view-models (tree rows, styled lines, diff rows), never git or fs directly.
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

A **look** = one TOML file with `[palette]` + `[chrome]` (+ optional `inherits = "darcula"`). Adding a design means writing a TOML file, not touching widget code. Built-in looks are embedded in the binary; user looks live in `~/.config/fb/themes/`. Hot reload while editing a theme file; `fb --theme quiet` or `:theme` to switch. A CI check fails if a widget uses a colour literal.

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

Terminal apps can be tested end to end much like Playwright tests a browser. The real `fb` binary runs inside a **pseudo-terminal (PTY)**, the test sends keystrokes, and a terminal-emulator library turns fb's output back into a grid of cells: text, colours, cursor. Assertions **auto-wait** for the screen to reach the expected state, like Playwright's `expect(...).toBeVisible()`. No sleeps, no flaky timing.

Five layers, fastest first:

| Layer | What it covers | Tools | When |
|---|---|---|---|
| 1. Unit | Pure logic: tree model, status mapping, fuzzy ranking, diff alignment, theme parsing, Neovim grid model | `cargo nextest`, `proptest` (e.g. "aligned diff rows always reconstruct both files") | every change, milliseconds |
| 2. Render snapshots | Each widget drawn into an in-memory terminal (`ratatui::TestBackend`); text **and colours** snapshotted, for every built-in look | `insta` + `cargo insta review` | every change, milliseconds |
| 3. Integration | Core crates against real git repos built by a fixture builder in a temp dir. Real `git status --porcelain=v2` and `rg --json` act as the **oracle**: our results must match theirs. | `tempfile`, `git`, `rg` | every change, seconds |
| 4. End-to-end | Real `fb` binary in a PTY, driven by Playwright-style scripts | in-repo `fb-e2e` harness (`portable-pty` + `vt100`) | every change, seconds |
| 5. Real-terminal smoke | Pixel images and keyboard protocol in Ghostty and Ghostty+herdr | short manual checklist | per release |

**What an e2e test looks like:**

```rust
#[test]
fn modified_file_turns_blue_and_commits() -> Result<()> {
    let repo = Fixture::rust_project().modify("src/status.rs", "// edit\n").build()?;
    let mut fb = Fb::launch(&repo).size(160, 45).theme("darcula").start()?;

    fb.expect_text("status.rs").to_have_fg(token("git.modified"))?;
    fb.press("alt+0")?;                                  // Commit tool window
    fb.expect_text("Changes  1 file").visible()?;
    fb.type_text("Fix status refresh")?;
    fb.press("ctrl+k")?;
    fb.expect_text("Committed 1 file").visible()?;

    assert_eq!(repo.git(["log", "-1", "--format=%s"])?, "Fix status refresh");
    Ok(())
}
```

**Harness features (`crates/fb-e2e`):**
- **Fixtures:** builder for git repos (files, commits, modifications, untracked, ignored, conflicts). Git runs hermetically (temp `HOME`, no global config, fixed author and dates), so screens are deterministic.
- **Locators + auto-wait:** `expect_text`, `expect_row(n)`, `to_have_fg/bg(token(..))`. Colours are asserted by **theme token**, not hex, so tests survive palette changes.
- **Input:** keys (including kitty-keyboard encoding), mouse clicks and scrolling at cells, paste, terminal resize.
- **Whole-screen golden snapshots** via `insta`.
- **Failure traces**, like Playwright's trace viewer: every step's screen is saved as a coloured HTML page with the action taken. Kept locally on failure and uploaded as a CI artifact.
- **Images:** the harness records fb's raw output. Tests assert that a kitty graphics sequence (`ESC _ G …`) is sent with the right size and placement, and that `protocol = "halfblocks"` draws block characters instead. Whether the pixels look right is the terminal's job, which is why layer 5 exists.
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

### M0 — Foundations
- [ ] Install Rust + Neovim; cargo workspace with the crates above.
- [ ] App shell: alternate screen, event loop, panic hook restoring the terminal, `tracing` to a log file.
- [ ] **Theme engine** (`fb-theme`): semantic tokens, chrome options, TOML loading, `inherits`, hot reload; `darcula.toml` built in.
- [ ] Pane/focus model, configurable keymap (avoiding `Ctrl-b`), status bar, help overlay (`?`).
- [ ] Config at `~/.config/fb/config.toml`.
- [ ] **Terminal spike:** kitty graphics + kitty keyboard detection inside herdr and plain Ghostty; record results here.
- [ ] CI: `fmt`, `clippy`, tests, "no colour literals in widgets" check.
- [ ] **Test infrastructure:** evaluate tui-test / termlens / ratatui-testlib, then build or adopt the `fb-e2e` harness (PTY, emulator, locators, auto-wait, HTML traces). Add the git `Fixture` builder, `insta` snapshot setup, and `nextest` config.

**Done when:** `fb [path]` opens design A's split layout and quits cleanly, in herdr and in plain Ghostty.
**Tests:** e2e: launch → first frame shows the layout → `q` quits and restores the terminal; resize redraws. Snapshot: empty layout in darcula. Unit: theme loading, `inherits`, invalid theme errors.

### M1 — Project tree with git colours
- [ ] Lazy tree, keyboard + mouse; folders first, natural sort.
- [ ] `.gitignore`-aware, ignored shown dimmed/olive (toggle), `.git` hidden.
- [ ] Git status via `gix`: file colours + folder roll-up.
- [ ] File watcher: tree and status refresh on disk changes (debounced).
- [ ] Reveal current file, collapse all.

**Done when:** `lms-context` browses instantly and colours match `git status`.
**Tests:** integration: status of every fixture state vs `git status --porcelain=v2`. E2e: expand/collapse via keys and mouse; colours per status; an external file change shows up without a keypress; ignored toggle.

### M2 — Viewer (code / Markdown / images)
- [ ] Code: syntect via theme tokens, line numbers, scroll, go-to-line, wrap toggle.
- [ ] Git change markers in the gutter.
- [ ] Large/binary file safety.
- [ ] Markdown rendered view + raw toggle.
- [ ] Images via `ratatui-image` (kitty → half-block fallback), fit-to-pane.
- [ ] Tabs + recent files (`Ctrl-E`), find in file.

**Done when:** opening any file in the tree shows the right view.
**Tests:** snapshots: highlighted Rust/TS/JSON, rendered Markdown, gutter markers. E2e: open code / Markdown / image (kitty sequence emitted; half-blocks when forced) / binary / huge file.

### M3 — Editing with embedded Neovim
- [ ] `fb-nvim`: spawn `nvim --embed`, attach with `ext_linegrid`, model the grid (cells, hl attrs, cursor, mode, resize).
- [ ] Render the grid in the viewer pane; forward keys/mouse while focused; focus-return chord.
- [ ] Open/switch buffers from tree, tabs, finder; unsaved-changes indicator in tabs and tree.
- [ ] Theme sync (`nvim_set_hl` from the active look) or keep the user colourscheme.
- [ ] `BufWritePost` → refresh status, tree colours, gutter, commit view.
- [ ] Config modes `user` / `minimal`; `$EDITOR` fallback when `nvim` is missing.

**Done when:** a file can be opened, edited, saved, and its tree colour flips to "modified" right away.
**Tests:** unit: grid model fed recorded `redraw` events. E2e (real `nvim`): `e` → insert text → `:w` → file on disk changed, tree colour now `git.modified`; focus returns to tree; unsaved-changes marker; missing `nvim` falls back to `$EDITOR`.

### M4 — Go to File (fuzzy)
- [ ] Popup per mockup A: tabs `All / Files / Text / Recent`, `nucleo` over non-ignored paths kept fresh by the watcher.
- [ ] IntelliJ-like ranking: camel-hump/word-start boosts, filename over directory, recent first.
- [ ] `name:42` jumps to a line; open in viewer or straight into Neovim.

**Done when:** any file is a few keystrokes away, each keystroke under 16 ms.
**Tests:** unit: ranking table (`stat` → `status.rs` first, camel-hump cases). E2e: `Ctrl-P`, type, `Enter` opens; `name:42` lands on line 42; keystroke latency budget on the 50k-file fixture.

### M5 — Find in Path (full-text)
- [ ] The `Text` tab of the same popup, or a docked panel: case / whole word / regex toggles, file mask, scope.
- [ ] Streaming parallel search, grouped by file, highlighted context, preview; `Enter` opens at the line.

**Done when:** results match `rg` and feel instant on `lms-context`.
**Tests:** integration: results vs `rg --json` for literal, regex, case and whole-word queries. E2e: search → navigate results → `Enter` opens at the matching line; typing cancels the previous search.

### M6 — Commit view
- [ ] `0: Commit` tool window: **Changes** and **Unversioned Files** groups, flat or by directory.
- [ ] File-level checkboxes, commit message editor (single-line fast path; full editor via Neovim), **Commit**, **Amend**, optional **Commit & Push**. All writes go through the `git` CLI.
- [ ] Rollback (confirm), add unversioned to VCS, refresh.

**Done when:** a normal commit workflow happens without leaving fb.
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

### Later / ideas
- Git log & file history, blame ("Annotate"), branch switcher.
- Compare two files/revisions; edit the right side of a diff via Neovim.
- Desktop front-end reusing the core crates.

---

## 4. Remaining open questions

1. **Name:** keep `fb`, or pick one of:
   - **`perch`**: sitting above the project, looking down at it.
   - **`loupe`**: a jeweller's magnifier for close reading (a GNOME image viewer shares the name, on Linux only).
   - **`keel`**: the steady backbone under the work; short to type.
   - **`vista`**: an overview of the whole project.
   - **`prism`**: one view split into tree, code and diffs.

   None of these collide with a command on this machine. A crates.io/Homebrew name check happens before renaming.
2. **Editor entry key:** should `Enter` on a code file open the read-only viewer (fast, default) or go straight into Neovim?
