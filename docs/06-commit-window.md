# 06 · Commit window

The `2: Commit` tool window (`Alt-2`; `Ctrl-K` from anywhere) replaces the Project tree on the left, like IntelliJ's Commit tool window.

## What it does

- **Changes** and **Unversioned Files** groups with counts, flat or grouped by folder (`g`, remembered), collapsible.
- **Checkboxes** on files, folders and groups (`Space` or click). Tracked changes are ticked by default, unversioned files are not. `[~]` marks a file with only some of its changes ticked.
- **Commit message** editor (`Tab` to focus). An empty message is refused.
- **Commit** (`Ctrl-K`), **Amend** (`m`, prefills the last message), **Commit & Push** (`P`).
- Only ticked files go in: the commit is built in a temporary index, so unticked files that were already staged stay staged.
- **Rollback** (`r`, asks first; added files are unstaged and kept on disk) and **add to VCS** (`a`) for unversioned files.
- Hook output and errors (a failing pre-commit hook, a rejected push) are shown in the window.
- `Enter` or a double-click opens the file's diff ([07](07-diff-viewer.md)).

## Partial commits

In the diff each change has a ☑/☐ checkbox in the gutter; `Space` ticks or unticks the change under the cursor and the header shows "2 of 3 in commit". Ticking one change of an unticked file includes only that change.

vista doesn't build a patch: it rebuilds the file from `HEAD` plus the ticked changes, stores that with `git hash-object -w` and puts the blob in the temporary index. The working tree is untouched. Changes are identified by a hash of their content, so exclusions survive edits elsewhere in the file; they reset after a successful commit. Picking is disabled while the diff ignores whitespace. Per-line selection inside a change is not done yet.

## Keys

| Key | Action |
|---|---|
| `Space`, click | tick / untick |
| `Tab` | message ⇄ list |
| `Ctrl-K` | commit |
| `m` | amend |
| `P` | commit & push |
| `r` | rollback |
| `a` | add to VCS |
| `g` | group by folder |
| `Enter`, double-click | diff |

## Code and tests

- `vista-git` (commit, amend, rollback, push via the CLI), `vista-diff::partial_content`, `vista-tui/src/commit.rs`.
- Integration: commit subsets, deletions, renames, partial content, amend, first commit, hooks, rollback, add, push.
- Proptest: no exclusions gives the new text, all excluded gives the old text, excluding a change keeps exactly its old lines.
- E2e: `e2e_m6_commit.rs` (counts, ticking, amend, hooks, rollback, add, empty message, push to a remote), `e2e_m9_partial.rs` (commit 1 of 3 changes; `git show HEAD:` holds exactly that change).

See also: [20 · Review](20-review.md) (everything changed since a base, committed or not).
