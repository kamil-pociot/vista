# 11 · Terminal tool window

`Alt-T` opens a shell under the editor, in the project's folder (IntelliJ's Terminal, which is `Alt-F12` there). Each project keeps its own. It's a real terminal: a pseudo-terminal parsed by vt100 and drawn cell by cell with the program's colours.

- While it has focus every key goes to the shell, except `Ctrl-\` (back to the editor) and the tool-window keys (`Alt-1`, `Alt-2`, `Alt-3`, `Alt-9`, `Alt-T`). Mouse events are passed on to programs that ask for them.
- `Shift-Enter` reaches the program as `ESC CR` (what `Alt-Enter` sends): programs inside vista can't use the kitty keyboard protocol, which is how they would otherwise tell it from Enter. zsh, for one, takes it as a newline.
- `Alt-T` again hides it; the shell keeps running. If the shell exits, `Enter` starts a new one.
- It shares the bottom area with the Find and Git windows: opening one hides the others.

Long-running programs (agents, servers) are better off in a terminal multiplexer such as herdr next to vista.

## Code and tests

- `vista-tui/src/term_tab.rs` (PTY, reader thread, key and mouse encoding), `ui/term.rs`, `app.rs` (the tool window).
- Unit: xterm key encoding (Shift-Enter as `ESC CR`), mouse reporting only when the program asks, a real program round trip.
- E2e: `e2e_r1_terminal.rs` (a shell in the project folder, focus and hiding, a new shell after `exit`, Shift-Enter arriving as `ESC CR`).
