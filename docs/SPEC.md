# Nucli specification

How Nucli behaves, from the user's point of view. The code follows this document; when behavior changes, this document changes in the same commit.

## Platforms

macOS 12+, Linux (X11 and Wayland) and Windows 10+. Everything below applies to all three unless a section says otherwise. Shortcuts are written with the macOS keys; `docs/KEYBINDINGS.md` lists the Linux and Windows defaults.

## Window

One window per open workspace. From top to bottom:

1. **Title bar**: the workspace name in plain bold text on the left (click: workspace menu), the location in the middle, status messages on the right (see Title bar). On macOS the traffic lights sit at the left edge of this bar; on Linux and Windows window controls can sit at the right.
2. **Body**: sidebar, panel, viewport. The panel sits between the sidebar and the viewport (left, default) or right of the viewport. Sidebar and panel widths are draggable and remembered. The viewport holds the selected tab or file, the status chips and the terminal panel.

Every pane except the viewport can be hidden. With all panes hidden the viewport fills the window.

## Look

- Square corners, 1px rules between panes, no shadows or blur.
- Maple Mono NF everywhere, at two sizes: `ui_font_size` for chrome, `font_size` for terminals and the file view.
- Icons are Nerd Font glyphs, colored by the theme. File types and known agents get their own glyph and color.
- Sidebar and panel tabs are a glyph and a name. The active tab, the selected row, focused list items and mode labels are inverted: accent background, background-colored text.
- Key hints in brackets: `[enter] open  [esc] close`.
- Spinners are character animations (`⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏`).
- If the graphics system cannot start or no window can be opened, for example without a working driver, Nucli reports why (a message box on Windows, stderr elsewhere) and quits instead of running invisibly.
- Confirmations are Nucli's own square dialogs on every platform: arrows or `tab` move, `enter` chooses, `esc` cancels. File and folder choosers stay native.

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
- Rename (double-click or context menu; empty restores the folder name), Color (12 theme colors or none; tints the folder glyph and the rows of its tabs), Move to Workspace (another workspace or a new one), Reveal in File Manager, Close Project (`⇧⌘W`, ends its tabs after confirming running agents).
- Collapsing a project hides its tabs; the state is remembered.
- Clicking a project shows its active file when one is open, otherwise its overview. `⌂` in the file view's tab strip goes to the overview.
- **No Project**: a project is optional. Clicking the empty space of the sidebar deselects, and with nothing selected new tabs and panel terminals open in the workspace's "No Project" group (home folder). It appears last in the sidebar only while it has tabs; clicking it deselects too. It has no Files, Changes or quick open.
- A running tab or panel terminal moves between projects by dragging it onto a project row. Dragging it onto the empty space of the sidebar, its context menu or "Move Out of Project" moves it to No Project. A move never restarts the process or changes its working folder.

### Project overview

Shown in the viewport for a selected project, and after its last tab closes:

- Header: project name, path, git branch.
- NEW: one row per agent profile ("new Claude session", last used first, with `[⇧⌘T enter]`), one per shell, and "new terminal" in the panel.
- TABS: the project's open tabs with status.
- CONVERSATIONS: the 12 newest agent conversations of this folder, with agent, age and `open` when a tab runs it. Clicking resumes it.
- Reveal in File Manager.

## Tabs

A tab is an agent or a shell living in a project. Agents and shells look the same in the sidebar; only the glyph differs.

- `⌘T`: new tab with the agent or shell last used in this project (fallback: last used anywhere, then the first agent).
- `⇧⌘T`: **New Tab picker** over the viewport: agents, then shells, then the project's recent conversations. The last used launcher is preselected, so `⇧⌘T` `enter` repeats it. Digits `1`–`9` pick a launcher directly. Conversation rows resume.
- `⌥⇧⌘T`: same, but asks for a folder first (and opens it as a project).
- With nothing selected, the tab goes to No Project.
- `⌘1`…`⌘9`: tab by position in this window's sidebar. The sidebar shows the number before each tab (`1 claude`).
- `⇧⌘]` / `⇧⌘[`: next / previous tab in sidebar order. `⌃Tab` / `⌃⇧Tab` open the tab switcher.
- Rename (double-click, context menu, palette "Rename Tab"; empty restores the automatic title, which follows the terminal title) and Color.
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

**Detection**: on first launch, and with the palette command "Detect Agents", Nucli looks for known agent CLIs (`claude`, `codex`, `gemini`, `opencode`, `pi`, `amp`, `aider`, `cursor-agent`, `goose`, `crush`, `qwen`, `copilot`, `droid`) in the login shell's `PATH` and common install folders, and adds missing profiles. For `claude` and `codex`, every used config folder (`~/.claude*`, `~/.codex*`) becomes its own profile.

### Launching

- macOS and Linux: agents run as `$SHELL -l -i -c "<env assignments> exec <executable> <arguments>"`, so `PATH`, aliases and environment from the user's dotfiles work. Shells run as `<shell> -l`.
- Windows: agents run through `cmd.exe /C`, so `.cmd` shims from package managers work; shells run directly.
- Environment: on macOS the shell starts from a clean environment like a login (`HOME`, `USER`, `LOGNAME`, `TMPDIR`, `SSH_AUTH_SOCK`, `LANG`, `PATH=/usr/bin:/bin:/usr/sbin:/sbin`). On Linux and Windows the app's environment is inherited. On every platform variables set by agent CLIs (`CLAUDECODE`, `CLAUDE_*`, `CODEX_*`, `PI_CODING_AGENT_DIR`) and by other terminals (`TERM_PROGRAM*`, `TERM_SESSION_ID`) are removed, then `TERM=xterm-256color`, `COLORTERM=truecolor`, `TERM_PROGRAM=Nucli`, `CLICOLOR=1` and `FORCE_HYPERLINK=1` are set.
- The working directory is the project folder.

### Conversations

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
- A desktop notification ("Claude finished", "Claude is waiting for input") is posted when a tab becomes done or waiting and is not the one being watched, if `notifications` is on. Clicking it shows the tab. On macOS the Dock icon shows the number of tabs that need attention; on Linux and Windows the window title does.

### Subagents

Running Claude Code subagents of a tab are listed under it in the sidebar (description and type), read from `<config>/projects/<cwd>/<session>/subagents/agent-*.jsonl`. A subagent counts as running until its transcript ends its turn or was not written for 10 minutes. Clicking one selects its tab.

## Terminal

- Real terminal emulation: 256 colors and true color, alternate screen, mouse reporting, bracketed paste, the kitty keyboard protocol (so `⇧↩` reaches agents), OSC 8 hyperlinks, IME.
- **Prompt at the bottom**: shell tabs start with the prompt on the last row and output grows upward. `clear` and `⌘K` keep the prompt at the bottom. Resizing a window keeps the prompt on the bottom row. Full screen programs (alternate screen) and agents are untouched.
- Selection with the mouse (double-click words, triple-click lines), `⌘C` copies, `⌘V` pastes (bracketed when the program asks). `⌘A` selects all output.
- `⌘K` clears the screen and scrollback.
- `⌘F` opens a find bar over the terminal: matches are highlighted, `enter` / `⌘G` next, `⇧⌘G` previous, `esc` closes.
- `⌘`-click opens URLs in the browser and file paths (`path`, `path:line`, `path:line:col`, relative to the shell's folder or the project) in the file view at that line. `⌘`-hover underlines them.
- Dropping files on a terminal types their shell-quoted paths; dropped text is typed as is.
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
- Dropping a sidebar tab on the viewport or a pane: the outer quarter of each edge splits on that side, the center replaces the pane.
- `⌥⌘←→↑↓` move focus. Clicking a pane focuses it. Unfocused panes are dimmed.
- Tabs in a split form a group, marked in the sidebar with a joined bar. Selecting a tab of the group focuses its pane; selecting another tab shows that tab alone and keeps the group, which comes back when one of its tabs is selected.
- Context menu of a grouped tab: Remove from Split, Unsplit All.
- Splits are saved with the workspace.

## Sidebar

- Tabs at the top: Projects (the tree), Agents (the board), History. `⌥⌘1`/`⌥⌘2`/`⌥⌘3` switch (and show the sidebar). The choice is remembered.
- `⌘B` hides and shows the sidebar.
- **Projects**: projects and their tabs. Tab rows: number, the agent's glyph and color, title (the terminal's title, or the profile name until there is one), folder subtitle (`exited` when the process ended), status on the right.
- **Agents**: this workspace's running agent tabs grouped by status: Waiting, Done, Working, Idle. Clicking selects the tab.
- **History**: every conversation of every agent profile, newest first (max 150). Clicking resumes it in the project for its folder (opened as a project if needed).
- **Filter** (`⌥⌘F`) at the bottom filters the visible list by name, path and title; a matching project shows all its tabs; `enter` selects the first match, `esc` clears.
- **Workspace popup** at the bottom: all workspaces (closed ones marked), New, Rename, Delete.
- **`+` button**: New Tab, New Tab…, New Tab Without Project…, New Terminal, Open Project….
- Clicking empty space deselects: no project, empty viewport with key hints and "or drop a folder here".
- Context menus: project (New Terminal, New Tab…, Rename, Color, Move to Workspace, Reveal, Close Project), tab (Rename, Color, Remove from Split, Unsplit All, Move Out of Project, Close Tab), empty space (Open Project, Rename Workspace, New Workspace, Delete Workspace).

## Panel

Tabs for Files, Changes and History (`⌃⌘1`/`⌃⌘2`/`⌃⌘3`, also showing a hidden panel). `⌥⌘B` hides and shows it in every window. Drag its header across the window middle, use its context menu or "Move Panel to Other Side" to switch sides; the side is shared by all windows. Everything follows the current project; with no project it says so.

### Files

- Lazy file tree of the project. Folders first, then files, by name. Git status letters on the right (`M` modified, `A` added, `D` deleted, `U` untracked); folders get a dot (green when everything inside is new, orange when mixed). Ignored files are dimmed. Expanded folders are remembered per project.
- Clicking a file opens it in the file view. Context menu: Open in Default App, Open With…, Reveal in File Manager, Copy Path, Copy Relative Path.
- Updates live.

### Changes

- Live `git diff` against `HEAD` (staged, unstaged and untracked) of the project folder; the empty tree when the repository has no commits.
- Header: branch, file count, `+added -removed`.
- Commit message input (`enter` commits staged changes) and a commit button.
- Sections STAGED and CHANGES with counts, collapsible. Rows: glyph, name, folder, status letter. Hover shows: open, discard (unstaged only, confirmed; tracked files are restored, new files go to the trash), stage `+` / unstage `-`. Section headers stage, unstage or discard all.
- At most 500 rows per section plus "N more files not shown".
- Clicking a row opens the file with its diff.
- "Working tree clean" when there is nothing.

### History (commits)

- `git log` of branches, remotes, tags and `HEAD` in topological order (first 500), with a lane graph, ref labels, subject and relative date. A lane changes color at each commit a branch points to, and that commit's branch labels use the same color, so a branch reads as one color. Lanes continue through expanded files. A row shows at most two ref labels and `+N` for the rest; they shrink before the subject disappears.
- Hovering a commit shows a card with everything about it: author and co-authors in the accent color, relative and full date, every ref, the whole message with its links and addresses highlighted, changed files with insertions in green and deletions in red, and the full hash.
- Clicking a commit expands its changed files (against the first parent). Clicking a file opens the commit's diff of it, read-only.
- Context menu: Copy Hash, Copy Message.

## File view

- Opens in the viewport over the project's tabs. A tab strip lists the project's open files (`⌂` goes back to the overview, `×` or middle-click closes). Open files and the active file are remembered per project. Opening a file inserts its tab after the active one.
- A path bar under the tab strip: breadcrumbs of the relative path, and buttons for previous/next change, Open in Default App, Open With…, Reveal. The file's data is on the status chips.
- Changed files show the whole file with added lines on a green background and removed lines on a red background, old and new line numbers in the gutter. `F7` / `⇧F7` jump between changes; the first open of a changed file jumps to its first change.
- Syntax highlighting with tree-sitter grammars for common languages; other files are plain text.
- Images (PNG, JPEG, GIF, WebP, BMP, SVG, ICO) are shown fit to the viewport, never upscaled, on a checkerboard, with pixel and file size. Binary files show their size and an Open in Default App button.
- **Editing**: working tree files are editable in place, diffs included. Removed lines cannot be typed into. `⌘S` saves; the tab shows `●` while unsaved. Undo/redo per file. `enter` keeps the indentation. Closing an unsaved tab asks Save / Don't Save / Cancel; quitting and closing a workspace with unsaved files ask first.
- External changes (an agent writing the file) reload the view unless it has unsaved edits; saving then overwrites.
- `esc` hides the file view (tabs stay); `⌘W` closes the shown file tab.
- Open With… asks for an application once and remembers it per file extension; later the menu lists remembered applications first.
- `⌘F` finds in the file and `⌘R` also replaces, in a bar at the top right. Selected text seeds the query. Every match is highlighted, the current one is selected, and the bar counts them (`3 of 12`, `no results`, `invalid`).
  - Toggles, remembered between sessions: match case, whole word, regular expression (with lookaround and backreferences), find in selection. A pattern containing `\n` matches across lines; removed diff lines are never touched by a replace.
  - Replacing: `enter` replaces and moves to the next match, Replace All is one undo step and reports how many it changed. Regular expressions expand `$1`, `$&`, `${name}`, `$$`, `\n`, `\t`, and change case with `\u`, `\l`, `\U`, `\L`, `\E`. Preserve Case keeps the match's casing, also per `-` or `_` part.
  - `enter` / `⌘G` next, `⇧enter` / `⇧⌘G` previous, `esc` closes.

## Command palette

- `⇧⌘P` searches everything: tabs (all windows), projects, workspaces (closed ones reopen), "New Tab: <agent>", "New Shell: <shell>", the project's conversations, every command with its shortcut, themes, and the project's files (ranked last, hidden while the query is empty).
- `⌘P` quick-opens the project's files: recently opened first, then all files (`git ls-files` including untracked, or a capped folder walk outside git). Without a project it opens the full palette.
- Fuzzy matching on subsequences with bonuses for word starts, separators, camel case and consecutive characters; file names score above folders. Matched characters are highlighted.
- `enter` runs, `⌘enter` runs the alternate action (files: open in the default app), arrows and page keys move, `esc` or clicking outside closes.
- **Select Theme** lists every theme and previews the highlighted one live; `enter` keeps it, `esc` restores.

## Title bar

- **Location**: a framed box in the middle with the folder glyph, the project name and the selected tab, file or `settings`. Clicking it opens the command palette. Path and branch are on the status chips.
- **Status messages** on the right show what Nucli is doing with a character spinner: `git status`, `git log`, `git commit`, `git switch main`, `listing files`, `loading history`, `detecting agents`, `starting <tab>`. A message appears once the work has taken 200 ms, so quick work stays silent; other running work shows as `+N`. With nothing running it shows this workspace's agents (`2 working`, `1 waiting`, `1 done`), or nothing.

## Status chips

One row of small framed chips directly under the selected tab, in the terminal's own background with no rule above, so it reads as part of the terminal:

- The shell's current folder (click: Reveal / Copy Path).
- Git branch (click: switch to one of the 30 newest local branches or create a new one).
- The project's `±files +added -removed` (click: Changes), when there are changes.
- The foreground program, unless it is the tab's shell.

In the file view the same row shows data only: line and column, language with its file glyph (click: choose another language, Auto Detect goes back, and the choice is remembered per file), `+added -removed` of the file, and the commit for a commit's version.

The row is hidden for projects and settings. An invalid settings or keybindings file shows its error in red on this row, also in those views.

## Settings

- `⌘,` shows the settings page in the viewport, with vertical tabs by category. The last category is remembered. Every change applies at once and is written to the same files. `esc` or selecting a tab or file closes it.
  - **General**: reopening tabs, notifications, link to `settings.json`.
  - **Appearance**: theme cards, system, light or dark mode, interface size.
  - **Terminal**: font size, line height, ligatures, cursor.
  - **Agents**: name, executable, arguments and config folder of each profile, add, remove, detect. An agent's `env` and `theme` are only in the file.
  - **Shells**: name and path, add, remove, detect.
  - **Keybindings**: every command with its key, a filter, and a link to `keybindings.json`. Click a key and press the new one (`esc` cancels); a changed key has a reset button; a key used twice in the same context turns red. "Open Keybindings" opens this category.
- "Open Settings File" and "Open Keybindings File" open the JSON files in the file view. Both reload when saved, from Nucli or any editor. Invalid JSON keeps the previous settings and shows the error on the status row.
- `settings.json` keys: `theme`, `appearance` (`system`, `light`, `dark`), `font_size`, `ui_font_size`, `line_height`, `cursor` (`block`, `bar`, `underline`), `cursor_blink`, `ligatures`, `restore_tabs`, `notifications`, `agents`, `shells`. Commands that change settings (theme, font size, detection, notifications) rewrite the file with all keys.
- `keybindings.json` maps a key to a command name, e.g. `{"cmd-t": "new_tab", "cmd-shift-d": null}`; `null` removes a default. The palette shows every command's name and current key.
- **Themes**: Nucli (dark and light pairs following the system), Nucli Amber and Nucli Phosphor (monochrome dark), and Vitesse Dark, Dark Soft, Black, Light, Light Soft. A theme sets every UI color, the 16 terminal colors and the syntax colors. An automatic theme follows the system appearance; `appearance` can force its light or dark side.
- **Shells**: `{name, path}` list. Filled on first launch from the login shell and `/etc/shells` (Windows: PowerShell, Windows PowerShell, Command Prompt, Git Bash and WSL when installed); "Detect Shells" adds missing ones.

## Restore

- When `restore_tabs` is on, quitting, also when the app is asked to terminate on Linux and macOS, saves every tab: kind, profile or shell, name, color, conversation id, folder, and the last 5000 lines of terminal output.
- On launch tabs reopen without stealing focus, then each window selects its saved tab. Agents resume their conversation (a Claude conversation whose file is gone starts fresh). Shells start in their last folder and show the previous output dimmed above a `── restored ──` line.
- Per project: open files, active file, active tab, recent files, last used launcher. Per window: selection, splits, sidebar and panel state, size and position.

## Files and folders

| What | macOS | Linux | Windows |
|---|---|---|---|
| Settings, keybindings | `~/.config/nucli` | `$XDG_CONFIG_HOME/nucli` | `%APPDATA%\Nucli` |
| State, restored terminal output | `~/Library/Application Support/Nucli` | `$XDG_DATA_HOME/nucli` | `%LOCALAPPDATA%\Nucli` |
| Conversation cache | `~/Library/Caches/Nucli` | `$XDG_CACHE_HOME/nucli` | `%LOCALAPPDATA%\Nucli\cache` |

The development build uses `nucli-dev` / `Nucli Dev` instead.

## Import from Nucli 1

On macOS, when `settings.json` does not exist yet and the `uk.polishch.nucli` defaults do, the first launch of a release build imports agent profiles, shells, appearance and terminal options, workspaces and projects (path, name, color). Tabs, scrollback and keybindings are not imported.

## Privacy

No network calls, no telemetry. Nucli runs local processes (the user's shells, agents and `git`), reads agents' local conversation and status files, and stores only the files listed above.
