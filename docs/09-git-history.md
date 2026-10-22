# 09 · Git history

`Alt-9` opens the `9: Git` tool window at the bottom, like IntelliJ's Log.

## What it does

- **Commits** newest first: short hash, branch and tag names, subject, author, date.
- **Changed files** of the selected commit on the right: `A`/`M`/`D`/`R`, with the old name for renames.
- `Enter` or a double-click on a file opens its change as a **read-only diff** of the commit against its parent, titled `Δ name @abc1234`. It's the normal diff viewer without checkboxes; `]`/`[` and `J`/`K` step through that commit's files, `e` opens the current version of the file.
- **File history:** `Alt-H` lists only the commits that touched the open file, following renames. `a` goes back to all commits.
- `r` reloads, `Esc` hides the window.

## Keys

| Key | Action |
|---|---|
| `Alt-9` | show / hide the Git window |
| `Alt-H` | history of the open file |
| `Tab`, `←`/`→` | commits ⇄ files |
| `Enter`, double-click | read-only diff |
| `a` | all commits |
| `r` | reload |
| `Esc` | hide |

## Code and tests

- `vista-git/src/log.rs` (`git log` with a field-separated format, `diff-tree -r -M --name-status -z`, content at any revision), `vista-tui/src/log.rs`, `ui/log.rs`, `diffview.rs` (revision pairs).
- Unit: log and name-status parsing, including renames.
- E2e: `e2e_r1_history.rs` (commit list, files, read-only diff, file history across a rename).
