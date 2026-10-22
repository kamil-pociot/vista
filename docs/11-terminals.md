# 11 · Terminals

vista runs real terminals: a pseudo-terminal per program, parsed by vt100 and drawn cell by cell with the program's colours. There are two kinds.

## Terminal tabs (full screen)

For long-running programs such as `claude agents`:

- Open one from `Ctrl-O`: the commands in `[terminal] commands` are listed (`claude agents` by default), or type `>` and any command. `>` alone opens your shell.
- It runs through your login shell (so your `PATH` applies) in the active project's folder, and shows as a tab `▶ claude agents` next to the project tabs, full screen.
- Every key goes to the program except `Ctrl-\` (back to the project). Mouse events are passed on to programs that ask for them.
- Click a tab to switch, its `×` to close it, drag it to reorder.
- When the program ends, `Enter` closes the tab. Quitting vista with terminal tabs still running asks first.

```toml
[terminal]
commands = ["claude agents", "htop"]
```

## Terminal tool window

`Alt-T` opens a shell under the editor, in the project's folder (IntelliJ's Terminal). Each project keeps its own.

- While it has focus every key goes to the shell, except `Ctrl-\` (back to the editor) and the tool-window keys (`Alt-1`, `Alt-2`, `Alt-3`, `Alt-9`, `Alt-T`).
- `Alt-T` again hides it; the shell keeps running. If the shell exits, `Enter` starts a new one.
- It shares the bottom area with the Find and Git windows: opening one hides the others.

## Code and tests

- `vista-tui/src/term_tab.rs` (PTY, reader thread, key and mouse encoding), `ui/term.rs`, `workspace.rs` (terminal tabs), `app.rs` (bottom terminal).
- Unit: xterm key encoding, mouse reporting only when the program asks, a real program round trip.
- E2e: `e2e_r1_terminal.rs` (run a command, type into it, exit and close, `>` shell, tool window focus and hiding), `e2e_r1_tabs.rs` (dragging terminal tabs).
