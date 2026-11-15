# Nucli specification

How Nucli behaves, from the user's point of view. The code follows this document; when behavior changes, this document changes in the same commit.

## Platforms

macOS 12+, Linux (X11 and Wayland) and Windows 10+. Everything below applies to all three unless a section says otherwise. Shortcuts are written with the macOS keys; `docs/KEYBINDINGS.md` lists the Linux and Windows defaults.

## Window

One window per open workspace. From top to bottom:

1. **Title bar**: the workspace name in plain bold text on the left (click: workspace menu), the location in the middle, status messages on the right (see Title bar). On Linux and Windows the File, Edit, View and Window menus follow the workspace name, since those systems have no global menu bar; File also holds Settings, About and Quit. On macOS the traffic lights sit at the left edge of this bar. On Windows this bar replaces the system title bar: it moves the window, and minimize, maximize and close sit at its right edge. On Linux window controls sit there when the compositor asks the app to draw them.
2. **Body**: sidebar, panel, viewport. The panel sits between the sidebar and the viewport (left, default) or right of the viewport. Sidebar and panel widths are draggable and remembered. The viewport holds the selected tab or file, the status chips and the terminal panel.

Every pane except the viewport can be hidden. With all panes hidden the viewport fills the window.

## Look

- Square corners, 1px rules between panes, no shadows or blur.
- The bundled Maple Mono NF for the whole interface at `ui_font_size`. Terminals and the file view use `font` (Maple Mono NF by default, or any installed font, with Maple Mono NF as the fallback for missing glyphs) at `font_size`.
- Icons are Nerd Font glyphs, colored by the theme. File types and known agents get their own glyph and color.
- Sidebar and panel tabs are a glyph and a name. The active tab, the selected row, focused list items and mode labels are inverted: accent background, background-colored text.
- Key hints in brackets: `[enter] open  [esc] close`.
- Spinners are character animations (`⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏`).
- If the graphics system cannot start or no window can be opened, for example without a working driver, Nucli reports why (a message box on Windows, stderr elsewhere) and quits instead of running invisibly.
- Confirmations are Nucli's own square dialogs on every platform: arrows or `tab` move, `enter` chooses, `esc` cancels. File and folder choosers stay native.

## Onboarding

- When `settings.json` does not exist, Nucli opens a setup window instead of a workspace. Nothing is written until the last step.
- **Welcome**: only the Nucli logo, which appears column by column behind a block cursor. Then a bar fades in at the bottom: the link "moving from another machine? import your settings" (plus "used Nucli 1 before? import its settings" on macOS when Nucli 1 settings exist) on the left, `[enter] next` on the right, where the steps keep it. Agent and shell detection runs in the background meanwhile.
- **Steps**, all full width: Appearance (a Theme section with the theme cards, then a separate Mode section with three tiles showing a miniature window in the light and dark side of the chosen theme), Terminal (live preview with the chosen font and a blinking cursor when blinking is on, the font combobox, font size, line height, ligatures, cursor), Agents and Shells (detected profiles with checkboxes, "detect again", and a note that more can be added by hand later; the first shell is the default), Ready (four large tiles filling the window: theme with swatches, terminal font with a sample, agents, default shell). Every change applies live. The bar at the bottom lists the steps (click to jump), `[esc] back` and `[enter] next` (`[enter] start` on the last step); arrows work too.
- **Start** writes `settings.json` with the checked profiles, closes the setup window and opens the workspace window.
- **Import backup…** takes a folder (or a file inside it) holding a previous `settings.json`, and copies `keybindings.json` and `state.json` (workspaces and projects) from it when present. **Import Nucli 1** reads the old preferences (see Import from Nucli 1). Both jump to Ready with the imported values, which can still be changed. An existing `state.json` is never overwritten.

## Workspaces

- `⌘N` asks for a name and opens a new workspace window. The first launch creates "Personal".
- Switch windows with the workspace popup at the bottom of the sidebar, the command palette, or the system window switcher.
- **Closing a workspace window closes the workspace**: its tabs are saved like on quit (conversations and terminal output included), the processes end, and the workspace is marked closed. It asks first only when processes are running or files are unsaved.
- A closed workspace reopens from the workspace popup or the palette ("Reopen Workspace: Work"). Its tabs come back as they were.
- Rename Workspace… and Delete Workspace… live in the workspace popup and the palette. Delete confirms, ends the workspace's processes and removes its projects.
- Quitting keeps every workspace and reopens the ones that were open. When every window was closed, the last used workspace opens.
- The window that was focused last comes up in front.

## Projects

- A project is a folder. Open one with `⌘O`, by dropping a folder on the window, from a History entry, by opening a tab in a new folder (`⌥⇧⌘T`), by dropping it on the Dock icon on macOS, or from the command line with `nucli <folder>`.
- Projects list in the sidebar in a user-defined order (drag to reorder). Each row shows the folder glyph, the name and the uncommitted diff against `HEAD` as green `+N` and red `-M` (hidden when clean or not a git repository). The numbers update live. A collapsed project also shows the status of its hidden tabs: waiting first, then working, then done.
- Rename (double-click or context menu; empty restores the folder name), Color (12 theme colors or none; tints the folder glyph and the rows of its tabs), Move to Workspace (another workspace or a new one), Reveal in File Manager, Copy Path, Close Project (`⇧⌘W`, ends its tabs after confirming running agents).
- Collapsing a project hides its tabs; the state is remembered.
- Clicking a project shows its active file when one is open, otherwise its overview. `⌂` in the file view's tab strip goes to the overview.
- **Without a project**: a project is optional. Clicking the empty space of the sidebar deselects, and with nothing selected new tabs and panel terminals open in the home folder. Those tabs list below the projects with no header, in line with the project rows. They have no Files, Changes or quick open, and the title bar shows `~` in place of a project name.
- A running tab or panel terminal moves between projects by dragging it onto a project row. Dragging it onto the empty space of the sidebar, its context menu or "Move Out of Project" takes it out of its project. A move never restarts the process or changes its working folder.

### Project overview

Shown in the viewport for a selected project, and after its last tab closes:

- Header: project name, path, git branch.
- NEW: one row per agent profile ("new Claude session", last used first, with `[⇧⌘T]`), one per shell, and "new terminal" in the panel.
- TABS: the project's open tabs with status.
- CONVERSATIONS: the 12 newest agent conversations of this folder, with agent, age and `open` when a tab runs it. Clicking resumes it.
- Reveal in File Manager.

## Tabs

A tab is an agent or a shell living in a project. Agents and shells look the same in the sidebar; only the glyph differs.

- `⌘T`: new tab with the last used shell (fallback: the first shell). Holding it for 100 ms opens the **shell picker** instead.
- `⇧⌘T`: new tab with the agent last used in this project (fallback: last used anywhere, then the first agent). Holding it for 100 ms opens the **agent picker** instead: agents, then the project's recent conversations.
- Pickers open over the viewport with the last used launcher preselected. Digits `1`–`9` pick a launcher directly. Conversation rows resume. "New Shell…" and "New Agent…" open them from the palette.
- **New Tab…** (`+` button, palette, project menu): one picker with agents, shells and conversations.
- `⌥⇧⌘T`: New Tab…, but asks for a folder first (and opens it as a project).
- With nothing selected, the tab opens without a project.
- `⌘1`…`⌘9`: tab by position in this window's sidebar. The sidebar shows the number before each tab (`1 claude`).
- `⇧⌘]` / `⇧⌘[`: next / previous tab in sidebar order. `⌃Tab` / `⌃⇧Tab` open the tab switcher.
- Rename (double-click, context menu, palette "Rename Tab"; empty restores the automatic title, which follows the terminal title) and Color (12 theme colors or none; marks the tab's sidebar row and split header with a stripe on the left edge and a light wash of the color, and tints its title in the terminal panel).
- `⌘W`: closes the shown file tab if a file is shown, else the current tab. A running agent asks first, unless it printed nothing for 2 hours or "don't ask again for 1 hour" was checked. `⌘W` never closes a project.
- After closing, the next tab of the same project is selected (previous when it was the last), then the project overview.
- A process that exits with code 0 closes its tab. Any other code keeps the tab, marks it red and prints `[process exited with code N]`.
- Tabs keep running when not shown, also in other projects and windows.
- Drag a tab in the sidebar to reorder it or move it into another project (the process keeps running), or onto the viewport to split.

### Tab switcher

`⌃Tab` shows an overlay with this window's tabs in most-recently-used order (status, title, project), plus projects whose file view was open. Keep `⌃` held and press `Tab` again to move; release to switch; `esc` cancels.

## Agents

### Profiles

An agent profile is `{name, executable, arguments, config_dir, env, theme}` in `settings.json`:

- `executable`: a name in `PATH` or an absolute path. Its file name decides the agent kind: `claude`, `codex` and `pi` are known kinds; anything else still runs, without history or resume.
- `arguments`: extra arguments appended as written.
- `config_dir`: the agent's account folder (`CLAUDE_CONFIG_DIR`, `CODEX_HOME`, `PI_CODING_AGENT_DIR`). Empty means the agent's default (`~/.claude`, `~/.codex`, `~/.pi/agent`). History is read from here, so two Claude profiles with different folders keep separate histories.
- `env`: extra environment variables.
- `theme`: optional theme for this profile's tabs (empty = app theme), so work and personal tabs can look different.

**Detection**: during onboarding, and with the palette command "Detect Agents", Nucli looks for known agent CLIs (`claude`, `codex`, `gemini`, `opencode`, `pi`, `amp`, `aider`, `cursor-agent`, `goose`, `crush`, `qwen`, `copilot`, `droid`) in the login shell's `PATH` and common install folders, and adds missing profiles. For `claude` and `codex`, every used config folder (`~/.claude*`, `~/.codex*`) becomes its own profile.

### Launching

- macOS and Linux: agents run as `$SHELL -l -i -c "<env assignments> exec <executable> <arguments>"`, so `PATH`, aliases and environment from the user's dotfiles work. Shells run as `<shell> -l`.
- Windows: agents run through `cmd.exe /C`, so `.cmd` shims from package managers work; shells run directly.
- Environment: on macOS the shell starts from a clean environment like a login (`HOME`, `USER`, `LOGNAME`, `TMPDIR`, `SSH_AUTH_SOCK`, `LANG`, `PATH=/usr/bin:/bin:/usr/sbin:/sbin`). On Linux and Windows the app's environment is inherited. On every platform variables set by agent CLIs (`CLAUDECODE`, `CLAUDE_*`, `CODEX_*`, `PI_CODING_AGENT_DIR`) and by other terminals (`TERM_PROGRAM*`, `TERM_SESSION_ID`) are removed, then `TERM=xterm-256color`, `COLORTERM=truecolor`, `TERM_PROGRAM=Nucli`, `CLICOLOR=1` and `FORCE_HYPERLINK=1` are set.
- The working directory is the project folder.

### Conversations

- Resuming a conversation that a running tab of the window already runs selects that tab instead of opening a second one. This applies to every resume entry point: pickers, palette, project overview and History.
- New Claude and pi tabs get a fresh `--session-id <uuid>`; resuming passes `--resume <id>` (Claude), `--session-id <id>` (pi) or `resume <id>` (Codex). The id of a new Codex tab is taken from the newest Codex session file for the project when the tab is saved.
- History is read from the agents' own files, never written: Claude `<config>/projects/<encoded cwd>/*.jsonl`, Codex `<config>/sessions/**/*.jsonl`, pi `<config>/sessions/--<cwd>--/*.jsonl`. Titles: Claude's custom title, else its AI title, else the first user prompt; Codex and pi the first user prompt.
- Titles, ids and folders are cached by path and modification time in the cache folder, so only new files are read.
- While Claude runs in a tab, Nucli follows its session id (`/clear` and `/resume` change what the tab resumes after a restart).

### Status and attention

- Each tab has an activity: **working**, **waiting** (asks for permission or input), **idle**, **done** (finished while not watched) or **exited**.
- Claude Code: read from its status file `<config>/sessions/<pid>.json` (`busy`, `waiting`, `idle`) once per second. The `<pid>.<hash>.key` files next to it hold secrets and are never read.
- Other agents: output based. Output gaps under 1.5s are one busy run; busy for 2s is working; after at least 5s of work, 2s of silence is done.
- The bell and OSC `9` / `777;notify` notifications also mark a tab done. A process that exits with an error does too.
- Any CLI started by hand in a shell tab is detected from the terminal's foreground process, gets its agent glyph, and gets the same status.
- Shown in the sidebar on the right of the tab row: spinner (working), `?` in orange (waiting), `✓` in green with a green title (done), `✗` in red (exited with an error).
- Done and waiting are ignored for the tab the user is looking at, and cleared when the tab is shown.
- A desktop notification is posted when a tab becomes done or waiting and is not the one being watched, if `notifications` is on: the tab's title, its project, and "Claude finished", "Claude is waiting for input" or "Needs attention" (the bell). On macOS they group by project. Clicking it shows the tab. On macOS the Dock icon shows the number of tabs that need attention; on Linux and Windows the window title does.

### Subagents

Running Claude Code subagents of a tab are listed under it in the sidebar (description and type), read from `<config>/projects/<cwd>/<session>/subagents/agent-*.jsonl`. A subagent counts as running until its transcript ends its turn or was not written for 10 minutes. Clicking one selects its tab.

## Terminal

- On Windows `ctrl-c` copies the selection when there is one and interrupts otherwise, and `ctrl-v` pastes, as in Windows Terminal.
- `shift-enter` sends a line feed when the program has not asked for the kitty keyboard protocol, so agents add a new line while shells still run the command.
- Real terminal emulation: 256 colors and true color, alternate screen, mouse reporting, bracketed paste, the kitty keyboard protocol (so `⇧↩` reaches agents), OSC 8 hyperlinks, IME.
- **Prompt at the bottom**: shell tabs start with the prompt on the last row and output grows upward. `clear` and `⌘K` keep the prompt at the bottom. Resizing a window keeps the prompt on the bottom row. Full screen programs (alternate screen) and agents are untouched. On Windows the view draws the grid lower instead of moving the cursor, which looks the same.
- Selection with the mouse (double-click words, triple-click lines), `⌘C` copies, `⌘V` pastes (bracketed when the program asks). `⌘A` selects all output.
- `⌘K` clears the screen and scrollback.
- `⌘F` opens a find bar over the terminal: matches are highlighted, `enter` / `⌘G` next, `⇧⌘G` previous, `esc` closes.
- Context menu (right-click; `⇧`-right-click when the program reports the mouse): Copy Link and Open Link over a URL, or Copy Path, Reveal and Open File over a file path, then Copy (with a selection), Paste, Select All, Find, Clear, Split Right and Split Down (tabs, not panel terminals), followed by the tab's own menu. Right-clicking keeps the selection and focuses the pane.
- `⌘`-click opens URLs in the browser and file paths (`path`, `path:line`, `path:line:col`, relative to the shell's folder or the project) in the file view at that line. `⌘`-hover underlines them, across every row a wrapped link spans, and shows a preview of an image file on the side of the link with more room, scaled down to fit the window and never enlarged.
- Dropping files on a terminal types their shell-quoted paths, also rows from Files and Changes and file tabs; dropped text is typed as is.
- macOS keys: `⌘←` / `⌘→` line start / end, `⌘⌫` delete line, `⌥←` / `⌥→` word back / forward, `⌥⌫` delete word, `⌥` is Meta.
- Box and block characters are drawn by Nucli so they join seamlessly at any line height. Rounded box corners are drawn square.
- Scrollback: 10000 lines. The scroll position sticks while new output arrives unless at the bottom.
- `⌘=` / `⌘-` / `⌘0` change the font size of terminals and the file view, saved to `font_size`.

## Terminal panel

- A panel under the viewport with the project's own shells, one tab strip in the terminal's background with no rule below it. Every tab of the project shows the same panel.
- Each tab shows what is running right now: `ssh` during `ssh host`, `sleep` during `sleep 60`, the shell's name otherwise, updated once per second. A custom name from renaming the tab wins. A green play glyph marks a running command. `×` appears on hover and on the selected tab.
- `` ⌃` `` shows and focuses it (starting a shell if the project has none), and hides it when it already has focus. `` ⌃⇧` `` opens another shell. `⌘W` inside it closes that shell. Hiding keeps the shells running and keeps the tab strip at the bottom of the viewport; clicking a tab or the chevron shows the terminal again. Its height is draggable and remembered.
- Panel shells use the shell last started anywhere, else the first available.

## Splits

- `⌘D` / `⇧⌘D` open a new tab (the last used agent or shell) to the right of / below the focused pane. `⌥⌘W` closes the split; the tab keeps running in the sidebar.
- Only tabs of the same project split together. A tab without a project that joins a project's split moves into that project. Panel terminals do not split.
- Dragging a tab over a pane (from the sidebar or a pane header) previews the resulting layout: the new pane is filled and labeled with its title and the action (`split right`, `swap`, `replace`, plus `· moves to <project>` for a tab without a project), the other panes are outlined. The outer quarter of each edge splits that pane on that side; the center replaces the pane, or swaps the two panes when both are in the split. A band along the outer edge of the whole split places the tab beside all panes at once (`bottom, full width`), so `w1 | w2` over `w3` is one drag. Over a tab of another project the pane turns red and the drop does nothing.
- In a split every pane has a one-line header: glyph, title, status and `×`. The focused pane's header is an inverted block. Dragging a header moves the pane; `×` removes it from the split and the tab keeps running. Right-clicking a header opens the tab's context menu.
- `⌥⌘←→↑↓` move focus. Clicking a pane focuses it. Unfocused panes are dimmed.
- Tabs in a split form a group. The sidebar keeps a group's tabs next to each other in layout order and joins them with a bracket, in the accent color while the group is shown. Selecting a tab of the group focuses its pane; selecting another tab shows that tab alone and keeps the group, which comes back when one of its tabs is selected.
- Hovering a tab row in the sidebar shows a split button (adds the tab next to the focused pane, along the pane's longer side) or, on a grouped tab, an unsplit button. `⌘`-click on a row does the same. Dragging a grouped tab within its group reorders the panes; dragging it out of the group removes it from the split.
- Context menu of a tab: Split with Shown Tab, Remove from Split, Unsplit All.
- Splits are saved with the workspace.

## Sidebar

- Tabs at the top: Projects (the tree), Agents (the board), History, and Uplink while Uplink is on. `⌥⌘1`/`⌥⌘2`/`⌥⌘3`/`⌥⌘4` switch (and show the sidebar). The choice is remembered. In a narrow sidebar only the active tab shows its name.
- `⌘B` hides and shows the sidebar.
- **Projects**: projects and their tabs. Tab rows: number, the agent's glyph and color, title (the terminal's title, or the profile name until there is one), the running program when the title does not name it (`exited` when the process ended), status on the right.
- **Agents**: this workspace's running agent tabs grouped by status: Waiting, Done, Working, Idle. While Uplink is on, running agents shared by connected devices join the same groups with `device · project` beside the title. Clicking selects the tab, or opens a remote one.
- **History**: every conversation of every agent profile, newest first (max 150). Clicking resumes it in the project for its folder (opened as a project if needed).
- **Uplink**: paired devices and what they share (see Uplink).
- **Filter** (`⌥⌘F`) at the bottom filters the visible list by name, path and title; a matching project shows all its tabs; `enter` selects the first match, `esc` clears.
- **Workspace popup** at the bottom: all workspaces (closed ones marked), New, Rename, Delete.
- **`+` button**: New Shell, New Agent, New Tab…, New Tab Without Project…, New Terminal, Open Project….
- Clicking empty space deselects: empty viewport with key hints and "or drop a folder here".
- Context menus: project (New Terminal, New Tab…, Rename, Color, Move to Workspace, Reveal, Copy Path, `[x] Share over Uplink` while Uplink is on, Close Project), tab (Rename, Color, Split with Shown Tab, Remove from Split, Unsplit All, Move Out of Project, `[x] Share over Uplink` while Uplink is on, Close Tab), conversation in History (Resume, Copy Title, Copy Conversation ID, Copy Path, Reveal), empty space (Open Project, Rename Workspace, New Workspace, Delete Workspace).

## Panel

Tabs for Files, Changes and History (`⌃⌘1`/`⌃⌘2`/`⌃⌘3`, also showing a hidden panel). `⌥⌘B` hides and shows it in every window. It starts on the right. Drag its header across the window middle, use its context menu or "Move Panel to Other Side" to switch sides; the side is shared by all windows. Everything follows the current project; with no project it says so.

### Files

- Lazy file tree of the project. Folders first, then files, by name. Git status letters on the right (`M` modified, `A` added, `D` deleted, `U` untracked); folders get a dot (green when everything inside is new, orange when mixed). Ignored files are dimmed. Expanded folders are remembered per project.
- Clicking a file opens it in the file view. Context menu: New File…, New Folder… (a name with `/` creates the folders on the way), Open as Project (folders), Open in Default App, Open With…, Reveal in File Manager, Copy Path, Copy Relative Path, Cut, Copy, Paste, Duplicate (`name copy.ext`), Rename…, Move… (a path relative to the project), Delete (to the trash, confirmed). Empty space: New File…, New Folder…, Paste, Reveal, Copy Path. New files open in the file view; renamed and moved files keep their open tabs, unsaved edits included; deleted files close theirs. A name that is taken gets ` copy`, ` copy 2`, … when pasting.
- Drag and drop: dragging a row onto a folder (or onto a file, meaning its folder, or onto empty space, meaning the project root) moves it there, `⌥` copies. Files dropped from the file manager are copied into the folder under the pointer. A row dropped on a terminal, the commit message or any other text field types its path; on the file view it opens; a folder dropped on the sidebar opens as a project. A row or file tab dragged out of the window becomes a file for other apps: file managers and mail take the file, apps that only take text get its path. This works on macOS, Windows and Linux under X11, not under Wayland.
- Updates live.

### Changes

- Live `git diff` against `HEAD` (staged, unstaged and untracked) of the project folder; the empty tree when the repository has no commits.
- Header: branch, file count, `+added -removed`.
- Commit message input (`enter` commits staged changes) and a commit button.
- Sections STAGED and CHANGES with counts, collapsible. Rows: glyph, name, folder, status letter. Hover shows: open, discard (unstaged only, confirmed; tracked files are restored, new files go to the trash), stage `+` / unstage `-`. Section headers stage, unstage or discard all.
- At most 500 rows per section plus "N more files not shown".
- Clicking a row opens the file with its diff.
- Context menu of a row: Open Changes, Stage or Unstage, Discard Changes (unstaged), Open in Default App, Open With…, Reveal in File Manager, Copy Path, Copy Relative Path. Rows can be dragged like Files rows.
- "Working tree clean" when there is nothing.

### History (commits)

- `git log` of branches, remotes, tags and `HEAD` in topological order (first 500), with a lane graph, ref labels, subject and relative date. A lane changes color at each commit a branch points to, and that commit's branch labels use the same color, so a branch reads as one color. Lanes continue through expanded files. A row shows at most two ref labels and `+N` for the rest; they shrink before the subject disappears.
- Hovering a commit shows a card with everything about it: author and co-authors in the accent color, relative and full date, every ref, the whole message with its links and addresses highlighted, changed files with insertions in green and deletions in red, and the full hash.
- Clicking a commit expands its changed files (against the first parent). Clicking a file opens the commit's diff of it, read-only.
- Context menu of a commit: Switch to each local branch on it, Check Out each remote branch on it (tracking), Check Out Detached, Create Branch… (and switch to it), Create Tag…, Cherry-Pick, Revert (merges against their first parent), Reset Current Branch to Here (Soft, Mixed, Hard after confirming), Delete Branch… and Delete Tag… for its refs (confirmed), Copy Hash, Copy Short Hash, Copy Message. A failing git command shows its message.
- Context menu of a file in an expanded commit: Open Changes, Open Current Version, Restore This Version… (confirmed), Open in Default App, Open With…, Reveal in File Manager, Copy Path, Copy Relative Path.

## File view

- Opens in the viewport over the project's tabs. A tab strip lists the open files (`⌂` goes back to the overview, `×` or middle-click closes). Open files, split groups and the active file are remembered per project. Opening a file inserts its tab after the active one.
- **Splits**: the file view splits like the terminals, into groups that each have a tab strip and path bar. Dragging a file tab onto an edge quarter of a group opens it in a new group on that side, onto the band along the outer edge of the whole view places it beside all groups, onto a group's middle or a tab strip moves it into that group; a preview shows the resulting layout while dragging. `⌘D` / `⇧⌘D` move the active file into a new group to the right / below, `⌥⌘W` merges the focused group into its neighbor, `⌥⌘←→↑↓` move focus. A file is open in one group at a time; opening it again focuses it there. Closing a group's last tab removes the group. The focused group's active tab is an inverted block, the others are muted.
- A path bar under the tab strip: breadcrumbs of the relative path, and buttons for previous/next change, Open in Default App, Open With…, Reveal. The file's data is on the status chips.
- Changed files show the whole file with added lines on a green background and removed lines on a red background, old and new line numbers in the gutter. `F7` / `⇧F7` jump between changes; the first open of a changed file jumps to its first change.
- Long lines do not wrap: the text scrolls horizontally while the gutter stays in place. Vertical and horizontal scrollbars appear when the file overflows and can be dragged.
- Syntax highlighting with tree-sitter grammars for common languages; other files are plain text.
- Images (PNG, JPEG, GIF, WebP, BMP, SVG, ICO) are shown fit to the viewport, never upscaled, on a checkerboard, with pixel and file size. Binary files show their size and an Open in Default App button.
- **Editing**: working tree files are editable in place, diffs included. Removed lines cannot be typed into. `⌘S` saves; the tab shows `●` while unsaved. Undo/redo per file, kept in memory for the whole app session: closing and reopening a file keeps its history unless the file changed on disk in between; saving keeps it too. `enter` keeps the indentation. Closing an unsaved tab asks Save / Don't Save / Cancel; quitting and closing a workspace with unsaved files ask first.
- External changes (an agent writing the file) reload the view unless it has unsaved edits; saving then overwrites.
- `esc` hides the file view (tabs stay); `⌘W` closes the shown file tab.
- Context menu of a file tab: Close, Close Others, Close to the Right, Close All (one Save All / Don't Save / Cancel prompt for several unsaved files), Split Right, Split Down, Open in Default App, Open With…, Reveal in File Manager, Copy Path, Copy Relative Path.
- Context menu of the text: Cut and Copy (with a selection), Paste (editable files), Select All, Find, Replace (editable files), Next Change and Previous Change (changed files), Save (unsaved), then the file items of the tab menu. Right-clicking outside the selection moves the cursor there.
- Open With… asks for an application once and remembers it per file extension; later the menu lists remembered applications first.
- Dropping files on the file view opens them (folders open as projects).
- `⌘F` finds in the file and `⌘R` also replaces, in a bar at the top right. Selected text seeds the query. Every match is highlighted, the current one is selected, and the bar counts them (`3 of 12`, `no results`, `invalid`).
  - Toggles, remembered between sessions: match case, whole word, regular expression (with lookaround and backreferences), find in selection. A pattern containing `\n` matches across lines; removed diff lines are never touched by a replace.
  - Replacing: `enter` replaces and moves to the next match, Replace All is one undo step and reports how many it changed. Regular expressions expand `$1`, `$&`, `${name}`, `$$`, `\n`, `\t`, and change case with `\u`, `\l`, `\U`, `\L`, `\E`. Preserve Case keeps the match's casing, also per `-` or `_` part.
  - `enter` / `⌘G` next, `⇧enter` / `⇧⌘G` previous, `esc` closes.

## Command palette

- `⇧⌘P` searches everything: tabs (all windows), remote tabs of paired devices, projects, workspaces (closed ones reopen), "New Tab: <agent>", "New Shell: <shell>", the project's conversations, every command with its shortcut, themes, and the project's files (ranked last, hidden while the query is empty).
- `⌘P` quick-opens the project's files: recently opened first, then all files (`git ls-files` including untracked, or a capped folder walk outside git). Without a project it opens the full palette.
- Fuzzy matching on subsequences with bonuses for word starts, separators, camel case and consecutive characters; file names score above folders. Matched characters are highlighted.
- `enter` runs, `⌘enter` runs the alternate action (files: open in the default app), arrows and page keys move, `esc` or clicking outside closes.
- **Select Theme** lists every theme and previews the highlighted one live; `enter` keeps it, `esc` restores.

## Title bar

- **Location**: a framed box in the middle with the folder glyph, the project name and the selected tab, file or `settings`. Clicking it opens the command palette. Path and branch are on the status chips.
- **Status messages** on the right show what Nucli is doing with a character spinner: `git status`, `git log`, `git commit`, `git switch main`, `listing files`, `loading history`, `detecting agents`, `starting <tab>`. A message appears once the work has taken 200 ms, so quick work stays silent; other running work shows as `+N`. With nothing running it shows this workspace's agents (`2 working`, `1 waiting`, `1 done`), or nothing.
- Clicking the status messages opens a list under them: every running message, then this workspace's agents that are waiting, done or working. An agent row shows its status, glyph, title, project and age, and its last notification (the agent's own notification text, "Claude finished", "exited with code 1"); a working agent shows its subagent count. Done rows disappear once the tab is seen. Clicking a row shows the tab. `esc` or clicking outside closes the list.

## Status chips

One row of small framed chips directly under the selected tab, in the terminal's own background with no rule above, so it reads as part of the terminal:

- The shell's current folder (click: Reveal / Copy Path).
- Git branch (click: switch to one of the 30 newest local branches or create a new one).
- The project's `±files +added -removed` (click: Changes), when there are changes.
- The foreground program, unless it is the tab's shell.
- **Uplink**: the paired devices watching this tab right now, with an eye glyph (click: Stop Sharing This Tab, Disconnect a device).

While a known agent CLI runs in the tab (an agent tab, or one started by hand in a shell), agent chips follow:

- **Skills**: `60 skills 2 used`. Click: a combobox above the chip listing every skill, the used ones first under "Used", then "Available", each with its description and source (`user`, `project`, `plugin`, `built-in`). Typing filters, `enter` opens the skill's `SKILL.md` in the file view, `⌘enter` opens it in the default app.
- **Instruction files**: `3 md`, the CLAUDE.md, AGENTS.md, GEMINI.md and similar files the agent loaded. Click: the same combobox with each file's path and scope (`user`, `project`, `local`, `nested`); `enter` opens the file.
- **Model**: the model of the last reply.
- **Context**: `31% 312k/1M`, the tokens of the last request against the model's context window, green under 50%, orange under 80%, red above. Without a known window only the tokens show. After the agent compacted the conversation the chip adds `compacted 2x`. While the agent compacts, the chip shows a spinner and `compacting`: Nucli spots the agent's own compaction spinner line on the visible screen (Claude Code's `Compacting conversation…`, pi's `Compacting context...` and `Auto-compacting...`).
- **MCP servers**: `9 mcp 1 used`. Click: the combobox with the called servers first and their call count; `enter` opens the config file that defines the server.
- **Session time**: how long the agent process has been running.

Clicking the model, context or session chip opens a menu: agent and model, context, compactions, running time, Open Transcript in Default App, Reveal, Copy Transcript Path, Copy Conversation ID.

The agent chips are read from the agent's own files once per second, never by talking to the agent:

- Claude Code: the transcript of the session in `<config>/sessions/<pid>.json` records the instruction files it loaded, its skill list, the skills it used, its MCP servers and the token usage of every reply. Skill files come from `<config>/skills`, `.claude/skills`, `.claude/commands` and the enabled plugins; MCP config from `.claude.json` and `.mcp.json`. The context window comes from Claude Code's cached model list, a `[1m]` model, or 1M once more than 200k tokens were used, else 200k.
- Codex: the rollout of the session (token counts include the context window); instruction files follow its lookup (`AGENTS.override.md` or `AGENTS.md` in the Codex folder and from the repository root to the working folder), skills `.agents/skills`, MCP servers `config.toml`.
- pi: the session file's system prompt lists the instruction files and skills; the context window comes from `models.json`.
- Gemini CLI: the session in `~/.gemini/tmp/<project>/chats`; GEMINI.md files, skills and `settings.json` servers by convention.
- Qwen Code, opencode, Copilot CLI, Amp, Droid, Cursor Agent, crush and goose: instruction files, skills and MCP servers from the folders each one reads by convention, and the session time.

In the file view the same row shows data only: line and column, language with its file glyph (click: choose another language, Auto Detect goes back, and the choice is remembered per file), `+added -removed` of the file, and the commit for a commit's version.

The row is hidden for projects and settings. An invalid settings or keybindings file shows its error in red on this row, also in those views.

By default the row follows the focused pane or file group. With `status_chips` set to `every_pane` every pane of a split and every file group has its own row, and the error gets a row of its own.

**Choosing chips**: every chip can be hidden, as in VS Code. Right-click a chip for "Hide <chip>" and the list of all chips with `[x]` / `[ ]`; right-click the empty row for the list alone. The same toggles are on the settings page (Status Chips) and in the palette ("Status Chips…", also in the View menu, `enter` toggles and keeps the list open). Hidden chips are stored as ids in `hidden_chips`: `folder`, `branch`, `changes`, `program`, `uplink`, `skills`, `instructions`, `model`, `context`, `mcp`, `session`, `position`, `language`, `file_changes`, `commit`.

## Uplink

Uplink lets computers running Nucli control each other's tabs: run agents on one machine and watch, type into and start them from another. A phone running Nucli Uplink is a paired device too: it shares no tabs and never listens, so it dials the computers it paired with and they never dial it. There is no server. Every computer is a peer that hosts its own tabs and connects straight to the computers it is paired with, over the local network or any private network the user sets up between them (for example a VPN that gives each machine an address).

### Turning it on and off

- Settings › Uplink starts with the **Uplink** toggle (`uplink.enabled`, off by default). Everything else on the page shows only while it is on. "Turn Uplink On or Off" in the palette does the same.
- Off means off: no port is open, nothing is announced, every connection closes and every remote view closes at once.
- On, Nucli listens on TCP port `uplink.port` (47100, the development build 47101) on every network interface and announces itself with multicast DNS (`_nucli-uplink._tcp`, with its name and public key) so devices on the same network find each other. On macOS the system asks once for Local Network access; on Windows the firewall asks once whether Nucli may accept connections. Turning Uplink off while its sidebar tab is shown shows Projects.
- The toggle then shows the status (`On as laptop · port 47100 · 1 of 2 devices online`) and the latest event ("Paired with laptop.", "A device entered a wrong code. 2 attempts left."). Below it are **Devices** and **This computer**: name, Share tabs by default, port, the networks it can be reached on, and its fingerprint.
- **Addresses** under This computer lists every network this computer is on, each with a checkbox. Unchecking one (`uplink.excluded_networks`, by interface name such as `en0`) refuses connections that arrive on it before any handshake, stops announcing and looking for devices there, and leaves its address out of the pairing link. Connections that other devices opened are dropped and come back over the remaining networks. Nucli may still reach paired devices through an unchecked network. An unchecked network stays listed while it is down.

### Pairing

Two devices pair once; after that they find and connect to each other by themselves whenever both are on.

- Choose **add device** under Devices on both computers (also "Uplink: Pair a Device…" in the palette). Each shows its own one-time code of 8 letters and digits (`K7QM-4XRP`), a QR code and a "copy pairing link" button, and a field for the code of the other computer. Typing the code of either computer into the other pairs them. A code works once, for 5 minutes, and pairing stops after 3 wrong codes; cancel stops it at once.
- Nearby computers that are adding a device are listed under the field. With one, it is chosen; with several, click one. With none, type its name or address (`host`, `host:port`, an IP address) next to the code. Press `enter` or `join`. Codes ignore case and dashes, and `O`/`I`/`L` read as `0`/`1`. Pasting the pairing link (from the QR code or "copy pairing link") into the code field fills in the addresses too and pins the device's key, so it works across networks where multicast does not reach.
- Both devices then list each other under Devices with control; change it any time. Adding a device stops on both computers once they are paired.

### Devices

Each paired device is one row: its system glyph, name, state (`online · address · watching N of your tabs`, `offline · seen 5 minutes ago · not reachable`, `blocked`) and what it **can** do with this computer's shared tabs:

- **control** means typing into shared tabs, resizing them, clearing them and opening new tabs in shared projects. A device that can control a tab can run any command there. **view** means watching only; its keystrokes are ignored.

Clicking a row opens its details: the fingerprint, **also reach it at**, and the buttons below.

- **disconnect** drops the connection (it reconnects later), **block** refuses it until **unblock**, **forget** (confirmed) removes the pairing on this side and, when the device is online, on its side too; the device must pair again to reconnect. A device forgotten while offline is refused when it calls (`it refused this device`) until it is forgotten there too.
- **also reach it at**: names or addresses, comma separated, for networks without multicast. Nucli also remembers the last addresses that worked.

### Sharing

- What a paired device sees is decided on the computer that runs the tabs. **Share tabs by default** (`uplink.share`, on by default): every tab and panel terminal is shared unless excluded. Off: only what is shared explicitly.
- `[x] Share over Uplink` in a project's menu shares or excludes the whole project; the same item in a tab's menu overrides its project. Both are saved with the workspace.
- Stopping sharing, changing a device to view, blocking or forgetting it applies at once: open remote views of tabs that stop being shared close.
- A tab being watched from another device shows an eye glyph in the sidebar, and the Uplink status chip names the devices watching it.
- A paired device can also ask for more than the tabs of shared projects: their files and file contents (up to 1 MB per file), git changes and diffs, git history and commit details, and the agent conversations that ran in them. This works with view or control. Paths cannot leave the project, symbolic links included.
- With control it can also browse folders and add one as a project, and resume a conversation of a shared project in a new tab.
- With control it can change shared projects the way the desktop's context menus do: create, rename, move, copy, cut and paste, duplicate and delete files (deleted files go to the trash), open or reveal them on the computer; stage, unstage, discard and commit changes, list, switch and create branches, check out, create branches and tags from, cherry-pick, revert and reset to a commit, delete branches and tags, restore a file from a commit; rename, color, move to a workspace and close a shared project; rename, color, move out of its project and close a shared tab. The computer checks every path and name: paths stay inside the project and branch, tag and commit names cannot be options. Closing a running agent from another device does not ask on the computer; the device asks first.

### Remote tabs

- The Uplink sidebar tab lists every paired device: online ones with their shared projects and tabs (glyph, title, program, status), offline ones with when they were last seen. The filter applies. Right-click a device for Disconnect and Uplink Settings….
- Clicking a remote tab opens it in the viewport like a local tab: the full terminal with its scrollback, live, with selection, copy, find and links (file paths of the other computer are not opened locally). The title bar shows `device › project › tab`, the status chips show the device (and `view only` or `offline`), its folder, branch and program. Typing, pasting text, mouse reporting and `⌘K` go to the other computer when this device can control it. Files dropped on a remote tab are not typed, since their paths exist only here, and images in the clipboard stay on this computer.
- The terminal has one size, the size of the device that typed into it last, as with `tmux`: typing on the other computer takes the size back. A device with view access never resizes it.
- `⌘W` or Close View closes the remote view; the tab keeps running on its computer. Remote views are not saved and reopen from the sidebar.
- Hovering a remote project shows `+`, and its context menu lists the other computer's agents and shells: choosing one opens a new tab there and shows it here (control only).
- Status follows the other computer once per second: working, waiting, done and exited. When a remote tab starts waiting or rings for attention, a desktop notification is posted (if `notifications` is on) titled with the tab and `device · project`; clicking it opens the remote tab. Seeing a remote tab here clears its done mark on its computer too.
- While a device is offline its open remote views stay with their last content and the chip says `offline`; they continue when it reconnects.

### Security

- Each device has its own X25519 key pair, created when Uplink first runs and stored in `uplink.json` in the data folder, readable only by the user. The fingerprint on the settings page is the start of the public key.
- Pairing uses SPAKE2, a password-authenticated key exchange: the short code becomes a strong shared key without ever being sent, and someone who listens or answers in the middle cannot test codes offline; each wrong attempt costs one of the 3 tries. That key then authenticates a Noise `XXpsk3` handshake in which both devices exchange and prove their keys.
- Every later connection is a Noise `XX` handshake (X25519, ChaCha20-Poly1305, BLAKE2s). Both sides check the other's key against their paired devices: a connection from an unknown or blocked key is dropped as soon as its key is known, before any data is exchanged, and a device answering at a known address with a different key is refused. All traffic is encrypted and authenticated; a tampered or replayed message closes the connection.
- The computer that runs a tab enforces every rule itself: what is shared, view or control, blocking. A device can only open tabs with the agent and shell profiles already configured there, in projects that are shared.
- Limits keep a misbehaving peer from using up resources: 16 handshakes at a time, 10 seconds per handshake, 64 nearby devices, 16 MB per message, a terminal size of at most 500 × 200, a remote view that opens midway gets at most 8 MB of the newest scrollback, and output for a slow viewer is dropped and resent as a fresh copy of the screen once 8 MB wait.

## Nucli Uplink

Nucli Uplink is the phone app: a remote control for the computers running Nucli. It runs no terminals of its own; everything it shows runs on a paired computer, over Uplink. Look and wording follow the desktop app: one monospace font, square corners, inverted blocks for the active choice, bracketed buttons.

- **Welcome**: on first launch, the animated pixel logo, three steps (run Nucli, turn on Uplink, pair this phone) and `[get started]`, which opens Add a computer.
- **Add a computer** (also `add computer` on the computers list): the computer shows its pairing QR code and code under Settings › Uplink › add device. The phone pairs by any of:
  - `[scan qr code]`: a full-screen camera view that closes as soon as it sees a Nucli pairing code; tap to cancel. The system Camera app works too: its link opens Nucli Uplink and pairs.
  - `[paste pairing link]`: reads the link from the clipboard.
  - its code: the phone looks for computers on the same network that are adding a device (multicast DNS) and lists them; with one it is chosen, otherwise tap one. Only the code is left to type. With none found, type the computer's name or address next to the code.
  - While pairing a spinner shows; a wrong code or an unreachable computer shows why. When paired, the new computer's screen opens.
- After pairing, the phone connects to each computer by itself whenever both are on and reachable: addresses it finds on the network first, then the ones under "also reach it at", then the ones that worked before. Computers never dial the phone.
- Every screen below the computers list has a back button at the top left.
- **Computers** (the home screen): each paired computer with its system, its projects and agents, how many agents are working and waiting, or why it is offline and when it was last seen. `add computer` opens Add a computer; the gear opens Settings.
- **Computer**: its name, a details button (Computer details) and three tabs like the desktop sidebar, the active one inverted:
  - `projects`: the computer's shared projects (grouped by workspace when it has several), each with its agents and shells and how many wait or work, and `home` for tabs outside projects. `add project` (control only) opens a folder browser that starts in the computer's home folder: tap a folder to go in, `..` to go up, `[add <folder>]` to add it as a project in the computer's window and open it.
  - `agents`: every agent of this computer grouped like the desktop Agents board (waiting, done, working, idle), with its project and status.
  - `history`: the agent conversations in shared projects, newest first, with project, profile and age. `resume` (control only) resumes one on the computer and opens its project.
- **Project**: the project's path, its agents and shells with branch and status (tap to open), and `[new agent]` and `[new shell]` (control only), which list the computer's agent profiles or shells; picking one starts it there and opens it. The `files`, `changes` and `history` buttons at the top open a drawer from the right, like the desktop panel; tapping outside it or `x` closes it, and back closes it first.
  - `files`: the project's folders and files; tap a folder to go in, the top row to go up, a file to view it with line numbers (up to 1 MB; binary files are not shown).
  - `changes`: the branch and the staged and unstaged changes with status letter and added and removed lines; tap one for its diff.
  - `history`: the commits with refs, subject, short hash, author and age; tap one for its message, date, stats and changed files, and a file for its diff in that commit.
- **Terminal**: tapping a tab opens it live. With control, opening a tab gives its terminal the phone's size at the phone's font size, and the size follows the screen and the keyboard while the phone holds it. When the computer or another device takes the size back, the phone shows the terminal scaled to fit the screen width until it types again. A phone that can only view always shows the computer's size, scaled to fit. Dragging scrolls the scrollback (or sends arrow keys to full-screen apps that ask for them). The key bar has `ctrl` (applies to the next key), `esc`, `tab`, the arrows, enter, paste and a keyboard toggle; it hides when the phone can only view. The title bar shows the tab, `device · project` and its status, or `offline`, `closed` or `view only`.
- **Computer details**: state and address, number of shared tabs, system, fingerprint, when it was paired, "also reach it at" (names or addresses, comma separated), `[disconnect]` and `[forget]` (tap twice; the computer forgets the phone too when online).
- **Settings**: this phone's name (shown on computers), its fingerprint, appearance (system, light, dark), theme (the desktop themes), terminal font size, the computers, and about.
- iOS suspends apps in the background, so the phone is connected only while the app is open.

## Settings

- `⌘,` shows the settings page in the viewport, with vertical tabs by category. The last category is remembered. Every change applies at once and is written to the same files. `esc` or selecting a tab or file closes it.
  - **General**: reopening tabs, notifications, link to `settings.json`.
  - **Appearance**: theme cards, Import… and Open folder for custom themes, system, light or dark mode, interface size.
  - **Terminal**: font, font size, line height, ligatures, cursor. The font is a combobox: Maple Mono NF first, then every installed font by name, each row with a sample in that font; typing filters, arrows and `enter` pick, `esc` or clicking outside closes.
  - **Agents**: name, executable, arguments and config folder of each profile, add, remove, detect. An agent's `env` and `theme` are only in the file.
  - **Shells**: name and path, add, remove, detect.
  - **Status Chips**: under the focused pane or every pane, and a `[x]` toggle for every chip, grouped by Agent, Terminal and File view.
  - **Uplink**: see Uplink.
  - **Keybindings**: every command with its key, a filter, and a link to `keybindings.json`. Click a key and press the new one (`esc` cancels); a changed key has a reset button; a key used twice in the same context turns red. "Open Keybindings" opens this category.
- "Open Settings File" and "Open Keybindings File" open the JSON files in the file view. Both reload when saved, from Nucli or any editor. Invalid JSON keeps the previous settings and shows the error on the status row.
- `settings.json` keys: `theme`, `appearance` (`system`, `light`, `dark`), `font`, `font_size`, `ui_font_size`, `line_height`, `cursor` (`block`, `bar`, `underline`), `cursor_blink`, `ligatures`, `restore_tabs`, `notifications`, `status_chips` (`focused`, `every_pane`), `hidden_chips`, `agents`, `shells`, `uplink` (`enabled`, `name`, `port`, `share`, `excluded_networks`). Commands that change settings (theme, font size, detection, notifications) rewrite the file with all keys.
- `keybindings.json` maps a key to a command name, e.g. `{"cmd-t": "new_shell", "cmd-shift-d": null}`; `null` removes a default. The palette shows every command's name and current key.
- **Themes**: Nucli (dark and light pairs following the system), Nucli Amber and Nucli Phosphor (monochrome dark), and Vitesse Dark, Dark Soft, Black, Light, Light Soft. A theme sets every UI color, the 16 terminal colors and the syntax colors. An automatic theme follows the system appearance; `appearance` can force its light or dark side.
- **Custom themes**: every `*.json` file in the `themes` folder next to `settings.json` is a theme `{name, base, dark, background, foreground, accent, selection, ansi, syntax}`. Every key is optional. `name` defaults to the file name without `.json`; it is also the id that `theme` and an agent's `theme` refer to. `base` is a built-in single theme id (e.g. `vitesse-dark`, not the `vitesse` pair) whose colors fill every missing key; without it the base is Nucli Dark, or Nucli Light when `dark` is `false`. Colors are `#rrggbb`. `ansi` lists the 16 terminal colors; `syntax` lists 8 colors in this order: keyword, string, number, comment, type, function, constant, attribute. The other UI colors are derived from these, as for the built-in themes. Custom themes appear after the built-in ones in Select Theme, the settings page and onboarding, and reload live when a file is added, changed or removed. A broken file is skipped and its error shows on the status row. **Import Theme…** (command palette and the Appearance page) validates the chosen JSON files, copies them into the folder (same name overwrites) and selects the last one; **Open Themes Folder** shows the folder in the file manager.
- **Shells**: `{name, path}` list. Filled during onboarding from the login shell and `/etc/shells` (Windows: PowerShell, Windows PowerShell, Command Prompt, Git Bash and WSL when installed); "Detect Shells" adds missing ones.

## Restore

- When `restore_tabs` is on, quitting, also when the app is asked to terminate on Linux and macOS, saves every tab: kind, profile or shell, name, color, conversation id, folder, and the last 5000 lines of terminal output.
- On launch tabs reopen without stealing focus, then each window selects its saved tab. Agents resume their conversation (a Claude conversation whose file is gone starts fresh). Shells start in their last folder and show the previous output dimmed above a `── restored ──` line.
- Per project: open files and their split groups, active file, active tab, recent files, last used launcher. Per window: selection, splits, sidebar and panel state, size and position.

## Files and folders

| What | macOS | Linux | Windows |
|---|---|---|---|
| Settings, keybindings | `~/.config/nucli` | `$XDG_CONFIG_HOME/nucli` | `%APPDATA%\Nucli` |
| State, restored terminal output | `~/Library/Application Support/Nucli` | `$XDG_DATA_HOME/nucli` | `%LOCALAPPDATA%\Nucli` |
| Conversation cache | `~/Library/Caches/Nucli` | `$XDG_CACHE_HOME/nucli` | `%LOCALAPPDATA%\Nucli\cache` |
| Uplink key and paired devices (`uplink.json`) | `~/Library/Application Support/Nucli` | `$XDG_DATA_HOME/nucli` | `%LOCALAPPDATA%\Nucli` |

The development build uses `nucli-dev` / `Nucli Dev` instead.

## Import from Nucli 1

On macOS, when the `uk.polishch.nucli` defaults exist, onboarding in a release build offers to import agent profiles, shells, appearance and terminal options, workspaces and projects (path, name, color). Tabs, scrollback and keybindings are not imported.

## Privacy

No telemetry and no server. Without Uplink Nucli makes no network calls: it runs local processes (the user's shells, agents and `git`), reads agents' local conversation, status and config files, and stores only the files listed above. With Uplink on, it talks only to devices the user paired, directly, end-to-end encrypted, and announces itself on the local network (see Uplink).
