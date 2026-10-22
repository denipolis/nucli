import AppKit

final class FileNode {
    let path: String
    let isDirectory: Bool
    var children: [FileNode]?

    init(path: String, isDirectory: Bool) {
        self.path = path
        self.isDirectory = isDirectory
    }

    var name: String { (path as NSString).lastPathComponent }
}

final class FileCell: NSTableCellView {
    let badge = NSTextField(labelWithString: "")
}

final class FilesViewController: NSViewController, NSOutlineViewDataSource, NSOutlineViewDelegate, NSMenuDelegate {
    weak var content: ContentViewController?
    weak var git: GitPanelViewController?
    private let outline = NSOutlineView()
    private let header = NSTextField(labelWithString: "")
    private let summary = NSTextField(labelWithString: "")
    private var root: FileNode?
    private var nodes: [String: FileNode] = [:]
    private var watcher: FileWatcher?
    private var ignored: Set<String> = []
    private var changed: [String: Character] = [:]
    private var expanded: [String: [String]] = [:]
    private var isLoadingIgnored = false
    private var needsIgnored = false
    private var needsReload = false

    var project: Project? {
        didSet {
            guard project !== oldValue else { return }
            if let oldValue { expanded[oldValue.path] = expandedPaths() }
            nodes = [:]
            ignored = []
            root = project.map { node($0.path, isDirectory: true) }
            header.stringValue = project?.name ?? "No project"
            summary.stringValue = project?.displayPath ?? ""
            outline.reloadData()
            expand(project.flatMap { expanded[$0.path] } ?? [])
            updateChanged()
            watcher = project.map { FileWatcher(paths: [$0.path]) { [weak self] in self?.reload() } }
            loadIgnored()
        }
    }

    override func loadView() {
        header.font = .systemFont(ofSize: NSFont.systemFontSize, weight: .semibold)
        header.lineBreakMode = .byTruncatingTail
        header.stringValue = "No project"
        let column = NSTableColumn(identifier: .init("file"))
        outline.addTableColumn(column)
        outline.outlineTableColumn = column
        outline.headerView = nil
        outline.style = .sourceList
        outline.backgroundColor = .clear
        outline.rowHeight = 22 + 2 * AppState.shared.sidebarTextSize.offset
        outline.indentationPerLevel = 12
        outline.dataSource = self
        outline.delegate = self
        outline.target = self
        outline.action = #selector(clicked)
        outline.menu = NSMenu()
        outline.menu?.delegate = self
        outline.setDraggingSourceOperationMask(.copy, forLocal: false)
        outline.setDraggingSourceOperationMask(.copy, forLocal: true)
        let scroll = NSScrollView()
        scroll.documentView = outline
        scroll.hasVerticalScroller = false
        scroll.drawsBackground = false
        summary.font = .systemFont(ofSize: NSFont.smallSystemFontSize)
        summary.textColor = .secondaryLabelColor
        summary.lineBreakMode = .byTruncatingMiddle
        let top = NSStackView(views: [header, summary])
        top.orientation = .vertical
        top.alignment = .leading
        top.spacing = 2
        top.edgeInsets = NSEdgeInsets(top: 8, left: 14, bottom: 0, right: 14)
        let stack = NSStackView(views: [top, scroll])
        stack.orientation = .vertical
        stack.alignment = .leading
        stack.spacing = 0
        stack.translatesAutoresizingMaskIntoConstraints = false
        let rootView = NSView()
        rootView.addSubview(stack)
        NSLayoutConstraint.activate([
            stack.topAnchor.constraint(equalTo: rootView.topAnchor),
            stack.bottomAnchor.constraint(equalTo: rootView.bottomAnchor),
            stack.leadingAnchor.constraint(equalTo: rootView.leadingAnchor),
            stack.trailingAnchor.constraint(equalTo: rootView.trailingAnchor),
            scroll.widthAnchor.constraint(equalTo: stack.widthAnchor),
            top.widthAnchor.constraint(equalTo: stack.widthAnchor),
        ])
        view = rootView
        NotificationCenter.default.addObserver(forName: .settingsChanged, object: nil, queue: .main) { [weak self] _ in
            MainActor.assumeIsolated {
                guard let self else { return }
                self.outline.rowHeight = 22 + 2 * AppState.shared.sidebarTextSize.offset
                self.outline.reloadData()
            }
        }
        NotificationCenter.default.addObserver(forName: .gitChanged, object: nil, queue: .main) { [weak self] note in
            MainActor.assumeIsolated {
                guard let self, note.object as AnyObject? === self.project else { return }
                self.updateChanged()
            }
        }
    }

    private func node(_ path: String, isDirectory: Bool) -> FileNode {
        if let existing = nodes[path], existing.isDirectory == isDirectory { return existing }
        let created = FileNode(path: path, isDirectory: isDirectory)
        nodes[path] = created
        return created
    }

    private func children(of parent: FileNode) -> [FileNode] {
        if let children = parent.children { return children }
        let names = (try? FileManager.default.contentsOfDirectory(atPath: parent.path)) ?? []
        let list = names.filter { $0 != ".git" && $0 != ".DS_Store" }.map { name -> FileNode in
            let path = parent.path + "/" + name
            var isDirectory: ObjCBool = false
            FileManager.default.fileExists(atPath: path, isDirectory: &isDirectory)
            return node(path, isDirectory: isDirectory.boolValue)
        }.sorted { $0.isDirectory != $1.isDirectory ? $0.isDirectory : $0.name.localizedStandardCompare($1.name) == .orderedAscending }
        parent.children = list
        return list
    }

    override func viewDidAppear() {
        super.viewDidAppear()
        reloadIfNeeded()
    }

    func reloadIfNeeded() {
        if needsReload { reload() }
    }

    private func reload() {
        guard view.window != nil, !view.isHiddenOrHasHiddenAncestor else {
            needsReload = true
            return
        }
        needsReload = false
        let paths = expandedPaths()
        nodes.values.forEach { $0.children = nil }
        outline.reloadData()
        expand(paths)
        loadIgnored()
    }

    private func expandedPaths() -> [String] {
        (0..<outline.numberOfRows).compactMap { row in
            (outline.item(atRow: row) as? FileNode).flatMap { outline.isItemExpanded($0) ? $0.path : nil }
        }
    }

    private func expand(_ paths: [String]) {
        for path in paths.sorted(by: { $0.count < $1.count }) {
            if let node = nodes[path] { outline.expandItem(node) }
        }
    }

    private func relative(_ node: FileNode) -> String {
        guard let base = project?.path, node.path.hasPrefix(base + "/") else { return "" }
        return String(node.path.dropFirst(base.count + 1))
    }

    private func isIgnored(_ node: FileNode) -> Bool {
        var path = relative(node)
        while !path.isEmpty {
            if ignored.contains(path) { return true }
            path = (path as NSString).deletingLastPathComponent
        }
        return false
    }

    private func loadIgnored() {
        guard let project else { return }
        if isLoadingIgnored {
            needsIgnored = true
            return
        }
        isLoadingIgnored = true
        let path = project.path
        DispatchQueue.global(qos: .utility).async {
            let output = Git.run(["ls-files", "--others", "--ignored", "--exclude-standard", "--directory"], in: path) ?? ""
            let result = Set(output.split(separator: "\n").map { String($0.hasSuffix("/") ? $0.dropLast() : $0) })
            DispatchQueue.main.async { [weak self] in
                guard let self else { return }
                isLoadingIgnored = false
                if self.project === project, result != ignored {
                    ignored = result
                    refreshRows()
                }
                if needsIgnored {
                    needsIgnored = false
                    loadIgnored()
                }
            }
        }
    }

    private func updateChanged() {
        var result: [String: Character] = [:]
        for change in git?.status?.changes ?? [] {
            result[change.path] = change.status
            var dir = (change.path as NSString).deletingLastPathComponent
            while !dir.isEmpty {
                result[dir] = result[dir].map { $0 == change.status ? $0 : "M" } ?? change.status
                dir = (dir as NSString).deletingLastPathComponent
            }
        }
        guard result != changed else { return }
        changed = result
        refreshRows()
    }

    private func refreshRows() {
        guard outline.numberOfRows > 0 else { return }
        outline.reloadData(forRowIndexes: IndexSet(integersIn: 0..<outline.numberOfRows), columnIndexes: [0])
    }

    @objc private func clicked() {
        guard let node = outline.item(atRow: outline.clickedRow) as? FileNode else { return }
        if node.isDirectory {
            outline.isItemExpanded(node) ? outline.collapseItem(node) : outline.expandItem(node)
        } else {
            open(node)
        }
    }

    private func open(_ node: FileNode) {
        guard let project else { return }
        let path = relative(node)
        let change = git?.status?.changes.first { $0.path == path } ?? FileChange(path: path, status: " ", added: 0, removed: 0)
        content?.showFileDiff(change, in: project)
    }

    func outlineView(_ outlineView: NSOutlineView, numberOfChildrenOfItem item: Any?) -> Int {
        guard let parent = item as? FileNode ?? root else { return 0 }
        return children(of: parent).count
    }

    func outlineView(_ outlineView: NSOutlineView, child index: Int, ofItem item: Any?) -> Any {
        children(of: item as? FileNode ?? root!)[index]
    }

    func outlineView(_ outlineView: NSOutlineView, isItemExpandable item: Any) -> Bool {
        (item as? FileNode)?.isDirectory == true
    }

    func outlineView(_ outlineView: NSOutlineView, pasteboardWriterForItem item: Any) -> NSPasteboardWriting? {
        (item as? FileNode).map { URL(fileURLWithPath: $0.path) as NSURL }
    }

    func outlineView(_ outlineView: NSOutlineView, viewFor tableColumn: NSTableColumn?, item: Any) -> NSView? {
        guard let node = item as? FileNode else { return nil }
        let cell = outlineView.makeView(withIdentifier: .init("FileCell"), owner: nil) as? FileCell ?? makeCell()
        let (image, color) = FileIcon.icon(for: node.name, isDirectory: node.isDirectory)
        cell.imageView?.image = image
        cell.imageView?.contentTintColor = color
        cell.textField?.stringValue = node.name
        cell.textField?.font = .systemFont(ofSize: NSFont.systemFontSize + AppState.shared.sidebarTextSize.offset)
        let status = changed[relative(node)]
        let statusColor: NSColor = switch status {
        case "?", "A": .systemGreen
        case "D": .systemRed
        case .some: .systemOrange
        case nil: isIgnored(node) ? .tertiaryLabelColor : .labelColor
        }
        cell.textField?.textColor = statusColor
        cell.badge.textColor = statusColor
        cell.badge.stringValue = switch status {
        case nil: ""
        case _ where node.isDirectory: "●"
        case "?": "U"
        case let letter?: String(letter)
        }
        cell.badge.font = node.isDirectory ? .systemFont(ofSize: 7) : .monospacedSystemFont(ofSize: NSFont.smallSystemFontSize, weight: .bold)
        cell.alphaValue = status == nil && isIgnored(node) ? 0.7 : 1
        cell.toolTip = relative(node)
        return cell
    }

    private func makeCell() -> FileCell {
        let cell = FileCell()
        cell.identifier = .init("FileCell")
        let icon = NSImageView()
        icon.symbolConfiguration = .init(pointSize: 12, weight: .regular)
        let text = NSTextField(labelWithString: "")
        text.lineBreakMode = .byTruncatingTail
        let row = NSStackView(views: [icon, text])
        row.spacing = 5
        row.translatesAutoresizingMaskIntoConstraints = false
        cell.addSubview(row)
        cell.imageView = icon
        cell.textField = text
        cell.badge.translatesAutoresizingMaskIntoConstraints = false
        cell.badge.alignment = .center
        cell.badge.setContentCompressionResistancePriority(.required, for: .horizontal)
        cell.addSubview(cell.badge)
        NSLayoutConstraint.activate([
            icon.widthAnchor.constraint(equalToConstant: 16),
            icon.heightAnchor.constraint(equalToConstant: 16),
            row.leadingAnchor.constraint(equalTo: cell.leadingAnchor),
            row.trailingAnchor.constraint(lessThanOrEqualTo: cell.badge.leadingAnchor, constant: -4),
            row.centerYAnchor.constraint(equalTo: cell.centerYAnchor),
            cell.badge.trailingAnchor.constraint(equalTo: cell.trailingAnchor, constant: -6),
            cell.badge.widthAnchor.constraint(equalToConstant: 14),
            cell.badge.centerYAnchor.constraint(equalTo: cell.centerYAnchor),
        ])
        return cell
    }

    func menuNeedsUpdate(_ menu: NSMenu) {
        menu.removeAllItems()
        guard let node = outline.item(atRow: outline.clickedRow) as? FileNode else { return }
        for (title, action) in [("Reveal in Finder", #selector(reveal(_:))), ("Open with Default App", #selector(openExternally(_:))), ("Copy Path", #selector(copyPath(_:))), ("Copy Relative Path", #selector(copyRelativePath(_:)))] {
            let item = NSMenuItem(title: title, action: action, keyEquivalent: "")
            item.target = self
            item.representedObject = node
            menu.addItem(item)
        }
    }

    @objc private func reveal(_ sender: NSMenuItem) {
        guard let node = sender.representedObject as? FileNode else { return }
        NSWorkspace.shared.activateFileViewerSelecting([URL(fileURLWithPath: node.path)])
    }

    @objc private func openExternally(_ sender: NSMenuItem) {
        guard let node = sender.representedObject as? FileNode else { return }
        NSWorkspace.shared.open(URL(fileURLWithPath: node.path))
    }

    @objc private func copyPath(_ sender: NSMenuItem) {
        guard let node = sender.representedObject as? FileNode else { return }
        NSPasteboard.general.clearContents()
        NSPasteboard.general.setString(node.path, forType: .string)
    }

    @objc private func copyRelativePath(_ sender: NSMenuItem) {
        guard let node = sender.representedObject as? FileNode else { return }
        NSPasteboard.general.clearContents()
        NSPasteboard.general.setString(relative(node), forType: .string)
    }
}
