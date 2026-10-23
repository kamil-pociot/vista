# 19 · Preferences

`Ctrl-,` or the `⚙` at the right of the top bar opens **Preferences**: vista's settings on one page, changed in place. A change applies at once and is remembered in `state.toml`, like every toggle elsewhere in vista. `config.toml` keeps the defaults, and its last row opens that file for what the page doesn't cover (key bindings, the editor, images).

## What's on it

| Section | Setting | Same as |
|---|---|---|
| Appearance | **Look**: Follow herdr, then every look | `Ctrl-T` (looks only) |
| | **Fill background**: paint the look's own background where it would show the terminal's | |
| Viewer | Line numbers, Wrap Markdown, Wrap code | `Shift-L`, `w` |
| Diff | Unified diffs, Collapse unchanged lines, Ignore whitespace (for diffs opened from now on) | `u`, `z`, `i` in a diff |
| Project tree | Show ignored files | `Alt-I` |
| Commit window | Group changes by folder | `g` there |
| Find in Files | Match case, Whole words, Regular expressions | `Alt-C`, `Alt-W`, `Alt-X` there |
| Mouse | Copy selection on release | `[mouse] copy_on_select` |
| More | Edit config.toml… | |

Under the list, a line explains the selected setting.

**Fill background.** herdr's looks (`kanagawa`, `nord`, …) leave the editor and tree to the terminal's background, as herdr leaves its panes, so vista blends in with herdr whatever the terminal's colours. With Fill background on, they get the look's own background instead (`terminal_bg`, see [13 · Looks](13-looks.md)). Looks without terminal colours don't change.

**Look.** Follow herdr uses herdr's theme and switches with it ([13 · Looks](13-looks.md#following-herdr)). Picking a look here, or with `Ctrl-T`, is remembered and wins over `theme = …` in `config.toml`; `--theme` and `VISTA_THEME` still win over both.

## Keys

| Key | Action |
|---|---|
| `↑` / `↓` (`k` / `j`) | select |
| `Space`, `Enter`, `→` (`l`) | change (on / off flips; the look steps forward) |
| `←` (`h`) | change back (the look steps back) |
| `Esc` (`q`), a click beside the page | close |

A click on a row selects and changes it.

## Code and tests

- `vista-tui/src/prefs.rs` (the sections and settings, keys), `ui/prefs.rs` (drawing; rows are clickable), `App::pref_value` / `App::change_pref` (reading and changing each setting through the same code its own key uses), `vista-core/src/state.rs` (`looks.fill_background`, `mouse.copy_on_select`), `vista-theme` (`Theme::fill_terminal`).
- Unit: keys on the page; every setting listed once.
- E2e: `e2e_r1_prefs.rs` (opening, keyboard and click changes remembered in `state.toml`, Fill background painting the terminal-background areas and back, the look stepping to Follow herdr), `e2e_r1_settings.rs` (Edit config.toml… writes and opens the file).
