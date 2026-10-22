import AppKit

extension AppController {
    @objc func quickOpen(_ sender: Any?) {
        guard let controller = active, let project = controller.currentProject, !project.isScratch else {
            showPalette(.everything, placeholder: "Search tabs, projects, commands…")
            return
        }
        showPalette(.files, placeholder: "Search files in \(project.name)")
    }

    @objc func commandPalette(_ sender: Any?) {
        showPalette(.everything, placeholder: "Search commands, tabs, projects, conversations, files…")
    }

    private func showPalette(_ mode: PaletteMode, placeholder: String) {
        guard let controller = active, let window = controller.window, window.attachedSheet == nil else { return }
        palette?.close()
        let commands = mode == .everything ? menuCommands() : []
        let project = controller.currentProject
        let palette = Palette(mode: mode, parent: window, anchor: controller.paletteAnchor, placeholder: placeholder) { [weak self, weak controller] in
            guard let self, let controller else { return [] }
            return mode == .files ? fileItems(project, in: controller) : everything(in: controller, commands: commands)
        }
        palette.onClose = { [weak self, weak palette] in
            if self?.palette === palette { self?.palette = nil }
        }
        self.palette = palette
        if let project {
            if !project.isScratch { FileIndex.shared.refresh(project.path) { [weak palette] in palette?.reload() } }
            conversations.refresh(project.path, profiles: state.profiles) { [weak palette] _ in palette?.reload() }
        }
    }

    private func fileItems(_ project: Project?, in controller: MainWindowController) -> [PaletteItem] {
        guard let project else { return [] }
        let recent = project.recentFiles
        let rest = (FileIndex.shared.files(in: project.path) ?? []).filter { !recent.contains($0) }
        return (recent + rest).enumerated().map { index, path in
            let (image, color) = FileIcon.icon(for: (path as NSString).lastPathComponent, isDirectory: false)
            let url = URL(fileURLWithPath: project.path).appendingPathComponent(path)
            return PaletteItem(title: path, subtitle: index < recent.count ? "recently opened" : "", image: image, tint: color, detail: "", isPath: true, rank: index < recent.count ? 30 - index : 0, alternate: { NSWorkspace.shared.open(url) }) { [weak controller] in
                controller?.openFile(path, in: project)
            }
        }
    }

    private func everything(in controller: MainWindowController, commands: [PaletteItem]) -> [PaletteItem] {
        var items: [PaletteItem] = []
        for session in state.allSessions {
            let image = session.runningAgent.map(AgentBadge.image(for:)) ?? NSImage(systemSymbolName: session.kind == .agent ? "sparkles" : "terminal", accessibilityDescription: nil)
            items.append(PaletteItem(title: session.title, subtitle: "\(session.project.name) · \(session.subtitle)", image: image, tint: session.runningAgent == nil ? .secondaryLabelColor : nil, detail: "Tab", rank: session.project.windowId == controller.id ? 12 : 8) { [weak self] in
                self?.select(session)
            })
        }
        for project in state.projects where !project.isScratch {
            items.append(PaletteItem(title: project.name, subtitle: project.displayPath, image: NSImage(systemSymbolName: "folder.fill", accessibilityDescription: nil), tint: project.tint?.color ?? .controlAccentColor, detail: "Project", rank: 10) { [weak self] in
                self?.select(project)
            })
        }
        for workspace in state.workspaces {
            items.append(PaletteItem(title: workspace.name, subtitle: "\(state.projects(inWindow: workspace.windowId).count) projects", image: NSImage(systemSymbolName: "macwindow", accessibilityDescription: nil), tint: .secondaryLabelColor, detail: "Workspace", rank: 6) { [weak self] in
                self?.controller(forWindowId: workspace.windowId).showWindow(nil)
            })
        }
        let project = controller.currentProject
        for profile in state.profiles where profile.isLaunchable {
            items.append(PaletteItem(title: "New Agent: \(profile.name)", subtitle: project?.name ?? "", image: profile.kind.map { AgentBadge.image(for: $0.rawValue) } ?? NSImage(systemSymbolName: "sparkles", accessibilityDescription: nil), detail: "Agent", rank: 4) { [weak self] in
                guard let self else { return }
                if let project { startAgent(profile, in: project) } else { newAgent(nil) }
            })
        }
        for shell in state.shells where shell.isLaunchable {
            items.append(PaletteItem(title: "New Shell: \(shell.name)", subtitle: project?.name ?? "", image: NSImage(systemSymbolName: "terminal", accessibilityDescription: nil), tint: .secondaryLabelColor, detail: "Shell", rank: 4) { [weak self] in
                guard let self else { return }
                if let project { startShell(shell, in: project) } else { newAgent(nil) }
            })
        }
        if let project {
            for conversation in conversations.cached(for: project.path) ?? [] {
                items.append(PaletteItem(title: conversation.title, subtitle: "\(conversation.profile.name) · \(conversation.date.relativeDescription)", image: AgentBadge.image(for: conversation.kind.rawValue), detail: "Conversation", rank: 2) { [weak self] in
                    self?.resume(conversation, in: project)
                })
            }
        }
        items += commands
        items.append(PaletteItem(title: state.notifications ? "Turn Off Notifications" : "Turn On Notifications", subtitle: "Settings", image: NSImage(systemSymbolName: "bell", accessibilityDescription: nil), tint: .secondaryLabelColor, detail: "Setting") { [weak self] in
            guard let self else { return }
            state.notifications.toggle()
            state.save()
        })
        if let project, !project.isScratch {
            items += fileItems(project, in: controller).map { item in
                var file = item
                file.rank -= 50
                file.detail = "File"
                return file
            }
        }
        return items
    }

    private func menuCommands() -> [PaletteItem] {
        var items: [PaletteItem] = []
        func walk(_ menu: NSMenu, path: [String]) {
            menu.update()
            for item in menu.items where !item.isSeparatorItem && !item.isHidden {
                if let submenu = item.submenu {
                    if submenu !== NSApp.windowsMenu || path.isEmpty { walk(submenu, path: path + [item.title]) }
                    continue
                }
                guard let action = item.action, item.isEnabled, !item.title.isEmpty, action != #selector(NSWindow.makeKeyAndOrderFront(_:)),
                      action != #selector(commandPalette(_:)), action != #selector(quickOpen(_:)) else { continue }
                let target = item.target
                let title = (path + [item.title.replacingOccurrences(of: "…", with: "")]).joined(separator: ": ")
                items.append(PaletteItem(title: title, subtitle: "", image: NSImage(systemSymbolName: "command", accessibilityDescription: nil), tint: .tertiaryLabelColor, detail: Self.shortcut(of: item), rank: 0) {
                    NSApp.sendAction(action, to: target, from: item)
                })
            }
        }
        if let main = NSApp.mainMenu { walk(main, path: []) }
        return items
    }

    static func shortcut(of item: NSMenuItem) -> String {
        guard !item.keyEquivalent.isEmpty else { return "" }
        let mask = item.keyEquivalentModifierMask
        var text = ""
        if mask.contains(.control) { text += "⌃" }
        if mask.contains(.option) { text += "⌥" }
        if mask.contains(.shift) { text += "⇧" }
        if mask.contains(.command) { text += "⌘" }
        let key = item.keyEquivalent
        let scalar = key.unicodeScalars.first.map { Int($0.value) } ?? 0
        let names: [Int: String] = [NSLeftArrowFunctionKey: "←", NSRightArrowFunctionKey: "→", NSUpArrowFunctionKey: "↑", NSDownArrowFunctionKey: "↓", 0x1b: "Esc", 0x0d: "↩"]
        if let name = names[scalar] { return text + name }
        if scalar >= NSF1FunctionKey, scalar <= NSF35FunctionKey { return text + "F\(scalar - NSF1FunctionKey + 1)" }
        return text + key.uppercased()
    }
}
