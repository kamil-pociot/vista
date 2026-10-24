# 22 · Go to Line:Column

`Ctrl-G` opens IntelliJ's **Go to Line:Column**: a small popup with the cursor's position filled in and selected, so typing replaces it. It works from the tree and the other panels too, on the file in front, and moves the focus there; only the Terminal tool window keeps `Ctrl-G` for the shell.

```
┌ Go to Line:Column ───────────────────────┐
│ [Line] [:column]: 150:1▏                 │
│ Lines 1–200 · Enter go · Esc cancel      │
└──────────────────────────────────────────┘
```

## What it takes

| Input | Goes to |
|---|---|
| `42` | line 42 |
| `42:7` | line 42, column 7 |
| `:7` | column 7 of the current line |

Lines past the end go to the last line, and columns past the end of the line to its end. Line and column 0 count as 1. Only digits and one `:` can be typed.

## Where

- **Code** (and raw Markdown): the cursor moves to the line, centred when it was off screen. A column puts a caret on that character (underlined, in the selection colour), scrolls sideways to it when lines don't wrap, and shows in the status bar (`150:7`). The caret stays until the cursor moves; `e` opens Neovim on the same line and column.
- **Rendered Markdown:** the block the line belongs to (its source line, as `Shift-L` numbers it) scrolls to the top.
- **Diffs:** lines are the working tree's (HEAD's when the file was deleted), the same numbers the right side shows; the status bar shows them too. A line folded away by `z` (collapse unchanged) unfolds the diff.
- **The editor:** `Ctrl-G` is taken from Neovim (where it would show file info) and opens the same popup, filled in with Neovim's cursor; Enter moves Neovim's cursor and centres the line. `:42` in Neovim still works.
- **Images and binary files** have no lines: the status bar says so. Without an open file it says to open one first.

## Keys

| Key | Action |
|---|---|
| `Ctrl-G` | open (rebind `goto-line` in [Preferences › Keymap](23-keymap.md) or `[keys]`) |
| digits, `:` | type (the first replaces the filled-in position) |
| `←`/`→`, `End` | keep the filled-in position to edit it |
| `Backspace`, `Ctrl-U` | delete a character / clear |
| `Enter` | go |
| `Esc`, a click beside the popup | cancel |

`Enter go` and `Esc cancel` in the popup can be clicked.

## Code and tests

- `vista-tui/src/goto_line.rs` (parsing, the popup's keys), `Tab::position` / `Tab::go_to` and the caret in `viewer.rs`, `DiffDoc::display_row_of` / `side_for_lines` in `diffview.rs`, `Nvim::position` / `Nvim::go_to` in `vista-nvim`, `App::open_goto_line` / `go_to_line`, `ui/viewer.rs` (`draw_goto_line`, the caret).
- Unit: parsing and keys (`goto_line`), line and column in code (`viewer`), display rows of file lines side by side, unified and folded (`diffview`).
- E2e: `e2e_r1_goto_line.rs` (from the tree, column caret, `:col`, past the end, Esc; no file and rebinding; a collapsed diff; rendered Markdown; Neovim), `e2e_m2_viewer.rs`.

See also: [02 · Viewer](02-viewer.md), [07 · Diff viewer](07-diff-viewer.md), [08 · Editing with Neovim](08-editing-with-neovim.md), [04 · Go to File](04-go-to-file.md) (`name:42` opens a file at a line).
