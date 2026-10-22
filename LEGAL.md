# Legal

Everything with legal impact in Nucli is tracked here. Update this file **before** making such a change (see `AGENTS.md`).

## Project license

MIT (`LICENSE`). Anyone may use, change and redistribute the code as long as the copyright and license notice stay with it. Compatible with every dependency below (MIT / Apache-2.0). `scripts/bundle.sh` copies it to `Contents/Resources/LICENSE.txt`.

Builds are ad-hoc signed and not notarized.

## Third-party dependencies

| Component | Version | License | Used for | Obligations |
|---|---|---|---|---|
| [SwiftTerm](https://github.com/migueldeicaza/SwiftTerm) | 1.99.0 | MIT | Terminal emulation | Keep copyright and license notice in distributions. |
| [swift-png](https://github.com/tayloraswift/swift-png) | 4.5.1 | Apache-2.0 | Transitive, SwiftTerm on non-Apple platforms only, not linked into Nucli on macOS | None for the app binary; notice bundled anyway. |
| [h](https://github.com/rarestype/h) | 1.0.1 | Apache-2.0 | Transitive, swift-png on non-Apple platforms only, not linked into Nucli on macOS | None for the app binary; notice bundled anyway. |
| [swift-argument-parser](https://github.com/apple/swift-argument-parser) | 1.8.2 | Apache-2.0 | Transitive, SwiftTerm package only, not linked into Nucli | None for the app binary. |

Build-time only, not bundled: [create-dmg](https://github.com/create-dmg/create-dmg) (MIT) creates the release DMG in CI.

Exact versions live in `Package.resolved`. When a version changes, update this table.

`scripts/bundle.sh` copies the LICENSE and NOTICE of SwiftTerm, swift-png and h, plus the Vitesse license, into `Contents/Resources/Notices/`. Nucli › About Nucli shows their full text in the credits. Add new dependencies to both.

## Fonts, icons, colors

- Fonts: system fonts only (SF Pro, SF Mono via `NSFont.systemFont` / `monospacedSystemFont`). Nothing bundled. Apple fonts must not be redistributed inside the app.
- Icons: SF Symbols referenced by name at runtime, nothing bundled. Apple's SF Symbols license forbids using symbols in app icons or logos, so the app icon must be original artwork. `Resources/AppIcon.icns` and the README banner `docs/banner.png` are original artwork made of plain geometric shapes: no fonts, no symbols, no third-party artwork. The same applies to the DMG window background `Resources/dmg-background.tiff`.
- Colors: AppKit system colors, plus the built-in Nucli terminal palette in `Sources/Nucli/Theme.swift`, made from Apple's published system color values. Do not copy third-party terminal schemes (Dracula, Solarized, …) without adding their license here first.
- Vitesse color themes (Dark, Light, Black, Dark Soft, Light Soft) by Anthony Fu, https://github.com/antfu/vscode-theme-vitesse, MIT License (Copyright (c) 2020 Primer, Copyright (c) 2021 Anthony Fu). Nucli reuses only color values in `Sources/Nucli/Theme.swift`. The notice is kept in `Resources/Vitesse-LICENSE.txt`, bundled and shown in About Nucli.
- Agent icons: if the vendor's own macOS app is installed on the user's machine, Nucli shows that app's icon, loaded at runtime through `NSWorkspace.icon(forFile:)`, the same way Finder or Spotlight show installed apps. The icon only identifies that vendor's agent next to its name. Nothing is bundled, copied or modified. Otherwise a generated monogram badge is drawn. Bundling vendor logos is not allowed without a new entry here.

## Third-party trademarks

Agent names (Claude, Codex, pi, opencode, …) are plain command names of tools the user installs and configures. Nucli is not affiliated with or endorsed by their vendors. It does not bundle or modify their logos; an installed vendor app's own icon may be shown at runtime (see Agent icons). All trademarks belong to their owners.

## Network and data

Nucli makes no network calls and collects no telemetry. It runs local processes (`$SHELL`, `/usr/bin/git`) and stores settings in `UserDefaults`, a conversation title cache in `~/Library/Caches/Nucli` and restored terminal output in `~/Library/Application Support/Nucli/Scrollback`.

It reads, never writes, files that agents store locally, only to show them in Nucli: conversation files (`~/.claude*/projects`, `~/.codex/sessions`, `~/.pi/agent/sessions`) for titles, ids and folders, and Claude Code's per-process status files (`<config dir>/sessions/<pid>.json`: session id and status) for tab status and resume. It never reads the `<pid>.<hash>.key` files next to them, which hold secrets. Nothing leaves the machine. The formats of these files belong to their vendors and may change. Any change to this section needs a privacy review.
