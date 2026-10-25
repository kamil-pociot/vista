# 15 · Keyboard reference

Default shortcuts. `?` in the app shows the active key map, including your changes; Preferences › Keymap changes them ([23](23-keymap.md)).

## Anywhere

| Key | Action |
|---|---|
| `Ctrl-O` | Projects popup |
| `Alt-.` / `Alt-,` | next / previous project |
| `Ctrl-P`, `Ctrl-Shift-N` | Go to File |
| `Ctrl-F`, `Ctrl-Shift-F` | Find in Files |
| `Ctrl-G` | Go to Line:Column ([22](22-go-to-line.md)) |
| `Ctrl-E` | recent files |
| `Alt-1` / `Alt-2` / `Alt-3` | Project / Commit / Find tool window |
| `Alt-9` / `Alt-T` | Git / Terminal tool window |
| `Alt-H` | history of the open file |
| `Alt-R` | Review panel ([20 · Review](20-review.md)) |
| `Ctrl-K` | commit |
| `Tab` / `Shift-Tab` | next / previous panel |
| `Alt-←` / `Alt-→` | previous / next file tab |
| `Ctrl-W` | close tab |
| `]` / `[` (`F7` / `Shift-F7`) | next / previous change |
| `Ctrl-Alt-↓` / `↑` | next / previous Find result |
| `Ctrl-L` (`Alt-F1`) | reveal the open file in the tree |
| `Ctrl-Shift-→` / `←` | widen / narrow the left panel |
| `Shift-L` (`Alt-L`) | line numbers |
| `Ctrl-R` (`F5`) | refresh |
| `Ctrl-T` | next look |
| `Ctrl-\` | leave the editor or a terminal |
| `Ctrl-,` (`⚙` in the top bar) | Preferences (`Tab`: Keymap) |
| `?` (`F1`) | help |
| `q`, `Ctrl-Q` | quit |

## Per area

| Area | Keys |
|---|---|
| Project tree | arrows, `Enter` open, `Alt-Enter` edit, `Alt-I` ignored, `-` collapse all; typing starts a speed search (`↑↓` matches, `Enter` open, `Esc` end) |
| Viewer | `w` wrap, `←`/`→` sideways, `/` `n` `N` find, `Ctrl-G` line:column, `m` raw/rendered, `d` diff ⇄ file (the `File \| Diff` switch), `e` edit; select with drag, double-click, `Shift-↑/↓`, `Ctrl-A`; `Ctrl-C`/`Cmd-C`/`y` copy (a mouse selection is copied on release), `Esc` clear |
| Go to File | `Enter` view, `Alt-Enter` edit, `Tab` tabs, `Alt-I` ignored |
| Find in Files | `Alt-C` case, `Alt-W` words, `Alt-X` regex, `Alt-S` scope, `Tab` mask, `Alt-↑↓` history, `Enter` open, `Alt-Enter` edit, `Alt-O` Find window |
| Commit | `Space` tick, `Tab` message, `Ctrl-K` commit, `m` amend, `P` commit & push, `r` rollback, `a` add, `g` group, `Enter` diff |
| Diff | `d` back to the file, `]`/`[` changes, `J`/`K` files, `Space` tick change, `u` unified, `i` whitespace, `z` collapse, `w` wrap, `Enter` open, `e` edit side by side |
| Git window | `Tab`/`←→` lists, `Enter` diff, `a` all commits, `r` reload, `Esc` hide |
| Projects popup | type to match, `Tab` complete folder, `Enter` open, `Ctrl-W` close project, `>` command |
| Editor | everything goes to Neovim; `Ctrl-\` leaves, `Ctrl-G` Go to Line:Column |
| Go to Line:Column | digits and `:`, `Enter` go, `Esc` cancel |
| Preferences › Keymap | `Enter` add a key, `Delete` remove, `←`/`→` pick a key, `r` reset, `R` reset all, `/` filter, `Tab` Settings |
| Terminal tool window | everything goes to the shell except `Ctrl-\` (leave) and the tool-window keys; `Shift-Enter` is sent as `Alt-Enter` |

## Action names

For `[keys]` in `config.toml` (the Keymap page shows them under each action): `quit`, `help`, `focus-next`, `focus-prev`, `project`, `commit-window`, `find-window`, `goto-file`, `goto-line`, `find-in-files`, `recent-files`, `edit`, `leave-editor`, `close-tab`, `next-tab`, `prev-tab`, `next-change`, `prev-change`, `commit`, `refresh`, `toggle-ignored`, `line-numbers`, `projects`, `git-log`, `terminal`, `file-history`, `grow-panel`, `shrink-panel`, `next-project`, `prev-project`, `reveal-file`, `collapse-all`, `cycle-theme`, `next-occurrence`, `prev-occurrence`, `settings`, `review`. Use `"none"` to unbind a key.
