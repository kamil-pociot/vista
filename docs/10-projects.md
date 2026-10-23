# 10 · Projects

Several projects can be open at once, shown as tabs at the left of the top bar.

## What it does

- **Tabs:** click one to switch, its `×` to close it, `+` to add one. Drag a tab left or right to reorder; the order is remembered.
- `Alt-.` / `Alt-,` step through open projects.
- **Projects popup** (`Ctrl-O`):
  - Type a few letters: open and recent projects match, and so do folders next to them (`lms` finds `~/Repositories/lms-context` when another project lives in `~/Repositories`).
  - Or type part of a path (`~/Repositories/lm`): matching folders there are listed, and `Tab` completes the selected one (`~/Repositories/lms-context/`). A path ending in `/` lists that folder itself first, so `Enter` opens it, followed by its sub-folders to go deeper (`↓`, `Tab`).
  - The list scrolls with the selection when there are more folders than rows.
  - `Enter` switches to, reopens or adds the selected folder; `Ctrl-W` closes the selected open project.
- **Each project keeps its own** tree, file tabs, Commit window, git status, file watcher, editor and bottom terminal. Settings and the look are shared.
- Projects open when vista quits are reopened next time, alongside the one vista is started in.
- **`vista --only [PATH]`** opens just that project: last time's projects aren't reopened, and which projects are open isn't remembered (projects added with `Ctrl-O` included), so a plain `vista` later reopens what it had. Recent projects still learn this one. herdr's vista plugin starts one vista per repository like this.

## Keys

| Key | Action |
|---|---|
| `Ctrl-O`, `+` | Projects popup |
| `Alt-.` / `Alt-,` | next / previous project |
| `Tab` (popup) | complete the folder |
| `Ctrl-W` (popup) | close the selected project |
| click / `×` / drag | switch / close / reorder |

## Code and tests

- `vista-tui/src/workspace.rs` (one `App` per project, events tagged with a project id), `projects.rs` (tab row, popup, folder matching, `~` handling), `ui/projects.rs`, `vista-core/src/state.rs` (`ProjectsState`).
- Unit: folder matching and completion, tab layout and hit testing, drag direction.
- Unit: `--only` parsing.
- E2e: `e2e_r1_projects.rs` (add, switch, close, folder suggestions, completion, reopen on restart, `--only` leaving the remembered projects alone), `e2e_r1_tabs.rs` (dragging project tabs).

See also: [21 · Control socket](21-control-socket.md) (other programs open and show projects in a running vista).
