# 12 · Tabs, tool windows and layout

## Screen layout

- **Top bar:** project tabs, the branch with ahead/behind.
- **Left panel:** `1: Project` (tree) or `2: Commit`; resizable.
- **Editor area:** file tabs with the viewer, diff or Neovim.
- **Bottom tool window** (one at a time): `3: Find`, `9: Git`, `T: Terminal`.
- **Status bar:** change counts, key hints and messages.

## File tabs

- Up to 12 tabs; the least recently used one closes when a 13th opens.
- `Alt-←` / `Alt-→` switch, `Ctrl-W` closes, a click selects.
- Each tab has an `×` to close it; a middle click anywhere on a tab closes it too.
- Drag a tab left or right to move it. A tab only moves in the direction the mouse is moving, so tabs of different widths don't swap back and forth. With tabs in several rows, dragging onto another row moves it there too.
- **Tabs that don't fit**, as in IntelliJ: by default they stay on **one row that scrolls**. `◂` and `▸` at its ends show that tabs are hidden (bright while there is more that way) and scroll it when clicked; the wheel (or a sideways scroll) over the row scrolls it too. A tab that becomes the active one (opened, `Alt-←`/`Alt-→`, a file opened from anywhere) scrolls into view; after that the wheel can scroll it away again.
- **Tabs in several rows** (Preferences › Viewer, remembered as `[tabs] multiple_rows` in `state.toml`): tabs that don't fit wrap onto more rows, and the file shows below them.
- A changed file's `File  Diff` switch ([07](07-diff-viewer.md#file--diff-switch)) keeps its place at the right; tabs never run under it.
- Diff tabs are titled `Δ name` (`Δ name @abc1234` for a commit).

## Focus and mouse

- `Tab` / `Shift-Tab` cycle focus between panels; `Alt-1`, `Alt-2`, `Alt-3`, `Alt-9`, `Alt-T` show, focus or hide tool windows. Labels such as `2: Commit` come from the key binding.
- Mouse: click to focus and select, double-click to open from every list (one 500 ms detector), the wheel scrolls, shift+wheel or a trackpad scrolls sideways, drag the divider to resize, drag tabs to reorder, drag text to select. Tool-window names, hints, toggles and buttons can be clicked too: see [17 · Clickable labels and buttons](17-clickable-labels.md).
- `?` shows every shortcut, generated from the active key map.

## No F-keys needed

Mac keyboards make F-keys awkward, so every action has a non-F binding: `]`/`[` changes, `?` help, `Ctrl-R` refresh, `Ctrl-L` reveal. `F7`, `F1`, `F5` and `Alt-F1` still work. Hints show the shortest key. `Alt-[` / `Alt-]` are avoided because terminals send them as the start of escape sequences.

On macOS, set Ghostty's `macos-option-as-alt = true` so `Alt-…` shortcuts reach vista.

## Code and tests

- `vista-tui/src/app.rs` (focus, routing), `workspace.rs` (top bar), `viewer.rs` (`move_tab`), `projects.rs` (`drag_moves`), `keymap.rs`, `ui/chrome.rs`.
- `ui/chrome.rs` (`layout_strip`: one scrolling row or several rows; `tab_strip`), `ui/viewer.rs` (keeping the active tab in view), `App::scroll_tabs`.
- Unit: `strip_layout_tests` (fitting, scroll limits and the tab shown, wrapping), `tab_strip_tests`.
- E2e: `e2e_r1_tabs.rs` (`×`, middle click, dragging file and project tabs, a row that scrolls with the wheel and ◂ ▸ and follows the active tab, tabs in several rows), `e2e_r1_resize.rs`, `e2e_m0_shell.rs` (help, focus, quit).
