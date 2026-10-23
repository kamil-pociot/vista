# 17 · Clickable labels and buttons

Everything on screen that names a window, an option or a key can be clicked. A click does exactly what the key does: the label runs the same action or presses the same key, so clicking and typing never behave differently.

## What can be clicked

| Where | Labels | Same as |
|---|---|---|
| Top bar | `Ctrl-P Go to File`, `Ctrl-F Find in Files`, `⎇ branch` | `Ctrl-P`, `Ctrl-F`, `Alt-9` (Git window) |
| Bottom bar | `1: Project`, `2: Commit`, `3: Find`, `T: Terminal`, `9: Git`, `?: Help` | their tool-window keys |
| Icon rail (looks with `tool_windows = "icon-rail"`) | `◫` `⎇` `⌕` `?` | Project, Commit, Find in Files, Help |
| Key-hint line (looks with `keyhints = true`) | each hint | its key |
| Empty editor | Go to File, Find in Files, Project, Commit, Help | their keys |
| Find in Files popup | scopes (`In Project`, `Directory`, `Changed files`, `Open files`), `Cc` `W` `.*`, the query, `File mask`, footer hints | picks that scope; `Alt-C` `Alt-W` `Alt-X`; moves the cursor to that field; `⏎`, `Alt-⏎`, `Alt-O`, `Alt-S`, `Tab` |
| Find tool window | result tabs | shows that tab |
| Go to File popup | `All` `Files` `Text` `Recent`, `[ ] include ignored (Alt-I)`, footer hints | shows that tab; `Alt-I`; `⏎`, `Alt-⏎`, `Tab` |
| Projects popup | `Enter open · Tab complete folder · Ctrl-W close project · Esc` | those keys |
| Git window | `Tab files · ⏎ diff · a all commits · r refresh · Esc` | those keys, with the Git window focused |
| Commit window | `[ ] Amend`, `Commit ^K`, `Commit & Push (P)` | `m`, `Ctrl-K`, `Ctrl-Alt-K` |
| Confirm dialog | `y yes`, `n no` | `y`, `n` |
| Diff viewer | `◂` `▸` (previous / next change), `(␣)`, `u unified  i whitespace  w wrap  z collapse  J/K file  ⏎ open  e edit` | `[` `]`, `Space`, those keys |
| Find in file (`/`) | `n`, `N`, `Esc close` | those keys |
| Help | anywhere | closes it, like any key |

While a popup is open only its own labels can be clicked; the screen behind it is covered.

## Code and tests

- `vista-tui/src/ui/click.rs`: `Click` (an action, a key with the window to focus first, or a direct pick: Find scope or field, Go to File tab, Find window tab, Amend), the per-frame list of buttons, and `click::line`, which draws a row of spans and records where the clickable ones landed (left, centred or right aligned).
- Drawing code in `ui/` builds its rows with `click::line` / `click::hints`. `ui::draw` drops the buttons under a popup and stores the frame's buttons in `App::buttons`.
- `App::handle_mouse` looks a left click up in `App::buttons` first (project tabs in the top bar win over the hints under them); `App::click` runs it.
- Unit: buttons land where their text is drawn for every alignment, gaps aren't buttons, cut-off labels aren't buttons, popups hide what's under them.
- E2e: `e2e_r1_clicks.rs` (bottom bar and top bar, Help, Find popup toggles, scopes and mask, Go to File tabs and toggle, Amend, diff hints).

See also [12 · Tabs, tool windows and layout](12-tabs-and-layout.md) for the rest of the mouse support.
