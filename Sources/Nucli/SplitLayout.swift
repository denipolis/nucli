import AppKit

enum DropEdge {
    case left, right, top, bottom, center
}

final class PaneView: NSView {
    let session: Session
    var onDrop: ((Session, PaneView, DropEdge) -> Void)?
    private let highlight = NSView()
    private var dropEdge: DropEdge?

    var isDimmed = false {
        didSet { session.container.alphaValue = isDimmed ? 0.55 : 1 }
    }

    init(session: Session) {
        self.session = session
        super.init(frame: .zero)
        let container = session.container
        container.removeFromSuperview()
        container.translatesAutoresizingMaskIntoConstraints = false
        addSubview(container)
        NSLayoutConstraint.activate([
            container.topAnchor.constraint(equalTo: topAnchor, constant: 10),
            container.bottomAnchor.constraint(equalTo: bottomAnchor, constant: -10),
            container.leadingAnchor.constraint(equalTo: leadingAnchor, constant: 10),
            container.trailingAnchor.constraint(equalTo: trailingAnchor, constant: -10),
        ])
        highlight.wantsLayer = true
        highlight.layer?.backgroundColor = NSColor.controlAccentColor.withAlphaComponent(0.18).cgColor
        highlight.layer?.borderColor = NSColor.controlAccentColor.cgColor
        highlight.layer?.borderWidth = 2
        highlight.layer?.cornerRadius = 8
        highlight.isHidden = true
        addSubview(highlight)
        registerForDraggedTypes([.nucliItem])
    }

    required init?(coder: NSCoder) { fatalError() }

    private func draggedSession(_ info: NSDraggingInfo) -> Session? {
        info.draggingPasteboard.string(forType: .nucliItem).flatMap { AppState.shared.item(for: $0) as? Session }
    }

    private func edge(at point: NSPoint) -> DropEdge {
        let x = point.x / bounds.width
        let y = point.y / bounds.height
        let distances: [(DropEdge, CGFloat)] = [(.left, x), (.right, 1 - x), (.bottom, y), (.top, 1 - y)]
        let nearest = distances.min { $0.1 < $1.1 }!
        return nearest.1 < 0.25 ? nearest.0 : .center
    }

    private func frame(for edge: DropEdge) -> NSRect {
        let inset = bounds.insetBy(dx: 6, dy: 6)
        var (left, right) = inset.divided(atDistance: inset.width / 2, from: .minXEdge)
        switch edge {
        case .left: return left
        case .right: return right
        case .bottom: (left, right) = inset.divided(atDistance: inset.height / 2, from: .minYEdge); return left
        case .top: (left, right) = inset.divided(atDistance: inset.height / 2, from: .maxYEdge); return left
        case .center: return inset
        }
    }

    override func draggingEntered(_ sender: NSDraggingInfo) -> NSDragOperation {
        draggingUpdated(sender)
    }

    override func draggingUpdated(_ sender: NSDraggingInfo) -> NSDragOperation {
        guard let session = draggedSession(sender), session !== self.session, session.project.windowId == self.session.project.windowId else {
            draggingExited(sender)
            return []
        }
        let edge = edge(at: convert(sender.draggingLocation, from: nil))
        if edge != dropEdge {
            dropEdge = edge
            highlight.frame = frame(for: edge)
            highlight.isHidden = false
        }
        return .move
    }

    override func draggingExited(_ sender: NSDraggingInfo?) {
        dropEdge = nil
        highlight.isHidden = true
    }

    override func performDragOperation(_ sender: NSDraggingInfo) -> Bool {
        let edge = dropEdge
        draggingExited(sender)
        guard let session = draggedSession(sender), let edge else { return false }
        onDrop?(session, self, edge)
        return true
    }
}

final class PaneSplitView: NSSplitView {
    override var dividerThickness: CGFloat { 1 }
    override var dividerColor: NSColor { .separatorColor }
}

@MainActor
final class SplitLayout {
    let host = NSView()
    private(set) var root: NSView?
    private(set) weak var focused: PaneView?
    private var stashed: [PaneSplitView] = []
    var onDrop: ((Session, PaneView, DropEdge) -> Void)?

    private static func collect(_ view: NSView?) -> [PaneView] {
        switch view {
        case let pane as PaneView: [pane]
        case let split as PaneSplitView: split.subviews.flatMap(collect)
        default: []
        }
    }

    var panes: [PaneView] { Self.collect(root) }
    var sessions: [Session] { panes.map(\.session) }
    var isSplit: Bool { root is PaneSplitView }

    private var splits: [PaneSplitView] { ((root as? PaneSplitView).map { [$0] } ?? []) + stashed }
    var groups: [[Session]] { splits.map { Self.collect($0).map(\.session) } }
    var allSessions: [Session] { sessions + stashed.flatMap { Self.collect($0).map(\.session) } }

    func group(of session: Session) -> [Session]? {
        groups.first { $0.contains { $0 === session } }
    }

    func pane(for session: Session) -> PaneView? {
        panes.first { $0.session === session }
    }

    func focus(_ session: Session) {
        focused = pane(for: session)
        updateDimming()
    }

    func show(_ session: Session) {
        if pane(for: session) != nil {
            focus(session)
            return
        }
        if let split = root as? PaneSplitView {
            split.removeFromSuperview()
            stashed.append(split)
            root = nil
        }
        if let index = stashed.firstIndex(where: { Self.collect($0).contains { $0.session === session } }) {
            setRoot(stashed.remove(at: index))
            focused = pane(for: session)
        } else {
            let pane = makePane(session)
            setRoot(pane)
            focused = pane
        }
        updateDimming()
    }

    func unsplit(_ session: Session) {
        if let index = stashed.firstIndex(where: { Self.collect($0).contains { $0.session === session } }) {
            stashed.remove(at: index)
        } else if isSplit, pane(for: session) != nil, let kept = focused?.session {
            let pane = makePane(kept)
            setRoot(pane)
            focused = pane
        }
        updateDimming()
    }

    func split(_ target: Session, with session: Session, edge: DropEdge) {
        guard session !== target else { return }
        remove(session)
        guard let targetPane = pane(for: target) ?? (root == nil ? nil : focused) else {
            show(session)
            return
        }
        guard edge != .center else {
            remove(session)
            let pane = makePane(session)
            replace(targetPane, with: pane)
            focused = pane
            updateDimming()
            return
        }
        let pane = makePane(session)
        let vertical = edge == .left || edge == .right
        let before = edge == .left || edge == .top
        if let parent = targetPane.superview as? PaneSplitView, parent.isVertical == vertical {
            let index = parent.subviews.firstIndex(of: targetPane)! + (before ? 0 : 1)
            insert(pane, into: parent, at: index)
        } else {
            let split = PaneSplitView()
            split.isVertical = vertical
            split.dividerStyle = .thin
            replace(targetPane, with: split)
            insert(targetPane, into: split, at: 0)
            insert(pane, into: split, at: before ? 0 : 1)
        }
        focused = pane
        updateDimming()
    }

    @discardableResult
    func remove(_ session: Session) -> Bool {
        guard let pane = Self.collect(root).first(where: { $0.session === session }) ?? stashed.flatMap(Self.collect).first(where: { $0.session === session }) else { return false }
        let wasFocused = focused === pane
        if pane === root {
            setRoot(nil)
        } else if let parent = pane.superview as? PaneSplitView {
            pane.removeFromSuperview()
            if parent.subviews.count == 1, let only = parent.subviews.first {
                only.removeFromSuperview()
                replace(parent, with: only)
            } else {
                equalize(parent)
            }
        }
        pane.session.container.alphaValue = 1
        pane.session.container.removeFromSuperview()
        if wasFocused { focused = panes.first }
        updateDimming()
        return true
    }

    func neighbor(of session: Session, toward edge: DropEdge) -> Session? {
        guard let pane = pane(for: session) else { return nil }
        let origin = pane.convert(pane.bounds, to: nil)
        let candidates = panes.filter { $0 !== pane }.map { ($0, $0.convert($0.bounds, to: nil)) }
        let matches = candidates.filter { _, frame in
            switch edge {
            case .left: frame.maxX <= origin.minX + 1 && frame.maxY > origin.minY && frame.minY < origin.maxY
            case .right: frame.minX >= origin.maxX - 1 && frame.maxY > origin.minY && frame.minY < origin.maxY
            case .top: frame.minY >= origin.maxY - 1 && frame.maxX > origin.minX && frame.minX < origin.maxX
            case .bottom: frame.maxY <= origin.minY + 1 && frame.maxX > origin.minX && frame.minX < origin.maxX
            case .center: false
            }
        }
        let center = NSPoint(x: origin.midX, y: origin.midY)
        return matches.min { hypot($0.1.midX - center.x, $0.1.midY - center.y) < hypot($1.1.midX - center.x, $1.1.midY - center.y) }?.0.session
    }

    private func makePane(_ session: Session) -> PaneView {
        let pane = PaneView(session: session)
        pane.onDrop = { [weak self] in self?.onDrop?($0, $1, $2) }
        return pane
    }

    private func insert(_ view: NSView, into split: PaneSplitView, at index: Int) {
        view.translatesAutoresizingMaskIntoConstraints = true
        view.frame = NSRect(origin: .zero, size: split.bounds.size)
        if index >= split.subviews.count {
            split.addSubview(view)
        } else {
            split.addSubview(view, positioned: .below, relativeTo: split.subviews[index])
        }
        equalize(split)
    }

    private func equalize(_ split: PaneSplitView) {
        split.adjustSubviews()
        resize(split)
        DispatchQueue.main.async { [weak split] in split.map(self.resize) }
    }

    private func resize(_ split: PaneSplitView) {
        split.superview?.layoutSubtreeIfNeeded()
        let count = split.subviews.count
        guard count > 1 else { return }
        let total = (split.isVertical ? split.bounds.width : split.bounds.height) - CGFloat(count - 1) * split.dividerThickness
        for index in 0..<(count - 1) {
            split.setPosition(total / CGFloat(count) * CGFloat(index + 1) + CGFloat(index) * split.dividerThickness, ofDividerAt: index)
        }
    }

    private func replace(_ old: NSView, with new: NSView) {
        if old === root {
            setRoot(new)
        } else if let index = stashed.firstIndex(where: { $0 === old }) {
            if let split = new as? PaneSplitView { stashed[index] = split } else { stashed.remove(at: index) }
        } else if let parent = old.superview as? PaneSplitView {
            let index = parent.subviews.firstIndex(of: old)!
            let frame = old.frame
            old.removeFromSuperview()
            new.translatesAutoresizingMaskIntoConstraints = true
            new.frame = frame
            if index >= parent.subviews.count {
                parent.addSubview(new)
            } else {
                parent.addSubview(new, positioned: .below, relativeTo: parent.subviews[index])
            }
            parent.adjustSubviews()
        }
    }

    private func setRoot(_ view: NSView?) {
        root?.removeFromSuperview()
        root = view
        guard let view else { return }
        view.removeFromSuperview()
        view.translatesAutoresizingMaskIntoConstraints = false
        host.addSubview(view)
        NSLayoutConstraint.activate([
            view.topAnchor.constraint(equalTo: host.topAnchor),
            view.bottomAnchor.constraint(equalTo: host.bottomAnchor),
            view.leadingAnchor.constraint(equalTo: host.leadingAnchor),
            view.trailingAnchor.constraint(equalTo: host.trailingAnchor),
        ])
        host.layoutSubtreeIfNeeded()
    }

    private func updateDimming() {
        let split = isSplit
        panes.forEach { $0.isDimmed = split && $0 !== focused }
        NotificationCenter.default.post(name: .layoutChanged, object: self)
    }
}
