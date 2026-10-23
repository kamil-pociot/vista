# 14 · Configuration and remembered settings

## Command line

```
vista [PATH]          open a folder (default: .)
vista path/to/file    open a file, with its folder as the project
vista --only [PATH]   just this project: don't reopen last time's, or remember which are open
vista --review <REV> [--review-label TEXT]
                      the Review panel: what changed since REV (docs/20-review.md)
vista --theme quiet   pick a look
vista --remote <PID> <JSON>
                      send a request to a running vista's control socket (docs/21-control-socket.md)
vista --probe         report TERM, TERM_PROGRAM, COLORTERM, TMUX and kitty graphics/keyboard support
vista --probe-image   draw a test gradient with the image protocol
```

## config.toml

`~/.config/vista/config.toml`, or `$VISTA_CONFIG_DIR/config.toml`. Most settings are on the **Preferences** page instead (`Ctrl-,`, the `⚙` at the right of the top bar; see [19 · Preferences](19-preferences.md)), which changes them in place. Its last row, Edit config.toml…, opens this file; when there is none yet it is written first, with every setting at its default and a comment on each. Changes here apply when vista starts again.

```toml
theme = "darcula"            # or quiet, paper, a herdr theme (kanagawa, nord, …), "herdr" to follow herdr, or a file in ~/.config/vista/themes/

[images]
protocol = "auto"            # auto | kitty | halfblocks | off

[editor]
config = "user"              # user = your init.lua, minimal = nvim --clean
theme_sync = true
nvim = "nvim"

[tree]
show_ignored = true
folder_colours = true

[mouse]
copy_on_select = true        # copy a mouse selection on release; false: Ctrl-C / Cmd-C / y

[keys]
"ctrl+o" = "goto-file"       # bind a key to an action
"q" = "none"                 # unbind
```

Action names for `[keys]` are listed in [15](15-keyboard-reference.md#action-names). Uppercase letters mean Shift (`"L"`).

## state.toml (remembered settings)

Settings changed in the app are saved to `state.toml` next to `config.toml` and restored on the next run:

- line numbers;
- wrapping, separately for Markdown and code;
- the diff's unified, collapse and ignore-whitespace options;
- Commit-window grouping;
- showing ignored files;
- Find in Files case, words and regex;
- the left panel's width;
- open projects and their order;
- the look picked with `Ctrl-T`.

Until a setting is changed, its default comes from `config.toml` (or vista's built-in). Delete `state.toml` to reset.

**Several vistas at once** (one per repository, as herdr's vista plugin runs them) share `state.toml` safely. Every save takes an exclusive lock on `state.toml.lock`, reads the file as it is now, applies just the change being made, writes a temp file of its own (`state.toml.<pid>.tmp`) and renames it into place. One vista's change is never undone by another saving, and each takes the merged result, so it also sees what the others saved. Settings another vista changes don't reach a running one until it next saves something. Recent files (per project) and the Find in Files history live in vista's cache folder, not in `state.toml`.

Each vista also listens on a control socket, `<runtime dir>/<pid>.sock`; `VISTA_RUNTIME_DIR` chooses the folder (else `$XDG_RUNTIME_DIR/vista`, `$TMPDIR/vista-<uid>`). See [21 · Control socket](21-control-socket.md).

## Code and tests

- `vista-core/src/config.rs`, `vista-core/src/state.rs` (`UiState`), `App::remember` in `vista-tui/src/app.rs`.
- Unit: config defaults, partial files, unknown fields rejected; state round trip.
- E2e: `e2e_r1_settings.rs` (settings survive a restart), `e2e_r1_resize.rs` (width), `e2e_r1_projects.rs` (projects).
