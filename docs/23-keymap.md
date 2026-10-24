# 23 · Keymap

Preferences has a second page, **Keymap**: every action with its keys, IntelliJ's Settings › Keymap. `Tab` in Preferences (`Ctrl-,`, `⚙`) switches to it, as does its Keymap… row or a click on `Keymap` at the top. A key is added by pressing it, taken from another action after asking, removed, or reset to the default. Changes apply at once, in every open project, and are remembered.

```
┌ Preferences ──────────────────────────────────────────────────────┐
│  Settings   Keymap    Tab switches                                │
│ Filter: go to                                                     │
│   Go to File                              Ctrl-P   Ctrl-Shift-N   │
│   Go to Line:Column                              Ctrl-G   Alt-G   │
│───────────────────────────────────────────────────────────────────│
│ goto-line · changed; r resets it to Ctrl-G                        │
│ Enter add · Del remove · r reset · R reset all · / filter · Esc   │
└───────────────────────────────────────────────────────────────────┘
```

## What it shows

- Every action, in the order of the help (`?`), with its keys on the right. Actions without a key show `—`.
- An action whose keys differ from the default is drawn in the accent colour; the line under the list says so and what `r` would restore.
- `/` filters by description, action name (`goto-line`) or key (`ctrl-g`).

## Changing keys

- **Add:** `Enter` (or `a`), then press the key. Modifier keys alone don't count; `Esc` cancels.
- **Taken:** a key another action has asks first: `Ctrl-P runs Go to File. Enter: use it for Go to Line:Column instead · Esc: keep it`. Taking it removes it from the other action.
- **Remove:** `←`/`→` pick one of the action's keys (or click it), `Delete` (`Backspace`, `d`) removes it.
- **Reset:** `r` gives the selected action its default keys again (also giving back a default key another action had taken from it); `R` resets every key after asking.

The defaults are vista's plus `[keys]` in `config.toml` ([14](14-configuration-and-settings.md#configtoml)): Reset goes back to those, and config.toml's `[keys]` stay as they are. Changes made here are saved as `[keys]` in `state.toml`, in the same form (`"alt+g" = "goto-line"`, `"ctrl+g" = "none"`), and apply on top of config.toml's. The Settings page's Keymap… row says how many are changed.

Plain letters can be bound too, but the tree's speed search, the viewer and other windows keep the letters they use themselves; a global action on such a letter runs only where nothing else takes it (as with `[keys]`).

## Keys

| Key | Action |
|---|---|
| `↑`/`↓` (`k`/`j`), `PgUp`/`PgDn`, `Home`/`End` | select an action |
| `←`/`→` (`h`/`l`) | select one of its keys |
| `Enter`, `a` | add a key: press it next |
| `Delete`, `Backspace`, `d` | remove the selected key |
| `r` / `R` | reset the action / every action |
| `/` | filter (`Enter` keeps it, `Esc` clears it) |
| `Tab` | Settings page |
| `Esc`, `q` | close (`Esc` clears a filter first) |

A click on a row selects it, a click on a key selects that key, and the hints under the list can be clicked.

## Code and tests

- `vista-tui/src/keymap_page.rs` (the page: rows, filter, capturing a key, conflicts), `keymap.rs` (`KeyChord::config_name`, `Keymap::with`, `bind`, `reset`), `prefs.rs` (`Page`), `ui/prefs.rs` (both pages), `App::keymap_outcome` / `change_keys` / `sync_keymap`, `vista-core/src/state.rs` (`UiState::keys`).
- Unit: key names read back as the same chord; binding, unbinding and resetting on top of config.toml (`keymap`); filtering, capturing, conflicts, picking and removing keys (`keymap_page`); changes applied at once, saved, and followed by another project (`app`).
- E2e: `e2e_r1_keymap.rs` (adding, taking, removing and resetting keys with the top bar following; a removed key after a restart; reset all; clicking the page, rows and keys).

See also: [19 · Preferences](19-preferences.md), [15 · Keyboard reference](15-keyboard-reference.md#action-names), [14 · Configuration](14-configuration-and-settings.md).
