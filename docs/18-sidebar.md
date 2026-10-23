# 18 · Sidebar: projects and Claude sessions

`Alt-B` shows a herdr-style sidebar on the left of the window: the open projects on top, and below them your Claude Code sessions with their state. It replaces the tabs in the top bar while it's shown, and is remembered.

## What it shows

```
 Projects                  +
 ▸ file-browser
   ⎇ main ↑2
 ● lms-context
   ⎇ feature/search ↓1

 Claude
 ▶ claude agents
 ● product-owner
   lms-context
 ◐ tree-search
   file-browser
```

- **Projects.** Every open project; the one on screen is highlighted, `●` marks one whose Terminal tool window asked for attention ([11 · Terminals](11-terminals.md)). Under each, its branch and how many commits it is ahead of (`↑`) and behind (`↓`) its upstream, once the project has loaded its status. `+` opens the Projects popup ([10 · Projects](10-projects.md)).
- **Claude.** First the terminal tabs you opened (`claude agents`, shells), then every session `claude agents --json` lists, background and interactive, from any folder, with its folder on the row under its name. The mark is the session's state:

| Mark | State |
|---|---|
| `●` (warning colour) | needs input: a question or a permission |
| `◐` | working |
| `✓` | done |
| `✗` | failed |
| `○` | stopped |

The list is refreshed every two seconds while the sidebar is shown (a run costs about 0.1 s of CPU), and not at all while it's hidden. When the command fails (no `claude` on `PATH`), the section says so.

## Clicks and keys

| Do | Result |
|---|---|
| `Alt-B` | show / hide the sidebar (remembered in `state.toml`) |
| drag its right border | resize it (the width is remembered) |
| click a project | show it |
| click a terminal tab | show it |
| click a session | open it full screen in a terminal tab running `claude attach <id>`, named after the session; a second click shows the same tab |
| click `+` | Projects popup |

A session's tab is an ordinary terminal tab ([11 · Terminals](11-terminals.md)): keys go to Claude, `Ctrl-\` goes back to the project, notifications are passed on and play the notify sound. With the sidebar hidden, its tab shows in the top bar like the others.

## Settings

```toml
[sidebar]
show = false                         # before Alt-B was ever pressed
width = 30                           # until its border is dragged
sessions = "claude agents --json"    # lists sessions as JSON; "" for none
attach = "claude attach {id}"        # opens session {id} in a terminal tab
```

The commands run through your login shell, so your `PATH` applies.

## Code and tests

- `vista-tui/src/sessions.rs`: parsing `claude agents --json` (id, name, folder, state) and the poller thread, which runs only while the sidebar is shown.
- `vista-tui/src/ui/sidebar.rs`: drawing the rows and where each landed for clicks.
- `vista-tui/src/workspace.rs`: the toggle, splitting the window, clicks, opening a session's tab (`TermTab::session` remembers which session a tab shows).
- Unit: parsing every state, rows and marks where their clicks land, branch and folder rows.
- E2e: `e2e_r1_sidebar.rs` (sessions with their marks and folders from a fake list, the project's branch, a click attaching through a fake `attach`, the same tab on a second click, hiding, remembered across restarts, resizing by dragging and keeping the width).
