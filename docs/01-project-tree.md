# 01 · Project tree

The `1: Project` tool window (`Alt-1`) on the left shows the project folder as a tree, like IntelliJ's Project view.

## What it does

- **Lazy tree.** Folders load when expanded, so large repositories (the ~4,000-file reference repo `lms-context`) draw their first frame in milliseconds. Folders first, natural sort (`file2` before `file10`). `.git` is never shown.
- **Git status colours.** File names are coloured by `git status`: modified/renamed blue, added green, unversioned red, deleted grey, ignored olive italic, conflicted bright red (darcula values). Folders take the colour of changes inside them (`[tree] folder_colours`). The top bar shows the branch with ahead/behind counts; the status bar shows change counts.
- **Ignored files.** `.gitignore`-aware. Ignored files are shown dimmed; `i` hides or shows them (remembered).
- **Live updates.** A file watcher refreshes the tree and git colours on changes on disk, including commits and checkouts made outside vista (debounced). Saving in Neovim refreshes colours right away.
- **Opening files.** `Enter` or a double-click opens the file in the viewer; `e` opens it in Neovim. The `●` marker shows files with unsaved edits.
- **Reveal current file.** The `⌖` button in the panel header, or `Ctrl-L`, expands the tree to the open file and selects it (IntelliJ's "Select Opened File").
- **Resizable.** Drag the divider between the tree and the editor, or press `Ctrl-Shift-→` / `Ctrl-Shift-←`. The width is remembered.

## Keys

| Key | Action |
|---|---|
| `↑`/`↓`, `j`/`k` | move |
| `→`/`←`, `l`/`h` | expand / collapse (or go to parent) |
| `Enter`, double-click | open in the viewer |
| `e` | edit in Neovim |
| `i` | show / hide ignored files |
| `-` | collapse all |
| `Ctrl-R` (`F5`) | refresh |
| `Ctrl-L` (`Alt-F1`), `⌖` | reveal the open file |
| `Ctrl-Shift-→` / `←`, drag divider | widen / narrow the panel |

Mouse: click selects, clicking the arrow expands, the wheel scrolls.

## Settings

`[tree] show_ignored`, `[tree] folder_colours` in `config.toml`; the ignored toggle and panel width are remembered in `state.toml`.

## Code and tests

- `vista-core` (tree model, watcher), `vista-git` (status via `git status --porcelain=v2 -z`), `vista-tui/src/project.rs`, `vista-tui/src/ui/chrome.rs`.
- Integration: status of every fixture state compared with `git status`, including random edit sequences and merge conflicts.
- E2e: `e2e_m1_tree.rs` (expand/collapse, colours, external changes, ignored toggle), `e2e_r1_reveal.rs`, `e2e_r1_resize.rs`.
