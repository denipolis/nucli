# Keybindings

Defaults below. Every menu shortcut can be changed in Settings › Keybindings (double-click a shortcut, press the keys; `⌫` removes it).

| Shortcut | Action |
|---|---|
| `⌘N` | New window |
| `⌘T` | New tab with the agent or shell last used in this project (fallback: last used globally, then the first agent) |
| `⇧⌘T` | New tab (picker with agents and shells, last used preselected); without a selection the tab has no project |
| `⌥⇧⌘T` | New tab in another folder |
| `⌘O` | Open project |
| `⌘P` | Quick open a file of the current project (`⌘Return` opens with the default app) |
| `⇧⌘P` | Command palette: commands, tabs, projects, workspaces, agents, conversations, files |
| `⌘W` | Close the open file tab, else close tab (asks for running agents), else clear the viewport. Never closes a project |
| `⇧⌘W` | Close project |
| `⌘D` / `⇧⌘D` | Split right / down with a new tab (last used agent or shell) |
| `` ⌃` `` | Terminal panel: show and focus it (opens a shell if the project has none), hide it when it already has focus |
| `` ⌃⇧` `` | New terminal in the project's terminal panel |
| `⌘W` in the terminal panel | Close that terminal |
| `⌥⌘W` | Close split (the tab keeps running) |
| `⌥⌘←` `⌥⌘→` `⌥⌘↑` `⌥⌘↓` | Focus the split in that direction |
| `⌃Tab` / `⌃⇧Tab`, `⇧⌘]` / `⇧⌘[` | Tab switcher (most recently used first); keep the modifier held and repeat, release to switch, `Esc` cancels |
| `⌘1` … `⌘9` | Tab by position in this window's sidebar |
| `⌘B` | Toggle sidebar |
| `⌥⌘B` | Toggle Files and Changes panel (all windows) |
| `⌥⌘F` | Filter the sidebar (`Return` selects the first match, `Esc` clears) |
| `⌥⌘1` / `⌥⌘2` / `⌥⌘3` | Sidebar: Projects / Agents board / History (opens the sidebar if hidden) |
| `⌃⌘1` / `⌃⌘2` / `⌃⌘3` | Panel: Files / Changes / Git History (opens the panel if hidden) |
| `⌃⌘F` | Full screen |
| `⌘=` / `⌘-` / `⌘0` | Font bigger / smaller / reset |
| `⌘K` | Clear terminal and scrollback |
| `⌘F`, `⌘G`, `⇧⌘G`, `⌘E` | Find, next, previous, use selection |
| `⌘,` | Settings |
| `⌘←` / `⌘→` | Line start / end (`^A` / `^E`) |
| `⌘⌫` | Delete to line start (`^U`) |
| `⌥←` / `⌥→` / `⌥⌫` | Word back / forward / delete word back (`ESC b` / `ESC f` / `^W`) |
| `⌥` + key | Meta (`ESC` prefix) |
| `Esc` in file view | Back to terminal (file tabs stay open) |
| `F7` / `⇧F7` in file view | Next / previous change |
| `⌘S` in file view | Save the edited file |
| `Return` / `Tab` in the suggestion popup | Accept the suggestion (`Esc` dismisses) |

Agent picker: arrows, `Return`, `1`–`9` (profiles), `Esc`. Rows under "Recent conversations" resume that conversation.

`⌘N` creates a new workspace (its own window); switch workspaces with the system `` ⌘` ``. Closing a workspace window asks to delete the workspace; `⌘Q` keeps all workspaces and reopens them.

Sidebar: double-click a tab or project to rename it inline (`Return` saves, `Esc` cancels, empty name restores the default). File › Rename Tab… renames the current tab.
