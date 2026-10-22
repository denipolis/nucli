import AppKit

@MainActor
final class FileIndex {
    static let shared = FileIndex()
    private var entries: [String: [String]] = [:]
    private var inFlight: Set<String> = []

    func files(in path: String) -> [String]? {
        entries[path]
    }

    func refresh(_ path: String, completion: @escaping () -> Void) {
        guard inFlight.insert(path).inserted else { return }
        DispatchQueue.global(qos: .userInitiated).async {
            let files = Self.list(path)
            DispatchQueue.main.async { [self] in
                entries[path] = files
                inFlight.remove(path)
                completion()
            }
        }
    }

    nonisolated private static func list(_ path: String) -> [String] {
        if let output = Git.run(["ls-files", "--cached", "--others", "--exclude-standard", "--deduplicate"], in: path) {
            return output.split(separator: "\n").map(String.init).filter { FileManager.default.fileExists(atPath: path + "/" + $0) }
        }
        var result: [String] = []
        let base = URL(fileURLWithPath: path)
        let enumerator = FileManager.default.enumerator(at: base, includingPropertiesForKeys: [.isDirectoryKey], options: [.skipsHiddenFiles, .skipsPackageDescendants])
        while let url = enumerator?.nextObject() as? URL, result.count < 50000 {
            if url.lastPathComponent == "node_modules" {
                enumerator?.skipDescendants()
                continue
            }
            if (try? url.resourceValues(forKeys: [.isDirectoryKey]).isDirectory) == false {
                result.append(String(url.path.dropFirst(base.path.count + 1)))
            }
        }
        return result
    }
}

struct PaletteItem {
    let title: String
    var subtitle = ""
    var image: NSImage?
    var tint: NSColor?
    var detail = ""
    var isPath = false
    var rank = 0
    var alternate: (() -> Void)?
    let run: () -> Void
}

enum PaletteMode {
    case everything, files
}

final class PalettePanel: NSPanel {
    override var canBecomeKey: Bool { true }
}

final class PaletteRowView: NSTableRowView {
    override func drawSelection(in dirtyRect: NSRect) {
        NSColor.controlAccentColor.withAlphaComponent(0.85).setFill()
        NSBezierPath(roundedRect: bounds.insetBy(dx: 6, dy: 1), xRadius: 6, yRadius: 6).fill()
    }

    override var interiorBackgroundStyle: NSView.BackgroundStyle { isSelected ? .emphasized : .normal }
}

@MainActor
final class Palette: NSObject, NSTextFieldDelegate, NSTableViewDataSource, NSTableViewDelegate, NSWindowDelegate {
    private let panel = PalettePanel(contentRect: NSRect(x: 0, y: 0, width: 640, height: 440), styleMask: [.borderless, .fullSizeContentView], backing: .buffered, defer: true)
    private let field = NSTextField()
    private let table = NSTableView()
    private let mode: PaletteMode
    private let source: () -> [PaletteItem]
    private var all: [PaletteItem] = []
    private var shown: [(PaletteItem, FuzzyMatch?)] = []
    private weak var parent: NSWindow?
    var onClose: (() -> Void)?

    init(mode: PaletteMode, parent: NSWindow, anchor: NSRect?, placeholder: String, source: @escaping () -> [PaletteItem]) {
        self.mode = mode
        self.parent = parent
        self.source = source
        super.init()
        build(placeholder: placeholder)
        reload()
        let frame = parent.frame
        let midX = anchor?.midX ?? frame.midX
        let top = anchor?.maxY ?? frame.maxY - 80
        panel.setFrameOrigin(NSPoint(x: midX - panel.frame.width / 2, y: top - panel.frame.height))
        parent.addChildWindow(panel, ordered: .above)
        panel.makeKeyAndOrderFront(nil)
        panel.makeFirstResponder(field)
    }

    private func build(placeholder: String) {
        panel.isOpaque = false
        panel.backgroundColor = .clear
        panel.hasShadow = true
        panel.delegate = self
        let background = NSVisualEffectView()
        background.material = .popover
        background.state = .active
        background.wantsLayer = true
        background.layer?.cornerRadius = 12
        background.layer?.masksToBounds = true
        background.layer?.borderWidth = 0.5
        background.layer?.borderColor = NSColor.separatorColor.cgColor
        field.placeholderString = placeholder
        field.font = .systemFont(ofSize: 17)
        field.isBordered = false
        field.drawsBackground = false
        field.focusRingType = .none
        field.delegate = self
        let icon = NSImageView(image: NSImage(systemSymbolName: "magnifyingglass", accessibilityDescription: nil)!)
        icon.symbolConfiguration = .init(pointSize: 15, weight: .regular)
        icon.contentTintColor = .secondaryLabelColor
        let separator = NSBox()
        separator.boxType = .separator
        table.addTableColumn(NSTableColumn(identifier: .init("item")))
        table.headerView = nil
        table.style = .plain
        table.backgroundColor = .clear
        table.rowHeight = 34
        table.intercellSpacing = NSSize(width: 0, height: 2)
        table.dataSource = self
        table.delegate = self
        table.target = self
        table.action = #selector(clicked)
        table.refusesFirstResponder = true
        let scroll = NSScrollView()
        scroll.documentView = table
        scroll.drawsBackground = false
        scroll.hasVerticalScroller = false
        for view in [icon, field, separator, scroll] as [NSView] {
            view.translatesAutoresizingMaskIntoConstraints = false
            background.addSubview(view)
        }
        panel.contentView = background
        NSLayoutConstraint.activate([
            icon.leadingAnchor.constraint(equalTo: background.leadingAnchor, constant: 16),
            icon.centerYAnchor.constraint(equalTo: field.centerYAnchor),
            field.topAnchor.constraint(equalTo: background.topAnchor, constant: 14),
            field.leadingAnchor.constraint(equalTo: icon.trailingAnchor, constant: 10),
            field.trailingAnchor.constraint(equalTo: background.trailingAnchor, constant: -16),
            separator.topAnchor.constraint(equalTo: field.bottomAnchor, constant: 12),
            separator.leadingAnchor.constraint(equalTo: background.leadingAnchor),
            separator.trailingAnchor.constraint(equalTo: background.trailingAnchor),
            scroll.topAnchor.constraint(equalTo: separator.bottomAnchor, constant: 6),
            scroll.leadingAnchor.constraint(equalTo: background.leadingAnchor),
            scroll.trailingAnchor.constraint(equalTo: background.trailingAnchor),
            scroll.bottomAnchor.constraint(equalTo: background.bottomAnchor, constant: -6),
        ])
    }

    func reload() {
        all = source()
        filter()
    }

    private func filter() {
        let query = field.stringValue.trimmingCharacters(in: .whitespaces)
        if query.isEmpty {
            shown = all.filter { mode == .files || !$0.isPath }.prefix(300).map { ($0, FuzzyMatch?.none) }
        } else {
            let scored = all.compactMap { item -> (PaletteItem, FuzzyMatch)? in
                let match = item.isPath ? Fuzzy.matchPath(query, path: item.title) : Fuzzy.match(query, in: item.title)
                return match.map { (item, $0) }
            }
            let sorted = scored.sorted { left, right in
                let a = left.1.score + left.0.rank
                let b = right.1.score + right.0.rank
                return a != b ? a > b : left.0.title.count < right.0.title.count
            }
            shown = sorted.prefix(300).map { ($0.0, Optional($0.1)) }
        }
        table.reloadData()
        if !shown.isEmpty {
            table.selectRowIndexes([0], byExtendingSelection: false)
            table.scrollRowToVisible(0)
        }
    }

    func controlTextDidChange(_ obj: Notification) {
        filter()
    }

    func control(_ control: NSControl, textView: NSTextView, doCommandBy selector: Selector) -> Bool {
        switch selector {
        case #selector(NSResponder.moveDown(_:)): move(1)
        case #selector(NSResponder.moveUp(_:)): move(-1)
        case #selector(NSResponder.pageDown(_:)), #selector(NSResponder.scrollPageDown(_:)): move(10)
        case #selector(NSResponder.pageUp(_:)), #selector(NSResponder.scrollPageUp(_:)): move(-10)
        case #selector(NSResponder.insertNewline(_:)): confirm(table.selectedRow)
        case #selector(NSResponder.cancelOperation(_:)): close()
        case Selector(("noop:")) where NSApp.currentEvent?.keyCode == 36: confirm(table.selectedRow)
        default: return false
        }
        return true
    }

    private func move(_ step: Int) {
        guard !shown.isEmpty else { return }
        let row = min(max(table.selectedRow + step, 0), shown.count - 1)
        table.selectRowIndexes([row], byExtendingSelection: false)
        table.scrollRowToVisible(row)
    }

    @objc private func clicked() {
        confirm(table.clickedRow)
    }

    private func confirm(_ row: Int) {
        guard let item = shown[safe: row]?.0 else { return }
        close()
        if NSApp.currentEvent?.modifierFlags.contains(.command) == true, let alternate = item.alternate {
            alternate()
        } else {
            item.run()
        }
    }

    func close() {
        guard panel.isVisible else { return }
        panel.delegate = nil
        parent?.removeChildWindow(panel)
        panel.orderOut(nil)
        parent?.makeKeyAndOrderFront(nil)
        onClose?()
    }

    func windowDidResignKey(_ notification: Notification) {
        close()
    }

    func numberOfRows(in tableView: NSTableView) -> Int {
        shown.count
    }

    func tableView(_ tableView: NSTableView, rowViewForRow row: Int) -> NSTableRowView? {
        PaletteRowView()
    }

    func tableView(_ tableView: NSTableView, viewFor tableColumn: NSTableColumn?, row: Int) -> NSView? {
        guard let (item, match) = shown[safe: row] else { return nil }
        let icon = NSImageView()
        icon.image = item.image
        icon.contentTintColor = item.tint
        icon.symbolConfiguration = .init(pointSize: 13, weight: .regular)
        icon.translatesAutoresizingMaskIntoConstraints = false
        icon.widthAnchor.constraint(equalToConstant: 18).isActive = true
        let title = NSTextField(labelWithAttributedString: highlighted(item, match))
        title.lineBreakMode = item.isPath ? .byTruncatingHead : .byTruncatingTail
        title.maximumNumberOfLines = 1
        title.usesSingleLineMode = true
        title.setContentCompressionResistancePriority(.defaultLow, for: .horizontal)
        let subtitle = NSTextField(labelWithString: item.subtitle)
        subtitle.font = .systemFont(ofSize: NSFont.smallSystemFontSize)
        subtitle.textColor = .secondaryLabelColor
        subtitle.lineBreakMode = .byTruncatingMiddle
        subtitle.setContentCompressionResistancePriority(.init(100), for: .horizontal)
        let spacer = NSView()
        spacer.setContentHuggingPriority(.init(1), for: .horizontal)
        let detail = NSTextField(labelWithString: item.detail)
        detail.font = .systemFont(ofSize: NSFont.smallSystemFontSize)
        detail.textColor = .tertiaryLabelColor
        detail.setContentCompressionResistancePriority(.required, for: .horizontal)
        let stack = NSStackView(views: [icon, title, subtitle, spacer, detail])
        stack.spacing = 8
        stack.edgeInsets = NSEdgeInsets(top: 0, left: 16, bottom: 0, right: 16)
        return stack
    }

    private func highlighted(_ item: PaletteItem, _ match: FuzzyMatch?) -> NSAttributedString {
        let paragraph = NSMutableParagraphStyle()
        paragraph.lineBreakMode = item.isPath ? .byTruncatingHead : .byTruncatingTail
        let text = NSMutableAttributedString(string: item.title, attributes: [.font: NSFont.systemFont(ofSize: 13), .foregroundColor: NSColor.labelColor, .paragraphStyle: paragraph])
        if item.isPath {
            let name = (item.title as NSString).lastPathComponent
            text.addAttribute(.foregroundColor, value: NSColor.secondaryLabelColor, range: NSRange(location: 0, length: (item.title as NSString).length - (name as NSString).length))
        }
        let characters = Array(item.title)
        var utf16 = 0
        var offsets: [Int] = []
        for character in characters {
            offsets.append(utf16)
            utf16 += character.utf16.count
        }
        for position in match?.positions ?? [] where position < characters.count {
            let range = NSRange(location: offsets[position], length: characters[position].utf16.count)
            text.addAttributes([.font: NSFont.systemFont(ofSize: 13, weight: .bold), .foregroundColor: NSColor.labelColor], range: range)
        }
        return text
    }
}
