# 05 · Find in Files

IntelliJ's `Ctrl+Shift+F`: `Ctrl-F` (or `Ctrl-Shift-F`) opens a search popup over the whole project. There is no index: each search scans files with the ripgrep crates, which takes ~30–120 ms on the 4,000-file reference repo.

## Popup

- **Query** prefilled from the viewer's find query or the last search, selected so typing replaces it. `Alt-↑`/`Alt-↓` step through the saved history.
- **Toggles:** match case `Alt-C`, whole words `Alt-W`, regex `Alt-X` (remembered). Regex errors are shown inline.
- **File mask:** `Tab` edits it; several patterns and exclusions, e.g. `*.md, !archive/**`.
- **Scopes** (`Alt-S`): In Project · Directory (the folder selected in the tree) · Changed files · Open files.
- **Live results** while typing: the line with matches highlighted, the file name in its git colour and the line number. A header shows "N matches in M files"; results are capped at 1,000.
- **Preview** under the results: the file around the match, syntax-highlighted, match marked.
- `.gitignore` is respected (hidden files included, `.git` never), binary files are skipped.
- Searches run off the UI thread and stream in; a burst of keystrokes cancels and restarts the search.

## Find tool window

`Alt-O` moves the results into the `3: Find` tool window at the bottom (`Alt-3` toggles it): results grouped by file, several result tabs (`←`/`→`, `Ctrl-W` closes one). `Ctrl-Alt-↓` / `Ctrl-Alt-↑` jump to the next / previous result across files, even with the window closed.

## Keys

| Key | Action |
|---|---|
| `Alt-C` / `Alt-W` / `Alt-X` | case / words / regex |
| `Alt-S` | next scope |
| `Tab` | edit the file mask |
| `Alt-↑` / `Alt-↓` | history |
| `Enter`, double-click | open at the match |
| `Alt-Enter` | open in Neovim |
| `Alt-O` | open results in the Find window |
| `Ctrl-Alt-↓` / `↑` | next / previous result |

## Not done

Replace in Files; multi-line queries; an index (planned as M10, only if a repository outgrows scanning).

## Code and tests

- `vista-search` (engine), `vista-tui/src/find.rs`.
- Integration: results compared with `git grep` across literal / case / word / regex / mask / directory-scope combinations; engine units for scopes, cancellation and the cap.
- E2e: `e2e_m5_find.rs` (toggles, mask, regex errors, scopes, preview, open at line, Find window, `Ctrl-Alt-↓`, history, hand-over from Go to File).
