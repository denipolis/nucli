# Architecture

One Rust crate on GPUI. One window per open workspace. `docs/SPEC.md` describes the behavior; this file describes the code and its gotchas.

## Files

| File | Role |
|---|---|
| `main.rs` | Startup: fonts, settings, keymap, then onboarding or `launch`: store, windows, config file polling, folders from the command line and the OS. |
| `onboarding.rs` | Setup window shown when `settings.json` is missing: welcome animation, steps, backup and Nucli 1 import. While `onboarding::Active` is set, `settings::update` changes the global without writing the file. |
| `store.rs` | `Store` (one global entity): workspaces, projects, tab ids, sessions, last choices, conversations, diff stats. Opening, closing, selecting, restoring tabs and writing `state.json`. |
| `persist.rs` | Serialized shapes of `state.json`, restored terminal output files. |
| `status.rs` | One second poll: foreground process, Claude Code status, subagents, branch. Attention, notifications, spinner tick, diff stat refresh, notification clicks. |
| `watcher.rs` | One `notify` watcher per open project. Events bump `Store.revisions`, which panels observe. |
| `session.rs` | `Session` entity, one per tab: its `Terminal`, title, color, status, command line and environment. |
| `terminal/mod.rs` | `Terminal`: pty, `alacritty_terminal` grid, reader, parser and waiter threads, prompt anchoring, OSC notification scanning. |
| `terminal/view.rs` | Grid element, mouse, selection, links, IME and focus reporting. `impl Render for Session`. |
| `terminal/keys.rs` | Keystroke to bytes: legacy sequences and the kitty protocol. |
| `terminal/boxes.rs` | Box and block characters as rectangles. |
| `terminal/links.rs` | Finding URLs and `path:line:col` under the mouse. |
| `terminal/find.rs` | Terminal find bar. |
| `window.rs` | `WorkspaceView`, the window root: title bar, layout, overlays, focus. |
| `actions.rs` | Handlers for commands, launching, closing, renaming, moving, context menus, quit and close prompts. |
| `commands.rs` | The command table: name, title, action, keys per platform, the menus shared by the native menu bar and the title bar, `keybindings.json`. |
| `sidebar.rs` | Projects tree, agents board, history, filter, workspace popup. |
| `overview.rs` | Project overview and empty viewport. |
| `panel.rs` | Panel frame and the terminal panel. |
| `files.rs`, `changes.rs`, `commits.rs` | Files, Changes and History panel views. |
| `drag_out.rs` | macOS only: hands a file dragged past the window edge to an `NSDraggingSession`. |
| `file_ops.rs` | File operations of the Files tree: create, rename, move, duplicate, cut, copy, paste, delete, drag and drop, and its context menu. |
| `file_view.rs` | File tabs, path bar, images, Open With, close and save prompts. |
| `editor.rs` | Text view: diff rows, highlighting, editing, undo, IME. |
| `editor/find_bar.rs` | Find and replace bar of the text view: fields, toggles, hits and edits. |
| `find.rs` | Find and replace: query building, matching across rows, replacement templates, case preservation. |
| `highlight.rs` | Tree-sitter grammars and capture to token mapping. |
| `hold.rs` | Tap or hold on the new shell and new agent keys. A key bound without a modifier, or run from a menu or the palette, acts at once. macOS sends no key-up while Cmd is held, so there it polls `CGEventSourceKeyState`. |
| `git.rs` | `git` CLI wrapper, parsers, commit graph lanes. |
| `history.rs` | Agent conversation files and their cache. |
| `claude.rs` | Claude Code status files and subagents. |
| `process.rs` | Foreground process name and folder through `sysinfo`. |
| `agents.rs` | Agent and shell profiles, detection, command lines. |
| `import.rs` | macOS only: one-time import of Nucli 1 preferences. |
| `notify.rs` | Desktop notifications and the Dock badge. macOS uses `UNUserNotificationCenter`, which needs an app bundle, so an unbundled debug binary posts none. The older `NSUserNotificationCenter` is refused for a bundle identifier that has already used the new API, as Nucli 1 did. |
| `palette.rs`, `palettes.rs` | Palette view (query, ranked rows, prompt mode) and its item lists. |
| `font_picker.rs` | Font combobox for terminals and the file view, used by the settings page and onboarding. Focus returns to whatever had it when the list closes. |
| `fuzzy.rs` | Fuzzy scorer with match positions. |
| `switcher.rs` | Tab switcher overlay. |
| `splits.rs`, `splits_view.rs` | Layout tree, panes with their headers, drop rules and the drop preview. |
| `menu.rs` | Context menu overlay. |
| `prompt.rs` | Confirmation dialogs, installed with `set_prompt_builder`. |
| `text_input.rs` | Single line input with IME. |
| `resize.rs` | Pane resize handles. |
| `titlebar.rs` | Title bar: workspace name, menus on Linux and Windows, location, status messages and their list, Windows caption buttons. |
| `statusbar.rs` | Status chips under the viewport. |
| `settings_view.rs` | Settings page in the viewport. |
| `theme.rs`, `ui.rs` | Palettes, custom themes from the `themes` folder, derived UI colors, active theme. |
| `icons.rs` | Nerd Font codepoints, file type glyphs. |
| `settings.rs`, `paths.rs` | `settings.json`, platform folders, atomic writes. |
| `devtools.rs` | Debug builds only: control file. |

## State

`Store` is the single source of truth. Tabs are referred to by id everywhere; `Store.sessions` maps ids to entities, so projects move between workspaces without touching the running processes. Windows observe the store and re-render. Output from a terminal notifies only its session.

Everything per window lives in `Workspace` so it is saved with it. A window view only keeps what does not outlive it: open menus, the palette, rename state, editors.

Background work reports itself with `store::begin_progress` and `store::finish_progress`; the title bar shows it after `PROGRESS_DELAY`. Beginning does not notify, because the 110 ms spinner timer redraws while anything is in progress, so quick work causes no extra renders.

Saving is debounced (`Store::schedule_save`). Terminal output is written only on quit and when a workspace closes (`Store::snapshot(true, _)`); `persist::prune_scrollback` removes files nothing refers to, because closed workspaces keep theirs.

Tabs without a project live in `Workspace::scratch`, a `Project` with an empty path that the sidebar lists without a row of its own. `Project::cwd` turns it into the home folder, and the panel and quick open skip it.

`Store::changed` calls `Workspace::normalize_groups`, which keeps the tabs of each split group next to each other in the project's tab list, in layout order. Every path that edits layouts or tab order relies on it, so the sidebar bracket never spans unrelated rows.

A drag shows its preview through `WorkspaceView::drop_target`: panes and file columns set it from `on_drag_move` and read it on drop. GPUI has no drag-cancel event, so `WorkspaceView::render` clears it whenever no drag is active.

The file view's split groups live in `Project::groups` and `Project::file_layout`, the same `Layout` tree as terminal splits with group ids as panes, so `splits.rs` serves both. A file belongs to one group, because an `Editor` entity can only be drawn once per frame. `state.json` from before groups stored `open_files` and `active_file`, which load into a single group; a layout that does not match its groups is flattened into one group on load.

## Commands and keys

`commands::COMMANDS` is the only list of commands: it drives key bindings, the palette and the macOS menus. A binding's context limits where it applies (`Terminal`, `FileView`, `TextInput`). `keybindings.json` is applied on top of the defaults, and a `null` becomes `NoAction` in every context.

On Linux and Windows `ctrl` letters must reach programs in the terminal, so app commands use `ctrl-shift`. The file view and text inputs add their own `ctrl` bindings in their contexts.

## Terminal

- The reader thread feeds bytes to `Term` itself rather than using `alacritty_terminal`'s event loop, because it must see the raw bytes: OSC 9 and 777 notifications are not exposed by the emulator, and shells need `ESC[999;1H` inserted after every `ESC[2J` to stay on the bottom row.
- `anchor_bottom` tracks `?1049`, `?1047` and `?47` inside each chunk, so a program that switches to the alternate screen and clears it in the same read is not moved.
- `Session` anchors only when the tab is not an agent and no known agent is in the foreground. Do not gate on "the shell is the foreground process": `clear` is its own process.
- Printable keys are left to the input handler so IME and dead keys work. `key_down` only handles keys that `keys::encode` turns into bytes and stops propagation for those.
- Environment: macOS starts from a clean environment, because an app launched from a tool inherits that tool's private variables, which break agents started inside Nucli. `FORCE_HYPERLINK=1` is needed because hyperlink detection in many CLIs is an allowlist of terminals.
- Non-ASCII characters are shaped one per run at their column, because fallback fonts can have other advances than the cell.
- Kills use `SIGHUP` (`portable-pty` default): an interactive zsh ignores `SIGTERM`.

## File view

`Editor.rows` is the rendered diff: file lines plus removed lines. Saving writes every row that is not removed. Typing into a removed row is refused; deleting across removed rows is fine because they never reach the file. Reloads are skipped while dirty, so an agent writing the file never discards unsaved edits.

Highlighting parses the new side and the old side of the diff separately and maps ranges back to rows, on a background thread. While typing, the edited line loses its colors until the next parse finishes.

Tabs for a commit's version of a file are stored as `path@@hash`.

## Agents

- Claude Code status comes from `<config>/sessions/<pid>.json`, an undocumented file: when it is missing or changes format, the output based heuristic takes over. The `.key` files next to it hold secrets and are never opened.
- `conversation` follows the session id in that file, so `/clear` and `/resume` change what the tab resumes.
- Codex assigns its own ids, so a new Codex tab's id is looked up when the tab is saved.

## Gotchas

- `libc` is pinned to 0.2.186 in `Cargo.lock`. `gpui`'s `xattr` 0.2 dependency needs `libc::ENOATTR`, which newer `libc` versions do not define on Linux. Do not run a plain `cargo update` without checking the Linux build.
- Platforms consume shift for symbols, so `ctrl-shift-2` arrives as `ctrl-@`. `commands::shifted_alias` also binds the US layout's shifted form of every such key.
- On Windows a native modal dialog pumps messages, so GPUI tasks run inside whatever update opened it and panic with `RefCell already borrowed`. Show one from its own thread, as `notify::alert` does.
- `Window::handle_input` panics outside paint: call it from a `canvas` paint closure, not prepaint.
- Inside `Store::update`, never call anything that reads the store through its global, such as `WorkspaceView` methods: GPUI panics. Put such logic on `Workspace`.
- Never call `WindowHandle::update` or `WindowHandle::read` from inside a window update, for example from a key action or a panel's click handler: they fail because the window is taken out while it updates, and `read` looks like the window is gone. Use `cx.defer`.
- On Windows `window_control_area(Drag)` turns the whole element into a window caption, so clicks on its children never arrive unless they use `occlude()`. The title bar's clickable children and its caption buttons all do. The bar itself uses it too: a click reaching the window root's `track_focus` counts as handled, and GPUI then keeps it from Windows, so the window would not move. `start_window_move` does nothing on Windows.
- A macOS build linked against the 26 SDK gets larger window buttons. `UIDesignRequiresCompatibility` in `Info.plist` keeps the smaller ones whichever SDK builds it.
- `alacritty_terminal`'s parser holds everything after `CSI ? 2026 h` until the matching end or a 150 ms timeout that only its caller can fire, so the parser runs apart from the blocking pty read.
- Prompt anchoring moves the cursor, which breaks ConPTY: it owns the screen and tracks the cursor itself, and a resize during startup, which every new tab gets, left the terminal blank for good. So on Windows (`LIFTS_PROMPT`) nothing is sent or rewritten. `TerminalElement` paints the grid lower by the blank rows under the content, and the same offset goes into `view.layout` for the mouse.
- GPUI runs key bindings before key listeners. Recording a shortcut uses `App::intercept_keystrokes`, which runs first, so the key does not also trigger its command. A plain `on_key_down` would.
- GPUI drags never leave the window: `on_drag` keeps a value inside the app and GPUI has no OS drag source. `WorkspaceView` watches drags of files with `on_drag_move` on the root, which fires even outside the window, and once the pointer leaves it starts an `NSDraggingSession` from the current `mouseDragged` event and stops the GPUI drag. The pasteboard item carries `public.file-url` and the path as plain text. GPUI reads only `NSFilenamesPboardType` on drop.
- `Window::dispatch_event` returns a private type, so input cannot be simulated with it from outside `gpui`.
- GPUI does not paint while the macOS screen is locked, and window captures come back black. It also stops presenting while a window is fully covered, so captures of a window behind others show an old frame.
- On X11 GPUI starts painting only after a window manager reports the window as visible, so a bare Xvfb stays black.
- `runtime_shaders` is on because the Metal compiler is not installed everywhere; shaders compile at startup instead.
- Release builds on Windows compile shaders with `fxc.exe` from the Windows SDK; they cannot be cross-compiled from another system. `GPUI_FXC_PATH` overrides its location.

## Testing the UI

Debug builds read commands from the file named by `NUCLI_CONTROL`: `keys <keystrokes>`, `type <text>`, `action <command>`, `open <folder>`, `draw` (a full layout and paint without presenting, which also works while the screen is locked), `dump`, `text <tab id>`, `activate`, and on macOS `capture <window id> <png>` and `menu <Menu/Item>`, which prints the native menus and the Dock badge, then performs that item, and `event <window number> <down|drag|up|rdown|rup|move> <x> <y>` `event <window number> key <text> <key code> <modifier flags>`, `event <window number> text|mark <text>` for the native text input calls, or `event <window number> front` to raise the window, which send real AppKit events to that window only. Key events follow AppKit's order: window key equivalent, menu, then the first responder. All of it works while the screen is locked if `draw` runs between steps. Lines appended to the file run in order.

Running in a Linux container under Xvfb with a window manager, a software Vulkan driver (`mesa-vulkan-drivers`) and `xdotool` allows real clicks, keys and screenshots with nothing reaching the host's desktop. Starting `dunst` in the app's D-Bus session shows its notifications, and `dunstctl action 0` clicks one.

The `x86_64-pc-windows-gnu` target builds with `mingw-w64` and its tests run under Wine with `CARGO_TARGET_X86_64_PC_WINDOWS_GNU_RUNNER=wine`, given a stub `icuuc.dll` exporting `u_strlen`. The app itself does not draw under Wine: older versions cannot compile GPUI's shaders and have no `ThreadPoolTimer`.
