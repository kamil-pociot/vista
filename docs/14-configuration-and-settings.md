# 14 · Configuration and remembered settings

## Command line

```
vista [PATH]          open a folder (default: .)
vista path/to/file    open a file, with its folder as the project
vista --theme quiet   pick a look
vista --probe         report TERM, TERM_PROGRAM, COLORTERM, TMUX and kitty graphics/keyboard support
vista --probe-image   draw a test gradient with the image protocol
```

## config.toml

`~/.config/vista/config.toml`, or `$VISTA_CONFIG_DIR/config.toml`:

```toml
theme = "darcula"            # or quiet, paper, or a file in ~/.config/vista/themes/

[images]
protocol = "auto"            # auto | kitty | halfblocks | off

[editor]
config = "user"              # user = your init.lua, minimal = nvim --clean
theme_sync = true
nvim = "nvim"

[tree]
show_ignored = true
folder_colours = true

[terminal]
commands = ["claude agents"]

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

Until a setting is changed, its default comes from `config.toml` (or vista's built-in). Delete `state.toml` to reset. Recent files (per project) and the Find in Files history live in vista's cache folder, not in `state.toml`.

## Code and tests

- `vista-core/src/config.rs`, `vista-core/src/state.rs` (`UiState`), `App::remember` in `vista-tui/src/app.rs`.
- Unit: config defaults, partial files, unknown fields rejected; state round trip.
- E2e: `e2e_r1_settings.rs` (settings survive a restart), `e2e_r1_resize.rs` (width), `e2e_r1_projects.rs` (projects).
