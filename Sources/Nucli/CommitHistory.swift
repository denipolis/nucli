import AppKit

struct Commit: Equatable {
    let hash: String
    let parents: [String]
    let refs: [String]
    let author: String
    let date: Date
    let subject: String
}

struct GraphSegment {
    let from: Int
    let to: Int
    let lower: Bool
    let lane: Int
}

struct GraphRow {
    let node: Int
    let segments: [GraphSegment]
}

enum CommitGraph {
    static func log(in dir: String, limit: Int = 400) -> [Commit]? {
        guard let output = Git.run(["log", "--branches", "--remotes", "--tags", "HEAD", "--topo-order", "-n", "\(limit)", "--format=%H%x1f%P%x1f%D%x1f%an%x1f%at%x1f%s"], in: dir) else { return nil }
        return output.split(separator: "\n").compactMap { line in
            let parts = line.split(separator: "\u{1f}", maxSplits: 5, omittingEmptySubsequences: false)
            guard parts.count == 6 else { return nil }
            let refs = parts[2].split(separator: ",").map { $0.trimmingCharacters(in: .whitespaces) }.filter { !$0.isEmpty && !$0.hasSuffix("/HEAD") }
            return Commit(hash: String(parts[0]), parents: parts[1].split(separator: " ").map(String.init), refs: refs, author: String(parts[3]), date: Date(timeIntervalSince1970: TimeInterval(parts[4]) ?? 0), subject: String(parts[5]))
        }
    }

    static func layout(_ commits: [Commit]) -> [GraphRow] {
        var lanes: [String?] = []
        return commits.map { commit in
            let node = lanes.firstIndex(of: commit.hash) ?? lanes.firstIndex(of: nil) ?? lanes.count
            if node == lanes.count { lanes.append(nil) }
            var segments: [GraphSegment] = []
            for (lane, hash) in lanes.enumerated() {
                guard let hash else { continue }
                segments.append(GraphSegment(from: lane, to: hash == commit.hash ? node : lane, lower: false, lane: lane))
                if hash == commit.hash { lanes[lane] = nil }
            }
            lanes[node] = commit.parents.first
            var targets: Set<Int> = commit.parents.isEmpty ? [] : [node]
            for parent in commit.parents.dropFirst() {
                let lane = lanes.firstIndex(of: parent) ?? lanes.firstIndex(of: nil) ?? lanes.count
                if lane == lanes.count { lanes.append(nil) }
                lanes[lane] = parent
                targets.insert(lane)
            }
            for (lane, hash) in lanes.enumerated() where hash != nil {
                if targets.contains(lane) { segments.append(GraphSegment(from: node, to: lane, lower: true, lane: lane)) }
                if !targets.contains(lane) || lane != node && segments.contains(where: { !$0.lower && $0.from == lane && $0.to == lane }) {
                    segments.append(GraphSegment(from: lane, to: lane, lower: true, lane: lane))
                }
            }
            while lanes.last == .some(nil) { lanes.removeLast() }
            return GraphRow(node: node, segments: segments)
        }
    }

    static let colors: [NSColor] = [.systemBlue, .systemPink, .systemGreen, .systemOrange, .systemPurple, .systemTeal, .systemYellow, .systemRed, .systemIndigo]

    static func color(_ lane: Int) -> NSColor {
        colors[lane % colors.count]
    }
}

final class CommitRowView: NSView {
    static let laneWidth: CGFloat = 12
    var commit: Commit?
    var row: GraphRow?
    var graphLanes = 1
    var isHead = false
    private static let dates: RelativeDateTimeFormatter = {
        let formatter = RelativeDateTimeFormatter()
        formatter.unitsStyle = .short
        return formatter
    }()

    override var isFlipped: Bool { true }

    private func x(_ lane: Int) -> CGFloat {
        8 + CGFloat(lane) * Self.laneWidth + Self.laneWidth / 2
    }

    override func draw(_ dirtyRect: NSRect) {
        guard let commit, let row else { return }
        let height = bounds.height
        let mid = height / 2
        for segment in row.segments {
            let path = NSBezierPath()
            path.lineWidth = 2
            let start = NSPoint(x: x(segment.from), y: segment.lower ? mid : 0)
            let end = NSPoint(x: x(segment.to), y: segment.lower ? height : mid)
            path.move(to: start)
            if start.x == end.x {
                path.line(to: end)
            } else {
                path.curve(to: end, controlPoint1: NSPoint(x: start.x, y: (start.y + end.y) / 2), controlPoint2: NSPoint(x: end.x, y: (start.y + end.y) / 2))
            }
            CommitGraph.color(segment.lane).setStroke()
            path.stroke()
        }
        let color = CommitGraph.color(row.node)
        let radius: CGFloat = isHead ? 5 : commit.parents.count > 1 ? 3 : 4
        let dot = NSBezierPath(ovalIn: NSRect(x: x(row.node) - radius, y: mid - radius, width: radius * 2, height: radius * 2))
        color.setFill()
        dot.fill()
        if isHead {
            NSColor.windowBackgroundColor.setFill()
            NSBezierPath(ovalIn: NSRect(x: x(row.node) - 2, y: mid - 2, width: 4, height: 4)).fill()
        }

        var left = 8 + CGFloat(graphLanes) * Self.laneWidth + 6
        let right = bounds.width - 8
        let small = NSFont.systemFont(ofSize: 10, weight: .medium)
        for ref in commit.refs where left < right - 20 {
            let name = ref.replacingOccurrences(of: "HEAD -> ", with: "").replacingOccurrences(of: "tag: ", with: "")
            let text = NSAttributedString(string: name, attributes: [.font: small, .foregroundColor: color])
            let width = min(text.size().width + 10, right - left)
            let pill = NSRect(x: left, y: 5, width: width, height: 15)
            color.withAlphaComponent(ref.hasPrefix("HEAD") ? 0.3 : 0.15).setFill()
            NSBezierPath(roundedRect: pill, xRadius: 4, yRadius: 4).fill()
            text.draw(with: pill.insetBy(dx: 5, dy: 1), options: [.usesLineFragmentOrigin, .truncatesLastVisibleLine])
            left += width + 4
        }
        let paragraph = NSMutableParagraphStyle()
        paragraph.lineBreakMode = .byTruncatingTail
        let subject = NSAttributedString(string: commit.subject, attributes: [.font: NSFont.systemFont(ofSize: NSFont.smallSystemFontSize), .foregroundColor: NSColor.labelColor, .paragraphStyle: paragraph])
        if right > left { subject.draw(with: NSRect(x: left, y: 4, width: right - left, height: 16), options: [.usesLineFragmentOrigin, .truncatesLastVisibleLine]) }
        let textLeft = 8 + CGFloat(graphLanes) * Self.laneWidth + 6
        let meta = "\(commit.hash.prefix(7))  \(commit.author)  \(Self.dates.localizedString(for: commit.date, relativeTo: Date()))"
        let detail = NSAttributedString(string: meta, attributes: [.font: NSFont.monospacedDigitSystemFont(ofSize: 10, weight: .regular), .foregroundColor: NSColor.secondaryLabelColor, .paragraphStyle: paragraph])
        detail.draw(with: NSRect(x: textLeft, y: 21, width: max(0, right - textLeft), height: 14), options: [.usesLineFragmentOrigin, .truncatesLastVisibleLine])
    }
}

final class CommitFileRowView: NSView {
    var change: FileChange?
    var lanes: [Int] = []
    var graphLanes = 1

    override var isFlipped: Bool { true }

    override func draw(_ dirtyRect: NSRect) {
        guard let change else { return }
        for lane in lanes {
            let x = 8 + CGFloat(lane) * CommitRowView.laneWidth + CommitRowView.laneWidth / 2
            let path = NSBezierPath()
            path.lineWidth = 2
            path.move(to: NSPoint(x: x, y: 0))
            path.line(to: NSPoint(x: x, y: bounds.height))
            CommitGraph.color(lane).setStroke()
            path.stroke()
        }
        var left = 8 + CGFloat(graphLanes) * CommitRowView.laneWidth + 14
        let right = bounds.width - 8
        let name = (change.path as NSString).lastPathComponent
        let (image, color) = FileIcon.icon(for: name, isDirectory: false)
        if let icon = image?.withSymbolConfiguration(NSImage.SymbolConfiguration(pointSize: 10, weight: .regular).applying(.init(paletteColors: [color ?? .secondaryLabelColor]))) {
            icon.draw(in: NSRect(x: left, y: (bounds.height - icon.size.height) / 2, width: icon.size.width, height: icon.size.height))
            left += icon.size.width + 5
        }
        let font = NSFont.monospacedDigitSystemFont(ofSize: 10, weight: .medium)
        let counts = NSMutableAttributedString(string: "+\(change.added)", attributes: [.font: font, .foregroundColor: NSColor.systemGreen])
        counts.append(NSAttributedString(string: " −\(change.removed)", attributes: [.font: font, .foregroundColor: NSColor.systemRed]))
        let countsWidth = counts.size().width
        counts.draw(at: NSPoint(x: right - countsWidth, y: (bounds.height - counts.size().height) / 2))
        let paragraph = NSMutableParagraphStyle()
        paragraph.lineBreakMode = .byTruncatingMiddle
        let title = NSMutableAttributedString(string: name, attributes: [.font: NSFont.systemFont(ofSize: NSFont.smallSystemFontSize), .foregroundColor: FileChange.color(for: change.status), .paragraphStyle: paragraph])
        let dir = (change.path as NSString).deletingLastPathComponent
        if !dir.isEmpty { title.append(NSAttributedString(string: "  " + dir, attributes: [.font: NSFont.systemFont(ofSize: 10), .foregroundColor: NSColor.tertiaryLabelColor, .paragraphStyle: paragraph])) }
        let width = right - countsWidth - 8 - left
        if width > 0 { title.draw(with: NSRect(x: left, y: (bounds.height - 15) / 2, width: width, height: 15), options: [.usesLineFragmentOrigin, .truncatesLastVisibleLine]) }
    }
}

final class CommitHistoryViewController: NSViewController, NSTableViewDataSource, NSTableViewDelegate {
    private let header = NSTextField(labelWithString: "")
    private let summary = NSTextField(labelWithString: "")
    private let table = NSTableView()
    private let empty = NSTextField(labelWithString: "")
    weak var content: ContentViewController?
    private enum Item {
        case commit(Int)
        case file(Int, FileChange)
    }
    private var commits: [Commit] = []
    private var items: [Item] = []
    private var expanded: Set<String> = []
    private var files: [String: [FileChange]] = [:]
    private var rows: [GraphRow] = []
    private var head: String?
    private var total = 0
    private var graphLanes = 1
    private var isLoading = false
    private var needsLoad = false

    var project: Project? {
        didSet {
            guard project !== oldValue else { return }
            commits = []
            rows = []
            expanded = []
            files = [:]
            rebuildItems()
            refresh()
        }
    }

    override func loadView() {
        header.font = .systemFont(ofSize: NSFont.systemFontSize, weight: .semibold)
        header.lineBreakMode = .byTruncatingTail
        summary.font = .monospacedDigitSystemFont(ofSize: NSFont.smallSystemFontSize, weight: .regular)
        summary.textColor = .secondaryLabelColor
        table.addTableColumn(NSTableColumn(identifier: .init("main")))
        table.headerView = nil
        table.style = .plain
        table.backgroundColor = .clear
        table.intercellSpacing = .zero
        table.rowHeight = 38
        table.dataSource = self
        table.delegate = self
        table.target = self
        table.action = #selector(clicked)
        table.menu = NSMenu()
        table.menu?.addItem(withTitle: "Copy Hash", action: #selector(copyHash), keyEquivalent: "").target = self
        table.menu?.addItem(withTitle: "Copy Message", action: #selector(copySubject), keyEquivalent: "").target = self
        table.setAccessibilityLabel("Commit history")
        let scroll = NSScrollView()
        scroll.documentView = table
        scroll.drawsBackground = false
        scroll.hasVerticalScroller = false
        scroll.automaticallyAdjustsContentInsets = false
        empty.textColor = .tertiaryLabelColor
        empty.alignment = .center
        let titles = NSStackView(views: [header, summary])
        titles.orientation = .vertical
        titles.alignment = .leading
        titles.spacing = 2
        titles.edgeInsets = NSEdgeInsets(top: 8, left: 14, bottom: 6, right: 12)
        let root = NSView()
        for child in [titles, scroll, empty] {
            child.translatesAutoresizingMaskIntoConstraints = false
            root.addSubview(child)
        }
        NSLayoutConstraint.activate([
            titles.topAnchor.constraint(equalTo: root.topAnchor),
            titles.leadingAnchor.constraint(equalTo: root.leadingAnchor),
            titles.trailingAnchor.constraint(equalTo: root.trailingAnchor),
            scroll.topAnchor.constraint(equalTo: titles.bottomAnchor),
            scroll.bottomAnchor.constraint(equalTo: root.bottomAnchor),
            scroll.leadingAnchor.constraint(equalTo: root.leadingAnchor),
            scroll.trailingAnchor.constraint(equalTo: root.trailingAnchor),
            empty.centerXAnchor.constraint(equalTo: root.centerXAnchor),
            empty.topAnchor.constraint(equalTo: scroll.topAnchor, constant: 24),
        ])
        view = root
        NotificationCenter.default.addObserver(forName: .gitChanged, object: nil, queue: .main) { [weak self] note in
            MainActor.assumeIsolated { if note.object as? Project === self?.project { self?.refresh() } }
        }
        NotificationCenter.default.addObserver(forName: NSApplication.didBecomeActiveNotification, object: nil, queue: .main) { [weak self] _ in
            MainActor.assumeIsolated { self?.refresh() }
        }
        updateHeader()
    }

    func refresh() {
        guard let project else { return updateHeader() }
        guard !view.isHiddenOrHasHiddenAncestor else {
            needsLoad = true
            return
        }
        needsLoad = false
        guard !isLoading else {
            needsLoad = true
            return
        }
        isLoading = true
        updateHeader()
        let path = project.path
        DispatchQueue.global(qos: .userInitiated).async {
            let result = CommitGraph.log(in: path)
            let layout = CommitGraph.layout(result ?? [])
            let head = Git.run(["rev-parse", "HEAD"], in: path)?.trimmingCharacters(in: .whitespacesAndNewlines)
            let total = Git.run(["rev-list", "--count", "--branches", "--remotes", "--tags", "HEAD"], in: path).flatMap { Int($0.trimmingCharacters(in: .whitespacesAndNewlines)) }
            DispatchQueue.main.async { [weak self] in
                guard let self else { return }
                isLoading = false
                if self.project === project, result != commits || head != self.head {
                    commits = result ?? []
                    rows = layout
                    self.head = head
                    self.total = total ?? commits.count
                    graphLanes = min(12, (layout.flatMap { $0.segments.flatMap { [$0.from, $0.to] } } + layout.map(\.node)).max().map { $0 + 1 } ?? 1)
                    rebuildItems()
                    empty.stringValue = result == nil ? "Not a git repository" : commits.isEmpty ? "No commits yet" : ""
                }
                updateHeader()
                if needsLoad { refresh() }
            }
        }
    }

    func reloadIfNeeded() {
        if needsLoad || commits.isEmpty { refresh() }
    }

    private func updateHeader() {
        guard let project else {
            header.stringValue = "No project"
            summary.stringValue = ""
            empty.stringValue = ""
            return
        }
        header.stringValue = project.name
        summary.stringValue = isLoading && commits.isEmpty ? "Loading…" : commits.isEmpty ? "" : "\(total) commits"
    }

    private func rebuildItems() {
        items = commits.indices.flatMap { index -> [Item] in
            let hash = commits[index].hash
            return [.commit(index)] + (expanded.contains(hash) ? (files[hash] ?? []).map { .file(index, $0) } : [])
        }
        table.reloadData()
    }

    @objc private func clicked() {
        guard let project, table.clickedRow >= 0, table.clickedRow < items.count else { return }
        switch items[table.clickedRow] {
        case .file(_, let change):
            content?.showFileDiff(change, in: project)
        case .commit(let index):
            let commit = commits[index]
            if expanded.remove(commit.hash) != nil { return rebuildItems() }
            expanded.insert(commit.hash)
            if files[commit.hash] != nil { return rebuildItems() }
            let path = project.path
            DispatchQueue.global(qos: .userInitiated).async {
                let changes = Git.changes(in: commit, dir: path)
                DispatchQueue.main.async { [weak self] in
                    guard let self, self.project === project else { return }
                    files[commit.hash] = changes
                    rebuildItems()
                }
            }
        }
    }

    func numberOfRows(in tableView: NSTableView) -> Int { items.count }

    func tableView(_ tableView: NSTableView, heightOfRow row: Int) -> CGFloat {
        if case .file = items[row] { return 22 }
        return 38
    }

    func tableView(_ tableView: NSTableView, viewFor tableColumn: NSTableColumn?, row: Int) -> NSView? {
        let index: Int
        switch items[row] {
        case .file(let commitIndex, let change):
            let view = tableView.makeView(withIdentifier: .init("file"), owner: nil) as? CommitFileRowView ?? CommitFileRowView()
            view.identifier = .init("file")
            view.change = change
            view.lanes = rows[commitIndex].segments.filter(\.lower).map(\.to)
            view.graphLanes = graphLanes
            view.toolTip = change.path
            view.needsDisplay = true
            return view
        case .commit(let commitIndex):
            index = commitIndex
        }
        let view = tableView.makeView(withIdentifier: .init("commit"), owner: nil) as? CommitRowView ?? CommitRowView()
        view.identifier = .init("commit")
        let commit = commits[index]
        view.commit = commit
        view.row = rows[index]
        view.graphLanes = graphLanes
        view.isHead = commit.hash == head
        view.toolTip = "\(commit.hash)\n\(commit.author), \(commit.date.formatted(date: .abbreviated, time: .shortened))\n\n\(commit.subject)"
        view.needsDisplay = true
        return view
    }

    private var menuCommit: Commit? {
        let row = table.clickedRow >= 0 ? table.clickedRow : table.selectedRow
        guard let item = items[safe: row] else { return nil }
        switch item {
        case .commit(let index), .file(let index, _): return commits[index]
        }
    }

    @objc private func copyHash() {
        guard let clicked = menuCommit else { return }
        NSPasteboard.general.clearContents()
        NSPasteboard.general.setString(clicked.hash, forType: .string)
    }

    @objc private func copySubject() {
        guard let clicked = menuCommit else { return }
        NSPasteboard.general.clearContents()
        NSPasteboard.general.setString(clicked.subject, forType: .string)
    }
}
