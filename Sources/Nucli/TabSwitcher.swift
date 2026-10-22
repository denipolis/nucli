import AppKit

enum SwitcherEntry {
    case session(Session)
    case files(Project)

    var id: String {
        switch self {
        case .session(let session): session.id
        case .files(let project): "files:" + project.path
        }
    }
}

@MainActor
final class TabSwitcher {
    private let panel = PalettePanel(contentRect: NSRect(x: 0, y: 0, width: 460, height: 100), styleMask: [.borderless], backing: .buffered, defer: true)
    private let stack = NSStackView()
    private var entries: [SwitcherEntry] = []
    private var index = 0
    private weak var controller: MainWindowController?
    private(set) var modifier: NSEvent.ModifierFlags = []

    var isVisible: Bool { panel.isVisible }

    init() {
        panel.isOpaque = false
        panel.backgroundColor = .clear
        panel.hasShadow = true
        let background = NSVisualEffectView()
        background.material = .hudWindow
        background.state = .active
        background.wantsLayer = true
        background.layer?.cornerRadius = 14
        background.layer?.masksToBounds = true
        stack.orientation = .vertical
        stack.alignment = .leading
        stack.spacing = 2
        stack.distribution = .fill
        stack.edgeInsets = NSEdgeInsets(top: 8, left: 8, bottom: 8, right: 8)
        stack.translatesAutoresizingMaskIntoConstraints = false
        background.addSubview(stack)
        NSLayoutConstraint.activate([
            stack.widthAnchor.constraint(equalToConstant: 460),
            stack.topAnchor.constraint(equalTo: background.topAnchor),
            stack.bottomAnchor.constraint(equalTo: background.bottomAnchor),
            stack.leadingAnchor.constraint(equalTo: background.leadingAnchor),
            stack.trailingAnchor.constraint(equalTo: background.trailingAnchor),
        ])
        panel.contentView = background
    }

    func step(_ delta: Int, in controller: MainWindowController, entries ordered: [SwitcherEntry], modifier: NSEvent.ModifierFlags) {
        guard ordered.count > 1 || isVisible else {
            ordered.first.map { open($0, in: controller) }
            return
        }
        if !isVisible {
            self.controller = controller
            self.modifier = modifier
            entries = ordered
            index = 0
            show(in: controller)
        }
        index = (index + delta + entries.count) % entries.count
        render()
    }

    func commit() {
        guard isVisible else { return }
        let target = entries[safe: index]
        close()
        if let target, let controller { open(target, in: controller) }
    }

    private func open(_ entry: SwitcherEntry, in controller: MainWindowController) {
        switch entry {
        case .session(let session): controller.select(session)
        case .files(let project): controller.showProject(project)
        }
    }

    func close() {
        controller?.window?.removeChildWindow(panel)
        panel.orderOut(nil)
    }

    private func show(in controller: MainWindowController) {
        guard let window = controller.window else { return }
        render()
        let size = panel.contentView?.fittingSize ?? panel.frame.size
        panel.setContentSize(NSSize(width: 460, height: size.height))
        panel.setFrameOrigin(NSPoint(x: window.frame.midX - 230, y: window.frame.midY - size.height / 2 + 60))
        window.addChildWindow(panel, ordered: .above)
        panel.orderFront(nil)
    }

    private func render() {
        stack.arrangedSubviews.forEach { $0.removeFromSuperview() }
        for (offset, entry) in entries.prefix(12).enumerated() {
            let row = row(for: entry, selected: offset == index)
            stack.addArrangedSubview(row)
            row.widthAnchor.constraint(equalTo: stack.widthAnchor, constant: -16).isActive = true
        }
    }

    private func row(for entry: SwitcherEntry, selected: Bool) -> NSView {
        let icon = NSImageView()
        let titleText: String, subtitleText: String, attention: Bool
        switch entry {
        case .session(let session):
            if let agent = session.runningAgent {
                icon.image = AgentBadge.image(for: agent)
            } else {
                icon.image = NSImage(systemSymbolName: session.kind == .agent ? "sparkles" : "terminal", accessibilityDescription: nil)
                icon.contentTintColor = selected ? .white : .secondaryLabelColor
            }
            titleText = session.title
            subtitleText = "\(session.project.name) · \(session.subtitle)"
            attention = session.needsAttention
        case .files(let project):
            icon.image = NSImage(systemSymbolName: "folder.fill", accessibilityDescription: "Files")
            icon.contentTintColor = selected ? .white : project.tint?.color ?? .controlAccentColor
            titleText = project.activeFile.map { ($0 as NSString).lastPathComponent } ?? project.name
            subtitleText = "\(project.name) · Files"
            attention = false
        }
        icon.widthAnchor.constraint(equalToConstant: 20).isActive = true
        let title = NSTextField(labelWithString: titleText)
        title.font = .systemFont(ofSize: 13, weight: .semibold)
        title.textColor = selected ? .white : .labelColor
        title.lineBreakMode = .byTruncatingTail
        let subtitle = NSTextField(labelWithString: subtitleText)
        subtitle.font = .systemFont(ofSize: NSFont.smallSystemFontSize)
        subtitle.textColor = selected ? .white.withAlphaComponent(0.8) : .secondaryLabelColor
        subtitle.lineBreakMode = .byTruncatingMiddle
        subtitle.setContentCompressionResistancePriority(.init(100), for: .horizontal)
        title.setContentCompressionResistancePriority(.init(200), for: .horizontal)
        let text = NSStackView(views: [title, subtitle])
        text.orientation = .vertical
        text.alignment = .leading
        text.spacing = 1
        let dot = NSView()
        dot.wantsLayer = true
        dot.layer?.cornerRadius = 4
        dot.layer?.backgroundColor = NSColor.systemGreen.cgColor
        dot.isHidden = !attention
        dot.widthAnchor.constraint(equalToConstant: 8).isActive = true
        dot.heightAnchor.constraint(equalToConstant: 8).isActive = true
        text.setContentHuggingPriority(.init(1), for: .horizontal)
        let row = NSStackView(views: [icon, text, dot])
        row.spacing = 10
        row.edgeInsets = NSEdgeInsets(top: 6, left: 10, bottom: 6, right: 12)
        row.wantsLayer = true
        row.layer?.cornerRadius = 8
        row.layer?.backgroundColor = selected ? NSColor.controlAccentColor.cgColor : nil
        row.heightAnchor.constraint(equalToConstant: 44).isActive = true
        return row
    }
}
