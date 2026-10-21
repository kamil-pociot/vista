# file-browser — Research & Roadmap

A lean, keyboard-driven, **read-only** replacement for the parts of IntelliJ IDEA that are actually used day to day:

1. **Project view** — file tree with expandable folders, file names coloured by git status.
2. **Viewer** — images, rendered Markdown, source code with syntax highlighting.
3. **Find file** — IntelliJ-style "Go to File" (type a few letters, fuzzy / camel-hump matching).
4. **Find in Path** — full-text search across the project.
5. **Commit view** — changed files (tracked + unversioned) with an IntelliJ-like side-by-side diff against `HEAD`.

Non-goals: text editing, language intelligence (LSP), run configurations, plugins, debugger.

> Status: **proposal — awaiting approval.** Nothing below is implemented yet. UI design options are published separately for review (see §4).

---

## 1. Research summary

### 1.1 Terminal or desktop app?

**Conclusion: a terminal (TUI) app can deliver everything on the list.** The few compromises are cosmetic.

| Requirement | Terminal feasibility | Notes |
|---|---|---|
| File tree, expand/collapse, mouse | ✅ Full | Standard TUI territory (yazi, gitui, lazygit prove it). |
| Git-status colours | ✅ Full | Truecolor is supported by every modern terminal. |
| Code highlighting | ✅ Full | Same TextMate/Sublime grammars that `bat` and `delta` use. |
| Images | ✅ Good, **terminal-dependent** | Kitty graphics protocol (kitty, Ghostty, WezTerm), iTerm2 inline images, Sixel. Falls back to coloured half-blocks elsewhere. Inside **tmux** it needs `allow-passthrough on` and is less reliable. |
| Rendered Markdown | 🟡 Good, not "browser-pretty" | Bold/italic/colour, highlighted code blocks, tables, lists, links and inline images are all possible. **Not possible:** proportional fonts, real heading sizes (except kitty's new text-sizing protocol), HTML blocks, Mermaid diagrams. |
| IntelliJ-like diff | 🟡 Very close | Side-by-side with aligned blocks, word-level highlights, change markers in the gutter, next/prev change: all doable (delta does most of this). **Not possible:** the curved "connector" ribbons between the two panes. They can be approximated with a coloured centre gutter. |
| Full-text search, fuzzy find | ✅ Full | These are backend concerns. The UI is just lists and previews. |
| Keybindings | 🟡 Mostly | macOS terminals usually swallow `Cmd`, so bindings use `Ctrl`/`Alt`. "Double-Shift" needs the kitty keyboard protocol (kitty, Ghostty, WezTerm), so we provide an alternative binding too. |

When a desktop app (Tauri / GPUI / egui) would be worth it: if pixel-perfect Markdown (GitHub-style rendering, Mermaid) or the exact IntelliJ diff look matters more than leanness. The architecture below keeps that door open: all logic lives in UI-agnostic crates, so a desktop front-end could be added later without a rewrite.

### 1.2 Language choice

| Option | Verdict | Why |
|---|---|---|
| **Rust + Ratatui** | ✅ **Recommended** | Best ecosystem for *exactly* this app (see below). Single static binary, ~instant startup, low memory, true parallelism for search/indexing. |
| Go + Bubble Tea | Good runner-up | Pleasant TUI framework (lazygit-style apps). Weaker image support, and syntax highlighting (chroma) is less accurate than syntect. No equivalent of the ripgrep library crates. |
| TypeScript (Bun) + OpenTUI/Ink | Possible | Fast to prototype, but heavier runtime and startup. Image and diff tooling is thinner. |
| Python + Textual | Not recommended | Lovely widgets, but slow startup and high memory for large repos. Search/indexing would need native extensions anyway. |

**The Rust building blocks, all mature and used in production tools:**

| Concern | Crate | Proven in |
|---|---|---|
| TUI framework | `ratatui` + `crossterm` | gitui, yazi, television, many more |
| Images in terminal | `ratatui-image` (kitty / iTerm2 / Sixel / half-block auto-detect) | — |
| Syntax highlighting | `syntect` + `two-face` (bat's extra grammars) | bat, delta |
| Markdown parsing | `pulldown-cmark` | mdBook, rustdoc |
| Fuzzy matching | `nucleo` | Helix editor, television |
| Directory walking / `.gitignore` | `ignore` | ripgrep, fd |
| Full-text search | `grep-searcher` + `grep-regex` | ripgrep |
| Persistent index (optional) | trigram index (custom, or `tantivy` with an n-gram tokenizer) | Zed, zoekt, IntelliJ's own index |
| Git (read: status, blobs, HEAD tree) | `gix` (gitoxide) | gitui (partially), cargo, Helix |
| Git (write: stage, commit, rollback) | `git` CLI subprocess | Keeps hooks, GPG/SSH signing and credential helpers working with no extra effort |
| Line/word diff | `imara-diff` (histogram algorithm) + `similar` for intra-line | gix, Helix |
| File watching | `notify` (+ debouncer) | Zed, watchexec |

### 1.3 Full-text search strategy

IntelliJ's "Find in Path" is backed by a trigram index. For a lean tool we go in two steps:

1. **v1: no index.** A parallel ripgrep-library search, streamed into the UI as results arrive. ripgrep searches typical repos (≤100k files) in well under a second once the OS page cache is warm. Zero startup cost, zero disk usage, always up to date.
2. **v2 (only if needed): persistent trigram index.** Kept in a cache dir, built in the background, updated incrementally by the file watcher. Regex queries are narrowed to candidate files via trigrams, then verified with the ripgrep engine. We build this only if v1 is measurably too slow on your real repos.

### 1.4 Prerequisites on this machine

- Rust toolchain is **not installed**. It's needed first: `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh` (or `brew install rustup`).
- Already present: git 2.54, ripgrep, fd.

### 1.5 Why not build a Neovim client?

Neovim can run headless (`nvim --embed` / `nvim --listen`) and talks msgpack-RPC. Its UI protocol is what GUIs like Neovide and VimR use. It could fit in three ways:

| Approach | What you get | What it costs |
|---|---|---|
| Our app *is* a Neovim UI client (like Neovide) | An editor, for free | Little else. The tree, git colours, fuzzy finder, search, commit view and diff are **not** part of Neovim's protocol. They'd be Lua plugins inside Neovim, or our own code anyway. On top of that we'd re-implement Neovim's grid rendering. |
| Don't build; curate a Neovim config (neo-tree, snacks/telescope picker, gitsigns, diffview.nvim, render-markdown, snacks.image) | Roughly 80% of the list, in an afternoon | Vim modal editing everywhere. Feels like Vim, not IntelliJ. The commit view is fugitive/neogit-style rather than IntelliJ's. Plugin config to maintain. |
| **Recommended:** our own app, with Neovim **embedded in the viewer pane** later | Our IntelliJ-like UI everywhere; real Neovim editing when you want it | ~1–2k lines: an `nvim-rs` RPC client, render Neovim's grid into a ratatui pane, forward keys. Optional milestone **M8**. |

**Conclusion.** Being a Neovim client doesn't make the hard parts easier, because the hard parts are exactly what Neovim doesn't provide. The viewer pane sits behind a trait, so an `nvim --embed` pane can replace it later without touching the rest. Until then, M7's "open in `$EDITOR`" covers occasional edits.

Worth a 1-hour experiment before we start: if you're happy living in Neovim, a LazyVim setup with `diffview.nvim` may already be enough. If it isn't, this project is the answer.

---

## 2. Architecture

```
file-browser/              (cargo workspace)
├── crates/
│   ├── fb-core/     workspace model, file tree, fs watcher, config, events
│   ├── fb-git/      status, HEAD blobs, change list, write ops via git CLI
│   ├── fb-diff/     line + word diff, side-by-side alignment model
│   ├── fb-search/   fuzzy file finder (nucleo), full-text search (grep-*), index (later)
│   ├── fb-render/   syntax highlighting, markdown → styled lines, image decoding
│   └── fb-tui/      ratatui front-end (binary: `fb`)
└── ROADMAP.md
```

- **UI-agnostic core.** `fb-tui` only consumes view-models (tree rows, styled lines, diff rows). A future desktop front-end would reuse every other crate.
- **Never block the UI thread.** Git status, search, highlighting and image decoding run on worker threads and post results through a channel. The UI renders whatever is ready.
- **Swappable viewer.** The viewer pane is a trait (code / markdown / image / diff implementations), so an embedded Neovim editor can be added as one more implementation (M8).
- **Lazy everything.** Tree nodes load on expand. Highlighting covers only the visible viewport (plus a margin). Large and binary files are detected and handled safely.

### Git status colours (IntelliJ defaults, configurable)

| Status | Colour |
|---|---|
| Modified | blue |
| Added (staged new) | green |
| Unversioned (untracked) | red / brown |
| Deleted | grey |
| Ignored | olive / dimmed |
| Conflicted | bright red |
| Renamed | blue (shows "→ old name") |

Folders inherit a colour when they contain changes (optional, as in IntelliJ).

---

## 3. Roadmap

Each milestone ends in something usable. **Part 1 = M0–M4, Part 2 = M5–M6.**

### M0 — Foundations
- [ ] Install Rust toolchain; create cargo workspace with the crates above.
- [ ] App shell: crossterm alternate screen, event loop, panic hook that restores the terminal, logging to file (`tracing`).
- [ ] Focus/pane model, keymap layer (configurable), status bar, help overlay (`?`).
- [ ] Config file (`~/.config/file-browser/config.toml`): theme, keymap, ignore rules.
- [ ] CI: `cargo fmt`, `clippy`, tests.

**Done when:** `fb [path]` opens a split layout and quits cleanly.

### M1 — Project tree with git colours
- [ ] Lazy-loaded tree (expand/collapse, `←/→`, `Enter`, mouse click & scroll).
- [ ] `.gitignore`-aware: ignored files shown dimmed (toggleable), `.git` hidden.
- [ ] Sorting: folders first, natural sort.
- [ ] Git status via `gix`: per-file colours, folder roll-up colours.
- [ ] File watcher: tree and status refresh automatically on disk changes (debounced).
- [ ] "Reveal current file in tree", collapse all.

**Done when:** browsing a large repo (e.g. 50k files) is instant and colours match `git status`.

### M2 — Viewer (code / Markdown / images)
- [ ] Code: syntect highlighting, line numbers, scrolling, go-to-line, soft-wrap toggle.
- [ ] Git change markers in the gutter (added / modified / deleted lines vs HEAD).
- [ ] Large-file safety (size cap, lazy viewport highlighting), binary detection with hex/info fallback.
- [ ] Markdown: rendered view (headings, emphasis, lists, tables, quotes, highlighted fenced code, links, inline images), plus a raw/rendered toggle.
- [ ] Images: png/jpg/gif/webp/svg(rasterised) via `ratatui-image`, fit-to-pane, protocol auto-detect.
- [ ] Tabs or "recent files" switcher (`Ctrl-E` IntelliJ-style).
- [ ] Search within the open file (`/` or `Ctrl-F`), next/previous match.

**Done when:** opening any file in the tree shows the right view.

### M3 — Go to File (fuzzy)
- [ ] Popup with live results, using `nucleo` over all non-ignored paths (gathered in the background, kept fresh by the watcher).
- [ ] IntelliJ-like ranking: boosts for camel-hump/word-start hits (`FoBaCo` → `FooBarController`), filename over directory matches, recent files first.
- [ ] `path/segments` and `:line` suffix support (`main.rs:42`).
- [ ] Preview of the highlighted result.

**Done when:** any file in a large repo is a few keystrokes away, with results updating per keystroke in under 16 ms.

### M4 — Find in Path (full-text)
- [ ] Popup/panel: query, toggles for **case**, **whole word**, **regex**, file mask (`*.rs`), scope (project / directory).
- [ ] Parallel streaming search via `grep-searcher`, respecting `.gitignore`. Results grouped by file, with highlighted match context.
- [ ] Preview pane; `Enter` opens the file at the line.
- [ ] Cancellation on each keystroke (debounced).
- [ ] *(Conditional)* trigram index if v1 is too slow on real repos. Benchmark first.

**Done when:** searching a large repo feels instant and results are correct vs `rg`.

### M5 — Commit view (changes list)
- [ ] Dedicated view/tab (IntelliJ `Alt-0` / `Cmd-0` equivalent).
- [ ] Groups: **Changes** (tracked: modified/added/deleted/renamed, staged or not) and **Unversioned Files**.
- [ ] Flat list or grouped by directory; file counts; status colours.
- [ ] Checkboxes for inclusion; commit message box; **Commit** / **Amend** (via `git` CLI, so hooks and signing work).
- [ ] Actions: Rollback (with confirmation), Add unversioned to VCS, Refresh.
- [ ] *(Open question)* hunk/line-level partial commits.

**Done when:** a normal commit workflow can be done without leaving the app.

### M6 — IntelliJ-style diff viewer
- [ ] Side-by-side: HEAD (left, read-only) vs working tree (right), with scrolling synchronised and **blocks aligned**: filler lines inserted so unchanged regions line up.
- [ ] Block backgrounds: green (inserted), blue (modified), grey (deleted). **Word-level highlights** inside modified blocks.
- [ ] Centre gutter showing change markers (stand-in for IntelliJ's connector ribbons).
- [ ] Syntax highlighting on both sides.
- [ ] Next/previous change (`F7` / `Shift-F7`), next/previous file in the change list.
- [ ] Toggle unified view; toggle "ignore whitespace"; collapse unchanged regions.
- [ ] Unversioned/new files shown as all-added; deleted files as all-removed; binary/image diff = side-by-side images.

**Done when:** reviewing changes before a commit is as comfortable as in IntelliJ.

### M7 — Polish & distribution
- [ ] Themes (Darcula-like default, light theme).
- [ ] Performance pass on a big monorepo (startup time, memory, status refresh).
- [ ] "Open in `$EDITOR`" escape hatch for the occasional edit.
- [ ] Release binaries + Homebrew tap; `cargo install`.

### M8 — Embedded Neovim editor *(optional)*
- [ ] `nvim --embed` per open file via `nvim-rs`; render Neovim's line grid inside the viewer pane, with its highlights.
- [ ] Toggle read-only viewer ⇄ Neovim editor (`e`); forward keys while it has focus; `:w` triggers the usual tree/status refresh.
- [ ] Uses your own `init.lua`, or a minimal profile with a flag.

### Later / ideas (not committed)
- Git log & per-file history, blame ("Annotate"), branch switcher.
- Compare two arbitrary files/revisions.
- Desktop front-end (Tauri or GPUI) reusing the core crates, if terminal limits start to hurt.

---

## 4. Open questions for approval

1. **UI design:** pick one of the three directions in the [design mockups](https://claude.ai/artifact/UJ6tw99LFSzjDsXjW276Dr), or mix elements from several: **A** Darcula Classic, **B** Quiet Modal, **C** Paper Light.
2. **Neovim:** agree with §1.5 (own app now, embedded Neovim as optional M8)? Or try a curated Neovim setup first?
3. **Terminal emulator:** which one do you use (Ghostty / iTerm2 / kitty / WezTerm / Terminal.app)? And do you run inside **tmux**? This decides how good image previews will be. Terminal.app has no image protocol.
4. **Committing:** should the commit view *perform* commits (M5 checkboxes + message + commit), or only *show* changes and diffs?
5. **Partial commits:** are hunk/line-level checkboxes (IntelliJ's per-change checkboxes in the diff) needed, or is file-level enough?
6. **Repo sizes:** roughly how many files are in your biggest repo? This decides whether the trigram index (M4) is needed at all.
7. **Name:** working name `file-browser`, binary `fb`. Happy with that?
