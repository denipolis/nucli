import AppKit

enum BoardColumn: Int, CaseIterable {
    case waiting, done, working, idle

    var title: String {
        switch self {
        case .waiting: "Waiting for Input"
        case .done: "Done"
        case .working: "Working"
        case .idle: "Idle"
        }
    }

    @MainActor init(_ session: Session) {
        self = session.activity == .waiting ? .waiting
            : session.activity == .working ? .working
            : session.needsAttention ? .done
            : .idle
    }
}

@MainActor
final class BoardList: NSObject, NSTableViewDataSource, NSTableViewDelegate {
    let view = NSView()
    var filter = "" { didSet { reload() } }
    var windowId = { 0 }
    private let table = NSTableView()
    private let empty = NSTextField(labelWithString: "No agents running")
    private var rows: [Any] = []

    override init() {
        super.init()
        table.addTableColumn(NSTableColumn(identifier: .init("main")))
        table.headerView = nil
        table.style = .sourceList
        table.rowSizeStyle = .custom
        table.dataSource = self
        table.delegate = self
        table.target = self
        table.action = #selector(clicked)
        table.setAccessibilityLabel("Agents by status")
        let scroll = NSScrollView()
        scroll.documentView = table
        scroll.drawsBackground = false
        scroll.hasVerticalScroller = false
        empty.textColor = .tertiaryLabelColor
        for child in [scroll, empty] {
            child.translatesAutoresizingMaskIntoConstraints = false
            view.addSubview(child)
        }
        NSLayoutConstraint.activate([
            scroll.topAnchor.constraint(equalTo: view.topAnchor),
            scroll.bottomAnchor.constraint(equalTo: view.bottomAnchor),
            scroll.leadingAnchor.constraint(equalTo: view.leadingAnchor),
            scroll.trailingAnchor.constraint(equalTo: view.trailingAnchor),
            empty.centerXAnchor.constraint(equalTo: view.centerXAnchor),
            empty.topAnchor.constraint(equalTo: view.topAnchor, constant: 24),
        ])
        for name in [Notification.Name.projectsChanged, .sessionChanged, .settingsChanged] {
            NotificationCenter.default.addObserver(forName: name, object: nil, queue: .main) { [weak self] _ in
                MainActor.assumeIsolated { if self?.view.isHidden == false { self?.reload() } }
            }
        }
    }

    private func isAgent(_ session: Session) -> Bool {
        session.kind == .agent || session.runningAgent != nil
    }

    func reload() {
        let sessions = AppState.shared.allSessions.filter(isAgent).filter { session in
            session.isRunning && session.project.windowId == windowId() && (filter.isEmpty || [session.title, session.subtitle, session.project.name].contains { $0.localizedCaseInsensitiveContains(filter) })
        }
        let columns = Dictionary(grouping: sessions, by: BoardColumn.init)
        rows = BoardColumn.allCases.flatMap { column -> [Any] in
            guard let items = columns[column] else { return [] }
            return [column] + items
        }
        empty.isHidden = !rows.isEmpty
        empty.stringValue = filter.isEmpty ? "No agents running" : "No matches"
        table.reloadData()
    }

    func openFirst() {
        (rows.first { $0 is Session } as? Session).map { AppController.shared.select($0) }
    }

    @objc private func clicked() {
        guard let session = rows[safe: table.clickedRow] as? Session else { return }
        AppController.shared.select(session)
    }

    func numberOfRows(in tableView: NSTableView) -> Int {
        rows.count
    }

    func tableView(_ tableView: NSTableView, isGroupRow row: Int) -> Bool {
        rows[row] is BoardColumn
    }

    func tableView(_ tableView: NSTableView, shouldSelectRow row: Int) -> Bool {
        false
    }

    func tableView(_ tableView: NSTableView, heightOfRow row: Int) -> CGFloat {
        (rows[row] is BoardColumn ? 28 : 40) + 2 * AppState.shared.sidebarTextSize.offset
    }

    func tableView(_ tableView: NSTableView, viewFor tableColumn: NSTableColumn?, row: Int) -> NSView? {
        let offset = AppState.shared.sidebarTextSize.offset
        if let column = rows[row] as? BoardColumn {
            let count = rows.filter { ($0 as? Session).map { BoardColumn($0) == column } ?? false }.count
            let label = NSTextField(labelWithString: "\(column.title)  \(count)")
            label.font = .systemFont(ofSize: NSFont.smallSystemFontSize + offset, weight: .semibold)
            label.textColor = .secondaryLabelColor
            return label
        }
        guard let session = rows[row] as? Session else { return nil }
        let cell = tableView.makeView(withIdentifier: .init("board"), owner: nil) as? SidebarCell ?? SidebarCell()
        cell.identifier = .init("board")
        cell.icon.image = session.runningAgent.map(AgentBadge.image(for:)) ?? NSImage(systemSymbolName: "sparkles", accessibilityDescription: nil)
        cell.icon.contentTintColor = session.runningAgent == nil ? .secondaryLabelColor : nil
        cell.title.stringValue = session.title
        cell.title.font = .systemFont(ofSize: NSFont.systemFontSize + offset)
        cell.title.textColor = session.isRunning ? .labelColor : .tertiaryLabelColor
        cell.subtitle.stringValue = "\(session.project.name) · \(session.subtitle)"
        cell.subtitle.font = .systemFont(ofSize: NSFont.smallSystemFontSize + offset)
        cell.showStatus(of: session)
        cell.toolTip = "\(session.title)\n\(session.project.displayPath)"
        return cell
    }
}
