import AppKit
import UniformTypeIdentifiers

enum OpenWith {
    static func menu(for url: URL) -> NSMenu {
        let menu = NSMenu()
        let workspace = NSWorkspace.shared
        let defaultApp = workspace.urlForApplication(toOpen: url)
        let apps = workspace.urlsForApplications(toOpen: url)
            .filter { $0 != defaultApp }
            .sorted { name(of: $0).localizedStandardCompare(name(of: $1)) == .orderedAscending }
        if let defaultApp {
            menu.addItem(item(for: defaultApp, opening: url, title: name(of: defaultApp) + " (default)"))
            menu.addItem(.separator())
        }
        apps.forEach { menu.addItem(item(for: $0, opening: url, title: name(of: $0))) }
        if !apps.isEmpty { menu.addItem(.separator()) }
        menu.addItem(MenuAction.item("Other…") { chooseApp(opening: url) })
        return menu
    }

    static func submenuItem(for url: URL) -> NSMenuItem {
        let item = NSMenuItem(title: "Open With", action: nil, keyEquivalent: "")
        item.submenu = menu(for: url)
        return item
    }

    private static func name(of app: URL) -> String {
        app.deletingPathExtension().lastPathComponent
    }

    private static func item(for app: URL, opening url: URL, title: String) -> NSMenuItem {
        let item = MenuAction.item(title) { open(url, with: app) }
        let icon = NSWorkspace.shared.icon(forFile: app.path)
        icon.size = NSSize(width: 16, height: 16)
        item.image = icon
        return item
    }

    private static func open(_ url: URL, with app: URL) {
        NSWorkspace.shared.open([url], withApplicationAt: app, configuration: NSWorkspace.OpenConfiguration())
    }

    private static func chooseApp(opening url: URL) {
        let panel = NSOpenPanel()
        panel.directoryURL = URL(fileURLWithPath: "/Applications")
        panel.allowedContentTypes = [.application]
        panel.prompt = "Open"
        panel.message = "Choose an app to open \(url.lastPathComponent)"
        guard panel.runModal() == .OK, let app = panel.url else { return }
        open(url, with: app)
    }
}
