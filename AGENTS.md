# Nucli

Nucli is a cross-platform terminal built for AI coding agents. It runs on macOS, Linux and Windows. It is AI-first but it is **not a harness**: it never wraps, proxies or re-implements an agent. It launches whatever CLI the user configures (Claude Code with different profiles, Codex, pi, opencode, …) inside a real terminal, and gives it a project-oriented UI.

The terminal is the editor: Nucli organizes agents by project, shows their status, and lets the user read, review and touch up the files they change without leaving the window.

The full behavior is specified in `docs/SPEC.md`. Keep it in sync with the code.

## Product rules

- **Terminology**: a **project** is one folder (fixed path) with any number of tabs and a terminal panel. A **tab** runs an agent or a shell. A **workspace** is a named window with its own projects (e.g. Personal, Work); one window = one workspace.
- **Left side has two panes**: the sidebar (Projects, Agents, History) and the panel (Files, Changes, History of commits). Both can be hidden; the panel can sit on either side of the viewport.
- A project path **never follows a shell's cwd**.
- **Remember everything**: the app recalls the user's last choice so a repeated action is one keystroke (New Tab… then `Enter`).
- **Every command is in the command palette.** macOS also gets a native menu bar; Linux and Windows rely on the palette and the in-window controls.
- **Settings are files**: `settings.json` and `keybindings.json`, opened in Nucli's own file view and reloaded live. The settings page edits the same `settings.json`.
- Every pane is hideable except the main viewport.
- Keybindings: see `docs/KEYBINDINGS.md`. Keep it updated when shortcuts change.

## Look

- Square corners everywhere. No rounded rectangles, shadows, blur, glass or gradients.
- One monospace font for the whole UI and the terminal: the bundled Maple Mono NF. Icons are its Nerd Font glyphs, never images.
- Flat panes separated by 1px rules. Dense rows on a character grid.
- Text tabs (`FILES  CHANGES  HISTORY`); the active tab, the selected row and mode labels are inverted blocks.
- All colors come from the active theme. Retro, terminal-native feel: block cursor, character spinners, bracketed key hints like `[enter]`.

## Tech

- Rust (edition 2024) and GPUI. Cargo, one crate. Build: `cargo build`; run: `cargo run`. A debug build is the development variant: it is called "Nucli Dev" and uses its own config, data and cache folders, so it is safe to run next to an installed Nucli.
- Terminal: `alacritty_terminal` for emulation, `portable-pty` for the pseudo terminal.
- Git: shell out to `git`. File watching: `notify`. Syntax highlighting: tree-sitter.
- Persistence: JSON files in the platform's config and data folders (see `docs/SPEC.md`).
- Packaging: `scripts/package.sh [target]` builds the release archive for a platform into `dist/` (`.dmg`, `.tar.gz`, `.zip`). `scripts/icon.py` regenerates the app icons. CI builds macOS, Linux and Windows (`.github/workflows/build.yml`).
- Architecture and file map: `docs/ARCHITECTURE.md`.

## Code rules

- English only everywhere.
- No code comments. Explanations go into `docs/` or the commit message.
- Match the surrounding code: small files, plain structs, no speculative abstractions.
- `cargo fmt` and `cargo clippy` clean.
- UI on the main thread. Git, file I/O and process inspection on the background executor, results applied on the main thread.
- Platform differences live behind `cfg` in the smallest possible function, never as parallel copies of a feature.
- Prefer GPUI and std features over custom code, and existing dependencies over new ones.
- Conventional Commits, one logical change per commit.
- Knowledge about how the app works goes to `docs/`: what, why, and gotchas. Describe the current state only, no history.

## Legal (mandatory)

`LEGAL.md` must always be followed. **Before** any change with possible legal impact (adding or upgrading a dependency, bundling fonts, icons, images, sounds or color schemes, copying code or assets from elsewhere, adding telemetry or network calls, changing the license) update `LEGAL.md` first and verify the license is compatible. If unsure, stop and ask.
