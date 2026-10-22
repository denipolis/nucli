import AppKit

final class ClearDividerSplitView: NSSplitView {
    override var dividerColor: NSColor { TerminalTheme.current.backgroundColor }
}

@MainActor
final class MainWindowController: NSWindowController, NSWindowDelegate, NSToolbarDelegate {
    let id: Int
    let split = NSSplitViewController()
    let sidebar = SidebarViewController()
    let content = ContentViewController()
    let git = GitPanelViewController()
    let files = FilesViewController()
    let history = CommitHistoryViewController()
    private(set) lazy var panel = PanelViewController(files: files, changes: git, history: history)
    private(set) var panelItem: NSSplitViewItem!
    private(set) var currentSession: Session?
    private(set) var currentProject: Project?
    var onProjectChange: ((Project) -> Void)?
    var shouldClose: ((MainWindowController) -> Bool)?

    private var state: AppState { .shared }
    var projects: [Project] { state.orderedProjects(inWindow: id) }
    var sessions: [Session] { projects.flatMap(\.sessions) }

    init(id: Int) {
        self.id = id
        let window = NSWindow(contentRect: NSRect(x: 0, y: 0, width: 1280, height: 800), styleMask: [.titled, .closable, .miniaturizable, .resizable, .fullSizeContentView], backing: .buffered, defer: false)
        super.init(window: window)
        sidebar.owner = self
        content.layout.onDrop = { [weak self] session, pane, edge in
            guard let self else { return }
            content.layout.split(pane.session, with: session, edge: edge)
            sidebar.compactGroups()
            AppState.shared.changed()
            select(session)
        }
        let splitView = ClearDividerSplitView()
        splitView.isVertical = true
        splitView.dividerStyle = .thin
        split.splitView = splitView
        content.terminals.onSelect = { [weak self] session in self?.showTerminal(session) }
        content.terminals.onClose = { session in AppController.shared.closeTerminal(session) }
        content.terminals.onNew = { AppController.shared.newTerminal(nil) }
        content.terminals.onHide = { [weak self] in self?.hideTerminals() }
        content.terminals.onMaximize = { [weak self] in self?.content.toggleMaximizedTerminals() }
        git.content = content
        history.content = content
        files.content = content
        files.git = git
        let sidebarItem = NSSplitViewItem(sidebarWithViewController: sidebar)
        sidebarItem.minimumThickness = 180
        sidebarItem.maximumThickness = 420
        sidebarItem.holdingPriority = .init(270)
        panelItem = NSSplitViewItem(viewController: panel)
        panelItem.minimumThickness = 200
        panelItem.maximumThickness = 900
        panelItem.canCollapse = true
        panelItem.holdingPriority = .init(260)
        let contentItem = NSSplitViewItem(viewController: content)
        contentItem.minimumThickness = 320
        split.addSplitViewItem(sidebarItem)
        split.addSplitViewItem(contentItem)
        split.insertSplitViewItem(panelItem, at: state.panelSide == .left ? 1 : 2)
        let suffix = id == 0 ? "" : "-\(id)"
        split.splitView.autosaveName = "Panes" + suffix
        window.contentViewController = split
        window.delegate = self
        window.isReleasedWhenClosed = false
        window.tabbingMode = .disallowed
        window.setFrameAutosaveName("Main" + suffix)
        if !window.setFrameUsingName("Main" + suffix) {
            window.setContentSize(NSSize(width: 1280, height: 800))
            window.center()
            if id > 0 { window.setFrameOrigin(NSPoint(x: window.frame.minX + CGFloat(id) * 30, y: window.frame.minY - CGFloat(id) * 30)) }
        }
        let toolbar = NSToolbar(identifier: "Main")
        toolbar.delegate = self
        toolbar.displayMode = .iconOnly
        window.toolbar = toolbar
        window.toolbarStyle = .unifiedCompact
        window.title = "Nucli"
        window.titlebarAppearsTransparent = true
        window.titleVisibility = .hidden
        window.backgroundColor = TerminalTheme.current.backgroundColor
        statusPill.onClick = { AppController.shared.commandPalette(nil) }
        if let titlebar = window.standardWindowButton(.closeButton)?.superview {
            statusPill.translatesAutoresizingMaskIntoConstraints = false
            titlebar.addSubview(statusPill)
            NSLayoutConstraint.activate([
                statusPill.centerXAnchor.constraint(equalTo: titlebar.centerXAnchor),
                statusPill.centerYAnchor.constraint(equalTo: window.standardWindowButton(.closeButton)!.centerYAnchor),
                statusPill.widthAnchor.constraint(lessThanOrEqualTo: titlebar.widthAnchor, multiplier: 0.4),
            ])
        }
        for name in [Notification.Name.projectsChanged] {
            NotificationCenter.default.addObserver(forName: name, object: nil, queue: .main) { [weak self] _ in
                MainActor.assumeIsolated { self?.updateStatus() }
            }
        }
        applyPanelPlacement()
        updateStatus()
    }

    required init?(coder: NSCoder) { fatalError() }

    private static let togglePanelItem = NSToolbarItem.Identifier("togglePanel")
    private static let workspaceItem = NSToolbarItem.Identifier("workspace")
    private let workspaceLabel = NSTextField(labelWithString: "")
    private let statusPill = StatusPill()

    var paletteAnchor: NSRect? {
        guard let window, statusPill.window === window, !statusPill.isHiddenOrHasHiddenAncestor else { return nil }
        return window.convertToScreen(statusPill.convert(statusPill.bounds, to: nil))
    }

    private var toolbarIdentifiers: [NSToolbarItem.Identifier] {
        state.panelSide == .left
            ? [.toggleSidebar, Self.workspaceItem, .sidebarTrackingSeparator, Self.togglePanelItem, .flexibleSpace]
            : [.toggleSidebar, Self.workspaceItem, .sidebarTrackingSeparator, .flexibleSpace, Self.togglePanelItem, .space]
    }

    func updateStatus() {
        workspaceLabel.stringValue = state.workspace(forWindow: id)?.name ?? ""
        statusPill.show(currentProject)
    }

    func toolbarDefaultItemIdentifiers(_ toolbar: NSToolbar) -> [NSToolbarItem.Identifier] {
        toolbarIdentifiers
    }

    func toolbarAllowedItemIdentifiers(_ toolbar: NSToolbar) -> [NSToolbarItem.Identifier] {
        [.toggleSidebar, .sidebarTrackingSeparator, .flexibleSpace, Self.togglePanelItem, .space, Self.workspaceItem]
    }

    func toolbar(_ toolbar: NSToolbar, itemForItemIdentifier id: NSToolbarItem.Identifier, willBeInsertedIntoToolbar flag: Bool) -> NSToolbarItem? {
        if id == Self.workspaceItem {
            let item = NSToolbarItem(itemIdentifier: id)
            item.label = "Workspace"
            workspaceLabel.font = .systemFont(ofSize: NSFont.systemFontSize, weight: .semibold)
            workspaceLabel.textColor = .secondaryLabelColor
            workspaceLabel.lineBreakMode = .byTruncatingTail
            workspaceLabel.toolTip = "Workspace (⌘` switches windows)"
            item.view = workspaceLabel
            return item
        }
        guard id == Self.togglePanelItem else { return nil }
        let item = NSToolbarItem(itemIdentifier: id)
        item.label = "Files and Changes"
        item.toolTip = "Show or hide Files and Changes (\(MainMenu.shortcut(for: #selector(AppController.togglePanel(_:)))))"
        item.image = NSImage(systemSymbolName: state.panelSide == .left ? "sidebar.squares.left" : "sidebar.squares.right", accessibilityDescription: item.label)
        item.action = #selector(AppController.togglePanel(_:))
        return item
    }

    func applyPanelPlacement(animated: Bool = false) {
        let index = state.panelSide == .left ? 1 : 2
        if split.splitViewItems.firstIndex(of: panelItem) != index {
            split.removeSplitViewItem(panelItem)
            split.insertSplitViewItem(panelItem, at: index)
        }
        panel.side = state.panelSide
        (animated ? panelItem.animator() : panelItem).isCollapsed = state.panelHidden
        guard let toolbar = window?.toolbar, toolbar.items.map(\.itemIdentifier) != toolbarIdentifiers else { return }
        while !toolbar.items.isEmpty { toolbar.removeItem(at: 0) }
        for (index, id) in toolbarIdentifiers.enumerated() { toolbar.insertItem(withItemIdentifier: id, at: index) }
    }

    func showPanelTab(_ tab: PanelTab) {
        panel.select(tab)
    }

    func select(_ session: Session) {
        guard session.kind != .terminal else { return showTerminal(session) }
        currentSession = session
        currentProject = session.project
        session.project.activeSession = session
        AppController.shared.clearAttention(session)
        AppController.shared.noteUsed(.session(session))
        content.show(session)
        window?.makeFirstResponder(session.terminal)
        projectDidChange()
    }

    func select(_ project: Project?) {
        if let project, let session = project.activeSession.flatMap({ active in project.sessions.first { $0 === active } }) ?? project.sessions.first {
            select(session)
            return
        }
        if let project {
            showOverview(of: project)
            return
        }
        currentSession = nil
        currentProject = nil
        content.show(nil)
        projectDidChange()
    }

    func showOverview(of project: Project) {
        currentSession = nil
        currentProject = project
        content.show(ProjectOverview(project: project))
        projectDidChange()
    }

    func showProject(_ project: Project) {
        showOverview(of: project)
        if let file = project.activeFile, project.openFiles.contains(file) { openFile(file, in: project) }
    }

    func selectFallback(from index: Int = 0) {
        let list = projects
        select(list[safe: max(0, index - 1)] ?? list.first)
    }

    func refreshTitle() {
        window?.title = state.workspace(forWindow: id)?.name ?? "Nucli"
        window?.subtitle = currentProject.map { "\($0.name) · \($0.displayPath)" } ?? ""
        updateStatus()
    }

    private func projectDidChange() {
        if let currentProject {
            currentProject.lastSelected = Date()
            onProjectChange?(currentProject)
        }
        refreshTitle()
        let folder = currentProject?.isScratch == true ? nil : currentProject
        git.project = folder
        files.project = folder
        history.project = folder
        sidebar.syncSelection()
        updateTerminals()
    }

    var terminalProject: Project? {
        currentProject ?? state.projects.first { $0.isScratch && $0.windowId == id }
    }

    func updateTerminals() {
        content.showTerminals(of: terminalProject)
    }

    func showTerminal(_ session: Session) {
        let project = session.project
        if terminalProject !== project { select(project) }
        project.activeTerminal = session
        project.showsTerminals = true
        updateTerminals()
        AppController.shared.clearAttention(session)
        window?.makeFirstResponder(session.terminal)
        state.save()
    }

    func hideTerminals() {
        terminalProject?.showsTerminals = false
        updateTerminals()
        if let currentSession, !content.isShowingFile { window?.makeFirstResponder(currentSession.terminal) }
        state.save()
    }

    func openFile(_ path: String, in project: Project, line: Int? = nil) {
        if currentProject !== project || currentSession != nil { showOverview(of: project) }
        let change = git.status?.changes.first { $0.path == path } ?? FileChange(path: path, status: " ", added: 0, removed: 0)
        content.showFileDiff(change, in: project, line: line)
    }

    func detach(_ session: Session) {
        guard content.layout.remove(session), currentSession === session else { return }
        if let next = content.layout.focused?.session {
            select(next)
        } else {
            currentSession = nil
        }
    }

    func split(_ edge: DropEdge, with session: Session) {
        guard let currentSession else { return select(session) }
        content.layout.split(currentSession, with: session, edge: edge)
        select(session)
    }

    func closePane() {
        guard let currentSession, content.layout.isSplit else { return }
        detach(currentSession)
    }

    func focusPane(_ edge: DropEdge) {
        guard let currentSession, let next = content.layout.neighbor(of: currentSession, toward: edge) else { return }
        select(next)
    }

    func cycle(_ step: Int) {
        let all = sessions
        guard !all.isEmpty else { return }
        let index = currentSession.flatMap { all.firstIndex(of: $0) } ?? -step
        select(all[(index + step + all.count) % all.count])
    }

    func windowDidBecomeKey(_ notification: Notification) {
        UserDefaults.standard.set(id, forKey: "lastWindow")
        AppController.shared.clearAttention(currentSession)
    }

    func windowShouldClose(_ sender: NSWindow) -> Bool {
        shouldClose?(self) ?? true
    }
}
