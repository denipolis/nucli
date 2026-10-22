import AppKit

final class StatusPill: NSView {
    var onClick: (() -> Void)?
    private let icon = NSImageView(image: NSImage(systemSymbolName: "magnifyingglass", accessibilityDescription: nil)!)
    private let name = NSTextField(labelWithString: "")
    private let path = NSTextField(labelWithString: "")

    override init(frame: NSRect) {
        super.init(frame: frame)
        wantsLayer = true
        layer?.cornerRadius = 7
        icon.contentTintColor = .secondaryLabelColor
        icon.symbolConfiguration = .init(pointSize: NSFont.smallSystemFontSize, weight: .medium)
        name.font = .systemFont(ofSize: NSFont.systemFontSize, weight: .medium)
        path.font = .systemFont(ofSize: NSFont.systemFontSize)
        path.textColor = .secondaryLabelColor
        name.lineBreakMode = .byTruncatingTail
        path.lineBreakMode = .byTruncatingHead
        name.setContentCompressionResistancePriority(.init(260), for: .horizontal)
        path.setContentCompressionResistancePriority(.init(250), for: .horizontal)
        let stack = NSStackView(views: [icon, name, path])
        stack.spacing = 6
        stack.translatesAutoresizingMaskIntoConstraints = false
        addSubview(stack)
        let preferred = widthAnchor.constraint(equalToConstant: 440)
        preferred.priority = .defaultLow
        NSLayoutConstraint.activate([
            preferred,
            heightAnchor.constraint(equalToConstant: 26),
            widthAnchor.constraint(greaterThanOrEqualToConstant: 200),
            widthAnchor.constraint(lessThanOrEqualToConstant: 560),
            stack.centerXAnchor.constraint(equalTo: centerXAnchor),
            stack.leadingAnchor.constraint(greaterThanOrEqualTo: leadingAnchor, constant: 12),
            stack.centerYAnchor.constraint(equalTo: centerYAnchor),
        ])
        toolTip = "Command Palette (\(MainMenu.shortcut(for: #selector(AppController.commandPalette(_:)))))"
        setAccessibilityRole(.button)
    }

    required init?(coder: NSCoder) { fatalError() }

    func show(_ project: Project?) {
        name.stringValue = project?.name ?? "Search"
        path.stringValue = project?.isScratch == false ? project!.displayPath : ""
        path.isHidden = path.stringValue.isEmpty
        setAccessibilityLabel([name.stringValue, path.stringValue].joined(separator: ", "))
    }

    override var wantsUpdateLayer: Bool { true }

    override func updateLayer() {
        layer?.backgroundColor = NSColor.labelColor.withAlphaComponent(0.06).cgColor
        layer?.borderColor = NSColor.separatorColor.cgColor
        layer?.borderWidth = 1
    }

    override func mouseDown(with event: NSEvent) {}

    override func mouseUp(with event: NSEvent) {
        if bounds.contains(convert(event.locationInWindow, from: nil)) { onClick?() }
    }

    override func accessibilityPerformPress() -> Bool {
        onClick?()
        return true
    }
}
