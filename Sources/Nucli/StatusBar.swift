import AppKit

final class Chip: NSView {
    private let icon = NSImageView()
    private let label = NSTextField(labelWithString: "")
    var onClick: ((Chip) -> Void)? {
        didSet { toolTip = onClick == nil ? nil : toolTip }
    }
    private var isHovered = false {
        didSet { needsDisplay = true }
    }

    init(symbol: String) {
        super.init(frame: .zero)
        wantsLayer = true
        layer?.cornerRadius = 5
        icon.image = NSImage(systemSymbolName: symbol, accessibilityDescription: nil)
        icon.symbolConfiguration = .init(pointSize: 10, weight: .medium)
        icon.contentTintColor = .secondaryLabelColor
        label.font = .monospacedSystemFont(ofSize: 11, weight: .semibold)
        label.textColor = .labelColor
        label.lineBreakMode = .byTruncatingHead
        let stack = NSStackView(views: [icon, label])
        stack.spacing = 4
        stack.edgeInsets = NSEdgeInsets(top: 3, left: 6, bottom: 3, right: 7)
        stack.translatesAutoresizingMaskIntoConstraints = false
        addSubview(stack)
        NSLayoutConstraint.activate([
            stack.topAnchor.constraint(equalTo: topAnchor),
            stack.bottomAnchor.constraint(equalTo: bottomAnchor),
            stack.leadingAnchor.constraint(equalTo: leadingAnchor),
            stack.trailingAnchor.constraint(equalTo: trailingAnchor),
        ])
    }

    required init?(coder: NSCoder) { fatalError() }

    var text: String? {
        didSet {
            label.stringValue = text ?? ""
            isHidden = text == nil
        }
    }

    var attributedText: NSAttributedString? {
        didSet {
            label.attributedStringValue = attributedText ?? NSAttributedString()
            isHidden = attributedText == nil
        }
    }

    override func updateTrackingAreas() {
        trackingAreas.forEach(removeTrackingArea)
        addTrackingArea(NSTrackingArea(rect: bounds, options: [.mouseEnteredAndExited, .activeInActiveApp], owner: self))
    }

    override func mouseEntered(with event: NSEvent) { isHovered = onClick != nil }
    override func mouseExited(with event: NSEvent) { isHovered = false }

    override func mouseDown(with event: NSEvent) {
        guard let onClick else { return super.mouseDown(with: event) }
        onClick(self)
    }

    override func resetCursorRects() {
        if onClick != nil { addCursorRect(bounds, cursor: .pointingHand) }
    }

    override func isAccessibilityElement() -> Bool { onClick != nil }
    override func accessibilityRole() -> NSAccessibility.Role? { .button }
    override func accessibilityLabel() -> String? { toolTip ?? label.stringValue }

    override func accessibilityPerformPress() -> Bool {
        onClick?(self)
        return onClick != nil
    }

    func popUp(_ menu: NSMenu) {
        menu.popUp(positioning: nil, at: NSPoint(x: 0, y: bounds.height + 4), in: self)
    }

    override func updateLayer() {
        layer?.backgroundColor = NSColor.labelColor.withAlphaComponent(isHovered ? 0.16 : 0.08).cgColor
        layer?.borderColor = NSColor.labelColor.withAlphaComponent(0.12).cgColor
        layer?.borderWidth = 1
    }

    override var wantsUpdateLayer: Bool { true }
}

final class StatusBar: NSView {
    let directory = Chip(symbol: "folder")
    let branch = Chip(symbol: "arrow.triangle.branch")
    let process = Chip(symbol: "play.fill")
    let diff = Chip(symbol: "plusminus")

    init() {
        super.init(frame: .zero)
        let separator = NSBox()
        separator.boxType = .separator
        let stack = NSStackView(views: [directory, branch, diff, process, NSView()])
        stack.spacing = 6
        stack.translatesAutoresizingMaskIntoConstraints = false
        separator.translatesAutoresizingMaskIntoConstraints = false
        addSubview(separator)
        addSubview(stack)
        NSLayoutConstraint.activate([
            heightAnchor.constraint(equalToConstant: 34),
            separator.topAnchor.constraint(equalTo: topAnchor),
            separator.leadingAnchor.constraint(equalTo: leadingAnchor),
            separator.trailingAnchor.constraint(equalTo: trailingAnchor),
            stack.leadingAnchor.constraint(equalTo: leadingAnchor),
            stack.trailingAnchor.constraint(equalTo: trailingAnchor),
            stack.centerYAnchor.constraint(equalTo: centerYAnchor, constant: 1),
        ])
        directory.text = nil
        branch.text = nil
        process.text = nil
        diff.text = nil
        directory.toolTip = "Reveal or copy the folder"
        branch.toolTip = "Switch branch"
        diff.toolTip = "Show Changes"
    }

    func setDiff(added: Int, removed: Int, files: Int) {
        guard files > 0 else {
            diff.attributedText = nil
            return
        }
        let font = NSFont.monospacedSystemFont(ofSize: 11, weight: .semibold)
        let text = NSMutableAttributedString(string: "\(files) ", attributes: [.font: font, .foregroundColor: NSColor.labelColor])
        text.append(NSAttributedString(string: "+\(added)", attributes: [.font: font, .foregroundColor: NSColor.systemGreen]))
        text.append(NSAttributedString(string: " −\(removed)", attributes: [.font: font, .foregroundColor: NSColor.systemRed]))
        diff.attributedText = text
    }

    required init?(coder: NSCoder) { fatalError() }
}
