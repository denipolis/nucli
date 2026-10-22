import AppKit

@MainActor
final class HistoryList: NSObject, NSTableViewDataSource, NSTableViewDelegate {
    let view = NSView()
    private let scroll = NSScrollView()
    var onOpen: ((HistoryEntry) -> Void)?
    var filter = "" { didSet { apply() } }
    private let table = NSTableView()
    private let empty = NSTextField(labelWithString: "")
    private var all: [HistoryEntry] = []
    private var shown: [HistoryEntry] = []
    private var isLoading = false

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
        table.setAccessibilityLabel("Agent conversation history")
        scroll.documentView = table
        scroll.drawsBackground = false
        scroll.hasVerticalScroller = false
        empty.textColor = .tertiaryLabelColor
        empty.alignment = .center
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
        NotificationCenter.default.addObserver(forName: .conversationsChanged, object: nil, queue: .main) { [weak self] _ in
            MainActor.assumeIsolated { if self?.view.isHidden == false { self?.refresh() } }
        }
        NotificationCenter.default.addObserver(forName: .settingsChanged, object: nil, queue: .main) { [weak self] _ in
            MainActor.assumeIsolated { self?.table.reloadData() }
        }
    }

    func refresh() {
        guard !isLoading else { return }
        isLoading = true
        if all.isEmpty { empty.stringValue = "Loading…" }
        let profiles = AppState.shared.profiles
        DispatchQueue.global(qos: .utility).async {
            let entries = ConversationHistory.all(profiles: profiles)
            DispatchQueue.main.async { [weak self] in
                guard let self else { return }
                isLoading = false
                all = entries
                apply()
            }
        }
    }

    private func projectName(_ entry: HistoryEntry) -> String {
        let project = AppState.shared.projects.first { $0.path == entry.cwd && !$0.isScratch }
        return project?.name ?? (entry.cwd as NSString).lastPathComponent
    }

    private func apply() {
        shown = filter.isEmpty ? all : all.filter { entry in
            [entry.conversation.title, entry.conversation.profile.name, projectName(entry), entry.cwd].contains { $0.localizedCaseInsensitiveContains(filter) }
        }
        empty.stringValue = isLoading && all.isEmpty ? "Loading…" : shown.isEmpty ? (filter.isEmpty ? "No conversations" : "No matches") : ""
        table.reloadData()
    }

    func openFirst() {
        shown.first.map { onOpen?($0) }
    }

    @objc private func clicked() {
        guard let entry = shown[safe: table.clickedRow] else { return }
        onOpen?(entry)
    }

    func numberOfRows(in tableView: NSTableView) -> Int {
        shown.count
    }

    func tableView(_ tableView: NSTableView, heightOfRow row: Int) -> CGFloat {
        40 + 2 * AppState.shared.sidebarTextSize.offset
    }

    func tableView(_ tableView: NSTableView, viewFor tableColumn: NSTableColumn?, row: Int) -> NSView? {
        let entry = shown[row]
        let offset = AppState.shared.sidebarTextSize.offset
        let cell = tableView.makeView(withIdentifier: .init("history"), owner: nil) as? SidebarCell ?? SidebarCell()
        cell.identifier = .init("history")
        cell.icon.image = AgentBadge.image(for: entry.conversation.profile.kind?.rawValue ?? entry.conversation.profile.executableName)
        cell.icon.contentTintColor = nil
        cell.title.stringValue = entry.conversation.title
        cell.title.font = .systemFont(ofSize: NSFont.systemFontSize + offset)
        cell.title.textColor = .labelColor
        cell.subtitle.stringValue = "\(projectName(entry)) · \(entry.conversation.profile.name) · \(entry.conversation.date.relativeDescription)"
        cell.subtitle.font = .systemFont(ofSize: NSFont.smallSystemFontSize + offset)
        cell.showStatus(of: nil)
        cell.toolTip = "\(entry.conversation.title)\n\(entry.cwd)"
        return cell
    }
}
