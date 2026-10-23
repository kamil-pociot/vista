# 02 · Viewer

Opening a file shows it read-only in the editor area. vista is "viewer first, with optional edits": `e` switches to Neovim when you want to change something ([08](08-editing-with-neovim.md)).

## Code

- **Syntax highlighting** with syntect and bat's grammars, mapped to the look's `syntax.*` tokens. Highlighting runs off the UI thread for the whole file (plain text above 2 MB).
- **Gutter change markers** against `HEAD`: added, modified and deleted lines.
- **Line numbers**, toggled with `Shift-L` or `Alt-L` (remembered).
- **Long lines:** `w` toggles between wrapping and scrolling sideways (`←`/`→`, shift+wheel, trackpad). Markdown opens wrapped, code opens scrolling; each choice is remembered separately.
- **Scrolling by screen rows.** The wheel and keys move by displayed rows, so wrapped files scroll the same as unwrapped ones. Views scroll a little past the end, so the last line can sit about 30% above the bottom edge.
- **Scrollbar** sized and placed by screen rows, correct with wrapped lines.
- **Find in file:** `/`, then `n`/`N` (smart case).
- **Go to line:** `Ctrl-G`.
- **Changes:** `]`/`[` jump between changes in the file. At the last change a popup offers the next changed file: press `]` again to open it on its first change ([07](07-diff-viewer.md#change-navigation-across-files)).
- **Diff vs HEAD:** `d` opens the file's diff.
- **Safety:** files over 20 MB aren't loaded; binary files are shown as a hex dump.

## Text selection

- Drag with the mouse to select; double-click selects a word.
- `Shift-↑`/`Shift-↓` extend the selection by lines, `Ctrl-A` selects everything.
- `Ctrl-C` or `y` copies, `Esc` clears.
- Copying uses `pbcopy`, `wl-copy` or `xclip` when available, otherwise the terminal's OSC 52 clipboard sequence.
- Selection works on code, rendered Markdown and wrapped lines: each frame records which file line and column every screen row shows.

## Keys

| Key | Action |
|---|---|
| `↑`/`↓`, `j`/`k`, `PgUp`/`PgDn`, `g`/`G` (`Home`/`End`) | move / page / start / end |
| `w` | wrap ⇄ scroll sideways |
| `←`/`→` | scroll sideways (when not wrapping) |
| `Shift-L` (`Alt-L`) | line numbers |
| `/`, `n`/`N` | find in file |
| `Ctrl-G` | go to line |
| `]`/`[` | next / previous change |
| `d` | diff vs HEAD |
| `e` | edit at the current line |
| `m` | Markdown raw ⇄ rendered |
| `Ctrl-L` | reveal in tree |
| drag, double-click, `Shift-↑/↓`, `Ctrl-A` | select |
| `Ctrl-C`, `y` / `Esc` | copy / clear selection |

## Code and tests

- `vista-render` (highlighting, file types), `vista-diff` (gutter markers), `vista-tui/src/viewer.rs`, `selection.rs`, `ui/viewer.rs`.
- Snapshots: highlighted Rust/TS/JSON, gutter markers (`render_m2.rs`).
- E2e: `e2e_m2_viewer.rs` (code, Markdown, images, binary, huge file), `e2e_r1_feedback.rs` (sideways scroll, scroll past end, cross-file changes), `e2e_r1_settings.rs` (line numbers), `e2e_r1_selection.rs` (drag, double-click, keyboard selection, copy).
