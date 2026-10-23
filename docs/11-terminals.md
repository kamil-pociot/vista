# 11 · Terminals

vista runs real terminals: a pseudo-terminal per program, parsed by vt100 and drawn cell by cell with the program's colours. There are two kinds.

## Terminal tabs (full screen)

For long-running programs such as `claude agents`:

- Open one from `Ctrl-O`: the commands in `[terminal] commands` are listed (`claude agents` by default), or type `>` and any command. `>` alone opens your shell.
- It runs through your login shell (so your `PATH` applies) in the active project's folder, and shows as a tab `▶ claude agents` next to the project tabs, full screen.
- Every key goes to the program except `Ctrl-\` (back to the project). Mouse events are passed on to programs that ask for them.
- `Shift-Enter` reaches the program as `ESC CR` (what `Alt-Enter` sends), so it adds a newline in Claude Code's prompt. Programs in vista's terminals can't use the kitty keyboard protocol, which is how Ghostty normally tells Shift-Enter from Enter. This needs a terminal around vista that reports Shift-Enter, such as Ghostty, kitty or iTerm2 with the kitty protocol on.
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

## Notifications

Programs ask the terminal for desktop notifications with escape sequences. Claude Code sends one when it is waiting for your input or a permission. vista passes these on to the terminal it runs in, so Ghostty (or iTerm2, kitty) still shows them. This works for both kinds of terminal.

- Passed on: `OSC 9 ; text` (iTerm2, Ghostty), `OSC 777 ; notify ; title ; body` (rxvt, Ghostty) and kitty's `OSC 99`. ConEmu-style `OSC 9 ; 4 ; …` progress reports and other OSCs stay inside.
- Claude Code picks its kind of notification from `TERM_PROGRAM`. Programs in vista inherit it from the terminal vista runs in, so directly in Ghostty nothing needs configuring. Multiplexers such as herdr and tmux drop `TERM_PROGRAM`, and then Claude Code sends nothing on `auto`. Set `"preferredNotifChannel": "ghostty"` in `~/.claude/settings.json`.
- Inside tmux the sequence is wrapped for tmux to hand on, which needs `set -g allow-passthrough on`.
- herdr doesn't pass these sequences on. It shows its own notifications for the agents it sees in its panes, and an agent inside vista is hidden from it. So inside a herdr pane (`HERDR_ENV=1`), vista also runs `herdr notification show "<title> · <tab>" --body <text> --sound request`. The `herdr` it runs is `$HERDR_BIN_PATH`, or the one on `PATH`.
- Ghostty doesn't show a notification that comes from the tab you're looking at, with or without vista. To try one, run `sleep 5; printf '\e]777;notify;Test;hello\a'` in a vista terminal and switch to another app. Claude Code's "waiting for your input" comes about 60 s after its reply, if you haven't touched it.
- Every notification passed on is logged at debug level (`VISTA_LOG=debug`), which helps when one doesn't show up.
- A tab that isn't on screen when its program sends one is marked until you look at it. A terminal tab shows `● claude agents` instead of `▶`. A project shows `● name` when its Terminal tool window's shell sent the notification while another project or a terminal tab was on screen. The mark uses the look's `ui.warning` colour.

## Code and tests

- `vista-tui/src/term_tab.rs` (PTY, reader thread, key and mouse encoding, picking notifications out of the output), `ui/term.rs`, `workspace.rs` (terminal tabs, marking tabs), `app.rs` (bottom terminal), `terminal.rs` (`notify`: the escape sequence, tmux wrapping, `herdr notification`), `ui/projects.rs` (the `●` mark).
- Unit: xterm key encoding (Shift-Enter as `ESC CR`), mouse reporting only when the program asks, a real program round trip, which OSCs count as notifications (and their title and body), the herdr command.
- E2e: `e2e_r1_terminal.rs` (run a command, type into it, exit and close, `>` shell, tool window focus and hiding, a notification reaching the outer terminal and marking a terminal tab or a project tab, a fake `herdr` getting it inside herdr, Shift-Enter arriving as `ESC CR`), `e2e_r1_tabs.rs` (dragging terminal tabs). The harness's `expect_output` waits for raw bytes vista writes, such as the passed-on notification.
