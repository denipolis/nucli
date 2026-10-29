import AppKit

extension Notification.Name {
    static let projectsChanged = Notification.Name("NucliProjectsChanged")
    static let sessionChanged = Notification.Name("NucliSessionChanged")
    static let gitChanged = Notification.Name("NucliGitChanged")
    static let conversationsChanged = Notification.Name("NucliConversationsChanged")
    static let settingsChanged = Notification.Name("NucliSettingsChanged")
    static let layoutChanged = Notification.Name("NucliLayoutChanged")
}

enum PanelSide: String {
    case left, right
}

enum AppearanceMode: String, CaseIterable {
    case system, light, dark

    var title: String { rawValue.capitalized }

    var appearance: NSAppearance? {
        switch self {
        case .system: nil
        case .light: NSAppearance(named: .aqua)
        case .dark: NSAppearance(named: .darkAqua)
        }
    }
}

enum CursorShape: String, CaseIterable {
    case block, bar, underline

    var title: String { rawValue.capitalized }
}

enum TextSize: String, CaseIterable {
    case small, medium, large

    var title: String { rawValue.capitalized }

    var offset: CGFloat {
        switch self {
        case .small: -1
        case .medium: 0
        case .large: 2
        }
    }
}

enum ItemColor: String, CaseIterable {
    case red, orange, yellow, green, mint, teal, blue, indigo, purple, pink, brown, gray

    var title: String { rawValue.capitalized }

    var color: NSColor {
        switch self {
        case .red: .systemRed
        case .orange: .systemOrange
        case .yellow: .systemYellow
        case .green: .systemGreen
        case .mint: .systemMint
        case .teal: .systemTeal
        case .blue: .systemBlue
        case .indigo: .systemIndigo
        case .purple: .systemPurple
        case .pink: .systemPink
        case .brown: .systemBrown
        case .gray: .systemGray
        }
    }
}

@MainActor
final class Workspace {
    let id: String
    var name: String
    var windowId: Int
    var isClosed = false

    init(id: String = UUID().uuidString, name: String, windowId: Int) {
        self.id = id
        self.name = name
        self.windowId = windowId
    }
}

@MainActor
final class Project {
    let path: String
    var sessions: [Session] = []
    var terminals: [Session] = []
    weak var activeTerminal: Session?
    var showsTerminals = false
    var lastProfile: String?
    var isCollapsed = false
    var restoredTabs: [[String: String]] = []
    var restoredActiveTab: Int?
    weak var activeSession: Session?
    var windowId = 0
    var customName: String?
    var color: ItemColor?
    var lastSelected: Date = .distantPast
    var diffStat: (added: Int, removed: Int)?
    var recentFiles: [String] = []
    var openFiles: [String] = []
    var activeFile: String?
    var isScratch = false

    var folderName: String { (path as NSString).lastPathComponent }
    var name: String { isScratch ? "No Project" : customName ?? folderName }
    var displayPath: String { (path as NSString).abbreviatingWithTildeInPath }
    var tint: ItemColor? { color }

    init(path: String, lastProfile: String?) {
        self.path = path
        self.lastProfile = lastProfile
    }
}

@MainActor
final class AppState {
    static let shared = AppState()

    private let defaults = UserDefaults.standard
    var projects: [Project] = []
    var profiles: [AgentProfile] = []
    var shells: [ShellProfile] = []
    var lastShell: String?
    var shell: String
    var fontSize: CGFloat
    var lastProfile: String?
    var lastFolder: String?
    var workspaces: [Workspace] = []
    var notifications: Bool
    var panelSide: PanelSide
    var panelHidden: Bool
    var fontName: String?
    var restoresTabs: Bool
    var appearance: AppearanceMode
    var theme: String
    var keyBindings: [String: [String: Any]]
    var lineSpacing: CGFloat
    var ligatures: Bool
    var cursorShape: CursorShape
    var cursorBlinks: Bool
    var sidebarTextSize: TextSize

    var terminalShell: String {
        let launchable = shells.filter(\.isLaunchable)
        return (launchable.first { $0.name == lastShell } ?? launchable.first)?.path.expandingHome ?? shell
    }

    var font: NSFont {
        let font = fontName.flatMap { NSFont(name: $0, size: fontSize) } ?? NSFont.monospacedSystemFont(ofSize: fontSize, weight: .regular)
        guard !ligatures else { return font }
        let off: [[NSFontDescriptor.FeatureKey: Int]] = [
            [.typeIdentifier: kLigaturesType, .selectorIdentifier: kCommonLigaturesOffSelector],
            [.typeIdentifier: kContextualAlternatesType, .selectorIdentifier: kContextualAlternatesOffSelector],
        ]
        return NSFont(descriptor: font.fontDescriptor.addingAttributes([.featureSettings: off]), size: fontSize) ?? font
    }

    private init() {
        shell = ProcessInfo.processInfo.environment["SHELL"] ?? "/bin/zsh"
        lastShell = defaults.string(forKey: "lastShell")
        let size = defaults.double(forKey: "fontSize")
        fontSize = size > 0 ? size : 13
        lastProfile = defaults.string(forKey: "lastProfile")
        lastFolder = defaults.string(forKey: "lastFolder")
        notifications = defaults.object(forKey: "notifications") as? Bool ?? true
        fontName = defaults.string(forKey: "fontName")
        panelSide = defaults.string(forKey: "panelSide").flatMap(PanelSide.init(rawValue:)) ?? .right
        panelHidden = defaults.bool(forKey: "panelHidden")
        restoresTabs = defaults.object(forKey: "restoresTabs") as? Bool ?? true
        appearance = defaults.string(forKey: "appearance").flatMap(AppearanceMode.init(rawValue:)) ?? .system
        theme = defaults.string(forKey: "theme") ?? "nucli"
        keyBindings = defaults.dictionary(forKey: "keyBindings") as? [String: [String: Any]] ?? [:]
        let spacing = defaults.double(forKey: "lineSpacing")
        lineSpacing = spacing > 0 ? spacing : 1
        ligatures = defaults.object(forKey: "ligatures") as? Bool ?? true
        cursorShape = defaults.string(forKey: "cursorShape").flatMap(CursorShape.init(rawValue:)) ?? .block
        cursorBlinks = defaults.object(forKey: "cursorBlinks") as? Bool ?? true
        sidebarTextSize = defaults.string(forKey: "sidebarTextSize").flatMap(TextSize.init(rawValue:)) ?? .medium
        profiles = (defaults.array(forKey: "profiles") as? [[String: Any]] ?? []).compactMap(AgentProfile.init(dictionary:))
        shells = (defaults.array(forKey: "shells") as? [[String: Any]])?.compactMap(ShellProfile.init(dictionary:)) ?? ShellProfile.detect(loginShell: shell)
        let isLegacy = defaults.array(forKey: "projects") == nil
        let savedWorkspaces = defaults.array(forKey: isLegacy ? "groups" : "workspaceList") as? [[String: Any]] ?? []
        let savedProjects = defaults.array(forKey: isLegacy ? "workspaces" : "projects") as? [[String: Any]] ?? []
        let workspaceKey = isLegacy ? "group" : "workspace"
        workspaces = savedWorkspaces.compactMap { dict in
            guard let id = dict["id"] as? String, let name = dict["name"] as? String else { return nil }
            let workspace = Workspace(id: id, name: name, windowId: dict["window"] as? Int ?? 0)
            workspace.isClosed = dict["closed"] as? Bool ?? false
            return workspace
        }
        var members: [String: [Project]] = [:]
        projects = savedProjects.compactMap { dict in
            guard let path = dict["path"] as? String else { return nil }
            let project = Project(path: path, lastProfile: dict["lastProfile"] as? String)
            project.restoredTabs = dict["tabs"] as? [[String: String]] ?? []
            project.restoredActiveTab = dict["activeTab"] as? Int
            project.windowId = dict["window"] as? Int ?? 0
            project.customName = dict["name"] as? String
            project.color = (dict["color"] as? String).flatMap(ItemColor.init(rawValue:))
            if let workspace = (dict[workspaceKey] as? String).flatMap({ id in workspaces.first { $0.id == id } }) {
                project.windowId = workspace.windowId
                members[workspace.id, default: []].append(project)
            }
            project.isCollapsed = dict["collapsed"] as? Bool ?? false
            project.lastSelected = dict["lastSelected"] as? Date ?? .distantPast
            project.recentFiles = dict["recentFiles"] as? [String] ?? []
            project.openFiles = dict["openFiles"] as? [String] ?? []
            project.activeFile = dict["activeFile"] as? String
            project.isScratch = dict["scratch"] as? Bool ?? false
            project.showsTerminals = dict["terminalsShown"] as? Bool ?? false
            return project
        }
        var seen = Set<Int>()
        var next = (workspaces.map(\.windowId) + projects.map(\.windowId)).max().map { $0 + 1 } ?? 0
        for workspace in workspaces where !seen.insert(workspace.windowId).inserted {
            workspace.windowId = next
            members[workspace.id]?.forEach { $0.windowId = next }
            next += 1
        }
        windowIds.forEach { _ = ensureWorkspace(windowId: $0) }
    }

    func save() {
        guard !isRestoring else { return }
        defaults.removeObject(forKey: "shell")
        defaults.set(lastShell, forKey: "lastShell")
        defaults.set(Double(fontSize), forKey: "fontSize")
        defaults.set(lastProfile, forKey: "lastProfile")
        defaults.set(lastFolder, forKey: "lastFolder")
        defaults.set(notifications, forKey: "notifications")
        defaults.set(fontName, forKey: "fontName")
        defaults.set(panelSide.rawValue, forKey: "panelSide")
        defaults.set(panelHidden, forKey: "panelHidden")
        defaults.set(restoresTabs, forKey: "restoresTabs")
        defaults.set(appearance.rawValue, forKey: "appearance")
        defaults.set(theme, forKey: "theme")
        defaults.set(Double(lineSpacing), forKey: "lineSpacing")
        defaults.set(ligatures, forKey: "ligatures")
        defaults.set(cursorShape.rawValue, forKey: "cursorShape")
        defaults.set(cursorBlinks, forKey: "cursorBlinks")
        defaults.set(sidebarTextSize.rawValue, forKey: "sidebarTextSize")
        defaults.removeObject(forKey: "customThemes")
        defaults.set(keyBindings, forKey: "keyBindings")
        defaults.set(profiles.map(\.dictionary), forKey: "profiles")
        defaults.set(shells.map(\.dictionary), forKey: "shells")
        defaults.set(workspaces.map { ["id": $0.id, "name": $0.name, "window": $0.windowId, "closed": $0.isClosed] as [String: Any] }, forKey: "workspaceList")
        defaults.set(projects.filter { !$0.isScratch || !$0.sessions.isEmpty || !$0.terminals.isEmpty || !$0.restoredTabs.isEmpty || isRestoring }.map { ws -> [String: Any] in
            var dict: [String: Any] = ["path": ws.path, "window": ws.windowId, "collapsed": ws.isCollapsed, "lastSelected": ws.lastSelected]
            dict["lastProfile"] = ws.lastProfile
            dict["recentFiles"] = ws.recentFiles
            dict["openFiles"] = ws.openFiles
            dict["activeFile"] = ws.activeFile
            dict["scratch"] = ws.isScratch ? true : nil
            dict["terminalsShown"] = ws.showsTerminals
            dict["name"] = ws.customName
            dict["color"] = ws.color?.rawValue
            let snapshot = tabs(of: ws)
            dict["activeTab"] = snapshot.isEmpty ? ws.restoredActiveTab : activeTab(of: ws)
            dict["tabs"] = snapshot.isEmpty ? ws.restoredTabs : snapshot
            return dict
        }, forKey: "projects")
    }

    func tabs(of project: Project) -> [[String: String]] {
        (project.sessions + project.terminals).filter(\.isRunning).map { session -> [String: String] in
            var tab = ["kind": session.kind.rawValue]
            tab["profile"] = session.profile?.name
            tab["conversation"] = session.conversationId
            tab["name"] = session.customName
            tab["color"] = session.color?.rawValue
            if session.kind != .agent {
                tab["shell"] = session.shell
                tab["cwd"] = session.directory
                tab["scrollback"] = session.scrollbackId
            }
            return tab
        }
    }

    func activeTab(of project: Project) -> Int? {
        project.activeSession.flatMap { active in (project.sessions + project.terminals).filter(\.isRunning).firstIndex { $0 === active } }
    }

    func project(for path: String, windowId: Int) -> Project {
        let normalized = URL(fileURLWithPath: path).standardizedFileURL.path
        if let existing = projects.first(where: { $0.path == normalized && !$0.isScratch }) {
            return existing
        }
        let ws = Project(path: normalized, lastProfile: nil)
        ws.windowId = windowId
        projects.append(ws)
        save()
        NotificationCenter.default.post(name: .projectsChanged, object: nil)
        return ws
    }

    func remove(_ ws: Project) {
        projects.removeAll { $0 === ws }
        save()
        NotificationCenter.default.post(name: .projectsChanged, object: nil)
    }

    var didDetectAgents: Bool {
        get { defaults.bool(forKey: "didDetectAgents") }
        set { defaults.set(newValue, forKey: "didDetectAgents") }
    }

    func detectAgents(completion: @escaping (Int) -> Void) {
        let shell = shell
        DispatchQueue.global(qos: .userInitiated).async {
            let found = AgentDetector.detect(shell: shell)
            DispatchQueue.main.async { [self] in
                let added = found.filter { profile in !profiles.contains { $0.name == profile.name } }
                profiles += added
                didDetectAgents = true
                save()
                completion(added.count)
            }
        }
    }

    var allSessions: [Session] { projects.flatMap { $0.sessions + $0.terminals } }

    var windowIds: [Int] { Array(Set(projects.map(\.windowId) + workspaces.map(\.windowId))).sorted() }

    func workspace(forWindow id: Int) -> Workspace? {
        workspaces.first { $0.windowId == id }
    }

    func ensureWorkspace(windowId: Int) -> Workspace {
        if let existing = workspace(forWindow: windowId) { return existing }
        let workspace = Workspace(name: workspaces.isEmpty ? "Workspace" : "Workspace \(workspaces.count + 1)", windowId: windowId)
        workspaces.append(workspace)
        return workspace
    }

    func projects(inWindow id: Int) -> [Project] {
        let all = projects.filter { $0.windowId == id }
        return all.filter { !$0.isScratch } + all.filter { $0.isScratch && !$0.sessions.isEmpty }
    }

    func scratchProject(windowId: Int) -> Project {
        if let existing = projects.first(where: { $0.isScratch && $0.windowId == windowId }) { return existing }
        let project = Project(path: NSHomeDirectory(), lastProfile: nil)
        project.isScratch = true
        project.windowId = windowId
        projects.append(project)
        return project
    }

    func orderedProjects(inWindow id: Int) -> [Project] {
        projects(inWindow: id)
    }

    func identifier(for item: AnyObject) -> String? {
        switch item {
        case let ws as Project: ws.isScratch ? "scratch:\(ws.windowId)" : "project:" + ws.path
        case let session as Session: "tab:" + session.id
        default: nil
        }
    }

    func item(for identifier: String) -> AnyObject? {
        let parts = identifier.split(separator: ":", maxSplits: 1).map(String.init)
        guard parts.count == 2 else { return nil }
        switch parts[0] {
        case "project": return projects.first { $0.path == parts[1] && !$0.isScratch }
        case "scratch": return projects.first { $0.isScratch && "\($0.windowId)" == parts[1] }
        case "tab": return allSessions.first { $0.id == parts[1] }
        default: return nil
        }
    }

    var isRestoring = false

    func changed() {
        save()
        NotificationCenter.default.post(name: .projectsChanged, object: nil)
    }
}
