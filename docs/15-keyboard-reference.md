# 15 · Keyboard reference

Default shortcuts. `?` in the app shows the active key map, including your changes.

## Anywhere

| Key | Action |
|---|---|
| `Ctrl-O` | Projects popup (also terminal commands) |
| `Alt-.` / `Alt-,` | next / previous project |
| `Ctrl-P`, `Ctrl-Shift-N` | Go to File |
| `Ctrl-F`, `Ctrl-Shift-F` | Find in Files |
| `Ctrl-E` | recent files |
| `Alt-1` / `Alt-2` / `Alt-3` | Project / Commit / Find tool window |
| `Alt-9` / `Alt-T` | Git / Terminal tool window |
| `Alt-H` | history of the open file |
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
| `Alt-B` | sidebar: projects and Claude sessions |
| `Ctrl-,` (`⚙` in the top bar) | open settings (`config.toml`) |
| `?` (`F1`) | help |
| `q`, `Ctrl-Q` | quit |

## Per area

| Area | Keys |
|---|---|
| Project tree | arrows, `Enter` open, `Alt-Enter` edit, `Alt-I` ignored, `-` collapse all; typing starts a speed search (`↑↓` matches, `Enter` open, `Esc` end) |
| Viewer | `w` wrap, `←`/`→` sideways, `/` `n` `N` find, `Ctrl-G` line, `m` raw/rendered, `d` diff, `e` edit; select with drag, double-click, `Shift-↑/↓`, `Ctrl-A`; `Ctrl-C`/`Cmd-C`/`y` copy (a mouse selection is copied on release), `Esc` clear |
| Go to File | `Enter` view, `Alt-Enter` edit, `Tab` tabs, `Alt-I` ignored |
| Find in Files | `Alt-C` case, `Alt-W` words, `Alt-X` regex, `Alt-S` scope, `Tab` mask, `Alt-↑↓` history, `Enter` open, `Alt-Enter` edit, `Alt-O` Find window |
| Commit | `Space` tick, `Tab` message, `Ctrl-K` commit, `m` amend, `P` commit & push, `r` rollback, `a` add, `g` group, `Enter` diff |
| Diff | `]`/`[` changes, `J`/`K` files, `Space` tick change, `u` unified, `i` whitespace, `z` collapse, `w` wrap, `Enter` open, `e` edit side by side |
| Git window | `Tab`/`←→` lists, `Enter` diff, `a` all commits, `r` reload, `Esc` hide |
| Projects popup | type to match, `Tab` complete folder, `Enter` open, `Ctrl-W` close project, `>` command |
| Editor | everything goes to Neovim; `Ctrl-\` leaves |
| Terminals | everything goes to the program; `Ctrl-\` leaves; the bottom terminal also passes tool-window keys to vista; `Shift-Enter` is sent as `Alt-Enter` (a newline in Claude Code) |

## Action names

For `[keys]` in `config.toml`: `quit`, `help`, `focus-next`, `focus-prev`, `project`, `commit-window`, `find-window`, `goto-file`, `find-in-files`, `recent-files`, `edit`, `leave-editor`, `close-tab`, `next-tab`, `prev-tab`, `next-change`, `prev-change`, `commit`, `refresh`, `toggle-ignored`, `line-numbers`, `projects`, `git-log`, `terminal`, `file-history`, `grow-panel`, `shrink-panel`, `next-project`, `prev-project`, `reveal-file`, `collapse-all`, `cycle-theme`, `next-occurrence`, `prev-occurrence`. Use `"none"` to unbind a key.
