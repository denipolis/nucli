import AppKit

final class PickerTableView: NSTableView {
    var onConfirm: (() -> Void)?
    var onCancel: (() -> Void)?
    var onDigit: ((Int) -> Void)?

    override func keyDown(with event: NSEvent) {
        switch event.keyCode {
        case 36, 76:
            onConfirm?()
        case 53:
            onCancel?()
        default:
            if let digit = event.characters.flatMap(Int.init), (1...9).contains(digit) {
                onDigit?(digit)
            } else {
                super.keyDown(with: event)
            }
        }
    }
}

enum PickerItem {
    case profile(AgentProfile)
    case shell(ShellProfile)
    case conversation(Conversation, isOpen: Bool)
    case header(String)
}

final class ProfilePicker: NSObject, NSTableViewDataSource, NSTableViewDelegate {
    private var items: [PickerItem]
    private let launcherCount: Int
    private let initialRow: Int
    private let panel: NSPanel
    private let table = PickerTableView()
    private weak var parent: NSWindow?
    private var completion: ((PickerItem?) -> Void)?

    init(profiles: [AgentProfile], shells: [ShellProfile], last: String?, title: String, parent: NSWindow, completion: @escaping (PickerItem?) -> Void) {
        let launchers: [PickerItem] = profiles.map { .profile($0) } + (shells.isEmpty || profiles.isEmpty ? [] : [.header("Shells")]) + shells.map { .shell($0) }
        items = launchers
        launcherCount = launchers.count
        initialRow = profiles.firstIndex { $0.name == last } ?? shells.firstIndex { $0.name == last }.map { $0 + launchers.count - shells.count } ?? 0
        self.parent = parent
        self.completion = completion
        panel = NSPanel(contentRect: NSRect(x: 0, y: 0, width: 520, height: 460), styleMask: [.titled], backing: .buffered, defer: true)
        super.init()
        let label = NSTextField(labelWithString: title)
        label.font = .systemFont(ofSize: NSFont.systemFontSize, weight: .semibold)
        label.lineBreakMode = .byTruncatingMiddle
        table.addTableColumn(NSTableColumn(identifier: .init("item")))
        table.headerView = nil
        table.style = .inset
        table.dataSource = self
        table.delegate = self
        table.target = self
        table.doubleAction = #selector(confirm)
        table.onConfirm = { [weak self] in self?.confirm() }
        table.onCancel = { [weak self] in self?.finish(nil) }
        table.onDigit = { [weak self] digit in
            guard let self, let item = launchers[safe: digit - 1] else { return }
            finish(item)
        }
        let scroll = NSScrollView()
        scroll.documentView = table
        scroll.drawsBackground = false
        scroll.hasVerticalScroller = true
        let stack = NSStackView(views: [label, scroll])
        stack.orientation = .vertical
        stack.alignment = .leading
        stack.edgeInsets = NSEdgeInsets(top: 12, left: 12, bottom: 12, right: 12)
        stack.translatesAutoresizingMaskIntoConstraints = false
        let root = NSView()
        root.addSubview(stack)
        panel.contentView = root
        NSLayoutConstraint.activate([
            stack.topAnchor.constraint(equalTo: root.topAnchor),
            stack.bottomAnchor.constraint(equalTo: root.bottomAnchor),
            stack.leadingAnchor.constraint(equalTo: root.leadingAnchor),
            stack.trailingAnchor.constraint(equalTo: root.trailingAnchor),
            scroll.widthAnchor.constraint(equalTo: stack.widthAnchor, constant: -24),
        ])
        parent.beginSheet(panel)
        panel.makeFirstResponder(table)
    }

    private var launchers: [PickerItem] {
        items.prefix(launcherCount).filter { if case .header = $0 { false } else { true } }
    }

    func show(conversations: [(Conversation, Bool)]?) {
        let row = table.selectedRow >= 0 ? table.selectedRow : initialRow
        let recent: [PickerItem] = switch conversations {
        case nil: [.header("Loading…")]
        case let list? where list.isEmpty: [.header("No conversations in this folder yet")]
        case let list?: list.map { .conversation($0.0, isOpen: $0.1) }
        }
        items = Array(items.prefix(launcherCount)) + [.header("Recent conversations")] + recent
        table.reloadData()
        table.selectRowIndexes([row], byExtendingSelection: false)
        table.scrollRowToVisible(row)
    }

    @objc private func confirm() {
        guard let item = items[safe: table.selectedRow] else { return }
        if case .header = item { return }
        finish(item)
    }

    private func finish(_ item: PickerItem?) {
        guard let completion else { return }
        self.completion = nil
        parent?.endSheet(panel)
        completion(item)
    }

    func numberOfRows(in tableView: NSTableView) -> Int {
        items.count
    }

    func tableView(_ tableView: NSTableView, isGroupRow row: Int) -> Bool {
        if case .header = items[row] { return true }
        return false
    }

    func tableView(_ tableView: NSTableView, shouldSelectRow row: Int) -> Bool {
        !self.tableView(tableView, isGroupRow: row)
    }

    func tableView(_ tableView: NSTableView, heightOfRow row: Int) -> CGFloat {
        if case .conversation = items[row] { return 38 }
        return 26
    }

    func tableView(_ tableView: NSTableView, viewFor tableColumn: NSTableColumn?, row: Int) -> NSView? {
        switch items[row] {
        case .header(let title):
            let label = NSTextField(labelWithString: title)
            label.font = .systemFont(ofSize: NSFont.smallSystemFontSize, weight: .semibold)
            label.textColor = .secondaryLabelColor
            return label
        case .profile, .shell:
            let number = items.prefix(row).filter { if case .header = $0 { false } else { true } }.count + 1
            let index = NSTextField(labelWithString: number <= 9 ? "\(number)" : "")
            index.font = .monospacedDigitSystemFont(ofSize: NSFont.smallSystemFontSize, weight: .regular)
            index.textColor = .tertiaryLabelColor
            let (image, title, detail): (NSImage?, String, String) = if case .profile(let profile) = items[row] {
                (AgentBadge.image(for: profile.kind?.rawValue ?? profile.executableName), profile.name, profile.summary)
            } else if case .shell(let shell) = items[row] {
                (NSImage(systemSymbolName: "terminal", accessibilityDescription: nil), shell.name, shell.path)
            } else {
                (nil, "", "")
            }
            let badge = NSImageView(image: image ?? NSImage())
            badge.contentTintColor = .secondaryLabelColor
            let name = NSTextField(labelWithString: title)
            let command = NSTextField(labelWithString: detail)
            command.font = .monospacedSystemFont(ofSize: NSFont.smallSystemFontSize, weight: .regular)
            command.textColor = .secondaryLabelColor
            command.lineBreakMode = .byTruncatingTail
            command.setContentCompressionResistancePriority(.defaultLow, for: .horizontal)
            let stack = NSStackView(views: [index, badge, name, command])
            stack.spacing = 8
            return stack
        case .conversation(let conversation, let isOpen):
            let badge = NSImageView(image: AgentBadge.image(for: conversation.kind.rawValue))
            let title = NSTextField(labelWithString: conversation.title)
            title.lineBreakMode = .byTruncatingTail
            title.setContentCompressionResistancePriority(.defaultLow, for: .horizontal)
            let detail = NSTextField(labelWithString: "\(conversation.profile.name) · \(conversation.date.relativeDescription)\(isOpen ? " · open" : "")")
            detail.font = .systemFont(ofSize: NSFont.smallSystemFontSize)
            detail.textColor = .secondaryLabelColor
            let text = NSStackView(views: [title, detail])
            text.orientation = .vertical
            text.alignment = .leading
            text.spacing = 1
            let stack = NSStackView(views: [badge, text])
            stack.spacing = 8
            return stack
        }
    }
}
