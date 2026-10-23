# vista documentation

vista is a lean, keyboard-driven terminal replacement for the parts of IntelliJ IDEA used every day. Each document below describes one implemented feature area: what it does, the keys, the settings that affect it, and where it lives in the code and tests.

| # | Document | In short |
|---|---|---|
| 01 | [Project tree](01-project-tree.md) | Lazy file tree coloured by git status, ignored files, reveal current file, resizable panel |
| 02 | [Viewer](02-viewer.md) | Code with syntax highlighting, gutter change markers, line numbers, wrap or scroll sideways, find in file, text selection |
| 03 | [Markdown and images](03-markdown-and-images.md) | Rendered Markdown (wrapped tables, source line numbers), raw toggle, images via kitty graphics or half-blocks |
| 04 | [Go to File](04-go-to-file.md) | IntelliJ-style fuzzy file search (`Ctrl-P`), camel humps, `name:42`, recent files |
| 05 | [Find in Files](05-find-in-files.md) | IntelliJ-style full-text search (`Ctrl-F`), toggles, scopes, masks, preview, Find tool window |
| 06 | [Commit window](06-commit-window.md) | Changes and unversioned files, ticking, commit / amend / push, rollback, partial commits |
| 07 | [Diff viewer](07-diff-viewer.md) | Aligned side-by-side or unified diffs, word highlights, change navigation across files |
| 08 | [Editing with Neovim](08-editing-with-neovim.md) | Embedded Neovim, theme sync, side-by-side diff editing with aligned sides |
| 09 | [Git history](09-git-history.md) | Git log tool window (`Alt-9`), commit files, read-only diffs, file history |
| 10 | [Projects](10-projects.md) | Several projects as tabs, the Projects popup, folder suggestions, remembered projects |
| 11 | [Terminal tool window](11-terminals.md) | A shell under the editor (`Alt-T`), in the project's folder |
| 12 | [Tabs, tool windows and layout](12-tabs-and-layout.md) | File tabs, closing and dragging tabs, tool windows, focus, mouse, no F-keys |
| 13 | [Looks](13-looks.md) | Theme files: palette tokens and chrome options; darcula, quiet, paper |
| 14 | [Configuration and remembered settings](14-configuration-and-settings.md) | `config.toml`, `state.toml`, key bindings, environment variables, `--probe` |
| 15 | [Keyboard reference](15-keyboard-reference.md) | Every default shortcut in one place |
| 16 | [Testing](16-testing.md) | Unit, snapshot, property, integration and PTY end-to-end tests |
| 17 | [Clickable labels and buttons](17-clickable-labels.md) | Tool-window names, hints, toggles and buttons do what their keys do |
| 19 | [Preferences](19-preferences.md) | Settings on one page (`Ctrl-,`, `⚙`), changed in place and remembered; Fill background |
| 20 | [Review](20-review.md) | Everything changed since a base (`vista --review <rev>`, `Alt-R`), each file a read-only diff against it |

See also: [`../README.md`](../README.md) (install and quick reference), [`../ROADMAP.md`](../ROADMAP.md) (research, decisions, milestones), [`../AGENTS.md`](../AGENTS.md) (working on the code).
