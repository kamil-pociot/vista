# 16 · Testing

Every feature has unit tests where its logic lives and end-to-end tests that drive the real binary. `cargo test --workspace` runs all of them (~300).

## Kinds of tests

| Kind | Where | Covers |
|---|---|---|
| Unit | `#[cfg(test)]` in each crate | parsing, ranking, layout, scrolling maths, key encoding, state round trips |
| Property (`proptest`) | `vista-diff` | aligned rows rebuild both sides, hunks apply, folding loses nothing, partial content |
| Integration | `vista-git`, `vista-search` | status vs `git status`, commit subsets, Find results vs `git grep` |
| Render snapshots (`insta`) | `vista-tui/tests/render_*.rs` | whole screens with colours recorded as theme tokens |
| End-to-end | `vista-tui/tests/e2e_*.rs` | the real `vista` binary in a pseudo-terminal |
| Lints | `vista-tui/tests/lints.rs` | no colour literals outside `style.rs` |
| Performance (opt-in) | `vista-tui/tests/perf.rs` | first frame, status and search latency on a real repo |

## End-to-end harness (`vista-e2e`)

A Playwright-like API over a PTY (`portable-pty`) and a vt100 screen:

```rust
let repo = Fixture::git().file("a.txt", "one\n").commit("init").modify("a.txt", "two\n").build();
let mut vista = Vista::launch(BIN, &repo).size(150, 40).start();
vista.press("alt+2");
vista.expect_text("Changes  ");
vista.expect_text("two").to_have_bg("diff.inserted");
```

- `Fixture` builds throwaway git repositories (files, commits, modifications, remotes, conflicts).
- Locators auto-wait up to 8 s; colours are checked by theme token.
- Input: `press`, `type_text`, `click`, `double_click`, `drag`, scrolling (vertical, sideways, shift+wheel), `resize`.
- A failing test writes an HTML trace of every step to `target/e2e-traces/<test>.html`.
- Test hooks: `VISTA_CLIPBOARD_FILE` (copies go to a file), `VISTA_NO_WATCH`, `VISTA_CONFIG_DIR`, `VISTA_RUNTIME_DIR` (control sockets; the harness puts them in the test's home), a fixed `SHELL` for terminals.
- Neovim tests are skipped when `nvim` isn't installed.

## Running

```sh
cargo test --workspace --no-fail-fast
cargo test -p vista-tui --test e2e_r1_selection
INSTA_UPDATE=always cargo test -p vista-tui --test render_m7   # after reviewing the diff
VISTA_PERF_REPO=~/some/repo cargo test --release --test perf -- --ignored --nocapture
```

## Habits that keep the suite reliable

- Wait for asynchronous results (a loaded diff, Go to File results) before pressing keys that act on them.
- Run the whole suite more than once after a change; timing bugs show up as rare failures.
- Undo a fix temporarily to confirm its test fails without it.

## Test files by feature

| Feature | E2e files |
|---|---|
| Shell, help, focus | `e2e_m0_shell.rs` |
| Tree | `e2e_m1_tree.rs`, `e2e_r1_reveal.rs`, `e2e_r1_resize.rs` |
| Viewer, Markdown, images | `e2e_m2_viewer.rs`, `e2e_r1_feedback.rs`, `e2e_r1_settings.rs`, `e2e_r1_selection.rs` |
| Mermaid diagrams | `e2e_r1_mermaid.rs` |
| Picture viewer, pictures half in view | `e2e_r1_picture_viewer.rs`, `e2e_r1_mermaid.rs` |
| Neovim | `e2e_m3_editor.rs`, `e2e_r1_diff_edit.rs` |
| Go to File | `e2e_m4_goto.rs` |
| Find in Files | `e2e_m5_find.rs` |
| Commit, partial commits | `e2e_m6_commit.rs`, `e2e_m9_partial.rs` |
| Repositories inside the project | `e2e_r1_nested_repos.rs` |
| Diff | `e2e_m7_diff.rs`, `e2e_r1_feedback.rs` |
| Looks | `e2e_m8_looks.rs` |
| Git history | `e2e_r1_history.rs` |
| Projects | `e2e_r1_projects.rs` |
| Terminals | `e2e_r1_terminal.rs` |
| Tabs | `e2e_r1_tabs.rs` |
