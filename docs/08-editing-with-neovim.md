# 08 · Editing with Neovim

vista embeds a real Neovim (`nvim --embed`, msgpack-RPC, line grid) and draws it in the editor area. Your own `init.lua`, plugins and LSP setup work as usual.

## Editing a file

- `e` opens the selected tree file, or the viewer's file at its current line. Switching file tabs switches buffers.
- While the editor has focus every key goes to Neovim, except `Ctrl-\` (back to the tree), the tool-window keys and `Ctrl-G` ([Go to Line:Column](22-go-to-line.md), filled in with Neovim's cursor).
- `:w` saves and refreshes git colours, the gutter and the Commit window at once. `:q` returns to the viewer.
- `●` marks unsaved buffers in tabs and the tree; quitting vista with unsaved changes asks for a second `q`.
- **Follows the viewer:** long lines wrap in Neovim if the viewer wrapped them, line numbers follow `Shift-L`, and `:set wrap` / `:set nowrap` carry back to the viewer.
- **Theme sync:** Neovim is painted with the active look (~50 highlight groups), or keeps your colourscheme with `theme_sync = false`.
- Without `nvim`, or with one older than 0.10 (Ubuntu's own package is 0.9), `$VISUAL` / `$EDITOR` runs instead (vista suspends and resumes the terminal) and the status bar says why.

## Side-by-side diff editing

`e` in a diff opens Neovim's diff mode: `HEAD` read-only on the left, the working file on the right, re-diffed as you type.

- It opens at the same line the diff was on, with line numbers.
- The sides stay aligned row by row, wrapped lines included: the shorter side gets blank rows, missing lines get ╱ rows, as in vista's diff. vista draws these with its own virtual lines (Neovim's own filler is turned off while it's open), and both windows scroll together.
- It follows the diff's wrap and collapse settings; `:set wrap` or `zi` there carry back to the diff.
- `:q` returns to the diff.

## Settings

```toml
[editor]
config = "user"     # user = your init.lua, minimal = nvim --clean
theme_sync = true   # paint Neovim with the active look
nvim = "nvim"
```

## Code and tests

- `vista-nvim` (RPC client, grid model, `open_diff.lua`, `close_diff.lua`), `vista-tui/src/editor.rs`.
- Unit: grid model fed recorded `redraw` events, key notation, a real Neovim round trip.
- Unit: reading `nvim --version` (0.10 or newer counts as available).
- E2e (skipped without Neovim 0.10+; CI installs it from Neovim's release, as Ubuntu's apt package is too old): `e2e_m3_editor.rs` (edit, save, colour flips to modified, unsaved marker, quit confirmation, theme sync, `$EDITOR` fallback), `e2e_r1_diff_edit.rs` (diff mode, opening line, alignment rows, wrap and collapse sync).
