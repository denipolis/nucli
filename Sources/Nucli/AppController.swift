import AppKit
import SwiftTerm
import UserNotifications

@MainActor
final class AppController: NSObject, NSApplicationDelegate, NSWindowDelegate, NSToolbarDelegate, UNUserNotificationCenterDelegate {
    static var shared: AppController { NSApp.delegate as! AppController }

    let state = AppState.shared
    private var windows: [MainWindowController] = []
    private var settings: SettingsWindowController?
    private var picker: ProfilePicker?
    var palette: Palette?
    private let tabSwitcher = TabSwitcher()
    private var recentSessions: [String] = []
    private var appearanceObservation: NSKeyValueObservation?
    let conversations = ConversationCache()
    private var terminationSource: DispatchSourceSignal?

    func applicationDidFinishLaunching(_ notification: Notification) {
        NSApp.mainMenu = MainMenu.build()
        let last = UserDefaults.standard.integer(forKey: "lastWindow")
        var ids = state.windowIds.filter { state.workspace(forWindow: $0)?.isClosed != true }
        if ids.isEmpty { ids = [state.windowIds.contains(last) ? last : state.windowIds.first ?? 0] }
        ids.forEach { state.ensureWorkspace(windowId: $0).isClosed = false }
        for id in ids {
            _ = makeWindow(id: id)
        }
        installKeyMonitor()
        applyAppearance()
        if canNotify { UNUserNotificationCenter.current().delegate = self }
        if !state.didDetectAgents { state.detectAgents { _ in } }
        startForegroundPolling()
        handleTerminationSignal()
        appearanceObservation = NSApp.observe(\.effectiveAppearance) { [weak self] _, _ in
            DispatchQueue.main.async { self?.applyTheme() }
        }
        windows.forEach { $0.window?.layoutIfNeeded() }
        restoreTabs(in: state.projects.filter { ids.contains($0.windowId) })
        state.projects.forEach { conversations.refresh($0.path, profiles: state.profiles) }
        windows.sorted { ($0.id == last ? 1 : 0, $1.id) < ($1.id == last ? 1 : 0, $0.id) }.forEach { $0.showWindow(nil) }
        NSApp.activate()
    }

    func applicationShouldTerminateAfterLastWindowClosed(_ sender: NSApplication) -> Bool {
        true
    }

    func applicationShouldTerminate(_ sender: NSApplication) -> NSApplication.TerminateReply {
        if windows.contains(where: \.content.hasUnsavedFiles), !confirm("Quit Nucli?", "Some open files have unsaved changes. They will be lost.", button: "Quit") { return .terminateCancel }
        let running = state.allSessions.filter(\.isRunning).count
        guard running > 0 else { return .terminateNow }
        return confirm("Quit Nucli?", "\(running) running \(running == 1 ? "session" : "sessions") will be closed and reopened on next launch.", button: "Quit") ? .terminateNow : .terminateCancel
    }

    func applicationWillTerminate(_ notification: Notification) {
        resolveConversationIds()
        let kept = Set(state.projects.flatMap(\.restoredTabs).compactMap { $0["scrollback"].map { $0 + ".txt" } })
        for file in (try? FileManager.default.contentsOfDirectory(at: Session.scrollbackDirectory, includingPropertiesForKeys: nil)) ?? [] where !kept.contains(file.lastPathComponent) {
            try? FileManager.default.removeItem(at: file)
        }
        state.allSessions.filter { $0.kind != .agent && $0.isRunning }.forEach { $0.saveScrollback() }
        windows.forEach { $0.currentProject?.lastSelected = Date() }
        state.save()
        state.allSessions.forEach { $0.kill() }
    }

    private func handleTerminationSignal() {
        signal(SIGTERM, SIG_IGN)
        let source = DispatchSource.makeSignalSource(signal: SIGTERM, queue: .main)
        source.setEventHandler { [weak self] in
            MainActor.assumeIsolated {
                self?.applicationWillTerminate(Notification(name: NSApplication.willTerminateNotification))
                exit(0)
            }
        }
        source.resume()
        terminationSource = source
    }

    private func resolveConversationIds() {
        var claimed = Set(state.allSessions.compactMap(\.conversationId))
        for session in state.allSessions where session.conversationId == nil {
            guard let profile = session.profile, let kind = profile.kind, kind == .codex else { continue }
            let id = ConversationHistory.newestCodexConversation(dataDir: kind.dataDirectory(for: profile), cwd: session.project.path, startedAfter: session.startedAt.addingTimeInterval(-5), excluding: claimed)
            session.conversationId = id
            id.map { claimed.insert($0) }
        }
    }

    var active: MainWindowController? {
        (NSApp.keyWindow?.windowController as? MainWindowController)
            ?? (NSApp.mainWindow?.windowController as? MainWindowController)
            ?? windows.first
    }

    func controller(for project: Project) -> MainWindowController {
        windows.first { $0.id == project.windowId } ?? makeWindow(id: project.windowId)
    }

    @discardableResult
    private func makeWindow(id: Int) -> MainWindowController {
        let workspace = state.ensureWorkspace(windowId: id)
        let reopening = workspace.isClosed
        workspace.isClosed = false
        let controller = MainWindowController(id: id)
        controller.onProjectChange = { [weak self] project in
            guard let self else { return }
            conversations.refresh(project.path, profiles: state.profiles)
        }
        controller.shouldClose = { [weak self] controller in self?.closeWindow(controller) ?? true }
        windows.append(controller)
        if reopening {
            controller.window?.layoutIfNeeded()
            restoreTabs(in: state.projects(inWindow: id))
            state.changed()
        }
        return controller
    }

    private func closeWindow(_ controller: MainWindowController) -> Bool {
        if controller.content.hasUnsavedFiles, !confirm("Close workspace?", "Some open files have unsaved changes. They will be lost.", button: "Close") { return false }
        let projects = state.projects.filter { $0.windowId == controller.id }
        let sessions = projects.flatMap { $0.sessions + $0.terminals }
        let name = state.workspace(forWindow: controller.id)?.name ?? "Workspace"
        let running = sessions.filter(\.isRunning).count
        guard running == 0 || confirm("Close workspace \(name)?", "\(running) running \(running == 1 ? "session" : "sessions") will be closed and reopened when you open the workspace again.", button: "Close") else { return false }
        resolveConversationIds()
        sessions.filter { $0.kind != .agent && $0.isRunning }.forEach { $0.saveScrollback() }
        controller.currentProject?.lastSelected = Date()
        for project in projects {
            project.restoredTabs = state.tabs(of: project)
            project.restoredActiveTab = state.activeTab(of: project)
        }
        sessions.forEach { $0.kill() }
        projects.forEach { $0.sessions.removeAll(); $0.terminals.removeAll() }
        state.workspace(forWindow: controller.id)?.isClosed = true
        windows.removeAll { $0 === controller }
        state.changed()
        updateDockBadge()
        return true
    }

    @objc func deleteWorkspace(_ sender: Any?) {
        guard let controller = active else { return }
        let projects = state.projects.filter { $0.windowId == controller.id }
        let sessions = projects.flatMap { $0.sessions + $0.terminals }
        let name = state.workspace(forWindow: controller.id)?.name ?? "Workspace"
        let running = sessions.filter(\.isRunning).count
        let detail = (running > 0 ? "\(running) running \(running == 1 ? "session" : "sessions") will be terminated. " : "") + "Its projects will be removed from Nucli. Files on disk are not touched."
        guard confirm("Delete workspace \(name)?", detail, button: "Delete") else { return }
        sessions.forEach { $0.kill() }
        projects.forEach { $0.sessions.removeAll(); $0.terminals.removeAll() }
        state.projects.removeAll { $0.windowId == controller.id }
        state.workspaces.removeAll { $0.windowId == controller.id }
        windows.removeAll { $0 === controller }
        state.changed()
        updateDockBadge()
        controller.window?.close()
    }

    func openFolders(_ paths: [String], windowId: Int) {
        let opened = paths.map { state.project(for: $0, windowId: windowId) }
        state.changed()
        if let last = opened.last { select(last) }
    }

    func application(_ sender: NSApplication, open urls: [URL]) {
        let folders = urls.filter { (try? $0.resourceValues(forKeys: [.isDirectoryKey]).isDirectory) == true }.map(\.path)
        guard !folders.isEmpty else { return }
        openFolders(folders, windowId: active?.id ?? 0)
    }

    func reconcileSelections() {
        for controller in windows {
            for session in controller.content.layout.allSessions where session.project.windowId != controller.id || !session.project.sessions.contains(where: { $0 === session }) {
                controller.detach(session)
            }
            if let session = controller.currentSession {
                if session.project.windowId != controller.id || !session.project.sessions.contains(where: { $0 === session }) {
                    controller.selectFallback()
                } else if session.project !== controller.currentProject {
                    controller.select(session)
                }
            } else if let project = controller.currentProject, project.windowId != controller.id || !state.projects.contains(where: { $0 === project }) {
                controller.selectFallback()
            }
        }
    }

    func select(_ session: Session) {
        let controller = controller(for: session.project)
        controller.select(session)
        controller.showWindow(nil)
    }

    func select(_ project: Project) {
        let controller = controller(for: project)
        controller.select(project)
        controller.showWindow(nil)
    }

    private func restoreTabs(in projects: [Project]) {
        state.isRestoring = true
        defer {
            state.isRestoring = false
            state.save()
        }
        for project in projects {
            if !state.restoresTabs { project.restoredTabs = [] }
            for tab in project.restoredTabs {
                if tab["kind"] == "terminal" {
                    let cwd = tab["cwd"].flatMap { FileManager.default.fileExists(atPath: $0) ? $0 : nil }
                    let shell = tab["shell"].map(\.expandingHome).flatMap { FileManager.default.isExecutableFile(atPath: $0) ? $0 : nil }
                    let session = open(.terminal, profile: nil, in: project, directory: cwd, shell: shell, select: false)
                    session.customName = tab["name"]
                    session.color = tab["color"].flatMap(ItemColor.init(rawValue:))
                    tab["scrollback"].map(session.restoreScrollback)
                } else if tab["kind"] == "shell", let shell = tab["shell"], FileManager.default.isExecutableFile(atPath: shell.expandingHome) {
                    let cwd = tab["cwd"].flatMap { FileManager.default.fileExists(atPath: $0) ? $0 : nil }
                    let session = open(.shell, profile: nil, in: project, directory: cwd, shell: shell.expandingHome, select: false)
                    session.customName = tab["name"]
                    session.color = tab["color"].flatMap(ItemColor.init(rawValue:))
                    tab["scrollback"].map(session.restoreScrollback)
                } else if let profile = state.profiles.first(where: { $0.name == tab["profile"] }) {
                    var id = tab["conversation"]
                    if let current = id, profile.kind == .claude,
                       !ConversationHistory.claudeConversationExists(dataDir: AgentKind.claude.dataDirectory(for: profile), cwd: project.path, id: current) {
                        id = nil
                    }
                    let session = open(.agent, profile: profile, resume: id, in: project, select: false)
                    session.customName = tab["name"]
                    session.color = tab["color"].flatMap(ItemColor.init(rawValue:))
                }
            }
            project.restoredTabs = []
            if let index = project.restoredActiveTab { project.activeSession = project.sessions[safe: index] }
        }
        for controller in windows where projects.contains(where: { $0.windowId == controller.id }) {
            controller.select(controller.projects.max { $0.lastSelected < $1.lastSelected })
        }
    }

    private func startForegroundPolling() {
        Timer.scheduledTimer(withTimeInterval: 1, repeats: true) { [weak self] _ in
            MainActor.assumeIsolated {
                guard let self else { return }
                let known = Set(AgentDetector.knownCommands + self.state.profiles.map(\.executableName))
                self.state.allSessions.forEach { $0.refreshForeground(knownAgents: known) }
            }
        }
    }

    func applyTheme() {
        state.allSessions.forEach { session in
            TerminalTheme.forProfile(state.profiles.first { $0.name == session.profile?.name } ?? session.profile).apply(to: session.terminal)
        }
        windows.forEach {
            $0.content.applyTheme()
            $0.panel.applyTheme()
        }
        NotificationCenter.default.post(name: .settingsChanged, object: nil)
    }

    @discardableResult
    private func open(_ kind: SessionKind, profile: AgentProfile?, resume conversationId: String? = nil, in project: Project, directory: String? = nil, shell: String? = nil, select shouldSelect: Bool = true) -> Session {
        var command: String?
        var id = conversationId
        if let profile {
            if let conversationId, let kind = profile.kind {
                command = profile.launchCommand(extraArguments: kind.resumeArguments(conversationId: conversationId))
            } else {
                id = profile.kind?.assignsConversationId == true ? UUID().uuidString.lowercased() : nil
                command = profile.launchCommand(extraArguments: profile.kind?.newArguments(conversationId: id) ?? [])
            }
        }
        let controller = controller(for: project)
        let bounds = controller.content.view.bounds.size
        let size = bounds.width > 100 && bounds.height > 100 ? NSSize(width: bounds.width - 20, height: bounds.height - 60) : NSSize(width: 800, height: 600)
        let session = Session(kind: kind, profile: profile, command: command, conversationId: id, project: project, directory: directory, shell: shell ?? (kind == .terminal ? state.terminalShell : state.shell), font: state.font, size: size)
        session.onExit = { [weak self] session, code in self?.sessionExited(session, code: code) }
        session.onAttention = { [weak self] session, message in self?.attention(session, message: message) }
        session.onOpenFile = { [weak self] session, path, line in self?.openFile(path, line: line, from: session) }
        if kind == .terminal {
            project.terminals.append(session)
            if shouldSelect || project.activeTerminal == nil { project.activeTerminal = session }
        } else {
            project.sessions.append(session)
            project.isCollapsed = false
        }
        state.changed()
        if shouldSelect { select(session) }
        return session
    }

    private func openFile(_ path: String, line: Int?, from session: Session) {
        let owners = ([session.project] + state.projects).filter { !$0.isScratch && path.hasPrefix($0.path + "/") }
        guard let project = owners.first else {
            NSWorkspace.shared.open(URL(fileURLWithPath: path))
            return
        }
        controller(for: project).openFile(String(path.dropFirst(project.path.count + 1)), in: project, line: line)
    }

    private var canNotify: Bool { Bundle.main.bundleIdentifier != nil }

    private func isWatched(_ session: Session) -> Bool {
        NSApp.isActive && windows.contains { $0.window?.isKeyWindow == true && ($0.content.terminals.shown === session && !$0.content.terminals.isHidden || $0.currentSession === session && !$0.content.isShowingFile) }
    }

    private func attention(_ session: Session, message: String?) {
        guard !isWatched(session) else { return }
        let isNew = !session.needsAttention
        session.needsAttention = true
        updateDockBadge()
        guard isNew || message != nil, state.notifications, canNotify else { return }
        let center = UNUserNotificationCenter.current()
        let content = UNMutableNotificationContent()
        content.title = session.title
        content.subtitle = session.project.name
        content.body = message.flatMap { $0.isEmpty ? nil : $0 } ?? (session.runningAgent != nil ? "Finished or waiting for input" : "Bell")
        content.sound = .default
        content.userInfo = ["session": session.id]
        center.requestAuthorization(options: [.alert, .sound]) { granted, _ in
            guard granted else { return }
            center.add(UNNotificationRequest(identifier: session.id, content: content, trigger: nil))
        }
    }

    func clearAttention(_ session: Session?) {
        guard let session, session.needsAttention else { return }
        session.needsAttention = false
        updateDockBadge()
        if canNotify { UNUserNotificationCenter.current().removeDeliveredNotifications(withIdentifiers: [session.id]) }
    }

    private func updateDockBadge() {
        let count = state.allSessions.filter(\.needsAttention).count
        NSApp.dockTile.badgeLabel = count > 0 ? "\(count)" : nil
    }

    func applicationDidBecomeActive(_ notification: Notification) {
        clearAttention(active?.currentSession)
    }

    nonisolated func userNotificationCenter(_ center: UNUserNotificationCenter, didReceive response: UNNotificationResponse, withCompletionHandler completionHandler: @escaping () -> Void) {
        let id = response.notification.request.content.userInfo["session"] as? String
        DispatchQueue.main.async {
            MainActor.assumeIsolated {
                if let id, let session = self.state.allSessions.first(where: { $0.id == id }) { self.select(session) }
                NSApp.activate()
            }
        }
        completionHandler()
    }

    nonisolated func userNotificationCenter(_ center: UNUserNotificationCenter, willPresent notification: UNNotification, withCompletionHandler completionHandler: @escaping (UNNotificationPresentationOptions) -> Void) {
        completionHandler([.banner, .sound])
    }

    private func sessionExited(_ session: Session, code: Int32?) {
        conversations.refresh(session.project.path, profiles: state.profiles)
        if code == 0 {
            remove(session)
        } else {
            session.terminal.feed(text: "\r\n\u{1b}[2m[Process exited with code \(code.map(String.init) ?? "?")]\u{1b}[0m\r\n")
        }
    }

    func closeTerminal(_ session: Session) {
        remove(session)
    }

    private func remove(_ session: Session) {
        session.kill()
        let project = session.project
        let controller = controller(for: project)
        if session.kind == .terminal {
            let index = project.terminals.firstIndex { $0 === session } ?? 0
            let hadFocus = controller.content.terminals.containsFocus
            project.terminals.removeAll { $0 === session }
            clearAttention(session)
            if project.activeTerminal == nil || project.activeTerminal === session { project.activeTerminal = project.terminals[safe: max(0, index - 1)] }
            if project.terminals.isEmpty { project.showsTerminals = false }
            state.changed()
            controller.updateTerminals()
            if hadFocus, let next = project.activeTerminal {
                controller.window?.makeFirstResponder(next.terminal)
            } else if hadFocus, let current = controller.currentSession {
                controller.window?.makeFirstResponder(current.terminal)
            }
            return
        }
        let all = project.sessions
        let index = all.firstIndex { $0 === session } ?? 0
        project.sessions.removeAll { $0 === session }
        clearAttention(session)
        state.changed()
        let wasCurrent = controller.currentSession === session
        windows.forEach { $0.content.layout.remove(session) }
        guard wasCurrent else { return }
        if let next = controller.content.layout.focused?.session {
            controller.select(next)
        } else if let next = project.sessions[safe: index] ?? project.sessions[safe: index - 1] {
            controller.select(next)
        } else {
            controller.select(project.isScratch ? nil : project)
        }
    }

    private func withProject(askFolder: Bool, _ body: @escaping (Project) -> Void) {
        guard let controller = active ?? Optional(makeWindow(id: (state.windowIds.max() ?? -1) + 1)), let window = controller.window else { return }
        if !askFolder {
            body(controller.currentProject ?? state.scratchProject(windowId: controller.id))
            return
        }
        guard window.attachedSheet == nil else { return }
        let panel = NSOpenPanel()
        panel.canChooseFiles = false
        panel.canChooseDirectories = true
        panel.canCreateDirectories = true
        panel.prompt = "Open"
        panel.message = "Choose a folder for the project"
        panel.directoryURL = state.lastFolder.map { URL(fileURLWithPath: $0) }
        panel.beginSheetModal(for: window) { [weak self] response in
            guard let self, response == .OK, let url = panel.url else { return }
            state.lastFolder = url.deletingLastPathComponent().path
            state.save()
            let project = state.project(for: url.path, windowId: controller.id)
            DispatchQueue.main.async { body(project) }
        }
    }

    private func pickAgent(in project: Project) {
        let controller = controller(for: project)
        guard let window = controller.window, window.attachedSheet == nil else { return }
        let profiles = state.profiles.filter(\.isLaunchable)
        let shells = state.shells.filter(\.isLaunchable)
        guard !profiles.isEmpty || !shells.isEmpty else {
            showSettings(nil)
            return
        }
        let picker = ProfilePicker(profiles: profiles, shells: shells, last: project.lastProfile ?? state.lastProfile, title: "New Tab in \(project.displayPath)", parent: window) { [weak self] item in
            guard let self else { return }
            self.picker = nil
            switch item {
            case .profile(let profile):
                startAgent(profile, in: project)
            case .shell(let shell):
                startShell(shell, in: project)
            case .conversation(let conversation, _):
                resume(conversation, in: project)
            case .header, nil:
                break
            }
        }
        self.picker = picker
        let openIds = Set(project.sessions.filter(\.isRunning).compactMap(\.conversationId))
        let mark = { (list: [Conversation]) in list.map { ($0, openIds.contains($0.id)) } }
        picker.show(conversations: conversations.cached(for: project.path).map(mark))
        conversations.refresh(project.path, profiles: profiles) { [weak picker] list in
            picker?.show(conversations: mark(list))
        }
    }

    func startAgent(_ profile: AgentProfile, in project: Project) {
        project.lastProfile = profile.name
        state.lastProfile = profile.name
        state.save()
        open(.agent, profile: profile, in: project)
    }

    func startShell(_ shell: ShellProfile, in project: Project) {
        project.lastProfile = shell.name
        state.lastProfile = shell.name
        state.lastShell = shell.name
        state.save()
        open(.shell, profile: nil, in: project, shell: shell.path.expandingHome)
    }

    func startTerminal(in project: Project) {
        open(.terminal, profile: nil, in: project)
    }

    func resume(_ conversation: Conversation, in project: Project) {
        if let session = project.sessions.first(where: { $0.conversationId == conversation.id && $0.isRunning }) {
            select(session)
        } else {
            open(.agent, profile: conversation.profile, resume: conversation.id, in: project)
        }
    }

    func open(_ entry: HistoryEntry) {
        let windowId = active?.id ?? 0
        guard FileManager.default.fileExists(atPath: entry.cwd) else { return }
        let project = state.project(for: entry.cwd, windowId: windowId)
        resume(entry.conversation, in: project)
        controller(for: project).showWindow(nil)
    }

    @objc func showSidebarTab(_ sender: NSMenuItem) {
        guard let controller = active else { return }
        if let item = controller.split.splitViewItems.first, item.isCollapsed { item.isCollapsed = false }
        controller.sidebar.showTab(sender.tag)
    }

    func showOverview(of project: Project) {
        let controller = controller(for: project)
        controller.showOverview(of: project)
        controller.showWindow(nil)
    }

    func controller(forWindowId id: Int) -> MainWindowController {
        windows.first { $0.id == id } ?? makeWindow(id: id)
    }

    @objc func newTerminal(_ sender: Any?) {
        withProject(askFolder: false) { [weak self] in self?.open(.terminal, profile: nil, in: $0) }
    }

    @objc func newAgent(_ sender: Any?) {
        withProject(askFolder: false) { [weak self] in self?.pickAgent(in: $0) }
    }

    @objc func newAgentInFolder(_ sender: Any?) {
        withProject(askFolder: true) { [weak self] in self?.pickAgent(in: $0) }
    }

    @objc func newTerminal(in sender: NSMenuItem) {
        guard let project = sender.representedObject as? Project else { return }
        open(.terminal, profile: nil, in: project)
    }

    @objc func newAgent(in sender: NSMenuItem) {
        guard let project = sender.representedObject as? Project else { return }
        pickAgent(in: project)
    }

    @objc func openProject(_ sender: Any?) {
        withProject(askFolder: true) { [weak self] in self?.select($0) }
    }

    @objc func newWindow(_ sender: Any?) {
        newEmptyWindow()?.showWindow(nil)
    }

    var windowControllers: [MainWindowController] { windows }

    func newEmptyWindow() -> MainWindowController? {
        guard let name = ask("New Workspace", "Each workspace is its own window, e.g. Personal and Work.", value: "", placeholder: "Workspace name"), !name.isEmpty else { return nil }
        let id = (windows.map(\.id) + state.windowIds).max().map { $0 + 1 } ?? 0
        state.workspaces.append(Workspace(name: name, windowId: id))
        let controller = makeWindow(id: id)
        controller.select(nil as Project?)
        state.changed()
        return controller
    }

    @objc func revealProject(_ sender: NSMenuItem) {
        guard let project = sender.representedObject as? Project else { return }
        NSWorkspace.shared.selectFile(nil, inFileViewerRootedAtPath: project.path)
    }

    @objc func closeSession(_ sender: Any?) {
        let explicit = (sender as? NSMenuItem)?.representedObject as? Session
        if explicit == nil, let content = active?.content, content.isShowingFile {
            content.closeFileTab()
            return
        }
        if explicit == nil, let controller = active, controller.content.terminals.containsFocus, let terminal = controller.content.terminals.shown {
            remove(terminal)
            return
        }
        guard let session = explicit ?? active?.currentSession else {
            active?.select(nil)
            return
        }
        if session.kind == .agent, session.isRunning, !confirmCloseAgent(session) { return }
        remove(session)
    }

    @objc func closeProject(_ sender: Any?) {
        guard let project = (sender as? NSMenuItem)?.representedObject as? Project ?? active?.currentProject else { return }
        close(project)
    }

    private func close(_ project: Project) {
        let running = (project.sessions + project.terminals).filter(\.isRunning).count
        if running > 0, !confirm("Close \(project.name)?", "\(running) running \(running == 1 ? "session" : "sessions") will be terminated.", button: "Close") {
            return
        }
        let controller = controller(for: project)
        (project.sessions + project.terminals).forEach { $0.kill() }
        project.sessions.removeAll()
        project.terminals.removeAll()
        let index = controller.projects.firstIndex { $0 === project } ?? 0
        state.remove(project)
        if controller.currentProject === project {
            controller.selectFallback(from: index)
        }
    }

    @objc func splitRight(_ sender: Any?) { splitAgent(.right) }
    @objc func splitDown(_ sender: Any?) { splitAgent(.bottom) }

    @objc func toggleTerminalPanel(_ sender: Any?) {
        withProject(askFolder: false) { [weak self] project in
            guard let self else { return }
            let controller = controller(for: project)
            if project.terminals.isEmpty {
                open(.terminal, profile: nil, in: project)
            } else if !project.showsTerminals || !controller.content.terminals.containsFocus {
                project.activeTerminal.map(controller.showTerminal)
            } else {
                controller.hideTerminals()
            }
        }
    }

    private func lastProfile(in project: Project) -> PickerItem? {
        let profiles = state.profiles.filter(\.isLaunchable)
        let shells = state.shells.filter(\.isLaunchable)
        let last = project.lastProfile ?? state.lastProfile
        if let profile = profiles.first(where: { $0.name == last }) { return .profile(profile) }
        if let shell = shells.first(where: { $0.name == last }) { return .shell(shell) }
        return profiles.first.map { .profile($0) } ?? shells.first.map { .shell($0) }
    }

    @objc func newAgentWithLastProfile(_ sender: Any?) {
        withProject(askFolder: false) { [weak self] project in
            guard let self else { return }
            switch lastProfile(in: project) {
            case .profile(let profile): startAgent(profile, in: project)
            case .shell(let shell): startShell(shell, in: project)
            default: pickAgent(in: project)
            }
        }
    }

    private func splitAgent(_ edge: DropEdge) {
        withProject(askFolder: false) { [weak self] project in
            guard let self else { return }
            let session: Session
            switch lastProfile(in: project) {
            case .profile(let profile): session = open(.agent, profile: profile, in: project, select: false)
            case .shell(let shell): session = open(.shell, profile: nil, in: project, shell: shell.path.expandingHome, select: false)
            default: return pickAgent(in: project)
            }
            let controller = controller(for: project)
            if let current = controller.currentSession, let index = project.sessions.firstIndex(where: { $0 === current }) {
                project.sessions.removeAll { $0 === session }
                project.sessions.insert(session, at: index + 1)
                state.changed()
            }
            controller.split(edge, with: session)
        }
    }

    @objc func closePane(_ sender: Any?) { active?.closePane() }

    @objc func removeFromSplit(_ sender: NSMenuItem) {
        guard let session = sender.representedObject as? Session else { return }
        controller(for: session.project).detach(session)
    }

    @objc func unsplit(_ sender: NSMenuItem) {
        guard let session = sender.representedObject as? Session else { return }
        controller(for: session.project).content.layout.unsplit(session)
    }
    @objc func validateMenuItem(_ item: NSMenuItem) -> Bool {
        switch item.action {
        case #selector(nextChange(_:)), #selector(previousChange(_:)): active?.content.isShowingFile == true
        case #selector(closePane(_:)), #selector(focusPane(_:)): active?.content.layout.isSplit == true
        default: true
        }
    }

    @objc func nextChange(_ sender: Any?) { active?.content.fileView?.nextChange() }
    @objc func previousChange(_ sender: Any?) { active?.content.fileView?.previousChange() }

    @objc func focusPane(_ sender: NSMenuItem) {
        let edges: [DropEdge] = [.left, .right, .top, .bottom]
        edges[safe: sender.tag].map { active?.focusPane($0) }
    }

    @objc func selectNextSession(_ sender: Any?) { switchTabs(1) }
    @objc func selectPreviousSession(_ sender: Any?) { switchTabs(-1) }

    func noteUsed(_ entry: SwitcherEntry) {
        recentSessions.removeAll { $0 == entry.id }
        recentSessions.insert(entry.id, at: 0)
    }

    private func switchTabs(_ delta: Int, modifier: NSEvent.ModifierFlags? = nil) {
        guard let controller = active else { return }
        let held = modifier ?? (NSApp.currentEvent?.type == .keyDown ? NSApp.currentEvent?.modifierFlags.intersection([.command, .control]) : nil)
        guard let held, !held.isEmpty else { return controller.cycle(delta) }
        let order = Dictionary(uniqueKeysWithValues: recentSessions.enumerated().map { ($1, $0) })
        let files = controller.projects.filter { project in project.activeFile.map(project.openFiles.contains) == true && order["files:" + project.path] != nil }
        let entries = (controller.sessions.map(SwitcherEntry.session) + files.map(SwitcherEntry.files)).enumerated()
        let sorted = entries.sorted { (order[$0.element.id] ?? Int.max / 2 + $0.offset) < (order[$1.element.id] ?? Int.max / 2 + $1.offset) }.map(\.element)
        tabSwitcher.step(delta, in: controller, entries: sorted, modifier: held)
    }

    @objc func selectSessionByNumber(_ sender: NSMenuItem) {
        guard let controller = active, let session = controller.sessions[safe: sender.tag - 1] else { return }
        controller.select(session)
    }

    @objc func clearTerminal(_ sender: Any?) {
        guard let terminal = active?.currentSession?.terminal else { return }
        terminal.feed(text: "\u{1b}[3J")
        terminal.send([0x0c])
    }

    @objc func biggerFont(_ sender: Any?) { setFontSize(state.fontSize + 1) }
    @objc func smallerFont(_ sender: Any?) { setFontSize(state.fontSize - 1) }
    @objc func resetFont(_ sender: Any?) { setFontSize(13) }

    func setFontSize(_ size: CGFloat) {
        state.fontSize = min(max(size, 8), 32)
        applyFont()
    }

    func applyFont() {
        state.save()
        state.allSessions.forEach {
            $0.terminal.font = state.font
            $0.applyTerminalOptions()
        }
        NotificationCenter.default.post(name: .settingsChanged, object: nil)
    }

    func applyAppearance() {
        state.save()
        if TerminalTheme.isAutomatic(state.theme) {
            NSApp.appearance = state.appearance.appearance
        } else {
            NSApp.appearance = NSAppearance(named: TerminalTheme.resolve(state.theme).isDark ? .darkAqua : .aqua)
        }
        applyTheme()
    }

    func applyKeyBindings() {
        state.save()
        NSApp.mainMenu = MainMenu.build()
        NotificationCenter.default.post(name: .settingsChanged, object: nil)
        NotificationCenter.default.post(name: .projectsChanged, object: nil)
    }

    @objc func toggleSidebarPane(_ sender: Any?) { active?.split.toggleSidebar(sender) }
    @objc func togglePanel(_ sender: Any?) {
        state.panelHidden.toggle()
        applyPanelPlacement()
    }

    func showChanges(of project: Project) {
        if state.panelHidden {
            state.panelHidden = false
            applyPanelPlacement()
        }
        controller(for: project).showPanelTab(.changes)
    }

    @objc func movePanel(_ sender: Any?) {
        setPanelSide(state.panelSide == .left ? .right : .left)
    }

    func setPanelSide(_ side: PanelSide) {
        state.panelSide = side
        state.panelHidden = false
        applyPanelPlacement()
    }

    private func applyPanelPlacement() {
        state.save()
        windows.forEach { $0.applyPanelPlacement(animated: true) }
    }

    @objc func filterSidebar(_ sender: Any?) {
        guard let controller = active else { return }
        if let item = controller.split.splitViewItems.first, item.isCollapsed { item.isCollapsed = false }
        controller.sidebar.focusFilter()
    }
    @objc func showPanelTab(_ sender: NSMenuItem) {
        guard let tab = PanelTab(rawValue: sender.tag) else { return }
        if state.panelHidden {
            state.panelHidden = false
            applyPanelPlacement()
        }
        active?.showPanelTab(tab)
    }

    @objc func showAbout(_ sender: Any?) {
        let credits = NSMutableAttributedString(string: "Third-party software\n\n", attributes: [.font: NSFont.boldSystemFont(ofSize: 11), .foregroundColor: NSColor.labelColor])
        let notices = Bundle.main.resourceURL?.appendingPathComponent("Notices")
        let files = ((try? FileManager.default.contentsOfDirectory(at: notices ?? URL(fileURLWithPath: "/nonexistent"), includingPropertiesForKeys: nil)) ?? []).sorted { $0.lastPathComponent < $1.lastPathComponent }
        for file in files {
            guard let text = try? String(contentsOf: file, encoding: .utf8) else { continue }
            credits.append(NSAttributedString(string: file.deletingPathExtension().lastPathComponent.replacingOccurrences(of: "-", with: " ") + "\n", attributes: [.font: NSFont.boldSystemFont(ofSize: 10), .foregroundColor: NSColor.labelColor]))
            credits.append(NSAttributedString(string: text.trimmingCharacters(in: .whitespacesAndNewlines) + "\n\n", attributes: [.font: NSFont.systemFont(ofSize: 10), .foregroundColor: NSColor.secondaryLabelColor]))
        }
        NSApp.orderFrontStandardAboutPanel(options: [.credits: credits])
        NSApp.activate()
    }

    @objc func showSettings(_ sender: Any?) {
        if settings == nil {
            settings = SettingsWindowController()
            settings?.window?.center()
        }
        settings?.reload()
        settings?.showWindow(nil)
    }

    private func confirmCloseAgent(_ session: Session) -> Bool {
        let skipUntil = UserDefaults.standard.object(forKey: "skipCloseAgentConfirmUntil") as? Date ?? .distantPast
        guard skipUntil < Date(), Date().timeIntervalSince(session.lastOutput) < 7200 else { return true }
        let alert = NSAlert()
        alert.messageText = "Close \(session.title)?"
        alert.informativeText = "The agent process will be terminated."
        alert.addButton(withTitle: "Close")
        alert.addButton(withTitle: "Cancel")
        alert.showsSuppressionButton = true
        alert.suppressionButton?.title = "Don't ask again for 1 hour"
        guard alert.runModal() == .alertFirstButtonReturn else { return false }
        if alert.suppressionButton?.state == .on { UserDefaults.standard.set(Date().addingTimeInterval(3600), forKey: "skipCloseAgentConfirmUntil") }
        return true
    }

    private func confirm(_ title: String, _ text: String, button: String) -> Bool {
        let alert = NSAlert()
        alert.messageText = title
        alert.informativeText = text
        alert.addButton(withTitle: button)
        alert.addButton(withTitle: "Cancel")
        return alert.runModal() == .alertFirstButtonReturn
    }

    private func installKeyMonitor() {
        NSEvent.addLocalMonitorForEvents(matching: .leftMouseDown) { event in
            guard let controller = event.window?.windowController as? MainWindowController, controller.content.layout.isSplit,
                  let hit = event.window?.contentView?.hitTest(event.locationInWindow) else { return event }
            var view: NSView? = hit
            while let current = view, !(current is PaneView) { view = current.superview }
            if let pane = view as? PaneView, pane.session !== controller.currentSession { controller.select(pane.session) }
            return event
        }
        NSEvent.addLocalMonitorForEvents(matching: .flagsChanged) { [weak self] event in
            guard let self, tabSwitcher.isVisible, !event.modifierFlags.contains(tabSwitcher.modifier) else { return event }
            tabSwitcher.commit()
            return event
        }
        NSEvent.addLocalMonitorForEvents(matching: .keyDown) { [weak self] event in
            guard let self else { return event }
            let mods = event.modifierFlags.intersection([.command, .shift, .option, .control])
            if event.keyCode == 48, mods == .control || mods == [.control, .shift], event.window?.windowController is MainWindowController {
                switchTabs(mods.contains(.shift) ? -1 : 1, modifier: .control)
                return nil
            }
            if tabSwitcher.isVisible, event.keyCode == 53 {
                tabSwitcher.close()
                return nil
            }
            guard let terminal = event.window?.firstResponder as? TerminalView else { return event }
            switch (mods, event.keyCode) {
            case (.command, 123): terminal.send([0x01])
            case (.command, 124): terminal.send([0x05])
            case (.command, 51): terminal.send([0x15])
            case (.option, 123): terminal.send([0x1b, 0x62])
            case (.option, 124): terminal.send([0x1b, 0x66])
            case (.option, 51): terminal.send([0x17])
            default: return event
            }
            return nil
        }
    }
}
