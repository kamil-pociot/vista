# 13 · Looks

A look (theme) is one TOML file. Adding a design means writing a file, not code.

## Structure

- `[palette]`: semantic colour tokens such as `git.modified`, `syntax.keyword`, `diff.inserted`, `diff.word.inserted`, `ui.selection`.
- `[chrome]`: layout options: borders, panel headers, tab style, tool-window bar, git indicators, key hints, spacing.
- `inherits = "darcula"` starts from another look and overrides only what you list.

```toml
# ~/.config/vista/themes/mine.toml
name = "Mine"
inherits = "darcula"
[palette]
"git.modified" = "#5FA8FF"
[chrome]
borders = "rounded"
```

## Built in

- **darcula**: IntelliJ classic, the default (design A).
- **quiet**: borderless and modal.
- **paper**: light, rounded.

## Using looks

- `Ctrl-T` cycles looks; the choice is remembered.
- `theme = "…"` in `config.toml`, `--theme`, or `VISTA_THEME`. `--theme` and `VISTA_THEME` win over the remembered look.
- Theme files reload live while vista runs.
- Neovim is painted with the active look unless `[editor] theme_sync = false`.

## Rules

- Widgets never use colour literals; only `vista-tui/src/style.rs` turns tokens into colours (`tests/lints.rs` enforces it).
- Every token is listed in `vista_theme::TOKENS` (`crates/vista-theme/src/lib.rs`).
- A WCAG contrast test guards every built-in look.

## Code and tests

- `vista-theme`, `vista-tui/src/style.rs`.
- Snapshots: every look (`render_m8_looks.rs`); e2e: `e2e_m8_looks.rs` (cycling, live reload, inherits).
