import AppKit

@MainActor
enum MainMenu {
    static func build() -> NSMenu {
        let main = NSMenu()
        main.addItem(submenu("Nucli", [
            item("About Nucli", #selector(AppController.showAbout(_:)), ""),
            .separator(),
            item("Settings…", #selector(AppController.showSettings(_:)), ","),
            .separator(),
            item("Hide Nucli", #selector(NSApplication.hide(_:)), "h"),
            item("Hide Others", #selector(NSApplication.hideOtherApplications(_:)), "h", [.command, .option]),
            item("Show All", #selector(NSApplication.unhideAllApplications(_:)), ""),
            .separator(),
            item("Quit Nucli", #selector(NSApplication.terminate(_:)), "q"),
        ]))
        main.addItem(submenu("File", [
            item("New Workspace…", #selector(AppController.newWindow(_:)), "n"),
            item("Rename Workspace…", #selector(AppController.renameWorkspace(_:)), ""),
            item("New Tab", #selector(AppController.newAgentWithLastProfile(_:)), "t"),
            item("New Tab…", #selector(AppController.newAgent(_:)), "t", [.command, .shift]),
            alternate(item("New Tab in Folder…", #selector(AppController.newAgentInFolder(_:)), "t", [.command, .shift, .option])),
            item("Open Project…", #selector(AppController.openProject(_:)), "o"),
            item("Quick Open…", #selector(AppController.quickOpen(_:)), "p"),
            .separator(),
            item("Split Right", #selector(AppController.splitRight(_:)), "d"),
            item("Split Down", #selector(AppController.splitDown(_:)), "d", [.command, .shift]),
            item("Toggle Terminal", #selector(AppController.toggleTerminalPanel(_:)), "`", [.control]),
            item("New Terminal", #selector(AppController.newTerminal(_:)), "`", [.control, .shift]),
            item("Close Split", #selector(AppController.closePane(_:)), "w", [.command, .option]),
            .separator(),
            item("Rename Tab…", #selector(AppController.renameTab(_:)), ""),
            item("Close Tab", #selector(AppController.closeSession(_:)), "w"),
            item("Close Project", #selector(AppController.closeProject(_:)), "w", [.command, .shift]),
        ]))
        main.addItem(submenu("Edit", [
            item("Undo", Selector(("undo:")), "z"),
            item("Redo", Selector(("redo:")), "z", [.command, .shift]),
            .separator(),
            item("Cut", #selector(NSText.cut(_:)), "x"),
            item("Copy", #selector(NSText.copy(_:)), "c"),
            item("Paste", #selector(NSText.paste(_:)), "v"),
            item("Select All", #selector(NSText.selectAll(_:)), "a"),
            .separator(),
            find("Find…", .showFindPanel, "f"),
            find("Find Next", .next, "g"),
            find("Find Previous", .previous, "g", [.command, .shift]),
            find("Use Selection for Find", .setFindString, "e"),
            .separator(),
            item("Clear", #selector(AppController.clearTerminal(_:)), "k"),
        ]))
        main.addItem(submenu("View", [
            item("Command Palette…", #selector(AppController.commandPalette(_:)), "p", [.command, .shift]),
            .separator(),
            item("Toggle Sidebar", #selector(AppController.toggleSidebarPane(_:)), "b"),
            item("Toggle Files and Changes", #selector(AppController.togglePanel(_:)), "b", [.command, .option]),
            item("Move Files and Changes to Other Side", #selector(AppController.movePanel(_:)), ""),
            item("Next Change", #selector(AppController.nextChange(_:)), String(Character(UnicodeScalar(NSF7FunctionKey)!)), []),
            item("Previous Change", #selector(AppController.previousChange(_:)), String(Character(UnicodeScalar(NSF7FunctionKey)!)), [.shift]),
            item("Filter Sidebar", #selector(AppController.filterSidebar(_:)), "f", [.command, .option]),
        ] + ["Projects", "Agents", "History"].enumerated().map { index, title in
            let menuItem = item(title, #selector(AppController.showSidebarTab(_:)), "\(index + 1)", [.command, .option])
            menuItem.tag = index
            return menuItem
        } + PanelTab.allCases.map { tab in
            let menuItem = item(tab.title, #selector(AppController.showPanelTab(_:)), "\(tab.rawValue + 1)", [.command, .control])
            menuItem.tag = tab.rawValue
            return menuItem
        } + [
            .separator(),
        ] + focusItems() + [
            .separator(),
            item("Bigger", #selector(AppController.biggerFont(_:)), "="),
            item("Smaller", #selector(AppController.smallerFont(_:)), "-"),
            item("Actual Size", #selector(AppController.resetFont(_:)), "0"),
            .separator(),
            item("Enter Full Screen", #selector(NSWindow.toggleFullScreen(_:)), "f", [.command, .control]),
        ]))
        var tabs: [NSMenuItem] = [
            item("Minimize", #selector(NSWindow.performMiniaturize(_:)), "m"),
            item("Zoom", #selector(NSWindow.performZoom(_:)), ""),
            .separator(),
            item("Next Tab", #selector(AppController.selectNextSession(_:)), "]", [.command, .shift]),
            item("Previous Tab", #selector(AppController.selectPreviousSession(_:)), "[", [.command, .shift]),
            .separator(),
        ]
        for n in 1...9 {
            let tab = item("Tab \(n)", #selector(AppController.selectSessionByNumber(_:)), "\(n)")
            tab.tag = n
            tabs.append(tab)
        }
        let window = submenu("Window", tabs)
        NSApp.windowsMenu = window.submenu
        main.addItem(window)
        defaults = [:]
        commands(in: main).forEach { id, item in
            defaults[id] = Shortcut(key: item.keyEquivalent, modifiers: item.keyEquivalentModifierMask)
            if let override = AppState.shared.keyBindings[id].map(Shortcut.init(dictionary:)) {
                item.keyEquivalent = override.key
                item.keyEquivalentModifierMask = override.modifiers
            }
        }
        return main
    }

    private(set) static var defaults: [String: Shortcut] = [:]

    static func shortcut(for action: Selector) -> String {
        guard let menu = NSApp.mainMenu, let item = commands(in: menu).first(where: { $0.1.action == action && !$0.1.isAlternate })?.1 else { return "" }
        return AppController.shortcut(of: item)
    }

    static func commands(in menu: NSMenu, path: [String] = []) -> [(String, NSMenuItem)] {
        menu.items.flatMap { item -> [(String, NSMenuItem)] in
            if let submenu = item.submenu { return commands(in: submenu, path: path + [item.title]) }
            guard let action = item.action, action != #selector(NSWindow.makeKeyAndOrderFront(_:)), !item.isSeparatorItem else { return [] }
            return [((path + [item.title]).joined(separator: "/"), item)]
        }
    }

    private static func focusItems() -> [NSMenuItem] {
        let entries: [(String, Int)] = [("Focus Left Split", NSLeftArrowFunctionKey), ("Focus Right Split", NSRightArrowFunctionKey), ("Focus Upper Split", NSUpArrowFunctionKey), ("Focus Lower Split", NSDownArrowFunctionKey)]
        return entries.enumerated().map { index, entry in
            let menuItem = item(entry.0, #selector(AppController.focusPane(_:)), String(Character(UnicodeScalar(entry.1)!)), [.command, .option])
            menuItem.tag = index
            return menuItem
        }
    }

    private static func item(_ title: String, _ action: Selector, _ key: String, _ mods: NSEvent.ModifierFlags = .command) -> NSMenuItem {
        let item = NSMenuItem(title: title, action: action, keyEquivalent: key)
        item.keyEquivalentModifierMask = mods
        return item
    }

    private static func alternate(_ item: NSMenuItem) -> NSMenuItem {
        item.isAlternate = true
        return item
    }

    private static func find(_ title: String, _ action: NSFindPanelAction, _ key: String, _ mods: NSEvent.ModifierFlags = .command) -> NSMenuItem {
        let item = item(title, #selector(NSTextView.performFindPanelAction(_:)), key, mods)
        item.tag = Int(action.rawValue)
        return item
    }

    private static func submenu(_ title: String, _ items: [NSMenuItem]) -> NSMenuItem {
        let menu = NSMenu(title: title)
        items.forEach(menu.addItem)
        let root = NSMenuItem(title: title, action: nil, keyEquivalent: "")
        root.submenu = menu
        return root
    }
}

@MainActor
struct Shortcut: Equatable {
    var key: String
    var modifiers: NSEvent.ModifierFlags

    init(key: String, modifiers: NSEvent.ModifierFlags) {
        self.key = key
        self.modifiers = modifiers.intersection([.command, .shift, .option, .control])
    }

    init(dictionary: [String: Any]) {
        self.init(key: dictionary["key"] as? String ?? "", modifiers: NSEvent.ModifierFlags(rawValue: dictionary["modifiers"] as? UInt ?? 0))
    }

    var dictionary: [String: Any] { ["key": key, "modifiers": modifiers.rawValue] }

    var display: String {
        guard !key.isEmpty else { return "" }
        let item = NSMenuItem(title: "", action: nil, keyEquivalent: key)
        item.keyEquivalentModifierMask = modifiers
        return AppController.shortcut(of: item)
    }
}
