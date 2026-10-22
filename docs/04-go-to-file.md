# 04 · Go to File

IntelliJ's "Go to File": `Ctrl-P` (or `Ctrl-Shift-N`) opens a popup where a few letters find any file.

## What it does

- **Fuzzy matching** with nucleo over every non-ignored path: `FoBaCo` finds `FooBarController.rs`, `stat` finds `status.rs`.
- **IntelliJ-like ranking:** camel-hump and word-start hits score highest, matches in the file name beat matches in folders, exact names come first, recently opened files get a boost.
- **Tabs** `All / Files / Text / Recent` (`Tab` / `Shift-Tab`). `Text` hands the query to Find in Files ([05](05-find-in-files.md)).
- **`name:42`** opens the file at line 42.
- Matched characters are highlighted; the selected file is previewed.
- `Alt-I` includes ignored files.
- The index is rebuilt in the background when files change. Up to 500 results are listed.
- Mouse: click selects, double-click opens, the wheel scrolls.

## Recent files

`Ctrl-E` lists recently opened files (up to 50, newest first, saved per project).

## Keys

| Key | Action |
|---|---|
| type | filter |
| `↑`/`↓` | move |
| `Enter`, double-click | open in the viewer |
| `Alt-Enter` | open in Neovim |
| `Tab` / `Shift-Tab` | switch tabs |
| `Alt-I` | include ignored files |
| `Esc` | close |

## Code and tests

- `vista-search` (index, ranking), `vista-tui/src/goto.rs`.
- Unit: ranking table (`stat` → `status.rs` first, camel-hump cases), 50k paths ranked in under 16 ms (release).
- E2e: `e2e_m4_goto.rs` (open, `name:42`, keystroke latency on a 50k-file fixture).
