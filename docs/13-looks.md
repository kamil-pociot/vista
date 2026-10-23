# 13 · Looks

A look (theme) is one TOML file. Adding a design means writing a file, not code.

## Structure

- `[palette]`: semantic colour tokens such as `git.modified`, `syntax.keyword`, `diff.inserted`, `diff.word.inserted`, `ui.selection`.
- `[chrome]`: layout options: borders, panel headers, tab style, tool-window bar, git indicators, key hints, spacing.
- `inherits = "darcula"` starts from another look and overrides only what you list.
- A colour can be `"terminal"`: drawn in the terminal's own background (as herdr draws its panes), so vista blends in with whatever is around it. Such a look also says `terminal_bg = "#…"`, the background it's made for: that colour is used for the contrast test, for blends (diff shades, the cursor line) and wherever the token is a foreground. In Neovim the background becomes `NONE`.

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
- **herdr's themes**, with the same names, so vista can follow herdr: `catppuccin`, `catppuccin-latte`, `tokyo-night`, `tokyo-night-day`, `dracula`, `nord`, `gruvbox`, `gruvbox-light`, `one-dark`, `one-light`, `solarized`, `solarized-light`, `kanagawa`, `kanagawa-lotus`, `rose-pine`, `rose-pine-dawn`, `vesper`. Their chrome colours are the ones herdr draws, measured from herdr itself: the editor and the tree on the terminal's background (`"terminal"`), herdr's surface colour for the top and bottom bars and popups, its accent, text, secondary text (80% text, 20% dim), dim labels and selected row. Syntax, git and diff colours come from each theme's published palette (Kanagawa's status colours are herdr's). A few dim colours are lifted where herdr's fall short of the contrast rules below; the focused selection is a shade stronger than herdr's row so lists stay readable. herdr's `terminal` theme uses the terminal's own colours, which vista can't, so it has no look.

## Following herdr

`theme = "herdr"` in `config.toml` uses herdr's theme (`[theme]` in herdr's `config.toml`, `$XDG_CONFIG_HOME/herdr` or `~/.config/herdr`) and follows it live: change the theme in herdr and vista switches within a couple of seconds ("Look: Nord (following herdr)").

- With herdr's `auto_switch = true`, `dark_name` or `light_name` is used by the macOS appearance (dark elsewhere).
- No herdr config: herdr's default, `catppuccin`.
- A herdr theme with no vista look (`terminal`): darcula at start, else the current look stays, with a note.
- Follow herdr is also a choice in [Preferences](19-preferences.md) (`Ctrl-,`). Picking a look there or with `Ctrl-T` stops following; the pick is remembered and wins over `theme = …` in `config.toml`. `--theme` and `VISTA_THEME` win over both.
- **Fill background** in Preferences paints herdr's looks with their own background (`terminal_bg`) instead of the terminal's.

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
