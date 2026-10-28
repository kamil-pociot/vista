# 21 · Control socket

Every interactive vista listens on a Unix socket, so other programs can ask it to show a project, a [review](20-review.md) or the Commit window without restarting it (open files and unsaved Neovim buffers stay). herdr-vista uses it to switch a running vista to an agent's work. The socket is named after vista's process id, so a program that knows the process (herdr knows a pane's foreground process) can find it.

## Where

`<runtime dir>/<pid>.sock`, where the runtime dir is the first of:

1. `$VISTA_RUNTIME_DIR`
2. `$XDG_RUNTIME_DIR/vista`
3. `$TMPDIR/vista-<uid>`
4. `/tmp/vista-<uid>`

The folder is created only for you (`0700`; one belonging to another user is refused), the socket is `0600`. A leftover file with the same name is replaced. The socket goes when vista quits, including on `SIGTERM` and `SIGHUP` (a closed terminal or herdr pane), which quit like `q` does: settings are saved, the editor stopped. A second signal while that's stuck ends vista at once.

If the socket can't be made (path over 100 bytes, a folder that isn't yours), vista runs without one and says why in its log. `--help`, `--version`, `--probe` and the like don't listen.

## Protocol

One JSON request on one line, one JSON response line back, then the connection closes. Requests are applied on the UI thread, and the reply comes once they are, so a reply means it's on screen. A request is answered within 2 s, or with an error. Open popups and prompts are closed first.

| Request | Effect | Response |
|---|---|---|
| `{"cmd":"status"}` | | `{"ok":true,"protocol":1,"version":"0.6.4","pid":123,"only":true,"projects":[{"root":"/abs/repo","active":true}],"herdr_pane":"w1:p3"}` |
| `{"cmd":"open-project","root":"/abs"}` | shows that project, opening it if needed (as `Ctrl-O`, `Enter`) | `{"ok":true}` |
| `{"cmd":"review","root":"/abs","base":"<rev>","label":"fix auth"}` | `open-project`, then the Review panel on `base` (`label` optional, `null` is none) | `{"ok":true,"files":3,"commits":1}`, or when `base` is `HEAD`, the Commit window and `{"ok":true,"commit_window":true}` |
| `{"cmd":"commit","root":"/abs"}` | `open-project`, then the Commit window | `{"ok":true}` |
| `{"cmd":"open-file","path":"/abs/file","line":42}` | opens the file (`line` optional) in the open project that has it, else in its git repository's (or folder's) project | `{"ok":true}` |

- `only` is whether vista runs with `--only`; `herdr_pane` is `$HERDR_PANE_ID` (`null` outside herdr).
- Errors are `{"ok":false,"error":"…"}`: bad JSON, an unknown `cmd` (so a client can tell an older vista), a missing field, a relative path, a folder or file that isn't there, a bad revision (`` `nope` is not a commit here ``). Git work (resolving a base, listing a review) is done before the request reaches the UI.
- `protocol` goes up when a command changes incompatibly.

From a shell:

```sh
vista --remote <pid> '{"cmd":"status"}'
```

prints the response (and finds the socket the same way vista does).

## Code and tests

- `vista-tui/src/control.rs`: `parse` (requests), `prepare` (paths, git work, off the UI thread), `runtime_dir`, `socket_path`, `Server` (bind, permissions, a thread per connection, the file removed on drop), `remote`.
- `Workspace::control` (applies a request and builds the response), `Workspace::start_control` (from `main`), `AppEvent::Control`, `App::dismiss_popups`, `run.rs` (`SIGTERM` / `SIGHUP` via `signal-hook`), `args.rs` (`--remote`).
- Unit: each command, `null` fields, unknown commands, missing fields and relative paths; the runtime dir's order; the path length check; binding over a leftover file with the right permissions, answering, and removing the file on drop.
- E2e: `e2e_r1_control.rs`: `status` (project, `--only`, `herdr_pane`, version); `review` while a popup is open (Review panel on screen); `open-project` with a second repository; `commit`; a `HEAD` review; `open-file`; bad requests answered while vista carries on; `vista --remote`; the socket gone after `Ctrl-Q`, `SIGTERM` and `SIGHUP`. The harness sets `VISTA_RUNTIME_DIR` to the test's home.

See also: [20 · Review](20-review.md), [10 · Projects](10-projects.md), [14 · Configuration](14-configuration-and-settings.md).
