# AGENTS.md

Guidance for coding agents (and humans) working on **vista**, a lean, keyboard-driven terminal replacement for the parts of IntelliJ IDEA used every day: project tree with git colours, viewer, Go to File, Find in Files, Commit window, diff viewer, git history, embedded Neovim, terminals, several projects at once.

- User-facing overview and keys: [`README.md`](README.md)
- Feature-by-feature documentation: [`docs/`](docs/README.md)
- Plan, decisions and milestone notes: [`ROADMAP.md`](ROADMAP.md)

## Build, run, test

Rust (edition 2024), ratatui + crossterm. On this machine cargo comes from Homebrew's rustup, which is keg-only:

```sh
export PATH="/opt/homebrew/opt/rustup/bin:$PATH"   # if `cargo` isn't found

cargo build --release -p vista-tui                  # binary: target/release/vista
cargo run --release -p vista-tui -- <path>          # run on a folder or file

cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --no-fail-fast               # unit, snapshot, integration, e2e (~300 tests)
cargo test -p vista-tui --test e2e_r1_history       # one e2e file
VISTA_PERF_REPO=~/some/repo cargo test --release --test perf -- --ignored --nocapture
```

A change is done when `fmt --check`, `clippy -D warnings` and the **whole** suite pass — run the suite twice; several bugs here only showed up as rare failures under load.

Requirements at run time: `git` on `PATH`; optional `nvim` 0.10+ (editing; tests skip Neovim cases without it).

## Where to work

Work in the main checkout, on `main` or on a normal branch (`git switch -c <name>`), **not in git worktrees** (no `.claude/worktrees/…`, no `git worktree add`). The owner runs vista from the main checkout, so changes made in a worktree don't show up until they are merged.

- Small changes can go straight to `main`; larger ones on a branch, merged into `main` with `git merge --ff-only` once the checks below pass.
- Leave the owner's untracked or staged IDE files (`.idea/`) alone.
- There is no remote yet; don't push unless asked.

## Layout

| Crate | Role |
|---|---|
| `vista-core` | config (`config.toml`), remembered settings (`state.toml`, `UiState`), lazy file tree, file watcher |
| `vista-theme` | looks: semantic colour tokens + chrome options; built-ins `darcula`, `quiet`, `paper` |
| `vista-git` | everything through the `git` CLI: status, content at HEAD / any revision, log, commit files, commit/amend/rollback/push |
| `vista-diff` | gutter markers, aligned side-by-side diffs, hunks with content-hash ids, word ranges, partial-commit content |
| `vista-search` | Go to File index + fuzzy ranking, Find in Files engine (ripgrep crates) |
| `vista-render` | syntax highlighting (syntect + bat grammars), Markdown rendering (pulldown-cmark), file types |
| `vista-nvim` | embedded Neovim over msgpack-RPC (line grid); `open_diff.lua` / `close_diff.lua` for the side-by-side diff editor |
| `vista-tui` | the `vista` binary: `Workspace` (projects, terminal tabs) → `App` per project → views in `src/`, drawing in `src/ui/` |
| `vista-e2e` | PTY test harness (real binary, vt100 screen), git fixtures, HTML traces |

Inside `vista-tui`:

- `workspace.rs` — open projects (one `App` each), terminal tabs, the shared event channel (events are tagged with a `ProjectId`; terminal input goes to the active project), shared `UiState`, quit/close confirmations.
- `app.rs` — one project: focus, key/mouse routing, tool windows, background jobs. Key handling order: popups → prompt → bottom terminal → editor → viewer → tree / commit → global keymap.
- Views: `project.rs` (tree), `viewer.rs` (tabs, loading, scrolling, selection), `diffview.rs`, `commit.rs`, `find.rs`, `goto.rs`, `log.rs` (Git window), `projects.rs` (tab row + Ctrl-O popup), `term_tab.rs` (PTY + vt100), `editor.rs`, `selection.rs`, `keymap.rs`.
- `ui/` draws; `style.rs` is the only bridge from theme tokens to colours.

## Conventions (some are enforced by tests)

- **Colours only via theme tokens.** Widgets ask `theme.fg("git.modified")` etc.; only `src/style.rs` may construct `Color` (`tests/lints.rs` fails otherwise). New tokens go into `vista_theme::TOKENS` and every built-in look.
- **Git through the CLI**, machine-readable formats (`status --porcelain=v2 -z`, `cat-file`, `diff-tree -z`), never parsing human output.
- **No blocking on the UI thread.** Git, highlighting, search, image decoding, diffs and PTY reads run on threads and post `AppEvent`s through an `EventTx`; stale results are dropped by generation counters.
- **No F-key-only actions** (Mac keyboards): every action has a non-F binding; `Keymap::hint` shows the shortest. Avoid `Alt-[` / `Alt-]` (`ESC [` / `ESC ]` start escape sequences). Uppercase letters in bindings mean Shift.
- **Actions** are listed in `keymap.rs` (`Action`, its config name and description, default bindings); help and hints are generated from it. Tool-window labels (`2: Commit`) come from the binding.
- **Settings the user toggles in the app are remembered** with `App::remember(|s| …)` (writes `state.toml`); config values are only defaults.
- **One bottom tool window at a time** (Find, Git, Terminal); left panel is Project or Commit.
- Comments explain *why*; match the surrounding density. `rustfmt.toml` sets `max_width = 120`.

## Tests

Every feature gets unit tests where the logic lives **and** end-to-end tests that drive the real binary:

```rust
let repo = Fixture::git().file("a.txt", "one\n").commit("init").modify("a.txt", "two\n").build();
let mut vista = Vista::launch(BIN, &repo).size(150, 40).start();
vista.press("alt+2");
vista.expect_text("Changes  ");
vista.expect_text("two").to_have_bg("diff.inserted");
```

- Locators auto-wait (8 s). Failing tests write an HTML trace of every step to `target/e2e-traces/<test>.html`.
- **Wait for asynchronous loads before pressing keys** that act on them (a diff tab's title appears before its diff loads; Go to File results arrive after typing). Prefer waiting for a result marker (`"change 1 of 2"`, a selected row's background) over sleeping.
- Narrow searches with `expect_text_in(text, cols)` when the same text appears in the tree and elsewhere.
- Harness helpers: `press("ctrl+shift+left")`, `type_text`, `click`, `double_click`, `drag(from, to)`, `scroll_*`, `resize`, `expect_exit`. Single-character keys are sent as typed (`press("J")` is Shift-J).
- Hooks for tests: `VISTA_CLIPBOARD_FILE` (copy goes to a file), `VISTA_NO_WATCH`, `.env("SHELL", "/bin/sh")` for predictable terminals, `.theme("")` to use the remembered look.
- Render snapshots (`insta`) cover layouts; review diffs before accepting (`INSTA_UPDATE=always cargo test --test render_…` once the change is intended).
- To check a test really guards a fix, temporarily undo the fix and watch it fail.

## Docs

Every feature has its own document in `docs/`, numbered in order (`01-project-tree.md` … `16-testing.md`):

- **A new feature gets a new file** with the next free number (`17-<feature>.md`), never a section squeezed into an existing one. Follow the shape of the others: what it does, keys, settings, code and tests. Add a row for it to the index in `docs/README.md` and link related docs both ways.
- **A change to an existing feature** updates that feature's file (and `15-keyboard-reference.md` / `14-configuration-and-settings.md` if keys or settings changed).
- Numbers are never reused or reshuffled, so links stay stable.

Also keep the keys table in `README.md` and the milestone notes in `ROADMAP.md` up to date where relevant.
