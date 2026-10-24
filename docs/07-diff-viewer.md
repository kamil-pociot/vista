# 07 · Diff viewer

An IntelliJ-style diff opens from the Commit window, from `d` in the viewer, and (read-only) from the Git window ([09](09-git-history.md)). Its tab is titled `Δ name`.

## What it shows

- **Side by side:** `HEAD` on the left (read-only), the working file on the right, one scroll position. Blocks are aligned, with hatched filler where one side has no lines (Patience diff).
- **Colours:** inserted green, modified blue, deleted grey; **word-level highlights** inside modified blocks.
- A centre gutter coloured per change with the cursor marker, and syntax highlighting on both sides.
- **Unified view** (`u`), **ignore whitespace** (`i`), **collapse unchanged regions** (`z`). All three are remembered.
- **Long lines:** `w` wraps (both sides stay aligned row by row) or scrolls sideways (`←`/`→`, shift+wheel, trackpad; both sides together).
- New or unversioned files show as all added, deleted files as all removed, binaries as a size summary, images side by side. Renames compare against the old path.
- Diffs follow edits on disk.
- Scrollbar, scroll past the end and line numbers behave as in the viewer.

## Change navigation across files

- The diff opens on the first change; the header shows "change k of n".
- `]` / `[` move between changes. At the last change a popup says "Press ] again to go to b.txt"; the second press opens the next changed file on its first change. Backwards, `[` goes to the previous file's last change. Any other key dismisses the offer.
- After the last change of the last file it stops and says so, without wrapping around.
- `J` / `K` jump straight to the next / previous file of the Commit list.
- The same works in the plain viewer.

## Keys

| Key | Action |
|---|---|
| `]` / `[` | next / previous change (again at the end: next file) |
| `J` / `K` | next / previous file |
| `Space` | tick / untick the change for the commit ([06](06-commit-window.md#partial-commits)) |
| `u` / `i` / `z` / `w` | unified / ignore whitespace / collapse / wrap |
| `←`/`→` | scroll sideways |
| `Enter` | open the file at this line |
| `Ctrl-G` | go to a working-tree line, unfolding it if collapsed ([22](22-go-to-line.md)) |
| `e` | edit side by side in Neovim ([08](08-editing-with-neovim.md#side-by-side-diff-editing)) |

## Code and tests

- `vista-diff` (alignment, hunks with content-hash ids, word ranges), `vista-tui/src/diffview.rs`, `ui/diff.rs`.
- Proptest: aligned rows rebuild both sides exactly, applying all hunks gives the new text, folding never loses rows, EOF-newline-only changes.
- Snapshot: the mockup-A diff (`render_m7.rs`).
- E2e: `e2e_m7_diff.rs` (navigation, word highlights, toggles, new/deleted/image files, next file, live update, open at line), `e2e_r1_feedback.rs` (sideways, wrap, cross-file changes, no wrap-around).
