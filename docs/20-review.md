# 20 · Review

The **Review** panel lists every file that differs between a base revision and the working tree: what was committed since the base, what isn't committed yet, and new untracked files. It's for looking over a branch's work (an agent's, say) when the Commit window is empty because the work is already committed. Each file opens as a read-only diff, base ⇄ working tree, in the usual diff viewer.

## Opening it

- `vista --review <rev> [PATH]` (with or without `--only`). `<rev>` is anything git understands (`main`, a hash, `HEAD~3`); vista resolves it once (`git rev-parse --verify <rev>^{commit}`) and keeps the hash, so the base doesn't move while work goes on.
- `--review-label <text>` adds a line naming what's under review (herdr-vista passes the agent's branch or session).
- From another program through the control socket (the `review` command).

When the base is `HEAD` there's nothing committed to review: vista shows the [Commit window](06-commit-window.md) instead. A revision git doesn't know opens vista as usual with a message saying why (`` `nope` is not a commit here ``).

One review per project; a new one replaces it.

## The panel

It replaces the Project tree on the left (one left panel at a time), titled `R: Review`:

- a header, `Review · 3 files since abc1234 · 1 commit` (the commit count, `git rev-list --count <base>..HEAD`, moves to its own line when the panel is narrow), and the label when there is one;
- the files, coloured by status like the Commit window (modified, added, deleted, renamed with `← old name`), grouped by folder when the Commit window groups (`g` there, `commit.group_by_folder`), without checkboxes.

The list comes from `git diff --name-status -z -M <base>` (the working tree against the base, ignoring the index) plus `git ls-files --others --exclude-standard -z` (as added). It refreshes in the background whenever files change, and after new commits; the base stays.

## Diffs

`Enter` or a double-click opens the selected file's diff: the base on the left (`<short> · review base (read-only)`, the old path for renames, empty for added files), the working tree on the right (empty for deleted files). The tab is titled `Δ name ⇠ abc1234`. The diff follows the file as it changes on disk.

- Read-only: `Space` doesn't tick anything.
- `]` / `[` step through changes and on to the next or previous review file; `J` / `K` go straight to the next or previous file. The review list follows.
- `e` edits side by side against the base, as for any diff ([08 · Editing with Neovim](08-editing-with-neovim.md)).

## Keys

| Key | Action |
|---|---|
| `Alt-R` | show the review again (`review` in `[keys]`); without one, says how to open one |
| `↑` / `↓` (`k` / `j`), `PgUp` / `PgDn`, `Home` / `End` | select |
| `Enter`, double-click | open the file's diff |
| `Esc`, `Alt-1` | back to the Project tree (`Alt-2`: the Commit window) |

## Code and tests

- `vista-git/src/review.rs`: `resolve`, `is_head`, `Review::load` (name-status plus untracked, the commit count).
- `vista-tui/src/review.rs` (`ReviewView`: rows, folders, keys, clicks, list order for `J` / `K`), `ui/review.rs` (drawing), `diffview.rs` (`DiffSpec::base`, `Base`: base ⇄ working tree), `app.rs` (`review_base` off the UI thread, `App::apply_review`, background refreshes via `AppEvent::Review`, `open_review_diff`, `Action::ToggleReview`), `args.rs` (`--review`, `--review-label`).
- Unit: review rows from name-status and untracked files (renames keep the old path), the `HEAD` base, rows flat or by folder, keys, a refresh keeping the selection, base diffs (modified, renamed, added, deleted, binary), argument parsing.
- Integration: `vista-git/tests/review.rs` (a branch's work since `main`).
- E2e: `e2e_r1_review.rs`: `--review main` on a branch with a committed change and rename plus uncommitted and untracked files (header, colours, label, the diff, `J` / `K`, on-disk edits and new commits updating the diff and list); `Esc`, `Alt-R`, `Alt-2` and double-click; `--review HEAD` opening the Commit window; a bad revision's message.

See also: [06 · Commit window](06-commit-window.md), [07 · Diff viewer](07-diff-viewer.md), [09 · Git history](09-git-history.md).
