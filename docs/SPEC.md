# Nucli specification

How Nucli behaves, from the user's point of view. The code follows this document; when behavior changes, this document changes in the same commit.

## Platforms

macOS 12+, Linux (X11 and Wayland) and Windows 10+. Everything below applies to all three unless a section says otherwise. Shortcuts are written with the macOS keys; `docs/KEYBINDINGS.md` lists the Linux and Windows defaults.

## Window

One window per open workspace. From top to bottom:

1. **Title bar**: the workspace name in plain bold text on the left (click: workspace menu), the location in the middle, status messages on the right (see Title bar). On Linux and Windows the File, Edit, View, Go and Window menus follow the workspace name, since those systems have no global menu bar; File also holds Settings, About and Quit. On macOS the traffic lights sit at the left edge of this bar. On Windows this bar replaces the system title bar: it moves the window, and minimize, maximize and close sit at its right edge. On Linux window controls sit there when the compositor asks the app to draw them.
2. **Body**: the viewport with docks on its four sides (see Docks). The viewport holds the selected tab or file, the status chips and the terminal panel. By default the left dock holds Projects, Agents and History, and the right dock holds Files, Search, Changes and History of commits.

Every panel can be collapsed; the viewport cannot. With every panel collapsed the viewport fills the window except for thin strips.

## Look

- Square corners, 1px rules between panes, no shadows or blur.
- The bundled Maple Mono NF for the whole interface at `ui_font_size`. Terminals and the file view use `font` (Maple Mono NF by default, or any installed font, with Maple Mono NF as the fallback for missing glyphs) at `font_size`.
- Icons are Nerd Font glyphs, colored by the theme. File types and known agents get their own glyph and color.
- View tabs in panels are a glyph and a name. The active tab, the selected row, focused list items and mode labels are inverted: accent background, background-colored text.
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
- Switch windows with the workspace popup (click the workspace name in the title bar), the command palette, or the system window switcher. While Uplink is on, the popup also lists the workspaces of each connected paired computer under its name (see Remote workspaces).
- **Closing a workspace window closes the workspace**: its tabs are saved like on quit (conversations and terminal output included), the processes end, and the workspace is marked closed. It asks first only when processes are running or files are unsaved.
- A closed workspace reopens from the workspace popup or the palette ("Reopen Workspace: Work"). Its tabs come back as they were.
- Rename Workspace… and Delete Workspace… live in the workspace popup and the palette. Delete confirms, ends the workspace's processes and removes its projects.
- Quitting keeps every workspace and reopens the ones that were open. When every window was closed, the last used workspace opens.
- The window that was focused last comes up in front.

## Projects

- A project is a folder. Open one with `⌘O`, by dropping a folder on the window, from a History entry, by opening a tab in a new folder (`⌥⇧⌘T`), by dropping it on the Dock icon on macOS, or from the command line with `nucli <folder>`.
- Projects list in the Projects view in a user-defined order (drag to reorder). Each row shows the folder glyph, the name and the uncommitted diff against `HEAD` as green `+N` and red `-M` (hidden when clean or not a git repository). The numbers update live. A collapsed project also shows the status of its hidden tabs: waiting first, then working, then done.
- Rename (double-click or context menu; empty restores the folder name), Color (12 theme colors or none; tints the folder glyph and the rows of its tabs), Move to Workspace (another workspace or a new one), Reveal in File Manager, Copy Path, Close Project (`⇧⌘W`, ends its tabs after confirming running agents).
- Collapsing a project hides its tabs; the state is remembered.
- Clicking a project shows its active file when one is open, otherwise its overview. `⌂` in the file view's tab strip goes to the overview.
- **Without a project**: a project is optional. Clicking the empty space of the Projects view deselects, and with nothing selected new tabs and panel terminals open in the home folder. Those tabs list below the projects with no header, in line with the project rows. They have no Files, Changes or quick open, and the title bar shows `~` in place of a project name.
- A running tab or panel terminal moves between projects by dragging it onto a project row. Dragging it onto the empty space of the Projects view, its context menu or "Move Out of Project" takes it out of its project. A move never restarts the process or changes its working folder.

### Project overview

Shown in the viewport for a selected project, and after its last tab closes:

- Header: project name, path, git branch.
- NEW: one row per agent profile ("new Claude session", last used first, with `[⇧⌘T]`), one per shell, and "new terminal" in the panel.
- TABS: the project's open tabs with status.
- CONVERSATIONS: the 12 newest agent conversations of this folder, with agent, age and `open` when a tab runs it. Clicking resumes it.
- Reveal in File Manager.

## Tabs

A tab is an agent or a shell living in a project. Agents and shells look the same in the Projects view; only the glyph differs.

- `⌘T`: new tab with the last used shell (fallback: the first shell). Holding it for 100 ms opens the **shell picker** instead.
- `⇧⌘T`: new tab with the agent last used in this project (fallback: last used anywhere, then the first agent). Holding it for 100 ms opens the **agent picker** instead: agents, then the project's recent conversations.
- Pickers open over the viewport with the last used launcher preselected. Digits `1`–`9` pick a launcher directly. Conversation rows resume. "New Shell…" and "New Agent…" open them from the palette.
- **New Tab…** (`+` button, palette, project menu): one picker with agents, shells and conversations.
- `⌥⇧⌘T`: New Tab…, but asks for a folder first (and opens it as a project).
- With nothing selected, the tab opens without a project.
- `⌘1`…`⌘9`: tab by position in this window's Projects view, which shows the number before each tab (`1 claude`).
- `⇧⌘]` / `⇧⌘[`: next / previous tab in Projects view order. `⌃Tab` / `⌃⇧Tab` open the tab switcher.
- Rename (double-click, context menu, palette "Rename Tab"; empty restores the automatic title, which follows the terminal title) and Color (12 theme colors or none; marks the tab's row and split header with a stripe on the left edge and a light wash of the color, and tints its title in the terminal panel).
- `⌘W`: closes the shown file tab if a file is shown, else the current tab. A running agent asks first, unless it printed nothing for 2 hours or "don't ask again for 1 hour" was checked. `⌘W` never closes a project.
- After closing, the next tab of the same project is selected (previous when it was the last), then the project overview.
- A process that exits with code 0 closes its tab. Any other code keeps the tab, marks it red and prints `[process exited with code N]`.
- Tabs keep running when not shown, also in other projects and windows.
- Drag a tab in the Projects view to reorder it or move it into another project (the process keeps running), or onto the viewport to split.

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
- Shown on the right of the tab row: spinner (working), `?` in orange (waiting), `✓` in green with a green title (done), `✗` in red (exited with an error).
- Done and waiting are ignored for the tab the user is looking at, and cleared when the tab is shown.
- A desktop notification is posted when a tab becomes done or waiting and is not the one being watched, if `notifications` is on and the tab notifies: the tab's title, its project, and "Claude finished", "Claude is waiting for input" or "Needs attention" (the bell). On macOS they group by project. Clicking it shows the tab. On macOS the Dock icon shows the number of tabs that need attention; on Linux and Windows the window title does.
- `[x] Notifications` in the context menu of a tab, a project and the workspace (empty space) turns notifications on or off for it. A tab follows its project and a project its workspace until its own switch is used; everything notifies by default. The switches are saved with the workspace and live on the computer that runs the tab, so the same ones apply to other computers and phones watching it over Uplink, which can change them with control.

### Subagents

Running Claude Code subagents of a tab are listed under it in the Projects view (description and type), read from `<config>/projects/<cwd>/<session>/subagents/agent-*.jsonl`. A subagent counts as running until its transcript ends its turn or was not written for 10 minutes. Clicking one selects its tab.

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
- `` ⌃` `` shows and focuses it (starting a shell if the project has none), and hides it when it already has focus. `` ⌃⇧` `` opens another shell. While a panel shell has focus, tab keys act on the panel's shells: `⌘W` closes that shell and focuses the next one, `⌃Tab` / `⌃⇧Tab` and `⇧⌘]` / `⇧⌘[` move to the next / previous shell, `⌘1` … `⌘9` pick one. Hiding keeps the shells running and keeps the tab strip at the bottom of the viewport; clicking a tab or the chevron shows the terminal again. Its height is draggable and remembered.
- Panel shells use the shell last started anywhere, else the first available.

## Splits

- `⌘D` / `⇧⌘D` open a new tab (the last used agent or shell) to the right of / below the focused pane. `⌥⌘W` closes the split; the tab keeps running.
- Only tabs of the same project split together. A tab without a project that joins a project's split moves into that project. Panel terminals do not split.
- Dragging a tab over a pane (from the Projects view or a pane header) previews the resulting layout: the new pane is filled and labeled with its title and the action (`split right`, `swap`, `replace`, plus `· moves to <project>` for a tab without a project), the other panes are outlined. The outer quarter of each edge splits that pane on that side; the center replaces the pane, or swaps the two panes when both are in the split. A band along the outer edge of the whole split places the tab beside all panes at once (`bottom, full width`), so `w1 | w2` over `w3` is one drag. Over a tab of another project the pane turns red and the drop does nothing.
- In a split every pane has a one-line header: glyph, title, status and `×`. The focused pane's header is an inverted block. Dragging a header moves the pane; `×` removes it from the split and the tab keeps running. Right-clicking a header opens the tab's context menu.
- `⌥⌘←→↑↓` move focus. Clicking a pane focuses it. Unfocused panes are dimmed.
- Tabs in a split form a group. The Projects view keeps a group's tabs next to each other in layout order and joins them with a bracket, in the accent color while the group is shown. Selecting a tab of the group focuses its pane; selecting another tab shows that tab alone and keeps the group, which comes back when one of its tabs is selected.
- Hovering a tab row in the Projects view shows a split button (adds the tab next to the focused pane, along the pane's longer side) or, on a grouped tab, an unsplit button. `⌘`-click on a row does the same. Dragging a grouped tab within its group reorders the panes; dragging it out of the group removes it from the split.
- Context menu of a tab: Split with Shown Tab, Remove from Split, Unsplit All.
- Splits are saved with the workspace.

## Docks

- The viewport has a dock on each side: left, right, top and bottom. The top and bottom docks span the width between the left and right docks, and the bottom dock sits under the terminal panel.
- A dock holds stacks side by side: columns in the left and right docks, rows in the top and bottom docks. A stack holds panels, one above the other in a column and side by side in a row. A panel is a tab strip of views and shows its active view.
- Views: Projects, Agents, History (conversations), Files, Search, Changes and History (commits). Each view lives in exactly one panel.
- Drag a view's tab to move it. While dragging, the drop target is filled:
  - a tab: the view goes before that tab;
  - a tab strip or the middle of a panel: the view joins that panel;
  - the top or bottom part of a panel in a column (left or right part in a row): a new panel there;
  - the outer or inner side of a panel: a new column (row in the top and bottom docks) on that side of its stack;
  - an edge of the viewport: a new column or row in that dock, next to the viewport.
- A panel that loses its last view disappears, and so does an empty stack.
- Right-clicking a view's tab offers Move to Left, Right, Top or Bottom (joins the first panel of that dock's innermost stack, or starts the dock) and Reset Panel Layout.
- The chevron at the right of a tab strip collapses the panel. A collapsed panel in a column keeps only its tab strip; in a row it shrinks to a column of glyphs. A stack whose panels are all collapsed shrinks to a strip: glyphs in the left and right docks, view names in the top and bottom docks. Clicking a view in a strip or a collapsed tab strip expands its panel and shows the view; a strip's chevron expands its panels.
- Drag the rule on the inner side of a stack to resize it, and the rule between two expanded panels to share the space between them.
- The layout, the sizes and collapsed panels are shared by all windows and remembered. In a narrow panel only the active tab shows its name.
- Palette: Show Projects, Show Agents, Show Conversation History, Show Files, Find in Files, Show Changes and Show Commit History show a view and expand its panel. "Collapse or Expand Left Dock" (Right, Top, Bottom) collapses every panel of that dock, or expands them when all are collapsed. "Reset Panel Layout" restores the default layout.

## Lists

Projects (the tree), Agents (the board) and History are lists.

- **Projects**: projects and their tabs. Tab rows: number, the agent's glyph and color, title (the terminal's title, or the profile name until there is one), the running program when the title does not name it (`exited` when the process ended), status on the right. While the settings page is open, it shows as the selected row `Settings` with a gear glyph at the end of the list; selecting any other row closes it.
- **Agents**: this workspace's running agent tabs grouped by status: Waiting, Done, Working, Idle. Clicking selects the tab. A remote workspace's window lists that workspace's running agents on its computer.
- **History**: every conversation of every agent profile, newest first (max 150). Clicking resumes it in the project for its folder (opened as a project if needed).
- **Filter** at the bottom of each list filters it by name, path and title; a matching project shows all its tabs; `enter` selects the first match, `esc` clears. `⌥⌘F` focuses the filter of the shown list, or shows Projects when no list is shown.
- **`+` button** beside the Projects filter: New Shell, New Agent, New Tab…, New Tab Without Project…, New Terminal, Open Project….
- Clicking empty space deselects: empty viewport with key hints and "or drop a folder here".
- Context menus: project (New Terminal, New Tab…, Rename, Color, Move to Workspace, Reveal, Copy Path, `[x] Notifications`, `[x] Share over Uplink` while Uplink is on, Close Project), tab (Rename, Color, Split with Shown Tab, Remove from Split, Unsplit All, Move Out of Project, `[x] Notifications`, `[x] Share over Uplink` while Uplink is on, Close Tab), conversation in History (Resume, Copy Title, Copy Conversation ID, Copy Path, Reveal), empty space (Open Project, Rename Workspace, New Workspace, Delete Workspace, `[x] Notifications`, `[x] Share over Uplink` while Uplink is on).

## Project views

Files, Search, Changes and History (commits) follow the current project; with no project they say so.

### Files

- Lazy file tree of the project. Folders first, then files, by name. Git status letters on the right (`M` modified, `A` added, `D` deleted, `U` untracked); folders get a dot (green when everything inside is new, orange when mixed). Ignored files are dimmed. Expanded folders are remembered per project.
- Clicking a file opens it in the file view. Context menu: New File…, New Folder… (a name with `/` creates the folders on the way), Open as Project (folders), Open in Default App, Open With…, Reveal in File Manager, Copy Path, Copy Relative Path, Cut, Copy, Paste, Duplicate (`name copy.ext`), Rename…, Move… (a path relative to the project), Delete (to the trash, confirmed). Empty space: New File…, New Folder…, Paste, Reveal, Copy Path. New files open in the file view; renamed and moved files keep their open tabs, unsaved edits included; deleted files close theirs. A name that is taken gets ` copy`, ` copy 2`, … when pasting.
- Drag and drop: dragging a row onto a folder (or onto a file, meaning its folder, or onto empty space, meaning the project root) moves it there, `⌥` copies. Files dropped from the file manager are copied into the folder under the pointer. A row dropped on a terminal, the commit message or any other text field types its path; on the file view it opens; a folder dropped on a list opens as a project. A row or file tab dragged out of the window becomes a file for other apps: file managers and mail take the file, apps that only take text get its path. This works on macOS, Windows and Linux under X11, not under Wayland.
- Updates live.

### Search

- `⇧⌘F` (Find in Files) shows Search and focuses its query; `⇧⌘H` (Replace in Files) also opens the replace field. Text selected on one line in the file view seeds the query.
- Searches the project's files as you type: the files `git` knows plus untracked ones that are not ignored, or a capped folder walk outside git. Binary files, files over 4 MB and files that are not UTF-8 are skipped. At most 20000 results.
- Toggles, remembered between sessions and separate from the file view's: match case, whole word, regular expression (with lookaround and backreferences; a pattern containing `\n` matches across lines), preserve case.
- `…` shows "files to include" and "files to exclude": comma separated globs (`*`, `**`, `?`, `[…]`, `{a,b}`). A pattern matches anywhere in the path unless it starts with `./` or `/`, which anchor it to the project folder; a folder name matches everything inside it. Both are remembered.
- The line under the fields counts results (`12 results in 4 files`) and has `↻` refresh, collapse or expand all, and clear.
- Results are grouped by file: glyph, name, folder and count. A match row shows its line with the match highlighted and its line number. Clicking a file row collapses it; clicking a match opens the file at the match with the match selected. `F4` / `⇧F4` open the next or previous match. Results refresh when files of the project change while Search is shown.
- Replace: with the replace field open, matches show the removed text struck through in red and the replacement in green. Regular expressions expand `$1`, `$&`, `${name}`, `\n`, `\U` and the other templates of the file view's replace. Hovering a match offers replace and dismiss; hovering a file offers replace all in the file and dismiss. Replace All (`⌘enter`) asks first. Files with unsaved edits in the file view are skipped and the count says so.
- Context menu of a file row: Replace All in File, Dismiss, then the file items of Files. File rows can be dragged like Files rows.

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
- A path bar under the tab strip: breadcrumbs (the folders, the file, then the symbols around the cursor, like `Thing › stop`) and buttons for previous/next change, Open in Default App, Open With…, Reveal. The file's data is on the status chips.
  - Clicking a folder or the file opens a list of that folder under the pointer: `..` (up to the project root), its folders, then its files, with the current one selected; typing filters. A folder goes into it, a file opens (`⌘enter` opens it in the default app). Clicking a symbol lists the file's symbols. `⇧⌘.` (Browse Folder of File) opens the file's folder list from the keyboard.
  - `breadcrumb_icons` shows folder, file and symbol glyphs, `breadcrumb_symbols` the symbols; both are on by default (Settings, File View).
- Changed files show the whole file with added lines on a green background and removed lines on a red background, old and new line numbers in the gutter. `F7` / `⇧F7` jump between changes; the first open of a changed file jumps to its first change.
- Long lines do not wrap: the text scrolls horizontally while the gutter stays in place. Vertical and horizontal scrollbars appear when the file overflows and can be dragged.
- Syntax highlighting with tree-sitter grammars for common languages; other files are plain text.
- Images (PNG, JPEG, GIF, WebP, BMP, SVG, ICO) are shown fit to the viewport, never upscaled, on a checkerboard, with pixel and file size. Binary files show their size and an Open in Default App button.
- **Editing**: working tree files are editable in place, diffs included. Removed lines cannot be typed into. `⌘S` saves; the tab shows `●` while unsaved. Undo/redo per file, kept in memory for the whole app session: closing and reopening a file keeps its history unless the file changed on disk in between; saving keeps it too. `enter` keeps the indentation. Closing an unsaved tab asks Save / Don't Save / Cancel; quitting and closing a workspace with unsaved files ask first.
- External changes (an agent writing the file) reload the view unless it has unsaved edits; saving then overwrites. Commits, staging and checkouts reload the diff of every open file too, so a committed file shows no changes right away.
- **Code navigation**, without language servers:
  - `⌘`-hover underlines what can be followed: names, module paths in imports and includes, file paths in strings and comments, and URLs, and shows where it leads under it (`path:line` and that line, or the URL). Nothing is underlined when there is nowhere to go.
  - `⌘`-click or `F12` goes to the definition and selects its name. Imports are followed to the file they name and the definition in it, through re-exports (`export … from`, `pub use`, `__init__.py`): TypeScript and JavaScript (relative paths, `tsconfig.json` and `jsconfig.json` paths and `baseUrl`, `node_modules` with `package.json` and `@types`), Python (relative and package imports), Rust (`crate::`, `super::`, `self::`, workspace crates, `mod name;`), Go (packages of the module in `go.mod`), Java, C and C++ (`#include`), Lua and Ruby (`require`). A dependency name in `package.json` opens that package's `package.json` in `node_modules`. Local variables and parameters go to the nearest declaration in scope; `this.x`, `self.x` and `Self::x` stay in the enclosing type. Other names are looked up in the project's files of the same language, best matches first.
  - Several definitions show a list under the pointer; none shows "No definition found" at the name for a moment. `⌥⌘`-click opens the definition in a new group to the right. URLs open in the browser.
  - `⌘`-click on a definition itself, or `⇧F12`, lists its references: every match of the name in the project with match case and whole word, as `path:line` and the line; picking one jumps there.
  - `⇧⌘O` (Go to Symbol in File) lists the file's classes, functions, methods, types and fields, nested under their container. `⌃G` (Go to Line) takes `line` or `line:column`.
  - Go Back and Go Forward (`⌃-` / `⌃⇧-`, the mouse's back and forward buttons) move through the places left by go to definition, symbols, breadcrumbs and quick open. The Go menu holds these commands, quick open and next/previous change.
- The name under the cursor (or the selected text on one line) is highlighted wherever it appears; the bracket next to the cursor and its partner are boxed. Clicking a line number selects the line, `⇧`-click extends the selection by lines.
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
- **Keep awake**: a coffee cup at the right end, after the status messages. Clicking it (or Turn Keep Awake On or Off in the palette) keeps the computer from sleeping while idle, for every window, until it is clicked again or Nucli quits; on macOS and Windows the display can still turn off. While on, the cup is filled and in the accent color. It is off at launch. macOS runs `caffeinate -i`, Linux `systemd-inhibit --what=idle:sleep`, Windows sets `SetThreadExecutionState`.

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
- **Usage limits** (off by default): `5h 24% 7d 45%`, how much of the plan's 5-hour and 7-day limits the account has used (plus per-model weekly limits such as `7d opus` where the plan has them), green under 50%, orange under 80%, red above, and 0% once a window has reset. Gemini CLI shows its daily quota as `1d`, GitHub Copilot (through pi) its monthly premium requests as `1mo`. Only Claude Code, Codex, Gemini CLI, and pi and opencode signed in with a Claude or ChatGPT plan report such limits; what each one reads is in `docs/USAGE_LIMITS.md`. Turning the chip on first explains that some agents' limits are read through their API with the agent's own sign-in token, and turns it on only after `Turn On`; `Details` opens `docs/USAGE_LIMITS.md` on GitHub and asks again.
- **MCP servers**: `9 mcp 1 used`. Click: the combobox with the called servers first and their call count; `enter` opens the config file that defines the server.
- **Session time**: how long the agent process has been running.

Clicking the model, context, usage limits or session chip opens a menu: agent and model, context, compactions, each limit with its reset time, running time, Open Transcript in Default App, Reveal, Copy Transcript Path, Copy Conversation ID.

The agent chips are read from the agent's own files once per second, never by talking to the agent:

- Claude Code: the transcript of the session in `<config>/sessions/<pid>.json` records the instruction files it loaded, its skill list, the skills it used, its MCP servers and the token usage of every reply. Skill files come from `<config>/skills`, `.claude/skills`, `.claude/commands` and the enabled plugins; MCP config from `.claude.json` and `.mcp.json`. The context window comes from Claude Code's cached model list, a `[1m]` model, or 1M once more than 200k tokens were used, else 200k. Usage limits are the one value read over the network (see `docs/USAGE_LIMITS.md`), and are not shown for a profile that points Claude Code at another API.
- Codex: the rollout of the session (token counts include the context window and the usage limits; before the first reply the limits come from OpenAI's usage endpoint); instruction files follow its lookup (`AGENTS.override.md` or `AGENTS.md` in the Codex folder and from the repository root to the working folder), skills `.agents/skills`, MCP servers `config.toml`.
- pi: the session file's system prompt lists the instruction files and skills; the context window comes from `models.json`; usage limits use the plan sign-in in `auth.json` of the provider of the last reply (Claude, ChatGPT or GitHub Copilot).
- Gemini CLI: the session in `~/.gemini/tmp/<project>/chats`; GEMINI.md files, skills and `settings.json` servers by convention; the daily quota from Google's Code Assist API.
- Qwen Code, opencode, Copilot CLI, Amp, Droid, Cursor Agent, crush and goose: instruction files, skills and MCP servers from the folders each one reads by convention, and the session time; opencode's usage limits use the plan sign-in in its `auth.json`.

In the file view the same row shows data only: line and column (click: Go to Line), language with its file glyph (click: choose another language, Auto Detect goes back, and the choice is remembered per file), `+added -removed` of the file, and the commit for a commit's version.

The row is hidden for projects and settings. An invalid settings or keybindings file shows its error in red on this row, also in those views.

By default the row follows the focused pane or file group. With `status_chips` set to `every_pane` every pane of a split and every file group has its own row, and the error gets a row of its own.

**Moving chips**: drag a chip onto another to put it in that chip's place. The order is shared by every row and stored as chip ids in `chip_order`; chips missing from the list keep their default place after the listed ones.

**Choosing chips**: every chip can be hidden, as in VS Code. Right-click a chip for "Hide <chip>" and the list of all chips with `[x]` / `[ ]`; right-click the empty row for the list alone. The same toggles are on the settings page (Status Chips) and in the palette ("Status Chips…", also in the View menu, `enter` toggles and keeps the list open). Hidden chips are stored as ids in `hidden_chips`: `folder`, `branch`, `changes`, `program`, `uplink`, `skills`, `instructions`, `model`, `context`, `mcp`, `session`, `position`, `language`, `file_changes`, `commit`. The usage limits chip is off until turned on and is stored as `usage_limits`.

## Uplink

Uplink lets computers running Nucli control each other's tabs: run agents on one machine and watch, type into and start them from another. A phone running Nucli Uplink is a paired device too: it shares no tabs and never listens, so it dials the computers it paired with and they never dial it. There is no server. Every computer is a peer that hosts its own tabs and connects straight to the computers it is paired with, over the local network or any private network the user sets up between them (for example a VPN that gives each machine an address).

### Turning it on and off

- Settings › Uplink starts with the **Uplink** toggle (`uplink.enabled`, off by default). Everything else on the page shows only while it is on. "Turn Uplink On or Off" in the palette does the same.
- Off means off: no port is open, nothing is announced, every connection closes and every remote view closes at once.
- On, Nucli listens on TCP port `uplink.port` (47100, the development build 47101) on every network interface and announces itself with multicast DNS (`_nucli-uplink._tcp`, with its name and public key) so devices on the same network find each other. On macOS the system asks once for Local Network access; on Windows the firewall asks once whether Nucli may accept connections.
- The toggle then shows the status (`On as laptop · port 47100 · 1 of 2 devices online`) and the latest event ("laptop removed this device."). Below it are **Devices** (with `connect device`) and **This computer**: name, Share tabs by default, port, the networks it can be reached on, and its fingerprint.
- **Addresses** under This computer lists every network this computer is on, each with a checkbox. Unchecking one (`uplink.excluded_networks`, by interface name such as `en0`) refuses connections that arrive on it before any handshake, stops announcing and looking for devices there, and leaves its address out of the pairing link. Connections that other devices opened are dropped and come back over the remaining networks. Nucli may still reach paired devices through an unchecked network. An unchecked network stays listed while it is down.

### Pairing

Two devices pair once; after that they find and connect to each other by themselves whenever both are on.

- **Connect a device** is a full-window dialog in three steps: connect, confirm, permissions. `connect device` under Devices in Settings › Uplink and "Uplink: Connect a Device…" in the palette open it (the palette turns Uplink on first). `[esc]` closes it at any step.
- **Connect** shows this computer's QR code and `copy pairing link` for a phone or a device on another network, the **Computers nearby** that run Uplink and are not paired yet, each with `connect`, and **Not listed**: a field for a name, an address (`host`, `host:port`, an IP address, also over a VPN) or a pairing link copied on the other device, with `connect` (or `enter`). The code inside the QR code and the link works once and is replaced every 5 minutes and after 3 wrong tries; pasting a computer's own link into it is refused with a hint.
- **Confirm**: connecting needs no code. The device that connects waits ("Waiting for studio"); the other one opens the same dialog by itself, brings its window to the front and asks "laptop wants to connect" with **allow** and **deny**. Both show the same six digits (`123 456`), taken from the encrypted handshake; the dialog says to deny when they are not the same. The question waits up to 2 minutes; `[esc]` on the waiting side cancels and takes the question away on the other side. A link or QR code also ends in this question. Requests from a blocked device are refused without asking, one question is shown at a time, and when two computers connect to each other at once only one of them asks.
- **Permissions**: both devices then list each other under Devices with view, and the dialog asks what the other device can do with this computer's shared tabs and projects: **view** (watch tabs, read files, changes and history) or **control** (also type into tabs, start agents, change files and git). `[enter] done` closes it; Settings › Uplink changes it any time.

### Devices

Each paired device is one row: its system glyph, name, state (`online · address · watching N of your tabs`, `offline · seen 5 minutes ago · not reachable`, `blocked`) and what it **can** do with this computer's shared tabs:

- **control** means typing into shared tabs, resizing them, clearing them and opening new tabs in shared projects. A device that can control a tab can run any command there. **view** means watching only; its keystrokes are ignored.

Clicking a row opens its details: the fingerprint, **also reach it at**, and the buttons below.

- **disconnect** drops the connection (it reconnects later), **block** refuses it until **unblock**, **forget** (confirmed) removes the pairing on this side and, when the device is online, on its side too; the device must pair again to reconnect. A device forgotten while offline is refused when it calls (`it refused this device`) until it is forgotten there too.
- **also reach it at**: names or addresses, comma separated, for networks without multicast. Nucli also remembers the last addresses that worked.

### Sharing

- What a paired device sees is decided on the computer that runs the tabs. **Share tabs by default** (`uplink.share`, on by default): every tab and panel terminal is shared unless excluded. Off: only what is shared explicitly.
- `[x] Share over Uplink` in the workspace menu (the title bar popup and the empty space of the Projects view) shares or excludes the whole workspace; the same item in a project's menu overrides its workspace, and in a tab's menu its project. All three are saved with the workspace. A workspace that is excluded and has no project shared explicitly is hidden from paired devices: they do not see its name and cannot open, rename or start tabs in it.
- Stopping sharing, changing a device to view, blocking or forgetting it applies at once: open remote views of tabs that stop being shared close.
- A tab being watched from another device shows an eye glyph in the Projects view, and the Uplink status chip names the devices watching it.
- A paired device can also ask for more than the tabs of shared projects: their files and file contents (up to 1 MB per file), git changes and diffs, git history and commit details, and the agent conversations that ran in them. This works with view or control. Paths cannot leave the project, symbolic links included.
- With control it can also browse folders and add one as a project (to a given open workspace), open a closed workspace's window, rename a workspace, start a tab outside projects in a given workspace, and resume a conversation of a shared project in a new tab. Paired devices see the names of all workspaces, open and closed, except hidden ones.
- A paired device can download any file of a shared project, whole and in pieces; with control it can upload files into a shared project's folders. An upload never replaces a file: a name that exists gets ` copy`. The receiving side writes to a hidden temporary file next to the target and renames it when the transfer completes, so a broken transfer leaves nothing behind.
- With control it can change shared projects the way the desktop's context menus do: create, rename, move, copy, cut and paste, duplicate and delete files (deleted files go to the trash), open or reveal them on the computer; stage, unstage, discard and commit changes, list, switch and create branches, check out, create branches and tags from, cherry-pick, revert and reset to a commit, delete branches and tags, restore a file from a commit; rename, color, move to a workspace and close a shared project; rename, color, move out of its project and close a shared tab. The computer checks every path and name: paths stay inside the project and branch, tag and commit names cannot be options. Closing a running agent from another device does not ask on the computer; the device asks first.

### Remote workspaces

- Each workspace of a paired computer opens in a window of its own, a duplicate of that workspace on that computer: the workspace popup lists them under the computer's name, and the palette's search lists them as `device › workspace` with their number of shared tabs. Clicking a remote agent's notification opens its workspace's window and selects it. The title bar shows `device › workspace`. These windows are remembered and reopen at launch with their selection, folded projects, terminal panels, open files and splits; while the computer is offline they keep its last state and say `offline`.
- The window lists the computer's shared projects with their tabs and panel terminals, in its order and with its names and colors. Selection, folded projects, the terminal panel and the docks belong to this window. A tab starts streaming when it is first shown.
- A remote tab is the full terminal with its scrollback, live, with selection, copy, find and links. `⌘`-click on a path of a file in the tab's project opens it in this window's file view, read from that computer. The title bar shows `device › project › tab`, the status chips show the device (and `view only` or `offline`) and the same chips as on that computer: folder, branch, changes, program and the agent chips, in this computer's own order and choice. The device chip's menu has Disconnect and Uplink Settings…; the branch chip opens the branch picker of that computer and the changes chip the Changes view. The agent chips come with the catalog: picking a skill or instruction file inside the tab's project opens it in this window's file view from that computer, and the others are listed only. Typing, pasting text, mouse reporting and `⌘K` go to the other computer when this device can control it. With control, files dropped on a remote tab, and files or an image pasted into it (`⌘V`, or `ctrl+v` and `alt+v`, which agents use to paste images) while the clipboard holds no text, are sent to a new temporary folder on the other computer and their paths there are typed, so an agent there gets screenshots taken here. Sending to a computer with an older Nucli shows an alert to update it.
- The terminal has one size that every viewer agrees on, as with `tmux`'s `window-size smallest`: the fewest columns and the fewest rows among the computer's own window, when the tab is visible there, and every device with control that shows the tab. Each viewer draws that grid at the top left and fills the rest of its view with the surface color behind a 1px rule. A viewer that hides the tab, closes its window or goes offline drops out and the size grows to the next smallest; with nobody left the terminal keeps its size. A device with view access never resizes it.
- With control, `⌘T` and `⇧⌘T` open the computer's first shell or agent in the current project, Split Right and Split Down open its first shell beside the current tab, and holding them, New Tab… and the project's New Agent… and New Shell… list all its shells and agents. `⌘W` and Close Tab close the tab on that computer, asking first while an agent runs. Rename…, the color, Move Out of Project and Close Project in the tab and project menus change the computer's tabs and projects. Projects and tabs keep the computer's order and are not dragged.
- Files, Changes and History of commits show the computer's project, as they do locally: the tree with git letters, the changes and the commit graph with each commit's files. With control the Files menu creates, renames, duplicates and deletes there (to that computer's trash), opens or reveals a file on that computer and downloads it; staging, unstaging, discarding, committing and the commit menu's actions run there too. Files dropped from this computer onto a folder of the tree, or onto its empty space, and Upload Files… in the Files menu send them into that folder there. Rows of a remote project are not dragged, since their paths exist only there. Without control the window only shows: the commit box, the staging buttons, the git actions of the menus and the project page's ways to start a tab are hidden.
- Files of a remote project open in the file view with their changes, highlighting and find. With control they are edited and `⌘S` writes them on that computer; without it they open read only. Images are fetched to show them. Go to Definition (`F12`, `⌘`-click) and Find References look up the symbol on that computer; holding `⌘` over a name in a remote file neither underlines it nor previews its definition. Open in Default App, Open With…, Reveal and the folder breadcrumbs are local only and stay off for remote files.
- Find in Files, Go to File (`⌘P`) and the branch picker search and list on that computer, and with control Replace writes there too (a computer with an older Nucli does not replace and says so). History lists the conversations of the window's projects on that computer, and clicking one resumes it there (control) and selects its tab here. Open Project (`⌘O`) browses that computer's folders from its home folder, `..` goes up and `[add <folder>]` adds it to the workspace there. A new terminal in the panel opens one in the project's terminal panel on that computer and shows it here. The project's page lists the computer's shells and agents and its conversations in that folder, and has no Reveal. The empty space menu has Open Project…, Rename Workspace… and `[x] Notifications` for that workspace on that computer. Every other way to start a tab (palette items, launchers) lists the computer's shells and agents instead of this computer's.
- A remote project's context menu also has Download File…, which lists the project's files (those git knows, or every file outside hidden folders when it is not a repository, up to 20000) and saves the chosen one where the save dialog says, and Upload Files… (control only), which sends the chosen files into the project's folder. A transfer shows in the status bar while it runs and an alert if it fails.
- Status follows the other computer once per second: working, waiting, done and exited. When a remote tab starts waiting or rings for attention, a desktop notification is posted (if `notifications` is on and the tab notifies on its computer) titled with the tab and `device · project`; clicking it opens the remote tab. Seeing a remote tab here clears its done mark on its computer too.

### Security

- Each device has its own X25519 key pair, created when Uplink first runs. The private key is kept in the system's credential store (Keychain on macOS and iOS, Credential Manager on Windows, Secret Service on Linux) under "Nucli Uplink"; where none is available, and in development builds on macOS, it stays in `uplink.json` in the data folder, readable only by the user. A key found in `uplink.json` moves into the credential store on the next start. The fingerprint on the settings page is the start of the public key.
- Connecting to pair is a Noise `XX` handshake that accepts any key. Nothing is shared before the user allows the device, and the six digits both devices show come from the handshake hash: a device in the middle runs two handshakes and cannot make their digits match.
- A pairing link or QR code carries a one-time code and the computer's key. The code goes through SPAKE2, a password-authenticated key exchange: it becomes a strong shared key without ever being sent, and someone who listens or answers in the middle cannot test codes offline; each wrong attempt costs one of the 3 tries. That key then authenticates a Noise `XXpsk3` handshake in which both devices exchange and prove their keys.
- Every later connection is a Noise `XX` handshake (X25519, ChaCha20-Poly1305, BLAKE2s). Both sides check the other's key against their paired devices: a connection from an unknown or blocked key is dropped as soon as its key is known, before any data is exchanged, and a device answering at a known address with a different key is refused. All traffic is encrypted and authenticated; a tampered or replayed message closes the connection.
- The computer that runs a tab enforces every rule itself: what is shared, view or control, blocking. A device can only open tabs with the agent and shell profiles already configured there, in projects that are shared.
- Limits keep a misbehaving peer from using up resources: 16 handshakes at a time, 10 seconds per handshake, 64 nearby devices, 16 MB per message, a terminal size of at most 500 × 200, a remote view that opens midway gets at most 8 MB of the newest scrollback, and output for a slow viewer is dropped and resent as a fresh copy of the screen once 8 MB wait.

## Nucli Uplink

Nucli Uplink is the phone app: a remote control for the computers running Nucli. It runs no terminals of its own; everything it shows runs on a paired computer, over Uplink. Look and wording follow the desktop app: one monospace font, square corners, inverted blocks for the active choice, bracketed buttons.

- **Welcome**: on first launch, the animated pixel logo, three steps (run Nucli, turn on Uplink, pair this phone) and `[get started]`, which opens Add a computer.
- **Add a computer** (also `add computer` on the computers list) pairs by any of:
  - **computers nearby**: computers on the same network that run Uplink and are not paired yet (multicast DNS). Tapping one connects to it without a code.
  - `[scan qr code]`: a full-screen camera view that closes as soon as it sees a Nucli pairing code; tap to cancel. The computer shows the code in Connect a device. The system Camera app works too: its link opens Nucli Uplink and pairs.
  - `[paste pairing link]`: reads the link from the clipboard.
  - **not listed**: the computer's name or address and `[connect]`.
  - While connecting a spinner shows, then the six digits the computer shows too, with `[cancel]` (going back cancels too). The computer asks whether to allow this phone; an unreachable computer or a refusal shows why. When allowed, the new computer's screen opens.
- After pairing, the phone connects to each computer by itself whenever both are on and reachable: addresses it finds on the network first, then the ones under "also reach it at", then the ones that worked before. Computers never dial the phone.
- Every screen below the computers list has a back button at the top left; swiping from the left edge goes back too, and the screen follows the finger. A new screen fades in with a short slide from the right, or from the left when going back; a tab's content fades in when the tab changes.
- **Computers** (the home screen): each paired computer with its system, its projects and agents, how many agents are working and waiting, or why it is offline and when it was last seen. `add computer` opens Add a computer; the gear opens Settings.
- **Computer**: its name, a details button (Computer details), its open workspaces, each with its number of projects, its agents and shells and how many wait or work (tap: Workspace), then its closed workspaces under `closed`; with control, tapping one opens its window on the computer and then its Workspace screen.
- **Workspace**: its name and three tabs like the desktop's Projects, Agents and History views, the active one inverted, each limited to this workspace:
  - `projects`: the workspace's shared projects, each with its agents and shells and how many wait or work (tap: Project; hold: the project's menu), then its shared tabs outside projects as plain tab rows, then (control only) `new agent` and `new shell`, which start a tab outside projects in this workspace like on the Project screen, and `add project`, which opens a folder browser that starts in the computer's home folder: tap a folder to go in, `..` to go up, `[add <folder>]` to add it as a project to this workspace and open it.
  - `agents`: the workspace's agents grouped like the desktop Agents board (waiting, done, working, idle), with their project and status.
  - `history`: the agent conversations in the workspace's shared projects, newest first, with project, profile and age. `resume` (control only) resumes one on the computer and opens its project.
- **Project**: the project's path, then one list: `new agent` and `new shell` (control only), which start the agent profile or shell last picked on the phone (fallback: the computer's first) in the project and open it; holding either lists the computer's agent profiles or shells to pick from, and the project's tabs in the computer's order with branch and status (tap to open). The `files`, `changes` and `history` buttons at the top open a drawer from the right, like the desktop's Files, Changes and History views; it slides in over the dimmed project; tapping outside it or `x` closes it, and back closes it first.
  - `files`: the project's folders and files; tap a folder to go in, the top row to go up, a file to view it with line numbers (up to 1 MB; binary files are not shown). With control, `new file or folder` opens the folder's menu. `download` saves a file in the app's Documents folder, shown by the Files app under Nucli Uplink; `upload files` opens the system file picker and sends the picked files into the folder. A running transfer shows its name and percentage at the bottom.
  - `changes`: the branch (tap to switch or create one), with control a commit message and `[commit]`, and the staged and unstaged changes with status letter and added and removed lines; tap one for its diff, `+` or `−` to stage or unstage it; the section headers stage or unstage all, and discard all unstaged changes.
  - `history`: the commits with refs, subject, short hash, author and age; tap one for its message, date, stats and changed files, and a file for its diff in that commit.
- **Terminal**: tapping a tab opens it live. With control, the phone takes part in the size agreement only while its keyboard is open: then its size at the phone's font size counts, following the screen and the keyboard. With the keyboard closed, and always on a phone that can only view, it shows the agreed size scaled to fit the screen width. Dragging scrolls the scrollback (or sends arrow keys to full-screen apps that ask for them). The phone keyboard's return types a new line (shift+enter); the send key submits (enter). The key bar scrolls sideways: `ctrl` (applies to the next key), `esc`, `tab`, `⇧tab`, the arrows, `home`, `end`, `pgup`, `pgdn`, `/ | ~ - ` \` and paste; send and the keyboard toggle stay pinned at its right end. It hides when the phone can only view. The title bar shows the tab, `device · project` and its status, or `offline`, `closed` or `view only`. Under the terminal a row of status chips, as on the computer, scrolls sideways: folder, branch, changes, program, skills, instruction files, model, context, usage limits (when the computer shows them), MCP servers and session time.
- **Menus**: with control, holding a row opens its menu at the bottom, the same actions as the desktop's context menus (right click in the desktop window): files and folders (new file, new folder, download, upload files, open as project, open or reveal on the computer, copy path, cut, copy, paste, duplicate, rename, move, delete), changes (open changes, stage, unstage, discard), commits (switch to or check out its branches, check out detached, create a branch or tag, cherry-pick, revert, reset soft, mixed or hard, delete its branches and tags, copy hash, short hash or message), files of a commit (open changes, open the current version, restore this version), tabs (rename, color, move out of the project, `[x] notifications`, close), projects (new tab with any agent or shell, rename, color, move to a workspace, reveal, copy path, `[x] notifications`, close), workspaces (rename; a name another workspace has is refused; `[x] notifications`) and conversations (resume, copy title, id or path; these work without control too). With control, swiping a tab, project or file row to the left shows `close` or `delete` at its right end; tapping it, or swiping past half the screen, acts, and tapping anywhere else or swiping back hides it. Names are typed in a prompt at the bottom; actions that lose work (delete, discard, reset hard, delete a branch or tag, restore, close a running agent, close a project) ask first. Copy actions put text on the phone's clipboard; cut and copy remember a file for paste on the same computer. Failures show at the bottom for a few seconds.
- **Keyboard**: on iPhone and iPad the keyboard (on screen or attached) types into text fields and the terminal: text, return and backspace; the key bar covers the other keys. In the desktop window the arrow keys and tab move between the buttons, tabs, rows and fields of the screen, and the list scrolls to keep the focused one in view; enter or space opens it. The focused one is highlighted only while the keyboard is in use. With the drawer open, focus stays in the drawer. Escape or ⌘[ goes back; in a focused terminal escape goes to the terminal. A tab opened from the keyboard takes the keyboard at once, and ⌘V pastes into it.
- **Computer details**: state and address, number of shared tabs, system, fingerprint, when it was paired, "also reach it at" (names or addresses, comma separated), `[disconnect]` and `[forget]` (tap twice; the computer forgets the phone too when online).
- **Notifications**: when a tab on a paired computer starts waiting for input or finishes (done, the bell, an agent's own notification), the phone shows a notification: the tab's title, `computer · project` and the text, grouped by computer and project, with sound. A newer one for the same tab replaces the older. None is shown for the tab open on the phone while the app is in front. Tapping one opens the tab. The workspace, project and tab switches of the computer apply (the `[x] notifications` items in the menus change them).
- **Settings**: this phone's name (shown on computers), its fingerprint, appearance (system, light, dark), theme (the desktop themes), `[x] notify this phone` (on by default), terminal font size, the computers, and the version.
- iOS suspends apps in the background. With notifications on, allowed and at least one computer paired, Nucli Uplink keeps running in the background by playing silence (iOS's background audio mode, mixed with other audio), so it stays connected and notifies while the phone is locked. It costs some battery. A phone call or another app that takes the audio stops it until the app is opened again. Otherwise the phone is connected only while the app is open. The system asks for permission to notify once a computer is paired.

## Settings

- `⌘,` shows the settings page in the viewport, with vertical tabs by category. The last category is remembered. Every change applies at once and is written to the same files. `esc` or selecting a tab or file closes it.
  - **General**: reopening tabs, notifications, link to `settings.json`.
  - **Appearance**: theme cards, Import… and Open folder for custom themes, system, light or dark mode, interface size.
  - **Terminal**: font, font size, line height, ligatures, cursor.
  - **File View**: breadcrumb icons and symbols. The font is a combobox: Maple Mono NF first, then every installed font by name, each row with a sample in that font; typing filters, arrows and `enter` pick, `esc` or clicking outside closes.
  - **Agents**: name, executable, arguments and config folder of each profile, add, remove, detect. An agent's `env` and `theme` are only in the file.
  - **Shells**: name and path, add, remove, detect.
  - **Status Chips**: under the focused pane or every pane, and a `[x]` toggle for every chip, grouped by Agent, Terminal and File view.
  - **Uplink**: see Uplink.
  - **Keybindings**: every command with its key, a filter, and a link to `keybindings.json`. Click a key and press the new one (`esc` cancels); a changed key has a reset button; a key used twice in the same context turns red. "Open Keybindings" opens this category.
- "Open Settings File" and "Open Keybindings File" open the JSON files in the file view. Both reload when saved, from Nucli or any editor. Invalid JSON keeps the previous settings and shows the error on the status row.
- `settings.json` keys: `theme`, `appearance` (`system`, `light`, `dark`), `font`, `font_size`, `ui_font_size`, `line_height`, `cursor` (`block`, `bar`, `underline`), `cursor_blink`, `ligatures`, `breadcrumb_icons`, `breadcrumb_symbols`, `restore_tabs`, `notifications`, `status_chips` (`focused`, `every_pane`), `hidden_chips`, `chip_order`, `usage_limits`, `agents`, `shells`, `uplink` (`enabled`, `name`, `port`, `share`, `excluded_networks`). Commands that change settings (theme, font size, detection, notifications) rewrite the file with all keys.
- `keybindings.json` maps a key to a command name, e.g. `{"cmd-t": "new_shell", "cmd-shift-d": null}`; `null` removes a default. The palette shows every command's name and current key.
- **Themes**: Nucli (dark and light pairs following the system), Nucli Amber and Nucli Phosphor (monochrome dark), and Vitesse Dark, Dark Soft, Black, Light, Light Soft. A theme sets every UI color, the 16 terminal colors and the syntax colors. An automatic theme follows the system appearance; `appearance` can force its light or dark side.
- **Custom themes**: every `*.json` file in the `themes` folder next to `settings.json` is a theme `{name, base, dark, background, foreground, accent, selection, ansi, syntax}`. Every key is optional. `name` defaults to the file name without `.json`; it is also the id that `theme` and an agent's `theme` refer to. `base` is a built-in single theme id (e.g. `vitesse-dark`, not the `vitesse` pair) whose colors fill every missing key; without it the base is Nucli Dark, or Nucli Light when `dark` is `false`. Colors are `#rrggbb`. `ansi` lists the 16 terminal colors; `syntax` lists 8 colors in this order: keyword, string, number, comment, type, function, constant, attribute. The other UI colors are derived from these, as for the built-in themes. Custom themes appear after the built-in ones in Select Theme, the settings page and onboarding, and reload live when a file is added, changed or removed. A broken file is skipped and its error shows on the status row. **Import Theme…** (command palette and the Appearance page) validates the chosen JSON files, copies them into the folder (same name overwrites) and selects the last one; **Open Themes Folder** shows the folder in the file manager.
- **Shells**: `{name, path}` list. Filled during onboarding from the login shell and `/etc/shells` (Windows: PowerShell, Windows PowerShell, Command Prompt, Git Bash and WSL when installed); "Detect Shells" adds missing ones.

## Restore

- When `restore_tabs` is on, quitting, also when the app is asked to terminate on Linux and macOS, saves every tab: kind, profile or shell, name, color, conversation id, folder, and the last 5000 lines of terminal output.
- On launch tabs reopen without stealing focus, then each window selects its saved tab. Agents resume their conversation (a Claude conversation whose file is gone starts fresh). Shells start in their last folder and show the previous output dimmed above a `── restored ──` line.
- Per project: open files and their split groups, active file, active tab, recent files, last used launcher. Per window: selection, splits, terminal panel height, size and position. For all windows: the dock layout.

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
