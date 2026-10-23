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
- **herdr's themes**, with the same names, so vista can follow herdr: `catppuccin`, `catppuccin-latte`, `tokyo-night`, `tokyo-night-day`, `dracula`, `nord`, `gruvbox`, `gruvbox-light`, `one-dark`, `one-light`, `solarized`, `solarized-light`, `kanagawa`, `kanagawa-lotus`, `rose-pine`, `rose-pine-dawn`, `vesper`. Each is built from the theme's published palette, with darcula's layout. A few secondary colours (comments, dim text) are lifted a little where the original falls short of the contrast rules below. herdr's `terminal` theme uses the terminal's own colours, which vista can't, so it has no look.

## Following herdr

`theme = "herdr"` in `config.toml` uses herdr's theme (`[theme]` in herdr's `config.toml`, `$XDG_CONFIG_HOME/herdr` or `~/.config/herdr`) and follows it live: change the theme in herdr and vista switches within a couple of seconds ("Look: Nord (following herdr)").

- With herdr's `auto_switch = true`, `dark_name` or `light_name` is used by the macOS appearance (dark elsewhere).
- No herdr config: herdr's default, `catppuccin`.
- A herdr theme with no vista look (`terminal`): darcula at start, else the current look stays, with a note.
- `Ctrl-T` still picks another look for now; the next change in herdr takes over again. Following beats the look remembered from `Ctrl-T`; `--theme` and `VISTA_THEME` beat following.

## Using looks

- `Ctrl-T` cycles looks (vista's own, then herdr's, then yours); the choice is remembered.
- `theme = "…"` in `config.toml` (a look, or `"herdr"`), `--theme`, or `VISTA_THEME`. `--theme` and `VISTA_THEME` win over the remembered look.
- Theme files reload live while vista runs.
- Neovim is painted with the active look unless `[editor] theme_sync = false`.

## Rules

- Widgets never use colour literals; only `vista-tui/src/style.rs` turns tokens into colours (`tests/lints.rs` enforces it).
- Every token is listed in `vista_theme::TOKENS` (`crates/vista-theme/src/lib.rs`).
- A WCAG contrast test guards every built-in look.

## Code and tests

- `vista-theme` (`themes/*.toml`, `BUILTIN`), `vista-tui/src/style.rs`, `vista-core/src/herdr.rs` (reading herdr's `[theme]`), `vista-tui/src/workspace.rs` (`follow_herdr`).
- Unit: every built-in look has every token and passes the contrast test; reading herdr's theme, its defaults and `auto_switch`.
- Snapshots: the original looks (`render_m8_looks.rs`); e2e: `e2e_m8_looks.rs` (cycling, live reload, inherits, following herdr's theme as it changes).
