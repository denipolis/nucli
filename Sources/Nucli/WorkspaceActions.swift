import AppKit

final class MenuPayload: NSObject {
    let target: AnyObject
    let value: Any?

    init(_ target: AnyObject, _ value: Any? = nil) {
        self.target = target
        self.value = value
    }
}

final class MenuAction: NSObject {
    private let handler: () -> Void

    private init(_ handler: @escaping () -> Void) {
        self.handler = handler
    }

    static func item(_ title: String, handler: @escaping () -> Void) -> NSMenuItem {
        let action = MenuAction(handler)
        let item = NSMenuItem(title: title, action: #selector(run), keyEquivalent: "")
        item.target = action
        item.representedObject = action
        return item
    }

    @objc private func run() {
        handler()
    }
}

extension AppController {
    @objc func renameItem(_ sender: NSMenuItem) {
        guard let item = sender.representedObject as AnyObject? else { return }
        let target = (item as? Session).map { controller(for: $0.project) } ?? (item as? Project).map { controller(for: $0) } ?? active
        target?.sidebar.beginRename(item)
    }

    @objc func renameTab(_ sender: Any?) {
        guard let controller = active, let session = controller.currentSession else { return }
        if let item = controller.split.splitViewItems.first, item.isCollapsed { item.isCollapsed = false }
        controller.sidebar.beginRename(session)
    }

    func rename(_ item: AnyObject, to value: String) {
        let name = value.trimmingCharacters(in: .whitespaces)
        switch item {
        case let session as Session:
            session.customName = name.isEmpty ? nil : name
        case let project as Project:
            project.customName = name.isEmpty || name == project.folderName ? nil : name
        case let workspace as Workspace:
            guard !name.isEmpty else { return }
            workspace.name = name
        default:
            return
        }
        state.changed()
        refreshTitles()
    }

    @objc func setColor(_ sender: NSMenuItem) {
        guard let payload = sender.representedObject as? MenuPayload else { return }
        let color = payload.value as? ItemColor
        (payload.target as? Project)?.color = color
        if let session = payload.target as? Session {
            session.color = color
            controller(for: session.project).updateTerminals()
        }
        state.changed()
    }

    @objc func renameWorkspace(_ sender: Any?) {
        guard let controller = active, let workspace = state.workspace(forWindow: controller.id),
              let name = ask("Rename Workspace", nil, value: workspace.name, placeholder: "Workspace name"), !name.isEmpty else { return }
        rename(workspace, to: name)
    }

    @objc func moveToWindow(_ sender: NSMenuItem) {
        guard let payload = sender.representedObject as? MenuPayload else { return }
        guard let project = payload.target as? Project,
              let target = (payload.value as? Int).flatMap({ id in windowControllers.first { $0.id == id } }) ?? newEmptyWindow() else { return }
        let moved = [project]
        let sources = Set(moved.map(\.windowId)).compactMap { id in windowControllers.first { $0.id == id } }
        moved.forEach { $0.windowId = target.id }
        state.changed()
        for source in sources where source !== target {
            if let current = source.currentProject, moved.contains(where: { $0 === current }) {
                source.selectFallback()
            }
        }
        if let first = moved.first { target.select(first) }
        target.showWindow(nil)
    }

    func refreshTitles() {
        windowControllers.forEach { $0.refreshTitle() }
    }

    func ask(_ title: String, _ text: String?, value: String, placeholder: String) -> String? {
        let alert = NSAlert()
        alert.messageText = title
        alert.informativeText = text ?? ""
        let field = NSTextField(frame: NSRect(x: 0, y: 0, width: 260, height: 24))
        field.stringValue = value
        field.placeholderString = placeholder
        alert.accessoryView = field
        alert.addButton(withTitle: "OK")
        alert.addButton(withTitle: "Cancel")
        alert.window.initialFirstResponder = field
        guard alert.runModal() == .alertFirstButtonReturn else { return nil }
        return field.stringValue.trimmingCharacters(in: .whitespaces)
    }

    static func colorMenu(for target: AnyObject, current: ItemColor?) -> NSMenuItem {
        let menu = NSMenu()
        let none = NSMenuItem(title: "None", action: #selector(setColor(_:)), keyEquivalent: "")
        none.representedObject = MenuPayload(target)
        none.state = current == nil ? .on : .off
        menu.addItem(none)
        menu.addItem(.separator())
        for color in ItemColor.allCases {
            let item = NSMenuItem(title: color.title, action: #selector(setColor(_:)), keyEquivalent: "")
            item.representedObject = MenuPayload(target, color)
            item.image = swatch(color.color)
            item.state = current == color ? .on : .off
            menu.addItem(item)
        }
        let root = NSMenuItem(title: "Color", action: nil, keyEquivalent: "")
        root.submenu = menu
        return root
    }

    private static func swatch(_ color: NSColor) -> NSImage {
        NSImage(size: NSSize(width: 12, height: 12), flipped: false) { rect in
            color.setFill()
            NSBezierPath(ovalIn: rect.insetBy(dx: 1, dy: 1)).fill()
            return true
        }
    }
}
