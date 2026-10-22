import AppKit

enum PanelTab: Int, CaseIterable {
    case files, changes, history

    var title: String {
        switch self {
        case .files: "Files"
        case .changes: "Changes"
        case .history: "Git History"
        }
    }

    var symbol: String {
        switch self {
        case .files: "doc.on.doc"
        case .changes: "arrow.triangle.branch"
        case .history: "clock.arrow.circlepath"
        }
    }
}

@MainActor
private func trackPanelDrag(from event: NSEvent, in window: NSWindow?, onDrag: ((NSPoint) -> Void)?) -> NSEvent? {
    var dragging = false
    while let next = window?.nextEvent(matching: [.leftMouseDragged, .leftMouseUp]) {
        guard next.type == .leftMouseDragged else { return dragging ? nil : next }
        if !dragging, hypot(next.locationInWindow.x - event.locationInWindow.x, next.locationInWindow.y - event.locationInWindow.y) < 4 { continue }
        dragging = true
        NSCursor.closedHand.set()
        onDrag?(next.locationInWindow)
    }
    return nil
}

final class PanelHeaderView: NSView {
    var onDrag: ((NSPoint) -> Void)?
    var onMenu: (() -> NSMenu)?

    override var mouseDownCanMoveWindow: Bool { false }

    override func mouseDown(with event: NSEvent) {
        guard event.clickCount == 1 else { return }
        _ = trackPanelDrag(from: event, in: window, onDrag: onDrag)
        NSCursor.arrow.set()
    }

    override func menu(for event: NSEvent) -> NSMenu? {
        onMenu?()
    }

    override func resetCursorRects() {
        addCursorRect(bounds, cursor: .openHand)
    }
}

final class PanelTabControl: ResponsiveSegmentedControl {
    var onDrag: ((NSPoint) -> Void)?

    override var mouseDownCanMoveWindow: Bool { false }

    override func mouseDown(with event: NSEvent) {
        guard let up = trackPanelDrag(from: event, in: window, onDrag: onDrag) else {
            NSCursor.arrow.set()
            return
        }
        window?.postEvent(up, atStart: true)
        super.mouseDown(with: event)
    }
    override func resetCursorRects() {
        addCursorRect(bounds, cursor: .arrow)
    }
}

final class PanelViewController: NSViewController {
    private let panes: [NSViewController]
    private let control = PanelTabControl()
    private let holder = NSView()
    private let header = PanelHeaderView()
    private var leading: NSLayoutConstraint!
    private var trailing: NSLayoutConstraint!

    var side: PanelSide = .left {
        didSet {
            leading.constant = side == .left ? 8 : 4
            trailing.constant = side == .left ? -4 : -8
        }
    }
    private(set) var selected = PanelTab(rawValue: UserDefaults.standard.integer(forKey: "panelTab")) ?? .files

    init(files: NSViewController, changes: NSViewController, history: NSViewController) {
        panes = [files, changes, history]
        super.init(nibName: nil, bundle: nil)
    }

    required init?(coder: NSCoder) { fatalError() }

    override func loadView() {
        control.segmentCount = PanelTab.allCases.count
        control.trackingMode = .selectOne
        control.segmentDistribution = .fillEqually
        control.target = self
        control.action = #selector(changed)
        for tab in PanelTab.allCases {
            control.setImage(NSImage(systemSymbolName: tab.symbol, accessibilityDescription: tab.title), forSegment: tab.rawValue)
            control.setToolTip("\(tab.title) (⌃⌘\(tab.rawValue + 1))", forSegment: tab.rawValue)
        }
        control.labels = PanelTab.allCases.map { $0 == .history ? "History" : $0.title }
        control.controlSize = .small
        control.font = .systemFont(ofSize: NSFont.smallSystemFontSize)
        control.translatesAutoresizingMaskIntoConstraints = false
        holder.translatesAutoresizingMaskIntoConstraints = false
        let inner = NSView()
        header.translatesAutoresizingMaskIntoConstraints = false
        header.toolTip = "Drag to move Files and Changes to the other side"
        header.onDrag = { [weak self] point in self?.dragged(to: point) }
        control.onDrag = header.onDrag
        header.onMenu = { [weak self] in self?.placementMenu() ?? NSMenu() }
        inner.addSubview(header)
        inner.addSubview(control)
        inner.addSubview(holder)
        let panel = Self.panel(containing: inner)
        panel.translatesAutoresizingMaskIntoConstraints = false
        inner.translatesAutoresizingMaskIntoConstraints = false
        let root = NSView()
        root.wantsLayer = true
        root.addSubview(panel)
        leading = panel.leadingAnchor.constraint(equalTo: root.leadingAnchor, constant: 8)
        trailing = panel.trailingAnchor.constraint(equalTo: root.trailingAnchor, constant: -4)
        NSLayoutConstraint.activate([
            panel.topAnchor.constraint(equalTo: root.safeAreaLayoutGuide.topAnchor),
            panel.bottomAnchor.constraint(equalTo: root.bottomAnchor, constant: -8),
            leading,
            trailing,
            header.topAnchor.constraint(equalTo: inner.topAnchor),
            header.leadingAnchor.constraint(equalTo: inner.leadingAnchor),
            header.trailingAnchor.constraint(equalTo: inner.trailingAnchor),
            header.bottomAnchor.constraint(equalTo: holder.topAnchor),
            inner.topAnchor.constraint(equalTo: panel.topAnchor),
            inner.bottomAnchor.constraint(equalTo: panel.bottomAnchor),
            inner.leadingAnchor.constraint(equalTo: panel.leadingAnchor),
            inner.trailingAnchor.constraint(equalTo: panel.trailingAnchor),
            control.topAnchor.constraint(equalTo: inner.topAnchor, constant: 8),
            control.leadingAnchor.constraint(equalTo: inner.leadingAnchor, constant: 10),
            control.trailingAnchor.constraint(equalTo: inner.trailingAnchor, constant: -10),
            holder.topAnchor.constraint(equalTo: control.bottomAnchor, constant: 6),
            holder.bottomAnchor.constraint(equalTo: inner.bottomAnchor),
            holder.leadingAnchor.constraint(equalTo: inner.leadingAnchor),
            holder.trailingAnchor.constraint(equalTo: inner.trailingAnchor),
        ])
        view = root
        applyTheme()
        panes.forEach { addChild($0); _ = $0.view }
        select(selected)
    }

    func applyTheme() {
        view.layer?.backgroundColor = TerminalTheme.current.backgroundColor.cgColor
    }

    private static func panel(containing content: NSView) -> NSView {
        if #available(macOS 26, *) {
            let glass = NSGlassEffectView()
            glass.cornerRadius = 16
            let holder = NSView()
            holder.addSubview(content)
            glass.contentView = holder
            return glass
        }
        let effect = NSVisualEffectView()
        effect.material = .sidebar
        effect.blendingMode = .withinWindow
        effect.wantsLayer = true
        effect.layer?.cornerRadius = 12
        effect.layer?.masksToBounds = true
        effect.addSubview(content)
        return effect
    }

    func select(_ tab: PanelTab) {
        selected = tab
        UserDefaults.standard.set(tab.rawValue, forKey: "panelTab")
        control.selectedSegment = tab.rawValue
        if holder.subviews.isEmpty {
            for pane in panes.map(\.view) {
                pane.translatesAutoresizingMaskIntoConstraints = false
                holder.addSubview(pane)
                NSLayoutConstraint.activate([
                    pane.topAnchor.constraint(equalTo: holder.topAnchor),
                    pane.bottomAnchor.constraint(equalTo: holder.bottomAnchor),
                    pane.leadingAnchor.constraint(equalTo: holder.leadingAnchor),
                    pane.trailingAnchor.constraint(equalTo: holder.trailingAnchor),
                ])
            }
        }
        for (index, pane) in panes.enumerated() {
            pane.view.isHidden = index != tab.rawValue
        }
        (panes[tab.rawValue] as? FilesViewController)?.reloadIfNeeded()
        (panes[tab.rawValue] as? CommitHistoryViewController)?.reloadIfNeeded()
    }

    private func dragged(to point: NSPoint) {
        guard let width = view.window?.contentView?.bounds.width else { return }
        let target: PanelSide = point.x > width / 2 ? .right : .left
        if target != AppState.shared.panelSide { AppController.shared.setPanelSide(target) }
    }

    private func placementMenu() -> NSMenu {
        let menu = NSMenu()
        let other: PanelSide = AppState.shared.panelSide == .left ? .right : .left
        let move = NSMenuItem(title: other == .right ? "Move to Right Side" : "Move to Left Side", action: #selector(AppController.movePanel(_:)), keyEquivalent: "")
        let hide = NSMenuItem(title: "Hide", action: #selector(AppController.togglePanel(_:)), keyEquivalent: "")
        [move, hide].forEach {
            $0.target = AppController.shared
            menu.addItem($0)
        }
        return menu
    }

    @objc private func changed() {
        PanelTab(rawValue: control.selectedSegment).map(select)
    }
}

class ResponsiveSegmentedControl: NSSegmentedControl {
    var labels: [String] = [] { didSet { applyLabels() } }
    var minimumWidthPerLabel: CGFloat = 92

    override func setFrameSize(_ newSize: NSSize) {
        super.setFrameSize(newSize)
        applyLabels()
    }

    private func applyLabels() {
        let shows = frame.width >= CGFloat(labels.count) * minimumWidthPerLabel
        for (index, label) in labels.enumerated() where self.label(forSegment: index) != (shows ? label : "") {
            setLabel(shows ? label : "", forSegment: index)
        }
    }
}
