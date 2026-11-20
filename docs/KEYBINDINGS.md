# Keybindings

Defaults below. Every command can be rebound under Settings, Keybindings, or in `keybindings.json` (palette: "Open Keybindings File"): `{"cmd-t": "new_shell", "cmd-shift-d": null}`. Keys use `cmd`, `ctrl`, `alt`, `shift` and names like `enter`, `escape`, `tab`, `space`, `left`, `pageup`, `f7`; `null` removes a default. The command palette shows each command's name and current key.

On Linux and Windows every default uses the same key as on macOS with the modifiers translated: `cmd` becomes `ctrl-shift`, `cmd-shift` becomes `ctrl-alt-shift`, `cmd-alt` becomes `ctrl-alt` and `ctrl-cmd` becomes `ctrl-alt-shift`. Plain `ctrl` letters belong to the terminal (`ctrl-c`, `ctrl-r`, …). Exceptions: font size and settings use plain `ctrl`, `new_tab_in_folder` is `ctrl-alt-t` because `ctrl-alt-shift-t` is `new_agent`, full screen is `f11` because `ctrl-alt-shift-f` is `find_in_files`, go back and forward are `alt-left` and `alt-right` in the file view, and inside the file view and text inputs the usual `ctrl-c`, `ctrl-v`, `ctrl-x`, `ctrl-a`, `ctrl-z`, `ctrl-shift-z` and `ctrl-s` work.

| Command | Name | macOS | Linux, Windows |
|---|---|---|---|
| New workspace | `new_workspace` | `cmd-n` | `ctrl-shift-n` |
| New shell (last used), hold for the shell picker | `new_shell` | `cmd-t` | `ctrl-shift-t` |
| New agent (last used), hold for the agent picker | `new_agent` | `cmd-shift-t` | `ctrl-alt-shift-t` |
| Shell picker | `new_shell_picker` | | |
| Agent picker | `new_agent_picker` | | |
| New tab picker | `new_tab_picker` | | |
| New tab in another folder | `new_tab_in_folder` | `cmd-alt-shift-t` | `ctrl-alt-t` |
| New tab without project | `new_tab_without_project` | | |
| Move tab or panel terminal out of its project | `move_out_of_project` | | |
| Open project | `open_project` | `cmd-o` | `ctrl-shift-o` |
| Quick open a file | `quick_open` | `cmd-p` | `ctrl-shift-p` |
| Command palette | `command_palette` | `cmd-shift-p` | `ctrl-alt-shift-p` |
| Close file tab, else tab | `close` | `cmd-w` | `ctrl-shift-w` |
| Close project | `close_project` | `cmd-shift-w` | `ctrl-alt-shift-w` |
| Rename tab | `rename_tab` | | |
| Split right / down (file view: active file into a new group) | `split_right` / `split_down` | `cmd-d` / `cmd-shift-d` | `ctrl-shift-d` / `ctrl-alt-shift-d` |
| Close split (file view: merge the group) | `close_split` | `cmd-alt-w` | `ctrl-alt-w` |
| Focus split left / right / up / down | `focus_left` … | `cmd-alt-left` … | `ctrl-alt-left` … |
| Terminal panel: show and focus, hide when focused | `toggle_terminal` | ``ctrl-` `` | ``ctrl-` `` |
| New terminal in the panel | `new_terminal` | ``ctrl-shift-` `` | ``ctrl-shift-` `` |
| Tab switcher (hold the modifier, release to switch) | `tab_switcher` / `tab_switcher_back` | `ctrl-tab` / `ctrl-shift-tab` | `ctrl-tab` / `ctrl-shift-tab` |
| Next / previous tab | `next_tab` / `previous_tab` | `cmd-shift-]` / `cmd-shift-[` | `ctrl-alt-shift-]` / `ctrl-alt-shift-[` |
| Tab 1 … 9 | `select_tab_1` … | `cmd-1` … `cmd-9` | `ctrl-shift-1` … `ctrl-shift-9` |
| Collapse or expand a dock | `toggle_left_dock` / `toggle_right_dock` / `toggle_top_dock` / `toggle_bottom_dock` | | |
| Reset the panel layout | `reset_layout` | | |
| Filter the shown list | `filter_list` | `cmd-alt-f` | `ctrl-alt-f` |
| Show Projects / Agents / History (conversations) | `show_projects` / `show_agents` / `show_history` | | |
| Show Uplink (opens Settings, Uplink while it is off) | `show_uplink` | | |
| Turn Uplink on or off | `toggle_uplink` | | |
| Uplink: pair a device (shows a one-time code) | `pair_device` | | |
| Show Files / Changes / History (commits) | `show_files` / `show_changes` / `show_commits` | | |
| Find in files (shows Search) / replace in files | `find_in_files` / `replace_in_files` | `cmd-shift-f` / `cmd-shift-h` | `ctrl-alt-shift-f` / `ctrl-alt-shift-h` |
| Next / previous search result (file view and Search) | `next_search_result` / `previous_search_result` | `f4` / `shift-f4` | `f4` / `shift-f4` |
| Full screen | `toggle_full_screen` | `ctrl-cmd-f` | `f11` |
| Font bigger / smaller / reset | `font_bigger` … | `cmd-=` / `cmd--` / `cmd-0` | `ctrl-=` / `ctrl--` / `ctrl-0` |
| Clear terminal and scrollback | `clear` | `cmd-k` | `ctrl-shift-k` |
| Find / next / previous | `find` / `find_next` / `find_previous` | `cmd-f` / `cmd-g` / `cmd-shift-g` | `ctrl-shift-f` / `ctrl-shift-g` / `ctrl-alt-shift-g` |
| Replace in the file view | `replace` | `cmd-r` | `ctrl-shift-r` |
| Copy / paste / select all in a terminal (on Windows `ctrl-c` also copies while text is selected, otherwise it interrupts, and `ctrl-v` pastes) | `copy` / `paste` / `select_all` | `cmd-c` / `cmd-v` / `cmd-a` | `ctrl-shift-c` / `ctrl-shift-v` / `ctrl-shift-a` |
| Undo / redo / cut in the file view | `undo` / `redo` / `cut` | `cmd-z` / `cmd-shift-z` / `cmd-x` | `ctrl-z` / `ctrl-shift-z` / `ctrl-x` |
| Save file | `save` | `cmd-s` | `ctrl-s` |
| Next / previous change | `next_change` / `previous_change` | `f7` / `shift-f7` | `f7` / `shift-f7` |
| Go to definition (also `cmd`-click, `cmd-alt`-click opens it to the right) | `go_to_definition` | `f12` | `f12` |
| Find references (Search for the name) | `find_references` | `shift-f12` | `shift-f12` |
| Go to symbol in file | `go_to_symbol` | `cmd-shift-o` | `ctrl-alt-shift-o` |
| Go to line (`line` or `line:column`) | `go_to_line` | `ctrl-g` | `ctrl-g` |
| Browse the folder of the shown file (the breadcrumbs' list) | `browse_folder` | `cmd-shift-.` | `ctrl-alt-shift-.` |
| Go back / forward (also the mouse's back and forward buttons) | `go_back` / `go_forward` | `ctrl--` / `ctrl-_` | `alt-left` / `alt-right` |
| Hide the file view | `hide_file_view` | `escape` | `escape` |
| Settings page | `open_settings` | `cmd-,` | `ctrl-,` |
| Settings file | `open_settings_file` | | |
| Keybindings in settings | `open_keybindings` | | |
| Keybindings file | `open_keybindings_file` | | |
| Select theme | `select_theme` | | |
| Import theme / open themes folder | `import_theme` / `open_themes_folder` | | |
| Select language of the shown file | `select_language` | | |
| Rename / delete workspace | `rename_workspace` / `delete_workspace` | | |
| Detect agents / shells | `detect_agents` / `detect_shells` | | |
| Toggle notifications | `toggle_notifications` | | |
| Quit | `quit` | `cmd-q` | `ctrl-shift-q` |

While a shell in the terminal panel has focus, `close`, `tab_switcher`, `tab_switcher_back`, `next_tab`, `previous_tab` and `select_tab_1` … `select_tab_9` act on the panel's shells instead of the tabs.

Terminal keys on macOS (fixed): `cmd-left` / `cmd-right` line start / end, `cmd-backspace` delete to line start, `alt-left` / `alt-right` word back / forward, `alt-backspace` delete word back, `alt` + key sends Meta (`ESC` prefix).

Find and replace in the file view: `enter` / `shift-enter` next and previous match, `tab` moves between the fields, `enter` in the replace field replaces and moves on, `cmd-enter` (`ctrl-enter`) replaces all, `esc` closes. Toggles: `cmd-alt-c` match case, `cmd-alt-w` whole word, `cmd-alt-r` regular expression, `cmd-alt-p` preserve case, `cmd-alt-l` in selection (`ctrl-alt-c`, `ctrl-alt-w`, `ctrl-alt-r`, `ctrl-alt-p`, `ctrl-alt-l` on Linux and Windows).

Search view: `enter` searches at once, `tab` moves between the fields, `cmd-enter` (`ctrl-enter`) replaces all, `esc` returns focus to the viewport. The toggles use the same keys as the file view's find bar, without `cmd-alt-l`.

New tab picker: arrows, `enter`, `1`–`9` launch the numbered agent or shell, `esc` closes. Rows under CONVERSATIONS resume that conversation.

Projects view: double-click a project or tab to rename it inline (`enter` saves, `esc` cancels, empty restores the default name).
