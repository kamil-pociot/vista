# 01 · Project tree

The `1: Project` tool window (`Alt-1`) on the left shows the project folder as a tree, like IntelliJ's Project view.

## What it does

- **Lazy tree.** Folders load when expanded, so large repositories (the ~4,000-file reference repo `lms-context`) draw their first frame in milliseconds. Folders first, natural sort (`file2` before `file10`). `.git` is never shown.
- **Git status colours.** File names are coloured by `git status`: modified/renamed blue, added green, unversioned red, deleted grey, ignored olive italic, conflicted bright red (darcula values). Folders take the colour of changes inside them (`[tree] folder_colours`). The top bar shows the branch with ahead/behind counts; the status bar shows change counts.
- **Speed search.** Typing a letter, digit, `.` or `_` starts IntelliJ's speed search. A `⌕` bar at the bottom of the tree shows what you typed and how many rows match. The selection jumps to the nearest match, and the matched letters are highlighted. `↑`/`↓` step through the matches, wrapping round. `Enter` opens the row (or toggles a folder) and ends the search. `Esc`, or `Backspace` past the first letter, ends it without moving. Any other key, a click or leaving the tree also ends it. A name matches when it contains the text (ignoring case), or when the text's pieces start words in it: `stRs` finds `status.rs`, `mfb` finds `MyFileBrowser`. Only visible rows are searched (folders that are open), as in IntelliJ. The query turns red when nothing matches.
- **Repositories inside the project.** Clones in an ignored or untracked folder (`repos/web`) are coloured by their own repository's status and `.gitignore`, not shown as ignored ([24](24-nested-repositories.md)).
- **Ignored files.** `.gitignore`-aware. Ignored files are shown dimmed; `Alt-I` hides or shows them (`i` too, outside the tree), remembered.
- **Live updates.** A file watcher refreshes the tree and git colours on changes on disk, including commits and checkouts made outside vista (debounced). Saving in Neovim refreshes colours right away.
- **Opening files.** `Enter` or a double-click opens the file in the viewer; `Alt-Enter` opens it in Neovim (`e` too, outside the tree). The `●` marker shows files with unsaved edits.
- **Reveal current file.** The `⌖` button in the panel header, or `Ctrl-L`, expands the tree to the open file and selects it (IntelliJ's "Select Opened File").
- **Resizable.** Drag the divider between the tree and the editor, or press `Ctrl-Shift-→` / `Ctrl-Shift-←`. The width is remembered.

## Keys

| Key | Action |
|---|---|
| `↑`/`↓` | move (during a speed search: next / previous match) |
| `→`/`←` | expand / collapse (or go to parent) |
| letters, digits, `.`, `_` | speed search (`Esc` ends it) |
| `Enter`, double-click | open in the viewer |
| `Alt-Enter` | edit in Neovim |
| `Alt-I` | show / hide ignored files |
| `-` | collapse all |
| `Ctrl-R` (`F5`) | refresh |
| `Ctrl-L` (`Alt-F1`), `⌖` | reveal the open file |
| `Ctrl-Shift-→` / `←`, drag divider | widen / narrow the panel |

Mouse: click selects, clicking the arrow expands, the wheel scrolls (the selection stays where it is; the next arrow key brings it back into view).

## Settings

`[tree] show_ignored`, `[tree] folder_colours` in `config.toml`; the ignored toggle and panel width are remembered in `state.toml`.

## Code and tests

- `vista-core` (tree model, watcher), `vista-git` (status via `git status --porcelain=v2 -z`), `vista-search/src/speed.rs` (speed search matching), `vista-tui/src/project.rs` (keys, speed search), `vista-tui/src/ui/project.rs` (rows, highlights, search bar), `vista-tui/src/ui/chrome.rs`.
- Unit: speed search matching (substrings, word starts, backtracking), typing / stepping / wrapping / ending a search in the tree.
- Integration: status of every fixture state compared with `git status`, including random edit sequences and merge conflicts.
- E2e: `e2e_m1_tree.rs` (expand/collapse, colours, external changes, ignored toggle, speed search), `e2e_r1_reveal.rs`, `e2e_r1_resize.rs`.

See also: [24 · Repositories inside the project](24-nested-repositories.md) (clones inside the project keep their own colours).
