import AppKit

final class FileTabView: NSView {
    var onSelect: (() -> Void)?
    var onClose: (() -> Void)?
    private let close = NSButton()
    private var isHovered = false { didSet { updateClose() } }
    private let isActive: Bool

    convenience init(path: String, isActive: Bool) {
        let name = (path as NSString).lastPathComponent
        let (image, color) = FileIcon.icon(for: name, isDirectory: false)
        self.init(title: name, image: image, color: color, toolTip: path, isActive: isActive)
    }

    init(title name: String, image: NSImage?, color: NSColor?, toolTip: String, isActive: Bool) {
        self.isActive = isActive
        super.init(frame: .zero)
        wantsLayer = true
        layer?.cornerRadius = 6
        layer?.backgroundColor = isActive ? NSColor.quaternaryLabelColor.cgColor : nil
        let icon = NSImageView(image: image ?? NSImage())
        icon.contentTintColor = color
        icon.symbolConfiguration = .init(pointSize: 11, weight: .regular)
        let label = NSTextField(labelWithString: name)
        label.font = .systemFont(ofSize: NSFont.smallSystemFontSize, weight: isActive ? .semibold : .regular)
        label.textColor = isActive ? .labelColor : .secondaryLabelColor
        label.lineBreakMode = .byTruncatingMiddle
        close.image = NSImage(systemSymbolName: "xmark", accessibilityDescription: "Close")
        close.symbolConfiguration = .init(pointSize: 9, weight: .semibold)
        close.isBordered = false
        close.contentTintColor = .secondaryLabelColor
        close.target = self
        close.action = #selector(closeClicked)
        close.toolTip = "Close (\(MainMenu.shortcut(for: #selector(AppController.closeSession(_:)))))"
        let stack = NSStackView(views: [icon, label, close])
        stack.spacing = 5
        stack.edgeInsets = NSEdgeInsets(top: 0, left: 10, bottom: 0, right: 6)
        stack.translatesAutoresizingMaskIntoConstraints = false
        addSubview(stack)
        NSLayoutConstraint.activate([
            stack.topAnchor.constraint(equalTo: topAnchor),
            stack.bottomAnchor.constraint(equalTo: bottomAnchor),
            stack.leadingAnchor.constraint(equalTo: leadingAnchor),
            stack.trailingAnchor.constraint(equalTo: trailingAnchor),
            label.widthAnchor.constraint(lessThanOrEqualToConstant: 200),
            heightAnchor.constraint(equalToConstant: 26),
        ])
        self.toolTip = toolTip
        updateClose()
        addTrackingArea(NSTrackingArea(rect: .zero, options: [.mouseEnteredAndExited, .activeInActiveApp, .inVisibleRect], owner: self))
    }

    required init?(coder: NSCoder) { fatalError() }

    private func updateClose() {
        close.alphaValue = isActive || isHovered ? 1 : 0
    }

    override func mouseEntered(with event: NSEvent) { isHovered = true }
    override func mouseExited(with event: NSEvent) { isHovered = false }

    override func mouseDown(with event: NSEvent) {
        if event.buttonNumber == 2 { onClose?() } else { onSelect?() }
    }

    override func otherMouseDown(with event: NSEvent) {
        onClose?()
    }

    @objc private func closeClicked() {
        onClose?()
    }
}

final class FileTabBar: NSView {
    var onSelect: ((String) -> Void)?
    var onClose: ((String) -> Void)?
    var onHome: (() -> Void)?
    private let stack = NSStackView()

    override init(frame: NSRect) {
        super.init(frame: frame)
        let home = NSButton(image: NSImage(systemSymbolName: "house", accessibilityDescription: "Project overview")!, target: self, action: #selector(homeClicked))
        home.isBordered = false
        home.contentTintColor = .secondaryLabelColor
        home.toolTip = "Project overview"
        stack.spacing = 2
        let scroll = NSScrollView()
        scroll.drawsBackground = false
        scroll.hasHorizontalScroller = false
        scroll.verticalScrollElasticity = .none
        let document = FlippedView()
        document.addSubview(stack)
        scroll.documentView = document
        let separator = NSBox()
        separator.boxType = .separator
        for view in [home, scroll, separator] as [NSView] {
            view.translatesAutoresizingMaskIntoConstraints = false
            addSubview(view)
        }
        stack.translatesAutoresizingMaskIntoConstraints = false
        document.translatesAutoresizingMaskIntoConstraints = false
        NSLayoutConstraint.activate([
            heightAnchor.constraint(equalToConstant: 34),
            home.leadingAnchor.constraint(equalTo: leadingAnchor, constant: 10),
            home.centerYAnchor.constraint(equalTo: centerYAnchor),
            scroll.leadingAnchor.constraint(equalTo: home.trailingAnchor, constant: 8),
            scroll.trailingAnchor.constraint(equalTo: trailingAnchor, constant: -8),
            scroll.topAnchor.constraint(equalTo: topAnchor),
            scroll.bottomAnchor.constraint(equalTo: separator.topAnchor),
            separator.leadingAnchor.constraint(equalTo: leadingAnchor),
            separator.trailingAnchor.constraint(equalTo: trailingAnchor),
            separator.bottomAnchor.constraint(equalTo: bottomAnchor),
            document.topAnchor.constraint(equalTo: scroll.contentView.topAnchor),
            document.bottomAnchor.constraint(equalTo: scroll.contentView.bottomAnchor),
            document.leadingAnchor.constraint(equalTo: scroll.contentView.leadingAnchor),
            stack.leadingAnchor.constraint(equalTo: document.leadingAnchor),
            stack.trailingAnchor.constraint(equalTo: document.trailingAnchor),
            stack.centerYAnchor.constraint(equalTo: document.centerYAnchor),
        ])
    }

    required init?(coder: NSCoder) { fatalError() }

    func show(_ paths: [String], active: String?) {
        stack.arrangedSubviews.forEach { $0.removeFromSuperview() }
        for path in paths {
            let tab = FileTabView(path: path, isActive: path == active)
            tab.onSelect = { [weak self] in self?.onSelect?(path) }
            tab.onClose = { [weak self] in self?.onClose?(path) }
            stack.addArrangedSubview(tab)
        }
        layoutSubtreeIfNeeded()
        if let active = stack.arrangedSubviews[safe: paths.firstIndex(of: active ?? "") ?? -1] {
            active.scrollToVisible(active.bounds)
        }
    }

    @objc private func homeClicked() {
        onHome?()
    }
}
