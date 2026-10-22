# Nucli

Nucli is a native macOS terminal built for AI coding agents. It is AI-first but it is **not a harness**: it never wraps, proxies or re-implements an agent. It launches whatever CLI the user configures (Claude Code with different profiles, Codex, pi, opencode, …) inside a real terminal, and gives it a project-oriented UI.

Think "VS Code for agents without the code editor": the terminal is the editor.

## Product rules

- **Terminology**: a **project** is one folder (fixed path) with any number of agent tabs and a terminal panel. A **workspace** is a named window with its own projects (e.g. Personal, Work); one window = one workspace.
- **Left side has two panes**: the sidebar with projects and agent tabs (always there unless hidden with `⌘B`), and next to it a panel with tab buttons for Files, Changes and Git History (`⌃⌘1`, `⌃⌘2`, `⌃⌘3`; hide with `⌥⌘B` or the toolbar button).
- **Projects**: vertical tabs in the sidebar of their workspace window. Any number of agents live inside a project. The sidebar lists projects and agents only, no workspace rows; the workspace name sits next to the traffic lights and in the popup at the bottom of the sidebar. Projects and tabs (agents, shells, panel terminals) can be renamed and colored, workspaces renamed; projects move between workspaces with Move to Workspace.
- **Workspaces** are windows: `⌘N` creates one, `` ⌘` `` switches. Closing a workspace window asks to delete it (agents and terminals are terminated). Quitting keeps every workspace and reopens them.
- **Terminals** live in a VS Code style panel under the viewport, one per project, with horizontal tabs (`` ⌃` `` toggle, `` ⌃⇧` `` new). The panel belongs to the project: every agent of the project shows the same terminals. Hiding it keeps the shells running.
- A project path **never follows a shell's cwd**. `cd ..` in a tab does not move the project.
- **Shells** (Settings › Shells: zsh, bash, …) open as sidebar tabs next to agents, through the same `⌘T` / `⇧⌘T` flow. They are separate from the terminal panel.
- `⌘T` starts a new tab with the last used agent or shell. `⇧⌘T` opens the picker (agents, then shells); the preselected agent is the one last used in that project (fallback: last used globally). With no project selected, the tab opens without a project. Hold `⌥` (`⌥⇧⌘T`) to always ask for a folder.
- **Remember everything**: the app must recall the user's last choice so a repeated action is one keystroke (`⇧⌘T` then `Return`).
- **Changes** panel view shows live `git diff` against `HEAD` (staged + unstaged + untracked) for the project. Clicking a file opens the full file with `+`/`-` lines in the main viewport.
- **Files** panel view shows the project's file tree. Clicking a file opens it in the main viewport (with `+`/`-` lines when it has changes). Code is syntax-colored and files have colored type icons.
- Every pane is hideable except the main viewport. With all panes hidden the viewport fills the window.
- Design is decided by Apple: AppKit, system controls, system colors, SF Symbols, SF Mono. No custom chrome.
- Modern keybindings: see `docs/KEYBINDINGS.md`. Keep it updated when shortcuts change.

## Tech

- Swift, AppKit only (no SwiftUI, no Electron, no web views). Target macOS 14+, built with the macOS 26 SDK (Xcode 26).
- Swift Package Manager. No Xcode project. Build: `swift build`. App bundle: `./scripts/bundle.sh` → `build/Nucli.app`. Run: `open build/Nucli.app`.
- Terminal emulation: [SwiftTerm](https://github.com/migueldeicaza/SwiftTerm) (`TerminalView` + `LocalProcess`, driven by `Session`).
- Git: shell out to `/usr/bin/git`. File watching: FSEvents.
- Persistence: `UserDefaults` (domain `uk.polishch.nucli` when run from the bundle).
- Architecture and file map: `docs/ARCHITECTURE.md`.

## Code rules

- English only everywhere.
- No code comments. Explanations go into `docs/` or the commit message.
- Match the surrounding code: small files, plain classes, no speculative abstractions.
- All UI on the main actor. Git and file I/O on background queues, results applied on main.
- Prefer AppKit/Foundation features over custom code, and existing dependencies over new ones.
- Conventional Commits, one logical change per commit.
- Knowledge about how the app works goes to `docs/`: what, why, and gotchas. Describe the current state only, no history.

## Legal (mandatory)

`LEGAL.md` must always be followed. **Before** any change with possible legal impact — adding or upgrading a dependency, bundling fonts, icons, images, sounds or color schemes, copying code or assets from elsewhere, adding telemetry or network calls, changing the license — update `LEGAL.md` first and verify the license is compatible. If unsure, stop and ask.
