# Architecture

A Cargo workspace on GPUI: the desktop app at the root (`src/`), `core/` (`nucli-core`), the code shared with the mobile app, and `mobile/` (`nucli-uplink`), the iOS remote control (see Mobile). GPUI is used through `gpui-pre` and `gpui-pre-platform` (published snapshots of Zed's GPUI crates) pinned to one exact version, since any snapshot may change GPUI's API. One window per open workspace. `docs/SPEC.md` describes the behavior; this file describes the code and its gotchas.

## Files

Paths are under `src/` unless they start with `core/`.

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
| `core/src/terminal/render.rs` | Turns a terminal grid into rectangles and shaped text runs (`layout`) and paints them (`paint`): colors, selection, find matches, links, cursor and IME text. |
| `core/src/terminal/keys.rs` | Keystroke to bytes: legacy sequences and the kitty protocol. |
| `core/src/terminal/boxes.rs` | Box and block characters as rectangles. |
| `terminal/links.rs` | Finding URLs and `path:line:col` under the mouse. |
| `terminal/find.rs` | Terminal find bar. |
| `terminal/snapshot.rs` | Serializes a terminal's state (scrollback, screen, styles, links, modes, cursor) into the bytes that rebuild it, for a remote view joining midway. |
| `window.rs` | `WorkspaceView`, the window root: title bar, layout, overlays, focus. |
| `actions.rs` | Handlers for commands, launching, closing, renaming, moving, context menus, quit and close prompts. |
| `commands.rs` | The command table: name, title, action, keys per platform, the menus shared by the native menu bar and the title bar, `keybindings.json`. |
| `sidebar.rs` | Projects tree, agents board, history, filter, workspace popup. |
| `overview.rs` | Project overview and empty viewport. |
| `panel.rs` | Panel frame and the terminal panel. |
| `files.rs`, `changes.rs`, `commits.rs` | Files, Changes and History panel views. |
| `drag_out.rs` | Hands a file dragged past the window edge to the system: `NSDraggingSession` on macOS, `SHDoDragDrop` on Windows, XDND on X11. |
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
| `transcript.rs` | Reads agent transcripts (Claude Code, Codex, pi, Gemini CLI) incrementally: model, context tokens, instruction files, skills, MCP servers, compactions. |
| `agent_info.rs` | `AgentInfo` of a running agent: finds its transcript, merges it with what the agent's folders hold by convention (instruction files, skills, MCP config) for every known agent CLI. |
| `process.rs` | Foreground process name and folder through `sysinfo`. |
| `agents.rs` | Agent and shell profiles, detection, command lines. |
| `import.rs` | macOS only: one-time import of Nucli 1 preferences. |
| `notify.rs` | Desktop notifications and the Dock badge. macOS uses `UNUserNotificationCenter`, which needs an app bundle, so an unbundled debug binary posts none. The older `NSUserNotificationCenter` is refused for a bundle identifier that has already used the new API, as Nucli 1 did. |
| `palette.rs`, `palettes.rs` | Palette view (query, ranked rows, prompt mode) and its item lists. A palette with an `anchor` opens above that point instead of at the top, as the status chips' combobox. |
| `font_picker.rs` | Font combobox for terminals and the file view, used by the settings page and onboarding. Focus returns to whatever had it when the list closes. |
| `fuzzy.rs` | Fuzzy scorer with match positions. |
| `switcher.rs` | Tab switcher overlay. |
| `splits.rs`, `splits_view.rs` | Layout tree, panes with their headers, drop rules and the drop preview. |
| `menu.rs` | Context menu overlay. |
| `prompt.rs` | Confirmation dialogs, installed with `set_prompt_builder`. |
| `text_input.rs` | Single line input with IME. |
| `resize.rs` | Pane resize handles. |
| `titlebar.rs` | Title bar: workspace name, menus on Linux and Windows, location, status messages and their list, Windows caption buttons. |
| `statusbar.rs` | Status chips under the viewport: `Chip` ids and toggles, agent chips, their comboboxes and menus. |
| `settings_view.rs` | Settings page in the viewport. |
| `core/src/theme.rs` | Palettes, custom themes (`load` reads a `themes` folder), derived UI colors, `ItemColor`. |
| `theme.rs`, `ui.rs` | Re-export `core::theme`, reload custom themes from the config folder, active theme. |
| `core/src/icons.rs` | Nerd Font codepoints, file type glyphs. |
| `core/src/fonts.rs` | The bundled Maple Mono NF files. |
| `core/src/logo.rs` | The Nucli logo as text and as the animated pixel logo of onboarding. |
| `settings.rs`, `paths.rs` | `settings.json`, platform folders, atomic writes. |
| `devtools.rs` | Debug builds only: control file. |
| `uplink/mod.rs` | `Uplink` (one global entity): settings, paired devices, live links, remote catalogs, host side rules (what is shared, view or control) and remote views (mirror sessions). |
| `core/src/uplink/wire.rs` | Keys, codes, SPAKE2 pairing, Noise handshakes and the encrypted, fragmented frame stream. |
| `core/src/uplink/net.rs` | Threads: listener, handshakes, dialing, joining, per connection reader and writer, multicast DNS discovery. Events go to the main thread over one channel. |
| `core/src/uplink/protocol.rs` | Messages (JSON) and terminal frames (binary) between devices. |
| `core/src/uplink/devices.rs` | `uplink.json`: the device key and the paired devices, written readable by the user only. |
| `core/src/uplink/invite.rs` | Local addresses and the pairing link shown as a QR code. |
| `core/src/terminal/mod.rs` | The terminal `Feed` and `spawn_mirror`, the thread that rebuilds a remote terminal from its feeds. |
| `core/src/session.rs` | Tab `Kind` and `Activity`, carried by the Uplink protocol. Desktop re-exports them from `session.rs`, and `Feed` from `terminal/mod.rs`. |
| `uplink/host.rs` | Answers `Message::Request` from paired devices: files, file contents, git changes, diffs, log, commits and branches of shared projects on the background executor, conversation history, and with control folder browsing, adding projects and resuming conversations. |
| `uplink/act.rs` | `Request::Act` and `Request::Tab` with control: file operations and git on the background executor (through `file_ops`, `git` and `changes::discard`), project and tab changes on the main thread through the same helpers as the desktop's menus, without the desktop's confirmations. `target` keeps new paths inside the project, symbolic links included; names and hashes cannot start with `-`. |
| `uplink/page.rs`, `uplink/sidebar.rs` | Settings › Uplink and the Uplink sidebar tab. |

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

On Linux and Windows `ctrl` letters must reach programs in the terminal, so defaults keep the macOS key and translate the modifiers: `cmd` to `ctrl-shift`, `cmd-shift` and `ctrl-cmd` to `ctrl-alt-shift`, `cmd-alt` to `ctrl-alt` (`docs/KEYBINDINGS.md`). The file view and text inputs add their own `ctrl` bindings in their contexts.

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
- Agent info runs inside the one second poll for every tab whose foreground process is a known agent. `transcript::read` keeps a byte offset per file and parses only complete new lines, so a long transcript is read once. Folder discovery (`agent_info::scan`) is cached for 10 seconds per agent, config folder and working folder.
- Transcripts are matched to the process: Claude by the session id in its status file, Codex by the conversation id or the newest rollout of the folder started after the process (only the date folders around the start are listed), pi by the session id or the newest file of the folder, Gemini through `projects.json`. Without a match the info still has the folder discovery and the process start time.
- What the transcript records wins over discovery: Claude's `instructions`, `nested_memory`, `skill_listing` and MCP deltas, pi's system prompt sections. Codex records no instruction file paths, so they come from its lookup rules. Unknown models get no context window unless the tokens seen pass 200k, then 1M is assumed.
- No agent writes compaction progress to disk (Claude's `sessions/<pid>.json` stays `busy`, the transcript gets `compact_boundary` only after it ends), so `Session::apply` scans the visible screen for the agent's own spinner line (`Compacting conversation…`, `Compacting context...`, `Auto-compacting...`) and sets `AgentInfo.compacting`.

## Uplink

Peer to peer: every device listens and dials. Between two paired devices there is one TCP connection; when both dial at once (the existing connection is under 5 seconds old), both keep the one opened by the device with the smaller key and close the other; otherwise the newer connection replaces the older, which may be dead after sleep. A watchdog thread shuts a socket down when its whole handshake, `Hello` included, takes over 10 seconds, since socket timeouts apply to each read only. Each connection runs a reader and a writer thread; the writer pings every 15 seconds and a reader that hears nothing for 45 seconds drops the link. Dialing backs off from 2 to 30 seconds and restarts when multicast DNS sees the device.

Security lives in `wire.rs`: SPAKE2 turns the 40-bit code into a key that authenticates a Noise `XXpsk3` handshake, so a wrong guess fails online and costs one of 3 attempts; sessions use Noise `XX` and both sides check the remote static key. A PSK pattern alone must never take the short code directly: an active attacker could test codes offline. Both sides send `Hello` as the first encrypted message and only then report the link, so the dialing side learns about a refusal before it shows the device online. Frames are a 2-byte length and one Noise message (at most 65535 bytes), and a message of up to 16 MB is split across frames with a "more" byte.

A remote view mirrors the terminal byte stream instead of drawing cells: the host's `Terminal` has taps (`terminal::Tap`) that receive exactly what its own `Term` processes, under the same lock, so a viewer's `Term` of the same size stays identical and scrollback, selection, find and links work unchanged. `Feed` carries the bytes of both parsers (the reader thread's and the main thread's own, for anchoring and clears), resizes and parser flushes in order. A new viewer is attached by the reader thread between chunks and outside a synchronized update: `Start` resets the viewer (`ESC c` plus the size) and `snapshot.rs` sends the current state, then the live stream continues. The viewer never answers terminal queries (the host's terminal does) and never times out synchronized updates itself; it waits for the host's `Flush`.

The terminal has one size: `resize_remote` marks a host terminal as sized by a viewer, which makes the local view's own resizes no-ops until local input (`Terminal::claim`). A mirror's view sends a resize request when its own size changes while it holds the size (the terminal has the size it asked for last), and before input when the terminal has another size, so opening or showing a remote tab never takes the size.

Requests carry an id that the `Message::Response` repeats. A project in a request is its path as in the catalog and must be a shared project that is not the home folder; relative paths in it are joined to the project and canonicalized, so neither `..` nor a symbolic link can leave it. A peer that does not know requests ignores them as `Message::Unknown`, so the asking side times out.

All rules are checked on the host on every request: shared (`uplink::shared`), the device's access, blocked, and that the connection watches the tab. `Uplink::enforce` runs after every store change, stops watchers of tabs that stop being shared and pushes changed catalogs. A `Watch` for a tab the connection already watches is ignored; a viewer watches again only after `Stop::Behind`, which ends the old watcher first. A tap returns false to remove itself: when its connection closes, when the watcher is stopped, or when more than 8 MB wait on the connection, in which case the host sends `Stop::Behind` and the viewer watches again with a fresh snapshot.

Mirrors are `Session`s whose `Terminal` has no pty (`Backend::Mirror`). They sit in `Store.sessions` under `uplink:<key prefix>:<tab id>` and are selected like tabs, but belong to no project: `Store::shower_of` finds the window showing one, `remove_tab` closes them, and the status poll skips them because they have no process.

## Mobile

`mobile/` is Nucli Uplink (`nucli-uplink`), a GPUI app on Zed's own iOS platform (`gpui_ios`, pulled in by `gpui-pre-platform` for `target_os = "ios"`): a plain Rust `main` that `gpui_platform::application()` runs inside UIKit. On desktop targets the same binary opens a phone-sized window for development.

| File | Role |
|---|---|
| `mobile/src/main.rs` | Fonts, preferences, the window, pairing links opened by the system (`on_open_urls`). |
| `mobile/src/prefs.rs` | `settings.json` in the app's data folder: phone name, appearance, theme, font size, whether onboarding ran. |
| `mobile/src/link.rs` | `Link` (one entity): the Uplink client. Loads `uplink.json` from the data folder, dials paired computers with backoff (nearby addresses first), joins with a pairing link, a code and address, or a code and a nearby computer, keeps each online computer's catalog and the watched tabs and rewatches them after `Stop::Behind` or a reconnect, opens tabs with `Message::Open`, asks for files, git and history with `Message::Request` (answered by id, failed after 15 seconds or when the computer goes offline), forgets and disconnects. Its `Hello` has port 0: it never listens. |
| `mobile/src/nearby/` | Finds `_nucli-uplink._tcp` computers and posts `NetEvent::Nearby` and `Gone`. iOS uses DNS-SD (`DNSServiceBrowse`, resolve and address lookups on the main queue) because raw multicast needs an entitlement; other targets use core's `Discovery`. The resolve stays open, so a computer that starts or stops adding a device updates its `pairing` flag. |
| `mobile/src/scanner/` | `[scan qr code]`: on iOS a full-screen `AVCaptureSession` with a metadata output for QR codes, presented over the GPUI view controller; it reports the first `nucli-uplink://` link. Elsewhere it reports that there is no camera. |
| `mobile/src/home.rs` | `Home`: the screen stack (welcome, computers, computer, project, folders, file, diff, commit, add a computer, computer details, settings, terminal) with its back history, the computer tabs, the project drawer, the text fields, the spinner timer and the test commands. `fetch` sends a `Request` through `Link::ask` and applies the `Response` on the main thread. |
| `mobile/src/welcome.rs`, `computers.rs`, `computer.rs`, `project.rs`, `folders.rs`, `viewer.rs`, `pair.rs`, `device.rs`, `settings.rs`, `terminal.rs` | One screen each. `viewer.rs` draws files, diffs and commits; files and diffs are a `uniform_list` that scrolls both ways. `board.rs` has the shared tab row, agent groups and the agents list. |
| `mobile/src/mirror.rs` | `Mirror`: a remote tab. An `alacritty_terminal` `Term` fed by `core::terminal::spawn_mirror` and drawn by `core::terminal::render`, with IME input, keys through `core::terminal::keys`, scrolling and paste. |
| `mobile/src/field.rs` | `Field`: a one-line text field with IME and the iOS keyboard's return action. |
| `mobile/src/ui.rs` | The active theme (and the iOS status bar style), buttons, rows, sections and tabs. |
| `mobile/src/devtools.rs` | `NUCLI_CONTROL` file commands for tests, run by `Home::control`: `open <title>`, `back`, `start`, `computer <name>`, `section projects|agents|history`, `project <name>`, `drawer files|changes|history`, `file|diff <path>` and `commit <hash>` in a project, `new agent|shell`, `resume <title>`, `screen pair|settings|device|folders [computer]`, `browse <path>` and `add <path>` in the folder browser, `choose <nearby computer>`, `pairlink <link>`, `code <code> [address]`, `focus`, `type <text>`, `key <name>`, and `tap <x> <y>`, a real tap through GPUI's hit testing, in points from the window's top left. |
| `mobile/ios/Info.plist` | The bundle (`uk.polishch.nucli.uplink`): the `nucli-uplink` URL scheme, Bonjour service, local network and camera descriptions. |

A mirror draws at the remote terminal's size, scaled down when it is wider than the view. With control it asks the computer to resize the terminal to the cells that fit at the phone's font size on its first layout, whenever the view's size in cells changes, and on input when the terminal has another size, so the phone takes the size back after another viewer held it.

`NUCLI_PAIR=<pairing link>` pairs at startup. Pass it to a simulator as `SIMCTL_CHILD_NUCLI_PAIR`, and `NUCLI_CONTROL` as `SIMCTL_CHILD_NUCLI_CONTROL`; simulator apps read host paths. Capture with `xcrun simctl io <udid> screenshot`, and read the app's stderr with `xcrun simctl launch --console-pty <udid> uk.polishch.nucli.uplink`. Always name the simulator by its id: `booted` picks any booted one, including simulators the user has open for other work. Without `xcode-select` pointing at Xcode, set `DEVELOPER_DIR=/Applications/Xcode.app/Contents/Developer` (the script does). `xcrun simctl openurl` asks "Open in …?" first, and the simulator cannot be tapped from a script, so test links with `pairlink` instead. The simulator has no camera. For several computers, run one isolated desktop instance per computer, each with its own `HOME`, `NUCLI_CONTROL`, `uplink.name` and `uplink.port` (47101, 47102, …); the simulator finds them all through Bonjour on the loopback interface. A fake agent that desktop shows as Claude must be a binary named `claude` that writes `~/.claude/sessions/<pid>.json`; a shell script runs as `sh` and gets no status.

## Gotchas

- Platforms consume shift for symbols, so `ctrl-shift-2` arrives as `ctrl-@`. `commands::shifted_alias` also binds the US layout's shifted form of every such key.
- On Windows a native modal dialog pumps messages, so GPUI tasks run inside whatever update opened it and panic with `RefCell already borrowed`. Show one from its own thread, as `notify::alert` does.
- GPUI sends a click to every hitbox under the pointer unless one in front blocks it, so an overlay such as the phone's drawer needs `occlude()`.
- `Window::handle_input` panics outside paint: call it from a `canvas` paint closure, not prepaint.
- Wrapped text can get the height of a single line. A parent whose width is unknown in an intrinsic pass measures its children with unwrapped text, then `max_w` clamps it; when the clamped width equals the final width, taffy's cache returns that height. The settings column therefore has a fixed `w` with `max_w_full()`. GPUI also measures the min-content width of text as one line, so a flex item that holds text needs `min_w_0()` to shrink.
- Inside `Store::update`, never call anything that reads the store through its global, such as `WorkspaceView` methods: GPUI panics. Put such logic on `Workspace`.
- Never call `WindowHandle::update` or `WindowHandle::read` from inside a window update, for example from a key action or a panel's click handler: they fail because the window is taken out while it updates, and `read` looks like the window is gone. Use `cx.defer`.
- On Windows `window_control_area(Drag)` turns the whole element into a window caption, so clicks on its children never arrive unless they use `occlude()`. The title bar's clickable children and its caption buttons all do. The bar itself uses it too: a click reaching the window root's `track_focus` counts as handled, and GPUI then keeps it from Windows, so the window would not move. `start_window_move` does nothing on Windows.
- A macOS build linked against the 26 SDK gets larger window buttons. `UIDesignRequiresCompatibility` in `Info.plist` keeps the smaller ones whichever SDK builds it.
- `alacritty_terminal`'s parser holds everything after `CSI ? 2026 h` until the matching end or a 150 ms timeout that only its caller can fire, so the parser runs apart from the blocking pty read.
- Prompt anchoring moves the cursor, which breaks ConPTY: it owns the screen and tracks the cursor itself, and a resize during startup, which every new tab gets, left the terminal blank for good. So on Windows (`LIFTS_PROMPT`) nothing is sent or rewritten. `TerminalElement` paints the grid lower by the blank rows under the content, and the same offset goes into `view.layout` for the mouse.
- GPUI runs key bindings before key listeners. Recording a shortcut uses `App::intercept_keystrokes`, which runs first, so the key does not also trigger its command. A plain `on_key_down` would.
- GPUI drags never leave the window: `on_drag` keeps a value inside the app and GPUI has no OS drag source. `WorkspaceView` watches drags of files with `on_drag_move` on the root, which fires even outside the window because each platform keeps the pointer while a button is held, and once the pointer leaves it hands the file to the system and stops the GPUI drag. Each drag offers the file and the path as plain text.
  - macOS starts an `NSDraggingSession` from the current `mouseDragged` event, so it must run inside that event. GPUI reads only `NSFilenamesPboardType` on drop.
  - Windows takes the shell's data object of the file and runs `SHDoDragDrop`, which pumps messages until the drop, so it runs in a spawned task outside any update (see the modal dialog gotcha).
  - X11 runs the XDND source protocol on a thread with its own connection: it polls the pointer, finds the `XdndAware` window under it, sends enter, position, leave and drop, and answers the selection requests. GPUI keeps the implicit pointer grab, so the cursor shape does not change. Wayland would need GPUI's surface and the button serial, which GPUI does not expose, so nothing happens there.
- GPUI does not paint while the macOS screen is locked, and window captures come back black. It also stops presenting while a window is fully covered, so captures of a window behind others show an old frame.
- On X11 GPUI starts painting only after a window manager reports the window as visible, so a bare Xvfb stays black.
- The snapshot cannot reproduce state that `alacritty_terminal` keeps private: the scroll region, tab stops, the saved cursor and charsets. Programs redraw these on their next full repaint. The primary screen is private too while a program uses the alternate screen (and `swap_alt` clears the alternate screen when swapping back), so a viewer that attached then is attached again with a fresh snapshot as soon as the program leaves it.
- macOS only lets an app that declares `NSLocalNetworkUsageDescription` and `NSBonjourServices` (in `scripts/package.sh`) use the local network once the user allows it. Without that, multicast DNS answers only on loopback and other machines cannot find the device; an unbundled debug binary is attributed to the app that launched it.
- `runtime_shaders` is on because the Metal compiler is not installed everywhere; shaders compile at startup instead.
- Release builds on Windows compile shaders with `fxc.exe` from the Windows SDK; they cannot be cross-compiled from another system. `GPUI_FXC_PATH` overrides its location.

## Testing the UI

Debug builds read commands from the file named by `NUCLI_CONTROL`: `keys <keystrokes>`, `type <text>`, `action <command>`, `open <folder>`, `draw` (a full layout and paint without presenting, which also works while the screen is locked), `dump`, `text <tab id>`, `activate`, and on macOS `capture <window id> <png>` and `menu <Menu/Item>`, which prints the native menus and the Dock badge, then performs that item, and `event <window number> <down|drag|up|rdown|rup|move> <x> <y>` `event <window number> key <text> <key code> <modifier flags>`, `event <window number> text|mark <text>` for the native text input calls, or `event <window number> front` to raise the window, which send real AppKit events to that window only. Key events follow AppKit's order: window key equivalent, menu, then the first responder. All of it works while the screen is locked if `draw` runs between steps. Lines appended to the file run in order.

Running in a Linux container under Xvfb with a window manager, a software Vulkan driver (`mesa-vulkan-drivers`) and `xdotool` allows real clicks, keys and screenshots with nothing reaching the host's desktop. Starting `dunst` in the app's D-Bus session shows its notifications, and `dunstctl action 0` clicks one.

The `x86_64-pc-windows-gnu` target builds with `mingw-w64` and its tests run under Wine with `CARGO_TARGET_X86_64_PC_WINDOWS_GNU_RUNNER=wine`, given a stub `icuuc.dll` exporting `u_strlen`. The app itself does not draw under Wine: older versions cannot compile GPUI's shaders and have no `ThreadPoolTimer`.
