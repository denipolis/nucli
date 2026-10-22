# Architecture

AppKit app, built with SwiftPM, one window per workspace. Everything lives in `Sources/Nucli/`.

## Files

| File | Role |
|---|---|
| `main.swift` | Entry point, sets `AppController` as app delegate. |
| `AppController.swift` | App delegate: list of windows, session lifecycle, restore, folder prompt, agent picker, key monitor, all menu actions. Menu actions are nil-targeted and land here because the app delegate is always in the responder chain (also when Settings is key); they act on `active` (the key or main `MainWindowController`). |
| `MainWindowController.swift` | One window: split view (sidebar, Files/Changes panel, content), toolbar, current project/tab of that window, tab cycling. |
| `WorkspaceActions.swift` | Rename, color, workspace and move-to-window actions, `MenuPayload` for menu items that carry a target and a value. |
| `MainMenu.swift` | Main menu and keyboard shortcuts. |
| `State.swift` | `AppState` (singleton: projects, workspaces, profiles, shell, font, last choices, persistence), `Project`, `Workspace`, `ItemColor`. |
| `Session.swift` | One tab: a `TerminalView`, its `LocalProcess` and status bar. Handles OSC title/cwd updates, bottom anchoring and exit. |
| `SidebarViewController.swift` | Source-list `NSOutlineView`: projects → sessions, context menus. |
| `Subagent.swift` | Running Claude Code subagents of a tab, read from `<data dir>/projects/<cwd>/<session id>/subagents/agent-*.jsonl` (+ `.meta.json` with `description`, `agentType`). |
| `SplitLayout.swift` | Split panes of the viewport: `PaneView` (one session, drop target with edge zones), `PaneSplitView`, `SplitLayout` (the view tree is the model). |
| `FileTabs.swift` | `FileTabBar` / `FileTabView`: IDE-style tab strip over the file view (home button to the project overview, close on hover, middle-click closes). |
| `TerminalPanel.swift` | VS Code style terminal panel under the viewport: "Terminal" title, horizontal tabs (`FileTabView`), new / maximize / hide buttons, and the active shell's `Session.container`. `ContentViewController` puts the viewport (`main`) and the panel in a horizontal-divider `NSSplitView`; the panel height is the `terminalPanelHeight` default. |
| `StatusPill.swift` | VS Code style rounded pill centered in the toolbar: current project name and path ("Search" without a project); clicking opens the command palette (`⇧⌘P`). Not a toolbar item: it is added to the titlebar view (superview of the close button) and centered on it, because toolbar items, even with `centeredItemIdentifiers`, are centered in the area right of the sidebar tracking separator. `⌘P` / `⇧⌘P` drop the palette down from the pill (`MainWindowController.paletteAnchor`), or 80pt below the window top when the pill is hidden. |
| `TabSwitcher.swift` | `⌃Tab` / `⇧⌘]` overlay listing this window's tabs in most-recently-used order. |
| `ContentViewController.swift` | Main viewport. Shows the selected session's terminal view, or the full-file diff overlay (`FileDiffViewController`). Line numbers and diff markers are drawn by `DiffGutterView` (vertical `NSRulerView`) from the per-line `.gutter` attribute, so the text storage holds only file lines. Every text file view is editable in place, diffs included (see Editing). |
| `GitPanelViewController.swift` | Changes view of the panel, VS Code Source Control style: commit message field (↩ commits) and accent Commit button, collapsible "Staged Changes" / "Changes" sections with count badges, one-line file rows (icon, name, folder, status letter `M`/`A`/`D`/`U`). Hover shows actions: open, discard (unstaged only, confirmed; tracked files `git restore --worktree`, new files to the Trash), stage `+` / unstage `−`; section headers stage, unstage or discard all. Clicking a row opens the full-file diff in the viewport. `HoverRow` is the shared hover row. |
| `CommitHistory.swift` | Git History view of the panel: `CommitGraph` (`git log` of branches, remotes, tags and HEAD in topo order, lane layout) and `CommitRowView` (graph, ref pills, subject, hash, author, date drawn in one `draw`). Clicking a commit expands its changed files (`Git.changes(in:dir:)`, diff against the first parent) as `CommitFileRowView`s; clicking a file opens it in the file view with the diff of that commit (`FileChange.commit` / `.parent`, read-only, header shows `@ <hash>`). |
| `PanelViewController.swift` | `PanelTab` and `PanelViewController`: second pane right of the sidebar. A segmented control switches between Files, Changes and Git History (`panelTab` default); both views are loaded up front and stay in the view hierarchy (switching only toggles `isHidden`; re-adding a pane re-solved every change card's constraints), so their observers run while hidden. A plain second sidebar split item is not drawn as a floating panel on macOS 26, so the panel draws its own `NSGlassEffectView` (fallback `NSVisualEffectView`) inset 8pt on the terminal background, and the window uses a transparent titlebar with the title hidden so nothing covers the panel's top. |
| `FilesViewController.swift` | Files view of the panel: lazy file tree of the current project (`FileNode`), git status colors and VS Code-style letters on the right (`M`, `A`, `D`, `U`; folders get a dot, green when everything inside is new, orange when mixed), ignored files dimmed (`git ls-files --ignored --directory`), click opens the file in the viewport, drag out, context menu. Expanded folders are remembered per project in memory. |
| `ImageViewer.swift` | Image mode of the file view: any file whose type conforms to `public.image` or `com.adobe.pdf` (whatever `NSImage` can load, incl. WebP, HEIC, SVG, ICNS, the first PDF page) is shown in a magnifiable scroll view, centered (`CenteringClipView`), fit to the viewport by default (never upscaled past actual size), with zoom in/out/actual size/fit buttons, pinch zoom, pixel size and file size, and a checkerboard behind images that have alpha. |
| `Highlighter.swift` | `Language` (picked by extension or well-known file name, ~90 extensions: own keyword sets for Swift, JS/TS, Rust, Go, JVM, C family, C#, Dart, Zig, Python, Ruby, shell, Elixir, SQL, Lua, GraphQL…, own comment styles for Haskell, OCaml/F#, Lisp, Erlang, Julia, HCL, Nix, INI; Markdown headings and fences) and `Highlighter`: small per-line scanner for comments, strings, numbers, keywords, capitalized types and `@attributes`, carrying block-comment state across lines. No grammar files, no dependency. |
| `FileIcon.swift` | SF Symbol and system color per file extension/name, used by the Files and Changes views. |
| `Git.swift` | `git` CLI wrapper (status, full-file diff) and `FileWatcher` (FSEvents). |
| `AgentDetector.swift` | First-run discovery of installed agent CLIs. |
| `Theme.swift` | `TerminalTheme` (terminal palette, selection, `SyntaxPalette` for the file and Changes views): Nucli Dark/Light, five Vitesse variants, automatic pairs. |
| `ProcessInspector.swift` | Foreground process of a tab (`tcgetpgrp` + `KERN_PROCARGS2`) and agent monogram badges. |
| `StatusBar.swift` | Warp-style chip bar under each tab: cwd, git branch, foreground program. |
| `ProfilePicker.swift` | `⇧⌘T` sheet: agent profiles (new conversation), shells and recent conversations (resume). |
| `Profile.swift` | `AgentProfile`, `AgentKind` (claude/codex: config variable, session-id/resume arguments), `CommandLineParts` (quote-aware command line split). |
| `History.swift` | `ConversationHistory`: reads agents' local conversation files. |
| `HistoryList.swift` | History tab of the sidebar: every conversation of every agent profile found on disk (`ConversationHistory.all`: Claude `projects/*/*.jsonl` with the `cwd` from the file, Codex sessions, pi `sessions/*/`), newest first, max 150, loaded in the background. Title, cwd and id of every conversation file are cached on disk in `~/Library/Caches/Nucli/conversations.json`, keyed by path and modification date (`ConversationHistory.meta`), so only new or changed files are read; the very first load after install still reads every file and takes a few seconds. Click resumes the conversation in the project for its `cwd` (created if Nucli doesn't have it yet). The sidebar filter field filters it too. |
| `BoardList.swift` | Agents tab of the sidebar: the running agent tabs of this window's workspace only (exited and closed ones drop out), not grouped by project, in status sections (`BoardColumn`: Waiting for Input, Done, Working, Idle). Reloads on `.sessionChanged` / `.projectsChanged` while visible; clicking a row selects the tab in its window. |
| `ConversationCache.swift` | In-memory recent conversations per project, refreshed in the background. |
| `Overview.swift` | Clickable overview pages shown in the viewport: `ProjectOverview` (new session per profile, new terminal, open tabs, recent conversations, Finder) and `WorkspaceOverview` (recently used and all projects, add project). `ActionRow` is the hover/click row they are built from. |
| `Palette.swift` | `Palette` (borderless child panel over the window: search field + table), `PaletteItem`, `FileIndex` (project file list in the background). |
| `PaletteActions.swift` | `⌘P` / `⇧⌘P` sources: files, tabs, projects, workspaces, agent profiles, conversations, menu commands, settings. |
| `Fuzzy.swift` | VS Code-like fuzzy scorer (subsequence DP with word-start, separator, camelCase and consecutive bonuses, match positions for highlighting). |
| `SettingsWindowController.swift` | Settings window (`NSTabViewController`, toolbar style) and the Agents pane (profile editor). |
| `SettingsPanes.swift` | General (restore tabs, notifications), Shells (shell profiles for sidebar tabs), Keybindings (record/reset shortcuts) and Appearance (theme, automatic/light/dark, terminal font and size, line spacing, cursor shape and blink, sidebar text size) panes. Terminal options are applied by `Session.applyTerminalOptions`, sidebar and Files text size by reloading on `.settingsChanged`. |

## Windows, workspaces, names, colors

- Every `Project` and `Workspace` has a `windowId`. Each `MainWindowController` shows only its own; on launch one window is created per distinct id (frames autosaved as `Main`, `Main-1`, …).
- Sidebar tree: the window's projects, agent tabs under them. Tab order for `⌘1…9` and cycling follows this tree (`AppState.orderedProjects`). See "Workspaces are windows".
- `customName` (nil = folder name) and `color` (`ItemColor`, system colors) per project; workspaces have a name only. The color tints the folder icon and the row backgrounds (`TintedRowView`).
- Project rows show the project's uncommitted diff against `HEAD` as green `+N` / red `−M` on the right (`Git.diffStat`: `diff HEAD --numstat` plus line counts of untracked files, same numbers as the Changes view). `SidebarViewController.watchDiffs` keeps one `FileWatcher` per non-scratch project of the window (synced on every `reload`), refreshes on a background queue, coalesces events that arrive during a refresh into one rerun, and reloads only that row. Hidden when there are no changes or the folder is not a git repo.
- Every tab (agent, shell, panel terminal) has its own optional `Session.color`, set from Color in the sidebar row menu or the panel tab's right-click menu, saved in the tab as `color`. In the sidebar it overrides the project tint for that row and tints the icon; in the panel it tints the tab icon.
- Each window restores the project selected last (`lastSelected`) and each project its active tab (`activeTab`).

## Overview pages

Every sidebar row is clickable. A tab shows its terminal; a project shows `ProjectOverview`; a workspace shows `WorkspaceOverview` (`MainWindowController.showOverview(of:)`). `select(project)` without tabs also lands on the project overview, so closing the last tab shows it. Overviews rebuild themselves on `.projectsChanged` and `.conversationsChanged` (posted by `ConversationCache` after each refresh). The window title follows the selection; a workspace page clears the Files and Changes views.

## Split panes

- Each window's `ContentViewController.layout` (`SplitLayout`) holds a tree of nested `PaneSplitView`s with `PaneView` leaves; the view hierarchy is the model, nothing else tracks it. Splitting in the same direction as the parent adds a sibling, otherwise the pane is replaced by a new split. Removing the last-but-one child collapses the split. New siblings get equal sizes (`setPosition` once now and once after layout).
- `⌘D` / `⇧⌘D` open a new shell in the current project to the right / below. `⌥⌘W` closes the split (the session keeps running in the sidebar). `⌥⌘←→↑↓` move focus geometrically. Clicking a pane focuses it (mouse-down monitor in `AppController`).
- Split tabs are tied into a group. Selecting a tab that is in the shown layout focuses its pane. Selecting any other tab stashes the shown split (`SplitLayout.stashed`) and shows that tab alone, or restores the stashed split it belongs to. `groups` lists the shown and stashed splits; `remove` and `replace` work on stashed trees too, so closing or moving a tab of a hidden split shrinks or dissolves it. Overview pages hide the layout and keep it; the next tab selection brings it back.
- Sidebar context menu of a grouped tab: Remove from Split (`detach`) and Unsplit All (`SplitLayout.unsplit`; the shown split keeps only its focused tab). Dragging a grouped tab in the sidebar moves the whole group. `SidebarViewController.compactGroups` keeps the members of a group adjacent within each project after sidebar and pane drops. When a group member is selected, all its rows draw one joined selection (`TintedRowView.groupSelected`).
- Dropping a sidebar tab on a pane: outer 25% of each edge splits on that side, the center replaces the pane. Only tabs of the same window are accepted. A tab that is already in the layout moves.
- Unfocused panes are dimmed (`alphaValue` 0.55) while the layout is split. The sidebar marks tabs of every split group with a joined accent bar (`.layoutChanged`, `TintedRowView.split`); `⌘D` / `⇧⌘D` insert the new shell right after the current tab. Closing or moving a tab removes its pane (`MainWindowController.detach`).
- Layouts are not persisted yet; after relaunch each window shows one pane.

## Subagents

- Claude Code writes each subagent's transcript to `<data dir>/projects/<encoded cwd>/<session id>/subagents/agent-<id>.jsonl` next to a `.meta.json` (`description`, `agentType`, `toolUseId`). `sessions/<pid>.json` gives the session id and cwd, so `ConversationHistory.claudeProcess` returns the subagents folder.
- `Session.refreshSubagents` scans it on a background queue on every foreground poll. A subagent counts as running until its last transcript line has `toolEndsTurn: true` or is an assistant message with `stop_reason: end_turn`, or the file was not written for 10 minutes. Only running ones are listed.
- The sidebar shows them as children of their tab (expanded automatically). Clicking one selects the parent tab. Claude Code has no external API to open a subagent's view inside its TUI; typing into the prompt would clobber the user's input, so Nucli does not try.

## Sidebar tabs

A segmented control at the top of the sidebar switches between Projects (the outline), Agents (`BoardList`) and History (`HistoryList`), remembered in the `sidebarTab` default; View › Projects / Agents / History (`⌥⌘1` / `⌥⌘2` / `⌥⌘3`). The Filter field filters the visible tab.

## Sidebar rename and filter

- Rename is inline: the cell's title field becomes editable (`SidebarViewController.beginRename`), the result goes through `AppController.rename(_:to:)`. Sidebar reloads are skipped while a rename is in progress so the field editor survives. Tabs store `customName` (persisted as `name` in the tab dict); the terminal's OSC title is kept separately and used when the custom name is empty.
- The filter field at the bottom of the sidebar filters `children(of:)`: an item stays if it or any descendant matches (name, path, tab title or subtitle); a matching parent shows all its children. Filtering expands everything.

## Palette (`⌘P`, `⇧⌘P`)

- `⌘P` lists the current project's files: recently opened first (`Project.recentFiles`, persisted, max 20), then the `FileIndex` (`git ls-files --cached --others --exclude-standard`, or a capped directory walk outside git). The index is refreshed in the background each time the palette opens and the list reloads when it arrives. Without a project `⌘P` falls back to the full palette.
- `⇧⌘P` searches everything: tabs (all windows), projects, workspaces, "New Agent: <profile>", recent conversations of the project, every enabled main-menu item (collected by walking `NSApp.mainMenu` after `update()`, executed with `NSApp.sendAction` after the palette closes so the responder chain is the window's again), a notifications toggle, and files (ranked below everything else, hidden while the query is empty).
- Scoring: `Fuzzy.match` (DP over query × candidate). Paths try the file name first (+40) and fall back to the whole path; long paths lose a little. Each item has a `rank` bias (tabs of this window > projects > …). Matched characters are drawn bold.
- `Return` runs, `⌘Return` runs the alternate (files: open with the default app), arrows/page keys move, `Esc` or clicking elsewhere closes.

## Keybindings

All menu shortcuts are rebindable. `MainMenu.build()` records each command's default (`MainMenu.defaults`, keyed by its menu path like `File/New Terminal`) and then applies `AppState.keyBindings`. Changing a binding rebuilds the main menu and posts `.settingsChanged` / `.projectsChanged` so places that print shortcuts (empty viewport, overviews, palette) read them from the menu via `MainMenu.shortcut(for:)`. The recorder uses `characters(byApplyingModifiers: [])` so `⇧⌘]` is stored as `]` + shift, like the built-in items; a binding needs a modifier unless it is a function key. Shortcuts outside the menu (`⌃Tab`, `⌘←` and friends in the key monitor) are fixed.

## Titlebar and panel placement

- Compact unified toolbar (`.unifiedCompact`): the system sidebar toggle and workspace name next to the traffic lights, the `StatusPill` centered on the window, and a button for Files and Changes.
- Files and Changes placement is one global setting (`panelSide` left/right, default right; `panelHidden`), applied to every window by `MainWindowController.applyPanelPlacement`: the panel split item is moved to index 1 (next to the sidebar) or 2 (right of the viewport), the glass inset flips, and the toolbar is rebuilt so the panel toggle sits over the panel (left) or at the far right. The panel starts below the titlebar (safe area) like the sidebar, so both glass panes have the same height. The window inserts the panel item at its saved index before setting the split view `autosaveName`; otherwise the restored frames are applied in the wrong order and the viewport can come back collapsed.
- Ways to change it: drag the panel's header strip across the window middle (`PanelHeaderView`), right-click the header (Move to Right/Left Side, Hide), View › Move Files and Changes to Other Side, `⌥⌘B` / the toggle to hide and show. `⌃⌘1`/`⌃⌘2` show it again.
- Menu titles stay static on purpose: keybinding overrides are keyed by menu path, so a title that changes in `validateMenuItem` would lose its binding.

## Tabs without a project

- Clicking empty sidebar space deselects everything (window title "Nucli", empty viewport). `⌘T` / `⇧⌘T` with nothing selected open the tab in a per-window scratch `Project` (`isScratch`, path `~`, named "No Project"); `⌥⌘T` / `⌥⇧⌘T` still ask for a folder.
- The scratch project is a normal `Project` so sessions, restore and drag and drop need no special cases. It is listed last among loose projects and only while it has tabs, saved with `scratch: true`, never matched by `project(for:)`, identified as `scratch:<window>` in drags, not draggable itself, not renameable, and hidden from the palette's project list. The Files and Changes panel and `⌘P` ignore it so `~` is never watched or indexed.
- Tabs can be dragged from "No Project" into any project.

## File tabs

- Every project keeps `openFiles` (ordered) and `activeFile`, persisted. Opening a file (Files, Changes, `⌘P`) inserts it after the active tab. `ContentViewController` caches one `FileDiffViewController` per open file (key: project path + file path), so switching tabs keeps scroll position and selection; a cached view is rebuilt only when its `FileChange` differs.
- Opening a file from anywhere (terminal link, Files, Changes, Git History, `⌘P`) while a tab is shown switches the window to the project overview first (`MainWindowController.show`; never call `ContentViewController.showFileDiff` directly), so the sidebar selects the project row instead of keeping the agent tab highlighted under the file.
- `Esc` hides the file view (tabs stay); `⌘W` with a file shown closes that tab and shows its neighbor, or hides the view after the last one. Clicking a project row shows its overview with the last active file on top (`MainWindowController.showProject`); the house button in the tab strip goes to the overview.

## Tab switcher

`⌃Tab` (any first responder) and `⇧⌘]`/`⇧⌘[` pressed from the keyboard open `TabSwitcher`, a HUD over the window with the window's tabs in most-recently-used order (`AppController.noteUsed`, called from `MainWindowController.select`), agent badge, title, project and a green dot for tabs that need attention. Projects whose file view was shown are listed too (folder icon, active file name) and reopen the project with that file. Repeating the key moves the highlight; releasing the held modifier (`flagsChanged` monitor) selects; `Esc` cancels. Menu clicks on Next/Previous Tab still cycle in sidebar order without the overlay.

## Drag and drop

- Sidebar (`SidebarViewController`): workspaces, projects and tabs are dragged with a private pasteboard type `uk.polishch.nucli.sidebar-item` holding an identifier (`workspace:<id>`, `project:<path>`, `tab:<id>`, see `AppState.identifier(for:)`). It works between windows because all windows live in one process. Drops are normalized in `dropTarget`: workspaces only reorder at the root; projects go to the root (after workspaces) or into a workspace; tabs go into any project (the process keeps running, only its project changes). Order is the order of `AppState.workspaces` / `projects` / `Project.sessions`. After a drop `AppController.reconcileSelections()` fixes windows whose shown tab moved away.
- Sidebar drags are local only (`setDraggingSourceOperationMask([], forLocal: false)`): never put a file URL on a project drag, Finder would move the folder.
- Folders from Finder: dropped on the sidebar (at a position or into a workspace), on the empty viewport (`FolderDropView`) or on the Dock icon (`CFBundleDocumentTypes` for `public.folder` with `LSHandlerRank` `None`, so Nucli never becomes the default folder app) open projects.
- Files dropped on a terminal (`DropTerminalView`) are typed in as shell-quoted paths; plain text is typed as is.
- Files and Changes rows drag out as file URLs with copy-only operation (to a terminal, Finder, editors).

## Sessions

- Shell: `$SHELL -l`. Agent: `$SHELL -l -i -c "<profile command>"`, so PATH, aliases and env from the user's dotfiles work.
- Started with `currentDirectory = project.path`. The project never reads the shell's cwd back; OSC 7 cwd is only shown as a subtitle.
- Env: clean, like Terminal.app. Only `HOME`, `USER`, `LOGNAME`, `TMPDIR`, `SSH_AUTH_SOCK`, `__CF_USER_TEXT_ENCODING`, `LANG` are inherited; `PATH` starts at `/usr/bin:/bin:/usr/sbin:/sbin` and the login shell (`path_helper`, dotfiles) builds the rest. Plus `TERM=xterm-256color`, `COLORTERM=truecolor`, `TERM_PROGRAM=Nucli`, `SHELL`, and `FORCE_HYPERLINK=1`: tools that use the `supports-hyperlinks` check (Claude Code's `[Image #N]`, file paths) only emit OSC 8 links for terminals on their allowlist, and Nucli is not on it. Never pass the app's own env through: when Nucli is launched from another tool (e.g. Claude Code) it carries that tool's private variables (`CLAUDECODE`, `CLAUDE_CODE_CHILD_SESSION`, `CLAUDE_CONFIG_DIR`, …), which break agents started in Nucli.
- Terminal views stay alive when not shown (only removed from the view hierarchy), so background agents keep running.
- Closing the shown tab selects the next tab of the same project, or the previous one when it was the last.
- `⌘W` on a running agent asks first, except when the agent printed nothing for 2 hours (`Session.lastOutput`) or "Don't ask again for 1 hour" was checked (`skipCloseAgentConfirmUntil` default).
- Exit code 0 closes the tab. Non-zero keeps it and prints the code.
- Closing kills with `SIGHUP` (an interactive zsh ignores `SIGTERM`, which is all `LocalProcess.terminate()` sends).

## Changes view (git)

- Base is `HEAD`, or the empty tree in a repo without commits.
- Files: `git diff <base> --numstat/--name-status --relative --no-renames` plus `git ls-files --others --exclude-standard`. `--relative` scopes everything to the project folder when it is a subfolder of a repo.
- Full-file view: `git diff <base> -U100000000 -- file`, a context so large that the hunk is the whole file.
- Changes view: VS Code Source Control style list (see `GitPanelViewController.swift` in the file map).
- File view (`FileDiffViewController`): `+`/`-` line backgrounds are drawn full width by `DiffLayoutManager` (custom `.diffKind` attribute per line, painted in `drawBackground(forGlyphRange:at:)`), not with the text `.backgroundColor`, which stops at the last character. One line-number column unless the diff has removed lines; no `+`/`-` marker column for unchanged files. `F7` / `⇧F7` (also header chevrons) jump between change blocks; the first open of a changed file jumps to its first change. Header buttons reveal in Finder and open with the default app. `Esc` (`DiffTextView.cancelOperation`) and `⌘W` close the view.
- Live updates: FSEvents on the project path, plus the git dir when it lies outside it (so staging is noticed). Refreshes are coalesced: one git run at a time, one queued rerun.
- `.gitChanged` notification (object: project) lets an open full-file view reload.

## Editing

- The file view is the editor: working-tree files are editable in place, in diff and plain views alike. Not editable: deleted files, commit diffs (`FileChange.commit`), images, binary files. `⌘S` saves; the header shows `●` while the text differs from what was last loaded (typing and deleting back, or undoing, clears it).
- The text storage is the rendered diff without gutter: file lines plus removed (`-`) lines. Saving writes every character whose `.diffKind` is not `-` (`fileText`), drops the final newline when the file on disk had none, then reloads to re-render the diff. Typing inside a removed line is refused (`shouldChangeTextIn`); deleting across removed lines is allowed because they never reach the file.
- Typing attributes are always the base font/color, so typed text never inherits `.gutter` or `.diffKind`. New lines show no number until the save re-renders. The gutter skips a label equal to the previous line's, so splitting a line does not show its number twice.
- The edited lines are re-highlighted on every change with a fresh `Highlighter` (no block-comment state from lines above; the next reload fixes it).
- Reloads (watcher, settings) are skipped while dirty, so an agent writing the file never discards unsaved edits; saving overwrites the agent's version. The caret is kept by file line and column across reloads. Each view has its own `UndoManager`, cleared on reload because the storage is replaced.
- Suggestions: after a letter, digit or `_` with a partial word of 2+ characters, `DiffTextView` opens the system completion popup (`complete(_:)`) with words of the file (3+ chars, case-insensitive prefix, max 30, word list rebuilt at most every 2 s). `insertCompletion` only inserts on `Return`/`Tab`, so typing continues normally while the popup is shown and `Esc` keeps what was typed.
- `Return` keeps the current line's leading spaces/tabs.
- Closing a dirty tab asks Save / Cancel / Don't Save; quitting with dirty files asks first. Closing a workspace window does not check yet.

## Appearance

- Theme setting (`theme`): an automatic pair (`nucli`, `vitesse`) picks its dark or light variant from the effective appearance; a fixed theme (e.g. `vitesse-black`) forces `NSApp.appearance` to match its darkness, and the Appearance popup is disabled then. `AppController.applyAppearance` sets the appearance, `applyTheme` recolors every terminal, the viewport, panel, file views and Changes cards (`.settingsChanged`).
- Per-profile override: `AgentProfile.theme` (empty = app theme), chosen in Settings › Agents, so e.g. work and personal Claude tabs look different. Looked up by profile name at apply time because sessions keep a copy of their profile.
- Highlighter colors come from the theme's `SyntaxPalette` (system colors for the Nucli themes, Vitesse token colors for Vitesse), passed into `Highlighter` so rendering can stay off the main thread.

- `TerminalTheme.current` picks dark or light by `NSApp.effectiveAppearance`; a KVO observer re-applies it to all tabs on change. Cursor and selection use the accent color.
- Terminal is inset 10pt inside the content view, which has the same background color.
- `CLICOLOR=1` is set so BSD `ls` and friends use colors.

## Status bar

Each tab is `Session.container`: terminal on top, `StatusBar` below. The once-per-second poll also reads the foreground process cwd (`proc_pidinfo` `PROC_PIDVNODEPATHINFO`) and the branch from `.git/HEAD` (walking up, worktree `gitdir:` files supported) — plain file reads, no `git` process per tick. Chips are clickable (and accessible as buttons): folder → Reveal in Finder / Copy Path; branch → menu of the 30 most recent local branches (`git switch`) and New Branch… (`git switch -c`), errors shown in an alert; the diff chip (`files +added −removed`, from the `.gitChanged` notification that the Changes view posts with its `GitStatus`) opens the Changes view. The diff chip reflects the project, the folder and branch chips the shell's current directory.

## Running agent detection

Once per second `AppController` asks every tab for its foreground process: `tcgetpgrp(childfd)` gives the foreground process workspace, `KERN_PROCARGS2` its argv. For `node`/`bun`/`python` the script name is used. If the name is a known agent (`AgentDetector.knownCommands` or the executable of any profile command), the sidebar shows its badge, so `claude` started by hand in a plain shell tab is detected too.

## Agent profiles

`AgentProfile` (`Profile.swift`) is structured, not a command string:

| Field | Meaning |
|---|---|
| `name` | Shown in sidebar and picker, remembered as last used. |
| `executable` | Name in `PATH` or absolute path. The agent type (`kind`, computed) is detected from its file name: `claude` / `codex` are known (`AgentKind`), anything else is unknown to Nucli. The type drives history, resume/session-id flags, the config folder field and the icon. There is no user-facing type selector. |
| `arguments` | Raw extra arguments, appended as written. |
| `configDirectory` | The agent's account/config/history folder (`CLAUDE_CONFIG_DIR` / `CODEX_HOME`). Empty means the agent's default (`~/.claude`, `~/.codex`). History is read from here. |
| `environment` | Ordered key/value pairs. |

`launchCommand(extraArguments:)` builds `K='v' … exec 'executable' arguments 'extra'…` (values `$HOME`/`~`-expanded and single-quoted), run through `$SHELL -l -i -c`. Settings shows the exact command as a preview.

Profiles saved as `{name, command}` are converted on load with `CommandLineParts` (quote-aware split, wrappers like `env`/`command`/`exec` skipped, the config variable lifted into `configDirectory`).

Detection: no hardcoded profiles. On first launch (`didDetectAgents` unset) `AgentDetector` looks for known agent CLIs (`claude`, `codex`, `gemini`, `opencode`, `pi`, `amp`, `aider`, `cursor-agent`, `goose`, `crush`, `qwen`, `copilot`, `droid`) in the `PATH` of a non-interactive login shell (`$SHELL -l -c`, never `.zshrc`) and common install dirs, and creates profiles. For `claude`/`codex` every `~/.claude*` / `~/.codex*` dir that looks used (has `projects`, `history.jsonl`, `settings.json`, `.credentials.json`; codex: `sessions`, `config.toml`, `auth.json`) becomes its own profile with that `configDirectory`. Settings → "Detect Agents" reruns it and only adds missing names.

## Conversation history and restore

- New Claude tabs run with `--session-id <uuid>`; resume adds `--resume <id>` (keeps the id). pi works the same way with `--session-id <uuid>` for both new and resumed tabs. Codex resume adds `resume <id>`; the id of a new Codex tab is guessed on quit from the newest rollout file for the project.
- `ConversationHistory.list` scans every distinct data dir of the profiles (so two Claude profiles with different config dirs each contribute), filtered to the project folder, newest first.
- `ConversationCache` keeps the list per project path in memory. It is refreshed in the background on launch for every project, on project/tab switch, when a tab exits and each time the picker opens; the picker shows the cached list instantly and swaps in the fresh one. `ConversationHistory` also caches each file's title by path + modification date, so a rescan only re-reads changed files.
- On launch `restoreTabs()` reopens saved tabs without selecting them, then selects the saved tab. Terminals start a fresh shell in their last directory (`cwd`, from OSC 7 or the foreground process) and show the previous output dimmed above a `── restored ──` line. The output is written on quit only (`Session.saveScrollback`: normal buffer as plain text, last 5000 lines, `~/Library/Application Support/Nucli/Scrollback/<id>.txt`, the folder is wiped and rewritten each quit) and fed straight into the terminal before the shell's first output, which then anchors the prompt at the bottom. Shell history is not saved per tab. A Claude tab whose conversation file does not exist starts a new conversation.

## Persistence (`UserDefaults`)

`profiles` (`[{name, executable, arguments, configDirectory, environment}]`), `didDetectAgents`, `shells`, `lastShell`, `fontSize`, `fontName` (nil = SF Mono), `restoresTabs`, `notifications`, `appearance` (`system`/`light`/`dark`), `keyBindings` (`{"File/New Terminal": {key, modifiers}}`, only overrides), `lastProfile`, `lastFolder`, `workspaceList` (`[{id, name, color, window, collapsed}]`) and `projects` (`[{path, lastProfile, window, name, color, workspace, collapsed, lastSelected, activeTab, tabs: [{kind, profile, conversation}]}]`), written on every change and on quit.

Legacy keys: if `projects` is missing, projects are read from `workspaces` (with `group` as the workspace id) and workspaces from `groups`. Do not reuse the key `workspaces` for anything else.

## Prompt at the bottom (Warp style)

Shell tabs start with the prompt on the last row and output grows upward. `Session` drives the pty itself (`TerminalView` + `LocalProcess`, not `LocalProcessTerminalView`) because `LocalProcessTerminalView` feeds output on a private fast path that cannot be intercepted. In `dataReceived` for terminal tabs it prepends `ESC[999;1H` to the first output and inserts it after every `ESC[2J`, so `clear` (`ESC[3J ESC[H ESC[2J`), `⌃L` and `⌘K` land at the bottom. Skipped while the alternate screen is active (tracked from `?1049/?1047/?47 h/l`, so vim, less, top are untouched) and while an agent runs in the tab. Do not gate on "shell is the foreground process": `clear` is its own process. Tabs are created with the real content size so the first prompt lands on the real last row. On every resize (`sizeChanged`) a shell tab whose cursor is above the last row and has only blank rows below it gets `ESC[nT` (scroll content down) plus `ESC[nB` (cursor down), so the prompt stays on the bottom row while the window or a split grows, one row at a time during a drag. Lines pushed into scrollback by a shrink stay there.

## SwiftTerm notes

- Sessions own `LocalProcess` and implement `TerminalViewDelegate` + `LocalProcessDelegate` (see above).
- `TerminalView.keyDown` is `public`, not `open`, so it cannot be overridden. Extra key mappings (`⌘←`, `⌘→`, `⌘⌫`, `⌥←`, `⌥→`, `⌥⌫`, `⌃Tab`) live in a local `NSEvent` monitor in `AppController`.
- SwiftTerm supports the kitty keyboard protocol, which Claude Code uses for `⇧↩`.
- The underlying `Terminal` is internal; use `send(_:)`, `send(txt:)`, `feed(text:)` on the view.
- Find (`⌘F`) works through `performFindPanelAction` menu items with `NSFindPanelAction` tags.

## Attention (finished / waiting tabs)

- A tab gets `needsAttention` from: the terminal bell, an OSC `9;<text>` or `777;notify;<title>;<body>` notification found in the output (OSC `9;4` progress is ignored; a sequence split across two reads is missed), or an agent that finished working. SwiftTerm parses OSC 777 but swallows it in `TerminalView.notify`, so `Session` scans the raw bytes itself.
- Claude Code status: every interactive `claude` process writes `<config dir>/sessions/<pid>.json` with `sessionId`, `cwd` and `status` (`busy`, `shell`, `idle`, `waiting`; `waiting` = permission prompt or question). The neighboring `<pid>.<hash>.key` files hold secrets: never read them. In the once-per-second poll, when the tab's foreground process is `claude`, `Session.readAgentStatus` reads the file for the foreground pid (process group id) from the profile's config dir, or from every Claude profile dir plus `~/.claude` for a `claude` started by hand in a shell. It sets `activity` (`working` / `waiting` / `idle`), marks the tab done on `working` → `idle` (so within ~1s) and posts "Waiting for input" on → `waiting`. For agent tabs it also follows `sessionId` into `conversationId`, so `/clear` and `/resume` inside the tab change what restore resumes. This is an undocumented internal file: if it is missing or the format changes, the heuristic below takes over.
- Fallback for other agents: activity based. Output gaps under 1.5s count as one busy run; busy for 2s marks the tab `working`; when a tab whose foreground process is a known agent has been busy for at least 5s and then stays quiet for 2s, it is done (or waiting for input). Checked in the once-per-second foreground poll.
- Sidebar status on the right of each tab row (`SidebarCell.showStatus`): red `xmark.circle.fill` when the process exited with a non-zero code, orange `questionmark.circle.fill` while waiting, a small spinner while working, green `checkmark.circle.fill` while `needsAttention`. Each has a tooltip and an accessibility label.
- Ignored when the tab is what the user looks at (app active, key window, tab shown, no file view over it). Cleared when the tab is selected, its window becomes key or the app becomes active with it shown.
- Shown as a green row and green title in the sidebar, the dock badge counts marked tabs, and a macOS notification (`UNUserNotificationCenter`, only from the bundle; clicking it selects the tab) is posted whenever the tab is not the one being watched (also while Nucli is active, shown as a banner via `willPresent`) and `notifications` (Settings, default on) is set.

## Terminal behavior

- `⇧↩` in Claude Code: SwiftTerm implements the kitty keyboard protocol, which Claude Code enables, so `⇧↩` arrives as a distinct key and inserts a newline. Nothing to do in Nucli.
- Light/dark: switching the system appearance recolors running terminals, the viewport, panel and file view live (KVO on `NSApp.effectiveAppearance` → `applyTheme`). Checked with screenshots in both modes.
- Links: SwiftTerm detects OSC 8 links and plain URLs; `⌘`-hover underlines, `⌘`-click calls `requestOpenLink`: URLs open with `NSWorkspace`; file paths (also `path:line[:col]`, relative to the cwd or project) open in the file view at that line (`Session.openPath`).
- Drag and drop: files dropped on a terminal are typed as shell-quoted paths, text as is (`DropTerminalView`).
- IME: `TerminalView` implements `NSTextInputClient` (marked text for Japanese/Chinese input and the accent menu).

## Sidebar tabs and the terminal panel

- `Project.sessions` holds the sidebar tabs: agents (`.agent`) and shells (`.shell`). Sidebar shells behave like panel terminals (bottom anchoring, cwd and scrollback restore) but sit in the sidebar, splits, tab switcher and `⌘1…9`. Shells live in `Project.terminals` with `activeTerminal` and `showsTerminals` (saved as `terminalsShown`). `AppState.allSessions` covers both, so polling, themes, attention and quit see every process.
- `AppController.open` routes `.terminal` sessions into `terminals`; `select(session)` of a terminal (palette, board, notification) goes through `MainWindowController.showTerminal`, which selects its project and opens the panel. `MainWindowController.terminalProject` is the current project, else the window's scratch project.
- Saved tabs keep one list per project (agents first, then terminals); terminals come back in the panel, hidden until `` ⌃` ``.
- `⌘W` closes the panel's terminal when focus is inside the panel (`TerminalPanel.containsFocus`).
- Shell profiles (`ShellProfile`: name + path, `AppState.shells`, Settings › Shells) are separate from agent profiles. First load fills them from the login shell plus `/etc/shells` (existing files only); "Detect Shells" adds missing paths. A `.shell` session runs the profile's path directly as `-l` (no `$SHELL -c` wrapper); its path is saved in the tab as `shell`.
- There is no default shell setting. Panel terminals (`` ⌃` ``, `` ⌃⇧` ``) use `AppState.terminalShell`: the shell profile last started with `startShell` (`lastShell`), else the first launchable shell, else `$SHELL`. Panel terminals save their shell path too and restore with it. Agents still launch through `$SHELL -l -i -c` (`AppState.shell`, read from the environment only).
- The picker (`⇧⌘T`) lists agents, a "Shells" header, shells, then recent conversations; digits count launchers only. `lastProfile` stores either an agent or a shell name (agents win on a name clash), so `⌘T` and `⌘D` repeat whichever was used last.

## Workspaces are windows

- Exactly one `Workspace` per window id (`AppState.ensureWorkspace(windowId:)`, called on load for every window id and in `makeWindow`). Membership is `Project.windowId`. Workspaces have no color; only projects are colored.
- Legacy data on load: projects of an old workspace take its window id; when several old workspaces shared a window, every extra one gets a new window id and its projects follow. Loose projects stay in their window and join its workspace.
- The sidebar root lists `AppState.projects(inWindow:)`; the workspace name sits in the toolbar next to the traffic lights (`MainWindowController.workspaceLabel`) and in the workspace popup at the bottom of the sidebar (`SidebarViewController.updateWorkspaceTitle`). Rename via File › Rename Workspace… or the sidebar's empty-space context menu.
- `⌘N` (File › New Workspace…) asks for a name and opens a new window. The window title is the workspace name (hidden in the titlebar, used by the Window menu and Mission Control); the subtitle is the current project.
- Closing a window (`AppController.closeWindow`) confirms and deletes the workspace: kills its agents and terminals and removes its projects. Quitting does not go through it, so all workspaces come back.
- The key window's id is saved as the `lastWindow` default (`windowDidBecomeKey`); on launch that workspace window is shown last so it comes up in front.
- `ResponsiveSegmentedControl` (sidebar and panel tabs) shows icon + title, and drops the titles when narrower than ~92pt per segment.
- Gotcha: a vertical `NSStackView` with `.width` alignment does not reliably stretch its arranged views; pin widths with explicit constraints (`HoverRow.viewDidMoveToSuperview`, the commit field).
