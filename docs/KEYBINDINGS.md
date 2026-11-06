# Keybindings

Defaults below. Every command can be rebound in `keybindings.json` (palette: "Open Keybindings"): `{"cmd-t": "new_tab", "cmd-shift-d": null}`. Keys use `cmd`, `ctrl`, `alt`, `shift` and names like `enter`, `escape`, `tab`, `space`, `left`, `pageup`, `f7`; `null` removes a default. The command palette shows each command's name and current key.

On Linux and Windows, plain `ctrl` letters belong to the terminal (`ctrl-c`, `ctrl-r`, …), so app commands use `ctrl-shift`. Inside the file view and text inputs the usual `ctrl-c`, `ctrl-v`, `ctrl-x`, `ctrl-a`, `ctrl-z` and `ctrl-s` work.

| Command | Name | macOS | Linux, Windows |
|---|---|---|---|
| New workspace | `new_workspace` | `cmd-n` | `ctrl-shift-n` |
| New tab (last used agent or shell) | `new_tab` | `cmd-t` | `ctrl-shift-t` |
| New tab picker | `new_tab_picker` | `cmd-shift-t` | `ctrl-shift-space` |
| New tab in another folder | `new_tab_in_folder` | `cmd-alt-shift-t` | `ctrl-alt-shift-t` |
| New tab without project | `new_tab_without_project` | | |
| Open project | `open_project` | `cmd-o` | `ctrl-shift-o` |
| Quick open a file | `quick_open` | `cmd-p` | `ctrl-shift-e` |
| Command palette | `command_palette` | `cmd-shift-p` | `ctrl-shift-p` |
| Close file tab, else tab | `close` | `cmd-w` | `ctrl-shift-w` |
| Close project | `close_project` | `cmd-shift-w` | `ctrl-alt-shift-w` |
| Rename tab | `rename_tab` | | |
| Split right / down | `split_right` / `split_down` | `cmd-d` / `cmd-shift-d` | `ctrl-shift-d` / `ctrl-alt-shift-d` |
| Close split | `close_split` | `cmd-alt-w` | `ctrl-alt-w` |
| Focus split left / right / up / down | `focus_left` … | `cmd-alt-left` … | `ctrl-alt-shift-left` … |
| Terminal panel: show and focus, hide when focused | `toggle_terminal` | ``ctrl-` `` | ``ctrl-` `` |
| New terminal in the panel | `new_terminal` | ``ctrl-shift-` `` | ``ctrl-shift-` `` |
| Tab switcher (hold the modifier, release to switch) | `tab_switcher` / `tab_switcher_back` | `ctrl-tab` / `ctrl-shift-tab` | `ctrl-tab` / `ctrl-shift-tab` |
| Next / previous tab | `next_tab` / `previous_tab` | `cmd-shift-]` / `cmd-shift-[` | `ctrl-pagedown` / `ctrl-pageup` |
| Tab 1 … 9 | `select_tab_1` … | `cmd-1` … `cmd-9` | `alt-1` … `alt-9` |
| Toggle sidebar | `toggle_sidebar` | `cmd-b` | `ctrl-shift-b` |
| Toggle panel | `toggle_panel` | `cmd-alt-b` | `ctrl-alt-b` |
| Move panel to the other side | `move_panel` | | |
| Filter sidebar | `filter_sidebar` | `cmd-alt-f` | `ctrl-alt-f` |
| Sidebar: Projects / Agents / History | `show_projects` … | `cmd-alt-1` … `3` | `ctrl-alt-1` … `3` |
| Panel: Files / Changes / History | `show_files` … | `ctrl-cmd-1` … `3` | `ctrl-shift-1` … `3` |
| Full screen | `toggle_full_screen` | `ctrl-cmd-f` | `f11` |
| Font bigger / smaller / reset | `font_bigger` … | `cmd-=` / `cmd--` / `cmd-0` | `ctrl-=` / `ctrl--` / `ctrl-0` |
| Clear terminal and scrollback | `clear` | `cmd-k` | `ctrl-shift-k` |
| Find / next / previous | `find` / `find_next` / `find_previous` | `cmd-f` / `cmd-g` / `cmd-shift-g` | `ctrl-shift-f` / `f3` / `shift-f3` |
| Copy / paste / select all in a terminal | `copy` / `paste` / `select_all` | `cmd-c` / `cmd-v` / `cmd-a` | `ctrl-shift-c` / `ctrl-shift-v` / `ctrl-shift-a` |
| Undo / redo / cut in the file view | `undo` / `redo` / `cut` | `cmd-z` / `cmd-shift-z` / `cmd-x` | `ctrl-z` / `ctrl-shift-z` / `ctrl-x` |
| Save file | `save` | `cmd-s` | `ctrl-s` |
| Next / previous change | `next_change` / `previous_change` | `f7` / `shift-f7` | `f7` / `shift-f7` |
| Hide the file view | `hide_file_view` | `escape` | `escape` |
| Settings page | `open_settings` | `cmd-,` | `ctrl-,` |
| Settings file | `open_settings_file` | | |
| Keybindings | `open_keybindings` | | |
| Select theme | `select_theme` | | |
| Quit | `quit` | `cmd-q` | `ctrl-shift-q` |

Terminal keys on macOS (fixed): `cmd-left` / `cmd-right` line start / end, `cmd-backspace` delete to line start, `alt-left` / `alt-right` word back / forward, `alt-backspace` delete word back, `alt` + key sends Meta (`ESC` prefix).

New tab picker: arrows, `enter`, `1`–`9` launch the numbered agent or shell, `esc` closes. Rows under CONVERSATIONS resume that conversation.

Sidebar: double-click a project or tab to rename it inline (`enter` saves, `esc` cancels, empty restores the default name).
