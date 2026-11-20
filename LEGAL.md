# Legal

Everything with legal impact in Nucli is tracked here. Update this file **before** making such a change (see `AGENTS.md`).

## Project license

MIT (`LICENSE`). Anyone may use, change and redistribute the code as long as the copyright and license notice stay with it. Every bundled dependency and asset below is compatible with it.

Builds are not code signed for distribution (macOS builds are ad-hoc signed, not notarized; Windows builds are unsigned).

## Rust dependencies

Direct dependencies, declared in the workspace's `Cargo.toml` files (`Cargo.toml`, `core/Cargo.toml`, `mobile/Cargo.toml`). Exact versions of everything, transitive crates included, live in `Cargo.lock`. When a dependency is added or upgraded, update this table.

| Crate | License | Used for |
|---|---|---|
| [gpui-pre](https://crates.io/crates/gpui-pre), [gpui-pre-platform](https://crates.io/crates/gpui-pre-platform), [gpui-pre-ios](https://crates.io/crates/gpui-pre-ios) (mobile, iOS only) | Apache-2.0 | UI framework: windows, layout, text, input, rendering. A published snapshot of [GPUI](https://github.com/zed-industries/zed/tree/main/crates/gpui) and its platform crates from the Zed repository, pinned to one exact version |
| [alacritty_terminal](https://github.com/alacritty/alacritty) | Apache-2.0 | Terminal emulation (parser, grid, selection, search) |
| [portable-pty](https://github.com/wezterm/wezterm/tree/main/pty) | MIT | Pseudo terminals and child processes on every platform |
| [dirs](https://github.com/dirs-dev/dirs-rs) | MIT OR Apache-2.0 | Platform config, data and cache folders |
| [futures](https://github.com/rust-lang/futures-rs) | MIT OR Apache-2.0 | Channels from terminal threads to the UI |
| [anyhow](https://github.com/dtolnay/anyhow) | MIT OR Apache-2.0 | Error values |
| [uuid](https://github.com/uuid-rs/uuid) | MIT OR Apache-2.0 | Tab and conversation ids |
| [notify](https://github.com/notify-rs/notify) | CC0-1.0 | Watching project folders for changes |
| [trash](https://github.com/Byron/trash-rs) | MIT | Moving discarded new files to the system trash |
| [notify-rust](https://github.com/hoodie/notify-rust) | MIT OR Apache-2.0 | Desktop notifications on Linux and Windows |
| [sysinfo](https://github.com/GuillaumeGomez/sysinfo) | MIT | Foreground process name and folder of each tab |
| [tree-sitter](https://github.com/tree-sitter/tree-sitter), tree-sitter-highlight, tree-sitter-language | MIT | Syntax highlighting in the file view |
| tree-sitter grammars: bash, c, c-sharp, cmake, containerfile, cpp, css, dart, diff, go, html, java, javascript, json, kotlin-sg, lua, make, md, nix, php, powershell, prisma, proto, python, ruby, rust, sequel, solidity, svelte-ng, swift, toml-ng, typescript, xml, yaml, zig | MIT | Language grammars and their highlight queries. The kotlin-sg highlight query is based on nvim-treesitter's (Apache-2.0) |
| tree-sitter grammars: elixir, ini | Apache-2.0 | Language grammars and their highlight queries |
| tree-sitter grammar: astro-next | MIT OR Apache-2.0 | Astro grammar and its highlight query |
| [fancy-regex](https://github.com/fancy-regex/fancy-regex) | MIT | Regular expressions with backreferences and lookaround for find and replace in the file view |
| [image](https://github.com/image-rs/image) | MIT OR Apache-2.0 | Pixel size of images in the file view, PNG writing in the development build |
| [core-graphics](https://github.com/servo/core-foundation-rs) | MIT OR Apache-2.0 | macOS only: window capture in the development build's control file |
| [block](https://github.com/SSheldon/rust-block) | MIT | macOS only: completion handlers of the system notification API |
| [embed-resource](https://github.com/nabijaczleweli/rust-embed-resource) | MIT | Windows only, build time: the executable icon |
| [objc](https://github.com/SSheldon/rust-objc) | MIT | macOS only: the Dock badge, dragging files out of the window |
| [plist](https://github.com/ebarnard/rust-plist) | MIT | macOS only: reading Nucli 1 preferences when onboarding offers to import them |
| [sha2](https://github.com/RustCrypto/hashes) | MIT OR Apache-2.0 | macOS only: the name of the Keychain item Claude Code keeps its sign-in in for a custom config folder (usage limits chip) |
| [windows](https://github.com/microsoft/windows-rs) | MIT OR Apache-2.0 | Windows only: dragging files out of the window to other apps (shell drag and drop), loading the app and grammar DLLs |
| [x11rb](https://github.com/psychon/x11rb) | MIT OR Apache-2.0 | Linux only: dragging files out of the window to other apps on X11 (XDND) |
| [libc](https://github.com/rust-lang/libc) | MIT OR Apache-2.0 | Linux and macOS only: saving terminal output when the app is asked to terminate. Nucli Uplink on iOS: socket addresses from Bonjour lookups |
| [objc2](https://github.com/madsmtm/objc2), objc2-foundation, objc2-ui-kit, objc2-quartz-core, objc2-av-foundation, objc2-uniform-type-identifiers, objc2-user-notifications, [dispatch2](https://github.com/madsmtm/objc2) | Zlib OR Apache-2.0 OR MIT | Nucli Uplink on iOS only: the QR code scanner (camera preview and QR detection), the system file picker for uploads, running Bonjour lookups on the main queue, and local notifications |
| [block2](https://github.com/madsmtm/objc2) | MIT | Nucli Uplink on iOS only: the completion handlers of local notifications |
| [base64](https://github.com/marshallpierce/rust-base64) | MIT OR Apache-2.0 | Usage limits: reading the ChatGPT account id inside the OpenAI sign-in token pi or opencode saved |
| [serde](https://github.com/serde-rs/serde), [serde_json](https://github.com/serde-rs/json) | MIT OR Apache-2.0 | Settings and state files |
| [keyring](https://github.com/hwchen/keyring-rs) | MIT OR Apache-2.0 | Uplink: keeping this device's private key in the system's credential store (Keychain on macOS and iOS, Credential Manager on Windows, Secret Service on Linux) |
| [snow](https://github.com/mcginty/snow) | Apache-2.0 OR MIT | Uplink: Noise protocol handshakes and encrypted transport between paired devices |
| [spake2](https://github.com/RustCrypto/PAKEs) | MIT OR Apache-2.0 | Uplink: password-authenticated key exchange that turns a short pairing code into a strong key |
| [mdns-sd](https://github.com/keepsimple1/mdns-sd) | Apache-2.0 OR MIT | Uplink: finding Nucli devices on the local network (multicast DNS service discovery) |
| [qrcode](https://github.com/kennytm/qrcode-rust) | MIT OR Apache-2.0 | Uplink: the QR code of a pairing link, drawn as squares (no image output) |
| [getrandom](https://github.com/rust-random/getrandom) | MIT OR Apache-2.0 | Uplink: random pairing codes from the operating system's secure random source |
| [if-addrs](https://github.com/messense/if-addrs) | MIT OR BSD-3-Clause | Uplink: listing this device's network addresses on the settings page and in the pairing link |

Build-time only, not bundled: [create-dmg](https://github.com/create-dmg/create-dmg) (MIT) makes the macOS disk image in CI.

Only GPUI and the Apache-2.0 crates it is built from (published as `gpui-pre-*`) are used from the Zed repository. The rest of that repository (its editor, terminal and UI crates) is GPL or AGPL licensed and must not be copied or depended on.

Transitive crates are licensed under MIT, Apache-2.0 (some with the LLVM exception), BSD-2/3-Clause, ISC, Zlib, Unicode-3.0, NCSA, CC0-1.0, Unlicense or BSL-1.0, where a crate offers a choice the permissive option applies. Three crates are MPL-2.0 (`option-ext` linked unmodified, `cbindgen` used only at build time, `dwrote` listed in `Cargo.lock` but built for no supported target); MPL-2.0 is file-level copyleft and allows distribution in a larger MIT work as long as those files stay under MPL-2.0 and their source stays available (they are published unmodified on crates.io).

Every release bundles the license texts of all linked crates in `THIRD_PARTY_NOTICES.txt` (generated by `scripts/notices.py` from `Cargo.lock` and the crate sources), next to `LICENSE` and `LEGAL.md`.

## Fonts

- [Maple Mono NF](https://github.com/subframe7536/maple-font) v7.9 by the Maple Mono Project Authors, SIL Open Font License 1.1, no Reserved Font Name. Files in `assets/fonts/` (Regular, Bold, Italic, BoldItalic), unmodified, from the official `MapleMono-NF.zip` release (SHA-256 `59098b87c895d871635d37680e88000ae2b2b25b55428195b228ec589e35fb89`). The license text is `assets/fonts/OFL.txt`, bundled with every build. The fonts are embedded in the binary and never sold by themselves.
- The "NF" variant contains the [Nerd Fonts](https://github.com/ryanoasis/nerd-fonts) glyph sets, patched by the font's authors; patched fonts are distributed under SIL OFL 1.1 like the base font. Nucli shows these glyphs as icons through the font only; no icon files are bundled.
- No system fonts are bundled. Apple fonts (SF Pro, SF Mono) must not be redistributed and are never bundled or copied.
- The user can pick any font installed on their system for terminals and the file view. Nucli only lists the installed font names and asks the system to render the chosen one; it never bundles, copies or redistributes it. Maple Mono NF stays the default and the fallback for missing glyphs.

## Icons and artwork

- UI icons are Nerd Font glyphs from the bundled font (see Fonts).
- The app icon and the DMG background are original artwork made of plain geometric shapes: no fonts, no symbols, no third-party artwork. In `assets/icon/`, `AppIcon.icon` is the source, `Assets.car` is compiled from it for macOS, and the other formats are generated by `scripts/icon.py`.

## Colors

- Nucli Dark, Nucli Light, Nucli Amber and Nucli Phosphor in `src/theme.rs` are original palettes.
- Vitesse color themes (Dark, Dark Soft, Black, Light, Light Soft) by Anthony Fu, https://github.com/antfu/vscode-theme-vitesse, MIT License (Copyright (c) 2020 Primer, Copyright (c) 2021 Anthony Fu). Nucli reuses only color values in `src/theme.rs`. The notice is kept in `assets/notices/Vitesse-LICENSE.txt` and bundled with every build.
- Do not copy third-party terminal schemes without adding their license here first.

## Third-party trademarks

Agent names (Claude, Codex, pi, opencode, …) are plain command names of tools the user installs and configures. Nucli is not affiliated with or endorsed by their vendors and does not bundle or modify their logos: agent tabs use generic glyphs. File type glyphs come from the bundled font and may show a language's or tool's mark, only to identify that kind of file. All trademarks belong to their owners.

## Network and data

Nucli collects no telemetry and talks to no server of its own or of anyone else. Without Uplink and the usage limits chip it makes no network calls at all: it runs local processes (the user's shells and agents, `git`) and stores settings, state, restored terminal output and a conversation title cache in the folders listed in `docs/SPEC.md`.

**Uplink** is off until the user turns it on in Settings. While it is on, Nucli listens on one TCP port (default 47100) on every local network interface, announces itself on the local network with multicast DNS (`_nucli-uplink._tcp`: device name, device key and port), and connects only to devices the user paired. Any device that reaches the port can ask to pair; Nucli then shows its name and asks the user, and shares nothing with it unless the user allows it. Every connection is end-to-end encrypted and mutually authenticated (Noise `XX` with X25519, ChaCha20-Poly1305 and BLAKE2s; pairing through a link or QR code uses SPAKE2 so its short code cannot be guessed offline). What travels is what the user chose to share: the names, folders and status of shared tabs, their terminal output, and the keystrokes of a device allowed to control them. On request of a paired device it also sends, for shared projects only, file names and file contents, git changes, diffs and history, and the titles and folders of agent conversations that ran in them; and to a device allowed to control, the names of folders it browses to add a project. A device allowed to control can also change shared projects the way the desktop's menus do: create, rename, move, copy, duplicate and delete files (deleted files go to the trash), open or reveal them on the computer, stage, unstage, discard and commit changes, switch, create and delete branches and tags, cherry-pick, revert, reset and restore, rename, color, move and close shared projects and tabs. A paired device can download any file of a shared project, and a device allowed to control can upload files into one; the files travel whole, in pieces, over the same encrypted connection, and Nucli writes what it receives to a hidden temporary file next to the target until the transfer completes. Nothing goes to a third party; whether the network reaches beyond the local one is the user's own setup. The device's private key is kept in the system's credential store: the Keychain on macOS, Credential Manager on Windows and the Secret Service (GNOME Keyring, KWallet) on Linux; where none is available, and in development builds on macOS, it stays in `uplink.json` like the list of paired devices, readable only by the user. A new device is paired only after the user allows it on this computer, with view or control, after checking that both show the same six digits. Turning Uplink off closes the port and every connection at once.

**Usage limits** is a status chip that is off until the user turns it on; before turning it on Nucli says what it does and asks. While it is on and an agent whose plan limits are only available online runs in a tab, Nucli reads that agent's own saved sign-in and sends its access token with `curl` to the agent's provider, at most once every 5 minutes per sign-in, only to read the account's usage limits:

- Claude Code: the Keychain item Claude Code created (through the system's `security` tool on macOS), else `.credentials.json` in Claude Code's config folder, to Anthropic (`https://api.anthropic.com/api/oauth/usage`).
- Codex, only while its session file has no limits yet: `auth.json` in Codex's config folder, to OpenAI (`https://chatgpt.com/backend-api/wham/usage`).
- pi signed in with a Claude or ChatGPT plan, for the provider of its last reply: `auth.json` in pi's config folder, to Anthropic or OpenAI (the endpoints above). Signed in with GitHub Copilot: the GitHub token in the same file, to GitHub (`https://api.github.com/copilot_internal/user`, or the GitHub Enterprise host pi signed in to).
- opencode signed in with a Claude or ChatGPT plan (the Claude one when both): `auth.json` in opencode's data folder (`~/.local/share/opencode`), to Anthropic or OpenAI (the endpoints above).
- Gemini CLI signed in with Google: `oauth_creds.json` in Gemini CLI's config folder, to Google (`https://cloudcode-pa.googleapis.com/v1internal:loadCodeAssist` for the account's Code Assist project id, then `:retrieveUserQuota`).

The token reaches `curl` through standard input, never as a command-line argument. Nucli never stores, refreshes or logs it and sends it nowhere else; a token whose saved expiry has passed is not used. Codex limits otherwise come from Codex's own session files, without network calls. Turning the chip off stops the requests. `docs/USAGE_LIMITS.md` lists what each agent uses.

**Nucli Uplink** (the phone app) talks only to the computers the user paired it with, over the same end-to-end encrypted Uplink connections. It never listens and announces nothing. It looks for Nucli computers on the local network with multicast DNS, through the system's Bonjour service, only to list computers it can pair with and to find paired ones; iOS asks once for Local Network access. It keeps its device key in the iOS Keychain and its paired computers in `uplink.json` and its preferences in `settings.json`, both in its app data folder. It uses the camera only while the QR scanner is open, to read the pairing code, and keeps no images; iOS asks once for camera access. It reads the clipboard only when the user taps `[paste pairing link]` or paste in a terminal, and writes to it only when the user chooses a copy action in a menu. Files it downloads go to its Documents folder, which the Files app shows; it reads only the files the user picks in the system file picker to upload them.

It reads, never writes, files that agents store locally, only to show them in Nucli: conversation files (`~/.claude*/projects`, `~/.codex*/sessions`, `~/.pi/agent/sessions`) for titles, ids and folders, and Claude Code's per-process status files (`<config dir>/sessions/<pid>.json`: session id and status) for tab status and resume. It never reads the `<pid>.<hash>.key` files next to them, which hold secrets. With the usage limits chip on, it also reads the agents' saved sign-ins as described above. Onboarding, shown only while `settings.json` does not exist, checks on macOS whether Nucli 1's preferences (`uk.polishch.nucli`) exist and imports them only when the user chooses to; "import your settings" reads `settings.json`, `keybindings.json` and `state.json` from a folder the user picks and copies them into Nucli's own folders. The font picker reads the list of installed font names from the system. Also on macOS, for up to 100 ms after the new shell or new agent key, it asks the system whether any key is still down to tell a tap from a hold (`CGEventSourceKeyState`, a system framework); it never learns or stores which keys are typed. Without Uplink and the usage limits chip nothing leaves the machine. The formats of these files belong to their vendors and may change. Any change to this section needs a privacy review.
