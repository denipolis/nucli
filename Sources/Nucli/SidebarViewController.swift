import AppKit

final class SidebarCell: NSTableCellView {
    let icon = NSImageView()
    let title = NSTextField(labelWithString: "")
    let subtitle = NSTextField(labelWithString: "")
    let status = NSImageView()
    let spinner = NSProgressIndicator()
    let diff = NSTextField(labelWithString: "")

    init() {
        super.init(frame: .zero)
        title.lineBreakMode = .byTruncatingTail
        title.setContentHuggingPriority(.defaultLow, for: .horizontal)
        subtitle.setContentHuggingPriority(.defaultLow, for: .horizontal)
        subtitle.lineBreakMode = .byTruncatingMiddle
        subtitle.font = .systemFont(ofSize: NSFont.smallSystemFontSize)
        subtitle.textColor = .secondaryLabelColor
        icon.symbolConfiguration = .init(pointSize: 14, weight: .regular)
        let text = NSStackView(views: [title, subtitle])
        text.orientation = .vertical
        text.alignment = .leading
        text.spacing = 1
        spinner.style = .spinning
        spinner.controlSize = .small
        spinner.isDisplayedWhenStopped = false
        status.symbolConfiguration = .init(pointSize: 13, weight: .regular)
        status.setContentHuggingPriority(.required, for: .horizontal)
        diff.setContentHuggingPriority(.required, for: .horizontal)
        diff.setContentCompressionResistancePriority(.required, for: .horizontal)
        let row = NSStackView(views: [icon, text, diff, status, spinner])
        row.spacing = 8
        row.distribution = .fill
        text.setHuggingPriority(.defaultLow, for: .horizontal)
        row.translatesAutoresizingMaskIntoConstraints = false
        addSubview(row)
        textField = title
        imageView = icon
        NSLayoutConstraint.activate([
            icon.widthAnchor.constraint(equalToConstant: 18),
            title.widthAnchor.constraint(equalTo: text.widthAnchor),
            subtitle.widthAnchor.constraint(equalTo: text.widthAnchor),
            row.leadingAnchor.constraint(equalTo: leadingAnchor, constant: 2),
            row.trailingAnchor.constraint(equalTo: trailingAnchor, constant: -4),
            row.centerYAnchor.constraint(equalTo: centerYAnchor),
        ])
    }

    required init?(coder: NSCoder) { fatalError() }

    func showDiff(_ stat: (added: Int, removed: Int)?) {
        guard let stat, stat.added + stat.removed > 0 else {
            diff.isHidden = true
            return
        }
        let font = NSFont.monospacedDigitSystemFont(ofSize: NSFont.smallSystemFontSize, weight: .medium)
        let text = NSMutableAttributedString()
        if stat.added > 0 { text.append(NSAttributedString(string: "+\(stat.added)", attributes: [.font: font, .foregroundColor: NSColor.systemGreen])) }
        if stat.added > 0, stat.removed > 0 { text.append(NSAttributedString(string: " ", attributes: [.font: font])) }
        if stat.removed > 0 { text.append(NSAttributedString(string: "−\(stat.removed)", attributes: [.font: font, .foregroundColor: NSColor.systemRed])) }
        diff.attributedStringValue = text
        diff.isHidden = false
    }

    func showStatus(of session: Session?) {
        let state: (symbol: String, color: NSColor, label: String)? = switch session {
        case let session? where !session.isRunning: ("xmark.circle.fill", .systemRed, "Exited with code \(session.exitCode.map(String.init) ?? "?")")
        case let session? where session.activity == .waiting: ("questionmark.circle.fill", .systemOrange, "Waiting for input")
        case let session? where session.activity == .working: nil
        case let session? where session.needsAttention: ("checkmark.circle.fill", .systemGreen, "Done")
        default: nil
        }
        let working = session?.isRunning == true && session?.activity == .working
        if working {
            spinner.startAnimation(nil)
            spinner.setAccessibilityLabel("Working")
        } else {
            spinner.stopAnimation(nil)
        }
        spinner.isHidden = !working
        status.isHidden = state == nil
        status.image = state.flatMap { NSImage(systemSymbolName: $0.symbol, accessibilityDescription: $0.label) }
        status.contentTintColor = state?.color
        status.toolTip = state?.label
        spinner.toolTip = working ? "Working" : nil
    }
}

final class SidebarOutlineView: NSOutlineView {
    private let rootInset: CGFloat = 8

    override func frameOfOutlineCell(atRow row: Int) -> NSRect {
        var frame = super.frameOfOutlineCell(atRow: row)
        guard !frame.isEmpty else { return frame }
        frame.origin.y = rect(ofRow: row).midY - frame.height / 2
        if level(forRow: row) == 0 { frame.origin.x += rootInset }
        return frame
    }

    override func frameOfCell(atColumn column: Int, row: Int) -> NSRect {
        var frame = super.frameOfCell(atColumn: column, row: row)
        guard level(forRow: row) == 0 else { return frame }
        frame.origin.x += rootInset
        frame.size.width -= rootInset
        return frame
    }
}

final class TintedRowView: NSTableRowView {
    var tint: NSColor?
    var attention = false { didSet { if attention != oldValue { needsDisplay = true } } }
    var joinsAbove = false { didSet { if joinsAbove != oldValue { needsDisplay = true } } }
    var joinsBelow = false { didSet { if joinsBelow != oldValue { needsDisplay = true } } }
    var split: (above: Bool, below: Bool)? { didSet { if split?.above != oldValue?.above || split?.below != oldValue?.below || (split == nil) != (oldValue == nil) { needsDisplay = true } } }
    var groupSelected = false { didSet { if groupSelected != oldValue { needsDisplay = true } } }

    override func drawSelection(in dirtyRect: NSRect) {
        if split == nil { super.drawSelection(in: dirtyRect) } else { drawGroupSelection() }
        drawSplitMark()
    }

    private func drawGroupSelection() {
        guard let split else { return }
        (isEmphasized ? NSColor.selectedContentBackgroundColor : NSColor.unemphasizedSelectedContentBackgroundColor).setFill()
        NSGraphicsContext.saveGraphicsState()
        bounds.clip()
        NSBezierPath(roundedRect: joinedRect(above: split.above, below: split.below, radius: 6), xRadius: 6, yRadius: 6).fill()
        NSGraphicsContext.restoreGraphicsState()
    }

    private func joinedRect(above: Bool, below: Bool, radius: CGFloat) -> NSRect {
        var rect = bounds.insetBy(dx: 10, dy: 0)
        if above {
            rect.origin.y -= radius
            rect.size.height += radius
        } else {
            rect.origin.y += 2
            rect.size.height -= 2
        }
        rect.size.height += below ? radius : -2
        return rect
    }

    private func drawSplitMark() {
        guard let split else { return }
        var bar = NSRect(x: 28, y: 4, width: 3, height: bounds.height - 8)
        if split.above {
            bar.origin.y -= 4
            bar.size.height += 4
        }
        if split.below { bar.size.height += 4 }
        NSColor.controlAccentColor.withAlphaComponent(0.7).setFill()
        NSBezierPath(roundedRect: bar, xRadius: 1.5, yRadius: 1.5).fill()
    }

    override func drawBackground(in dirtyRect: NSRect) {
        super.drawBackground(in: dirtyRect)
        if groupSelected, !isSelected { drawGroupSelection() }
        drawSplitMark()
        if attention, !isSelected {
            NSColor.systemGreen.withAlphaComponent(0.3).setFill()
            NSBezierPath(roundedRect: bounds.insetBy(dx: 10, dy: 2), xRadius: 8, yRadius: 8).fill()
        }
        guard let tint else { return }
        NSGraphicsContext.saveGraphicsState()
        bounds.clip()
        tint.withAlphaComponent(0.14).setFill()
        NSBezierPath(roundedRect: joinedRect(above: joinsAbove, below: joinsBelow, radius: 8), xRadius: 8, yRadius: 8).fill()
        NSGraphicsContext.restoreGraphicsState()
    }
}

final class SidebarViewController: NSViewController, NSOutlineViewDataSource, NSOutlineViewDelegate, NSMenuDelegate, NSTextFieldDelegate, NSSearchFieldDelegate {
    weak var owner: MainWindowController?
    private let outline = SidebarOutlineView()
    private var isSyncing = false
    private var diffWatchers: [String: FileWatcher] = [:]
    private var diffRefreshing: Set<String> = []
    private var diffPending: Set<String> = []
    private let filterField = NSSearchField()
    private let tabs = ResponsiveSegmentedControl()
    private let workspacePopup = NSPopUpButton(frame: .zero, pullsDown: false)
    private let addButton = NSPopUpButton(frame: .zero, pullsDown: true)
    private let history = HistoryList()
    private let board = BoardList()
    private var outlineScroll: NSScrollView!
    var showsHistory: Bool { tabs.selectedSegment == 2 }
    var showsBoard: Bool { tabs.selectedSegment == 1 }
    private var filter = ""
    private var renaming: (item: AnyObject, field: NSTextField)?
    private var renameCancelled = false
    private var state: AppState { .shared }
    private var windowId: Int { owner?.id ?? 0 }

    override func loadView() {
        let column = NSTableColumn(identifier: .init("main"))
        outline.addTableColumn(column)
        outline.outlineTableColumn = column
        outline.headerView = nil
        outline.style = .sourceList
        outline.rowSizeStyle = .custom
        outline.dataSource = self
        outline.delegate = self
        outline.registerForDraggedTypes([.nucliItem, .fileURL])
        outline.setDraggingSourceOperationMask(.move, forLocal: true)
        outline.setDraggingSourceOperationMask([], forLocal: false)
        let menu = NSMenu()
        menu.delegate = self
        outline.menu = menu
        outline.target = self
        outline.doubleAction = #selector(doubleClicked)
        let scroll = NSScrollView()
        scroll.documentView = outline
        scroll.drawsBackground = false
        scroll.hasVerticalScroller = false
        filterField.placeholderString = "Filter"
        filterField.delegate = self
        filterField.sendsSearchStringImmediately = true
        if let button = (filterField.cell as? NSSearchFieldCell)?.searchButtonCell {
            button.image = NSImage(systemSymbolName: "magnifyingglass", accessibilityDescription: "Filter")?.withSymbolConfiguration(.init(pointSize: 12, weight: .regular))
            button.imageScaling = .scaleNone
        }
        outlineScroll = scroll
        tabs.segmentCount = 3
        tabs.trackingMode = .selectOne
        tabs.segmentDistribution = .fillEqually
        tabs.target = self
        tabs.action = #selector(tabChanged)
        for (index, (symbol, title)) in [("folder", "Projects"), ("rectangle.split.3x1", "Agents"), ("clock", "History")].enumerated() {
            tabs.setImage(NSImage(systemSymbolName: symbol, accessibilityDescription: title), forSegment: index)
            tabs.setToolTip("\(title) (⌥⌘\(index + 1))", forSegment: index)
        }
        tabs.labels = ["Projects", "Agents", "History"]
        tabs.controlSize = .small
        tabs.font = .systemFont(ofSize: NSFont.smallSystemFontSize)
        addButton.isBordered = false
        addButton.controlSize = .small
        let addMenu = addButton.menu!
        addMenu.addItem(withTitle: "", action: nil, keyEquivalent: "").image = NSImage(systemSymbolName: "plus", accessibilityDescription: "Add")
        for (title, action) in [("New Agent", #selector(AppController.newAgentWithLastProfile(_:))), ("New Agent…", #selector(AppController.newAgent(_:))), ("New Terminal", #selector(AppController.newTerminal(_:))), ("Open Project…", #selector(AppController.openProject(_:)))] {
            addMenu.addItem(withTitle: title, action: action, keyEquivalent: "").target = AppController.shared
        }
        addButton.toolTip = "Add"
        workspacePopup.isBordered = false
        workspacePopup.controlSize = .small
        workspacePopup.font = .systemFont(ofSize: NSFont.smallSystemFontSize)
        workspacePopup.toolTip = "Workspace (\u{2318}` switches windows)"
        tabs.selectedSegment = min(max(UserDefaults.standard.integer(forKey: "sidebarTab"), 0), 2)
        history.onOpen = { entry in AppController.shared.open(entry) }
        let historyScroll = history.view
        let root = NSView()
        let separator = NSBox()
        separator.boxType = .separator
        board.windowId = { [weak self] in self?.windowId ?? 0 }
        for child in [tabs, filterField, scroll, historyScroll, board.view, separator, addButton, workspacePopup] as [NSView] {
            child.translatesAutoresizingMaskIntoConstraints = false
            root.addSubview(child)
        }
        NSLayoutConstraint.activate([
            tabs.topAnchor.constraint(equalTo: root.safeAreaLayoutGuide.topAnchor, constant: 10),
            filterField.topAnchor.constraint(equalTo: tabs.bottomAnchor, constant: 8),
            tabs.leadingAnchor.constraint(equalTo: root.leadingAnchor, constant: 10),
            tabs.trailingAnchor.constraint(equalTo: root.trailingAnchor, constant: -10),
            scroll.topAnchor.constraint(equalTo: filterField.bottomAnchor, constant: 8),
            scroll.leadingAnchor.constraint(equalTo: root.leadingAnchor),
            scroll.trailingAnchor.constraint(equalTo: root.trailingAnchor),
            scroll.bottomAnchor.constraint(equalTo: separator.topAnchor),
            historyScroll.topAnchor.constraint(equalTo: scroll.topAnchor),
            historyScroll.bottomAnchor.constraint(equalTo: scroll.bottomAnchor),
            historyScroll.leadingAnchor.constraint(equalTo: scroll.leadingAnchor),
            historyScroll.trailingAnchor.constraint(equalTo: scroll.trailingAnchor),
            board.view.topAnchor.constraint(equalTo: scroll.topAnchor),
            board.view.bottomAnchor.constraint(equalTo: scroll.bottomAnchor),
            board.view.leadingAnchor.constraint(equalTo: scroll.leadingAnchor),
            board.view.trailingAnchor.constraint(equalTo: scroll.trailingAnchor),
            filterField.leadingAnchor.constraint(equalTo: root.leadingAnchor, constant: 12),
            filterField.trailingAnchor.constraint(equalTo: root.trailingAnchor, constant: -12),
            separator.leadingAnchor.constraint(equalTo: root.leadingAnchor),
            separator.trailingAnchor.constraint(equalTo: root.trailingAnchor),
            separator.bottomAnchor.constraint(equalTo: root.bottomAnchor, constant: -36),
            addButton.leadingAnchor.constraint(equalTo: root.leadingAnchor, constant: 10),
            addButton.centerYAnchor.constraint(equalTo: separator.bottomAnchor, constant: 18),
            workspacePopup.leadingAnchor.constraint(greaterThanOrEqualTo: addButton.trailingAnchor, constant: 8),
            workspacePopup.trailingAnchor.constraint(equalTo: root.trailingAnchor, constant: -10),
            workspacePopup.centerYAnchor.constraint(equalTo: addButton.centerYAnchor),
        ])
        view = root
        NotificationCenter.default.addObserver(forName: .projectsChanged, object: nil, queue: .main) { [weak self] _ in
            MainActor.assumeIsolated { self?.reload() }
        }
        NotificationCenter.default.addObserver(forName: .settingsChanged, object: nil, queue: .main) { [weak self] _ in
            MainActor.assumeIsolated { self?.reload() }
        }
        NotificationCenter.default.addObserver(forName: .layoutChanged, object: nil, queue: .main) { [weak self] note in
            MainActor.assumeIsolated {
                guard let self, note.object as AnyObject === self.owner?.content.layout else { return }
                self.updateSplitMarks()
            }
        }
        NotificationCenter.default.addObserver(forName: .sessionChanged, object: nil, queue: .main) { [weak self] note in
            MainActor.assumeIsolated {
                guard let self, let session = note.object as? Session, session.project.windowId == self.windowId else { return }
                let row = self.outline.row(forItem: session)
                guard row >= 0 else { return }
                self.outline.reloadItem(session, reloadChildren: true)
                if !session.subagents.isEmpty { self.outline.expandItem(session) }
                (self.outline.rowView(atRow: row, makeIfNecessary: false) as? TintedRowView)?.attention = session.needsAttention
            }
        }
        reload()
        applyTab()
    }

    func updateWorkspaceTitle() {
        workspacePopup.removeAllItems()
        let current = state.workspace(forWindow: windowId)
        for workspace in state.workspaces {
            workspacePopup.addItem(withTitle: workspace.name)
            workspacePopup.lastItem?.representedObject = workspace
            workspacePopup.lastItem?.target = self
            workspacePopup.lastItem?.action = #selector(workspaceChosen(_:))
        }
        workspacePopup.menu?.addItem(.separator())
        for (title, action) in [("New Workspace…", #selector(AppController.newWindow(_:))), ("Rename Workspace…", #selector(AppController.renameWorkspace(_:)))] {
            workspacePopup.menu?.addItem(withTitle: title, action: action, keyEquivalent: "").target = AppController.shared
        }
        if let current, let index = state.workspaces.firstIndex(where: { $0 === current }) { workspacePopup.selectItem(at: index) }
    }

    @objc private func workspaceChosen(_ sender: NSMenuItem) {
        guard let workspace = sender.representedObject as? Workspace else { return }
        AppController.shared.controller(forWindowId: workspace.windowId).showWindow(nil)
        updateWorkspaceTitle()
    }

    @objc private func tabChanged() {
        UserDefaults.standard.set(tabs.selectedSegment, forKey: "sidebarTab")
        applyTab()
    }

    func showTab(_ index: Int) {
        tabs.selectedSegment = index
        tabChanged()
    }

    private func applyTab() {
        outlineScroll.isHidden = showsHistory || showsBoard
        history.view.isHidden = !showsHistory
        board.view.isHidden = !showsBoard
        filterField.placeholderString = showsHistory ? "Filter History" : showsBoard ? "Filter Agents" : "Filter"
        if showsHistory { history.refresh() }
        if showsBoard { board.reload() }
    }

    func reload() {
        updateWorkspaceTitle()
        watchDiffs()
        guard renaming == nil else { return }
        isSyncing = true
        outline.reloadData()
        if !filter.isEmpty {
            outline.expandItem(nil, expandChildren: true)
            isSyncing = false
            syncSelection()
            return
        }
        for ws in state.projects where ws.windowId == windowId && !ws.isCollapsed {
            outline.expandItem(ws)
            ws.sessions.filter { !$0.subagents.isEmpty }.forEach { outline.expandItem($0) }
        }
        isSyncing = false
        syncSelection()
    }

    private func watchDiffs() {
        let projects = state.projects(inWindow: windowId).filter { !$0.isScratch }
        let paths = Set(projects.map(\.path))
        diffWatchers = diffWatchers.filter { paths.contains($0.key) }
        for project in projects where diffWatchers[project.path] == nil {
            diffWatchers[project.path] = FileWatcher(paths: [project.path]) { [weak self, weak project] in
                MainActor.assumeIsolated { project.map { self?.refreshDiff($0) } }
            }
            refreshDiff(project)
        }
    }

    private func refreshDiff(_ project: Project) {
        let path = project.path
        guard diffRefreshing.insert(path).inserted else {
            diffPending.insert(path)
            return
        }
        DispatchQueue.global(qos: .utility).async {
            let stat = Git.diffStat(in: path)
            DispatchQueue.main.async { [weak self, weak project] in
                guard let self else { return }
                diffRefreshing.remove(path)
                if let project, diffPending.remove(path) != nil { refreshDiff(project) }
                guard let project, project.diffStat?.added != stat?.added || project.diffStat?.removed != stat?.removed else { return }
                project.diffStat = stat
                if outline.row(forItem: project) >= 0 { outline.reloadItem(project) }
            }
        }
    }

    func syncSelection() {
        let item: AnyObject? = owner?.currentSession ?? owner?.currentProject
        let row = item.map { outline.row(forItem: $0) } ?? -1
        isSyncing = true
        if row >= 0 {
            outline.selectRowIndexes([row], byExtendingSelection: false)
            outline.scrollRowToVisible(row)
        } else {
            outline.deselectAll(nil)
        }
        isSyncing = false
        updateSplitMarks()
    }

    private func children(of item: Any?) -> [AnyObject] {
        let all: [AnyObject] = switch item {
        case nil: state.projects(inWindow: windowId)
        case let ws as Project: ws.sessions
        case let session as Session: session.subagents
        default: []
        }
        guard !filter.isEmpty, !(item.map { matches($0 as AnyObject) } ?? false) else { return all }
        return all.filter(contains)
    }

    private func matches(_ item: AnyObject) -> Bool {
        let text: [String] = switch item {
        case let project as Project: [project.name, project.displayPath]
        case let session as Session: [session.title, session.subtitle]
        default: []
        }
        return text.contains { $0.localizedCaseInsensitiveContains(filter) }
    }

    private func contains(_ item: AnyObject) -> Bool {
        if matches(item) { return true }
        switch item {
        case let project as Project: return project.sessions.contains(where: matches)
        default: return false
        }
    }

    func controlTextDidChange(_ obj: Notification) {
        guard obj.object as? NSSearchField === filterField else { return }
        filter = filterField.stringValue.trimmingCharacters(in: .whitespaces)
        history.filter = filter
        board.filter = filter
        reload()
    }

    func control(_ control: NSControl, textView: NSTextView, doCommandBy selector: Selector) -> Bool {
        if control === filterField {
            switch selector {
            case #selector(NSResponder.cancelOperation(_:)):
                filterField.stringValue = ""
                controlTextDidChange(Notification(name: NSControl.textDidChangeNotification, object: filterField))
                focusContent()
                return true
            case #selector(NSResponder.insertNewline(_:)):
                if showsHistory { history.openFirst() } else if showsBoard { board.openFirst() } else { selectFirstMatch() }
                return true
            case #selector(NSResponder.moveDown(_:)):
                view.window?.makeFirstResponder(outline)
                return true
            default:
                return false
            }
        }
        guard control === renaming?.field, selector == #selector(NSResponder.cancelOperation(_:)) else { return false }
        renameCancelled = true
        view.window?.makeFirstResponder(outline)
        return true
    }

    private func selectFirstMatch() {
        for row in 0..<outline.numberOfRows where outline.item(atRow: row) is Session {
            outline.selectRowIndexes([row], byExtendingSelection: false)
            return
        }
        if outline.numberOfRows > 0 { outline.selectRowIndexes([0], byExtendingSelection: false) }
    }

    private func focusContent() {
        if let terminal = owner?.currentSession?.terminal { view.window?.makeFirstResponder(terminal) }
    }

    @objc func focusFilter() {
        view.window?.makeFirstResponder(filterField)
    }

    @objc private func doubleClicked() {
        guard outline.clickedRow >= 0, let item = outline.item(atRow: outline.clickedRow) as AnyObject? else { return }
        beginRename(item)
    }

    func beginRename(_ item: AnyObject) {
        if (item as? Project)?.isScratch == true { return }
        let row = outline.row(forItem: item)
        guard row >= 0, let cell = outline.view(atColumn: 0, row: row, makeIfNecessary: true) as? SidebarCell else { return }
        let field = cell.title
        field.stringValue = switch item {
        case let session as Session: session.title
        case let project as Project: project.name
        default: field.stringValue
        }
        field.isEditable = true
        field.delegate = self
        renaming = (item, field)
        renameCancelled = false
        view.window?.makeFirstResponder(field)
        field.currentEditor()?.selectAll(nil)
    }

    func controlTextDidEndEditing(_ obj: Notification) {
        guard let renaming, obj.object as? NSTextField === renaming.field else { return }
        self.renaming = nil
        renaming.field.isEditable = false
        renaming.field.delegate = nil
        if renameCancelled {
            reload()
        } else {
            AppController.shared.rename(renaming.item, to: renaming.field.stringValue)
        }
        focusContent()
    }

    func outlineView(_ outlineView: NSOutlineView, numberOfChildrenOfItem item: Any?) -> Int {
        children(of: item).count
    }

    func outlineView(_ outlineView: NSOutlineView, child index: Int, ofItem item: Any?) -> Any {
        children(of: item)[index]
    }

    func outlineView(_ outlineView: NSOutlineView, isItemExpandable item: Any) -> Bool {
        !children(of: item).isEmpty
    }

    func outlineView(_ outlineView: NSOutlineView, heightOfRowByItem item: Any) -> CGFloat {
        (28 + (tint(of: item) == nil ? 0 : 6)) + 2 * state.sidebarTextSize.offset
    }

    func outlineView(_ outlineView: NSOutlineView, rowViewForItem item: Any) -> NSTableRowView? {
        let row = TintedRowView()
        row.tint = tint(of: item)
        row.attention = (item as? Session)?.needsAttention ?? false
        return row
    }

    private func tint(of item: Any?) -> NSColor? {
        switch item {
        case let project as Project: project.tint?.color
        case let session as Session: session.color?.color ?? session.project.tint?.color
        case let agent as Subagent: tint(of: agent.session)
        default: nil
        }
    }

    func outlineView(_ outlineView: NSOutlineView, didAdd rowView: NSTableRowView, forRow row: Int) {
        updateJoins(around: row)
        updateSplitMarks()
    }

    private func splitMark(forRow row: Int, in groups: [[Session]]) -> (above: Bool, below: Bool)? {
        guard let session = outline.item(atRow: row) as? Session, let split = groups.first(where: { $0.contains { $0 === session } }) else { return nil }
        func grouped(_ index: Int) -> Bool { (outline.item(atRow: index) as? Session).map { item in split.contains { $0 === item } } ?? false }
        return (grouped(row - 1), grouped(row + 1))
    }

    private func updateSplitMarks() {
        let groups = owner?.content.layout.groups ?? []
        let selected = (outline.item(atRow: outline.selectedRow) as? Session).flatMap { session in groups.first { $0.contains { $0 === session } } } ?? []
        outline.enumerateAvailableRowViews { view, row in
            guard let view = view as? TintedRowView else { return }
            view.split = splitMark(forRow: row, in: groups)
            view.groupSelected = (outline.item(atRow: row) as? Session).map { item in selected.contains { $0 === item } } ?? false
        }
    }

    func outlineView(_ outlineView: NSOutlineView, didRemove rowView: NSTableRowView, forRow row: Int) {
        updateJoins(around: row)
    }

    private func updateJoins(around row: Int) {
        let lower = max(0, row - 1)
        let upper = min(outline.numberOfRows - 1, row + 1)
        guard lower <= upper else { return }
        for index in lower...upper {
            guard let view = outline.rowView(atRow: index, makeIfNecessary: false) as? TintedRowView, let tint = view.tint else { continue }
            view.joinsAbove = self.tint(of: outline.item(atRow: index - 1)) == tint
            view.joinsBelow = self.tint(of: outline.item(atRow: index + 1)) == tint
        }
    }

    func outlineView(_ outlineView: NSOutlineView, viewFor tableColumn: NSTableColumn?, item: Any) -> NSView? {
        let cell = outlineView.makeView(withIdentifier: .init("cell"), owner: nil) as? SidebarCell ?? SidebarCell()
        cell.identifier = .init("cell")
        cell.title.textColor = .labelColor
        cell.subtitle.font = .systemFont(ofSize: NSFont.smallSystemFontSize + state.sidebarTextSize.offset)
        cell.showStatus(of: item as? Session)
        cell.showDiff((item as? Project)?.diffStat)
        defer {
            cell.toolTip = cell.subtitle.stringValue
            cell.subtitle.stringValue = ""
            cell.subtitle.isHidden = true
        }
        if let ws = item as? Project, ws.isScratch {
            cell.icon.image = NSImage(systemSymbolName: "tray", accessibilityDescription: nil)
            cell.icon.contentTintColor = .secondaryLabelColor
            cell.title.stringValue = ws.name
            cell.title.font = .systemFont(ofSize: NSFont.systemFontSize + state.sidebarTextSize.offset, weight: .semibold)
            cell.subtitle.stringValue = "~ · \(ws.sessions.count) \(ws.sessions.count == 1 ? "tab" : "tabs")"
        } else if let ws = item as? Project {
            cell.icon.image = NSImage(systemSymbolName: "folder.fill", accessibilityDescription: nil)
            cell.icon.contentTintColor = ws.tint?.color ?? .controlAccentColor
            cell.title.stringValue = ws.name
            cell.title.font = .systemFont(ofSize: NSFont.systemFontSize + state.sidebarTextSize.offset, weight: .semibold)
            cell.subtitle.stringValue = "\(ws.displayPath) · \(ws.sessions.count) \(ws.sessions.count == 1 ? "tab" : "tabs")"
        } else if let session = item as? Session {
            if let agent = session.runningAgent {
                cell.icon.image = AgentBadge.image(for: agent)
                cell.icon.contentTintColor = nil
            } else {
                let symbol = session.kind == .agent ? "sparkles" : "terminal"
                cell.icon.image = NSImage(systemSymbolName: symbol, accessibilityDescription: nil)
                cell.icon.contentTintColor = session.needsAttention ? .systemGreen : session.isRunning ? session.color?.color ?? .secondaryLabelColor : .tertiaryLabelColor
            }
            cell.title.stringValue = session.title
            cell.title.font = .systemFont(ofSize: NSFont.systemFontSize + state.sidebarTextSize.offset)
            cell.title.textColor = session.needsAttention ? .systemGreen : session.isRunning ? .labelColor : .tertiaryLabelColor
            cell.subtitle.stringValue = session.isRunning ? session.subtitle : "exited"
        } else if let agent = item as? Subagent {
            cell.icon.image = NSImage(systemSymbolName: "arrow.turn.down.right", accessibilityDescription: "Subagent")
            cell.icon.contentTintColor = .secondaryLabelColor
            cell.title.stringValue = agent.title
            cell.title.font = .systemFont(ofSize: NSFont.smallSystemFontSize + state.sidebarTextSize.offset)
            cell.title.textColor = .secondaryLabelColor
            cell.subtitle.stringValue = agent.type
        }
        return cell
    }

    func outlineViewSelectionDidChange(_ notification: Notification) {
        guard !isSyncing, let owner else { return }
        switch outline.item(atRow: outline.selectedRow) {
        case let session as Session: owner.select(session)
        case let agent as Subagent: owner.select(agent.session)
        case let project as Project: owner.showProject(project)
        default: if outline.selectedRow < 0 { owner.select(nil as Project?) }
        }
    }

    func outlineViewItemDidCollapse(_ notification: Notification) {
        setCollapsed(notification.userInfo?["NSObject"], true)
    }

    func outlineViewItemDidExpand(_ notification: Notification) {
        setCollapsed(notification.userInfo?["NSObject"], false)
    }

    private func setCollapsed(_ item: Any?, _ collapsed: Bool) {
        guard !isSyncing else { return }
        (item as? Project)?.isCollapsed = collapsed
        state.save()
    }

    func menuNeedsUpdate(_ menu: NSMenu) {
        menu.removeAllItems()
        let item = outline.item(atRow: outline.clickedRow)
        func add(_ title: String, _ action: Selector, _ object: Any?) {
            menu.addItem(withTitle: title, action: action, keyEquivalent: "").representedObject = object
        }
        if let ws = item as? Project, ws.isScratch {
            add("New Terminal", #selector(AppController.newTerminal(in:)), ws)
            add("New Agent…", #selector(AppController.newAgent(in:)), ws)
        } else if let ws = item as? Project {
            add("New Terminal", #selector(AppController.newTerminal(in:)), ws)
            add("New Agent…", #selector(AppController.newAgent(in:)), ws)
            menu.addItem(.separator())
            add("Rename…", #selector(AppController.renameItem(_:)), ws)
            menu.addItem(AppController.colorMenu(for: ws, current: ws.color))
            menu.addItem(windowMenu(for: ws))
            menu.addItem(.separator())
            add("Reveal in Finder", #selector(AppController.revealProject(_:)), ws)
            add("Close Project", #selector(AppController.closeProject(_:)), ws)
        } else if item is Subagent {
            return
        } else if let session = item as? Session {
            add("Rename…", #selector(AppController.renameItem(_:)), session)
            menu.addItem(AppController.colorMenu(for: session, current: session.color))
            if owner?.content.layout.group(of: session) != nil {
                menu.addItem(.separator())
                add("Remove from Split", #selector(AppController.removeFromSplit(_:)), session)
                add("Unsplit All", #selector(AppController.unsplit(_:)), session)
            }
            menu.addItem(.separator())
            add("Close Tab", #selector(AppController.closeSession(_:)), session)
        } else {
            add("Open Project…", #selector(AppController.openProject(_:)), nil)
            add("Rename Workspace…", #selector(AppController.renameWorkspace(_:)), nil)
            menu.addItem(.separator())
            add("New Workspace…", #selector(AppController.newWindow(_:)), nil)
        }
        setTargets(menu)
    }

    private func setTargets(_ menu: NSMenu) {
        for item in menu.items {
            item.target = AppController.shared
            if let submenu = item.submenu { setTargets(submenu) }
        }
    }

    private func windowMenu(for target: AnyObject) -> NSMenuItem {
        let menu = NSMenu()
        for controller in AppController.shared.windowControllers where controller.id != windowId {
            let item = NSMenuItem(title: state.workspace(forWindow: controller.id)?.name ?? "Workspace", action: #selector(AppController.moveToWindow(_:)), keyEquivalent: "")
            item.representedObject = MenuPayload(target, controller.id)
            menu.addItem(item)
        }
        if !menu.items.isEmpty { menu.addItem(.separator()) }
        let new = NSMenuItem(title: "New Workspace…", action: #selector(AppController.moveToWindow(_:)), keyEquivalent: "")
        new.representedObject = MenuPayload(target)
        menu.addItem(new)
        let root = NSMenuItem(title: "Move to Workspace", action: nil, keyEquivalent: "")
        root.submenu = menu
        return root
    }

    private enum Dragged {
        case projects([Project])
        case folders([String])
        case session(Session)
    }

    private func dragged(_ info: NSDraggingInfo) -> Dragged? {
        let pasteboard = info.draggingPasteboard
        if let id = pasteboard.string(forType: .nucliItem) {
            switch state.item(for: id) {
            case let ws as Project: return .projects([ws])
            case let session as Session: return .session(session)
            default: return nil
            }
        }
        let urls = pasteboard.readObjects(forClasses: [NSURL.self], options: [.urlReadingFileURLsOnly: true]) as? [URL] ?? []
        let folders = urls.filter { (try? $0.resourceValues(forKeys: [.isDirectoryKey]).isDirectory) == true }.map(\.path)
        return folders.isEmpty ? nil : .folders(folders)
    }

    private func rootIndex(of item: AnyObject) -> Int {
        children(of: nil).firstIndex { $0 === item } ?? children(of: nil).count
    }

    private func projectSlot(_ ws: Project) -> (item: AnyObject?, index: Int) {
        (nil, rootIndex(of: ws))
    }

    private func dropTarget(_ dragged: Dragged, item: Any?, index: Int) -> (item: AnyObject?, index: Int)? {
        switch dragged {
        case .projects, .folders:
            switch item {
            case nil: return (nil, index < 0 ? children(of: nil).count : index)
            case let ws as Project: return projectSlot(ws)
            case let session as Session: return projectSlot(session.project)
            default: return nil
            }
        case .session:
            switch item {
            case let ws as Project: return (ws, index < 0 ? ws.sessions.count : index)
            case let session as Session: return (session.project, session.project.sessions.firstIndex { $0 === session } ?? 0)
            default: return nil
            }
        }
    }

    func outlineView(_ outlineView: NSOutlineView, pasteboardWriterForItem item: Any) -> NSPasteboardWriting? {
        if (item as? Project)?.isScratch == true { return nil }
        guard let id = state.identifier(for: item as AnyObject) else { return nil }
        let pasteboardItem = NSPasteboardItem()
        pasteboardItem.setString(id, forType: .nucliItem)
        return pasteboardItem
    }

    func outlineView(_ outlineView: NSOutlineView, validateDrop info: NSDraggingInfo, proposedItem item: Any?, proposedChildIndex index: Int) -> NSDragOperation {
        guard let dragged = dragged(info), let target = dropTarget(dragged, item: item, index: index) else { return [] }
        outlineView.setDropItem(target.item, dropChildIndex: target.index)
        if case .folders = dragged { return .copy }
        return .move
    }

    func outlineView(_ outlineView: NSOutlineView, acceptDrop info: NSDraggingInfo, item: Any?, childIndex index: Int) -> Bool {
        guard let dragged = dragged(info), let target = dropTarget(dragged, item: item, index: index) else { return false }
        switch dragged {
        case .projects(let list):
            move(list, index: target.index)
        case .folders(let paths):
            move(paths.map { state.project(for: $0, windowId: windowId) }, index: target.index)
        case .session(let session):
            guard let project = target.item as? Project else { return false }
            var index = target.index
            for member in owner?.content.layout.group(of: session) ?? [session] {
                move(member, to: project, index: index)
                index = (project.sessions.firstIndex { $0 === member } ?? index) + 1
            }
        }
        compactGroups()
        state.changed()
        AppController.shared.reconcileSelections()
        if let target = target.item { outline.expandItem(target) }
        return true
    }

    private func move(_ list: [Project], index: Int) {
        var position = index
        for ws in list {
            var siblings = state.projects(inWindow: windowId)
            if let from = siblings.firstIndex(where: { $0 === ws }), from < position { position -= 1 }
            siblings.removeAll { $0 === ws }
            state.projects.removeAll { $0 === ws }
            reinsert(ws, in: &state.projects, siblings: siblings, at: min(max(position, 0), siblings.count))
            ws.windowId = windowId
            position += 1
        }
    }

    private func move(_ session: Session, to project: Project, index: Int) {
        var position = index
        if let from = project.sessions.firstIndex(where: { $0 === session }), from < position { position -= 1 }
        session.project.sessions.removeAll { $0 === session }
        project.sessions.insert(session, at: min(max(position, 0), project.sessions.count))
        session.project = project
        project.isCollapsed = false
    }

    func compactGroups() {
        for group in owner?.content.layout.groups ?? [] {
            func isMember(_ session: Session) -> Bool { group.contains { $0 === session } }
            var done: [Project] = []
            for project in group.map(\.project) where !done.contains(where: { $0 === project }) {
                done.append(project)
                guard let anchor = project.sessions.firstIndex(where: isMember) else { continue }
                var sessions = project.sessions.filter { !isMember($0) }
                sessions.insert(contentsOf: project.sessions.filter(isMember), at: anchor)
                project.sessions = sessions
            }
        }
    }

    private func reinsert<T: AnyObject>(_ element: T, in array: inout [T], siblings: [T], at position: Int) {
        if let anchor = siblings[safe: position], let anchorIndex = array.firstIndex(where: { $0 === anchor }) {
            array.insert(element, at: anchorIndex)
        } else if let last = siblings.last, let lastIndex = array.firstIndex(where: { $0 === last }) {
            array.insert(element, at: lastIndex + 1)
        } else {
            array.append(element)
        }
    }
}
