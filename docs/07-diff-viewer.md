# 07 · Diff viewer

An IntelliJ-style diff opens from the Commit window, from `d` or the `File | Diff` switch in the viewer, and (read-only) from the Git window ([09](09-git-history.md)). Its tab is titled `Δ name`.

## What it shows

- **Side by side:** `HEAD` on the left (read-only), the working file on the right, one scroll position. Blocks are aligned, with hatched filler where one side has no lines (Patience diff).
- **Colours:** inserted green, modified blue, deleted grey; **word-level highlights** inside modified blocks.
- A centre gutter coloured per change with the cursor marker, and syntax highlighting on both sides.
- **Unified view** (`u`), **ignore whitespace** (`i`), **collapse unchanged regions** (`z`). All three are remembered.
- **Long lines:** `w` wraps (both sides stay aligned row by row) or scrolls sideways (`←`/`→`, shift+wheel, trackpad; both sides together).
- New or unversioned files show as all added, deleted files as all removed, binaries as a size summary, images side by side. Renames compare against the old path.
- Diffs follow edits on disk.
- Scrollbar, scroll past the end and line numbers behave as in the viewer.

## File | Diff switch

A changed file's tab shows `d  File  Diff` at the right of the tab row; the view shown is highlighted, and a click on the other one switches to it. `d` does the same.

- From the file, the tab turns into its diff against HEAD at the same line (from the top of the file, at the first change). From the diff, it turns back into the file at the diff's working-tree line.
- One tab: the switch changes the tab in place. When the other view is already open in a tab of its own (a diff opened from the Commit window, the file opened with `Enter`), that tab comes to the front instead.
- In a history or review diff, which stays as it is, the file opens in its own tab.
- Unchanged files have no switch (`d` still opens their empty diff); it's hidden while the file is in Neovim.

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
| `d` (`File \| Diff` switch) | back to the file, in the same tab |
| `Ctrl-G` | go to a working-tree line, unfolding it if collapsed ([22](22-go-to-line.md)) |
| `e` | edit side by side in Neovim ([08](08-editing-with-neovim.md#side-by-side-diff-editing)) |

## Code and tests

- `vista-diff` (alignment, hunks with content-hash ids, word ranges), `vista-tui/src/diffview.rs`, `ui/diff.rs`.
- Proptest: aligned rows rebuild both sides exactly, applying all hunks gives the new text, folding never loses rows, EOF-newline-only changes.
- Snapshot: the mockup-A diff (`render_m7.rs`).
- E2e: `e2e_m7_diff.rs` (navigation, word highlights, toggles, new/deleted/image files, next file, live update, open at line, the File / Diff switch and `d` both ways in one tab), `e2e_r1_feedback.rs` (sideways, wrap, cross-file changes, no wrap-around).
