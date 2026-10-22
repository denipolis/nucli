import AppKit
import UniformTypeIdentifiers

final class CenteringClipView: NSClipView {
    override func constrainBoundsRect(_ proposedBounds: NSRect) -> NSRect {
        var rect = super.constrainBoundsRect(proposedBounds)
        guard let document = documentView else { return rect }
        if rect.width > document.frame.width { rect.origin.x = (document.frame.width - rect.width) / 2 }
        if rect.height > document.frame.height { rect.origin.y = (document.frame.height - rect.height) / 2 }
        return rect
    }
}

final class CheckerImageView: NSView {
    let image: NSImage

    init(image: NSImage, size: NSSize) {
        self.image = image
        super.init(frame: NSRect(origin: .zero, size: size))
    }

    required init?(coder: NSCoder) { fatalError() }

    override func draw(_ dirtyRect: NSRect) {
        let dirtyRect = dirtyRect.intersection(bounds)
        guard !dirtyRect.isEmpty else { return }
        guard image.representations.contains(where: \.hasAlpha) else { return image.draw(in: bounds) }
        let tile: CGFloat = 8 / max(enclosingScrollView?.magnification ?? 1, 0.01)
        NSColor.white.setFill()
        dirtyRect.fill()
        NSColor(white: 0.85, alpha: 1).setFill()
        let firstColumn = Int(dirtyRect.minX / tile), lastColumn = Int(dirtyRect.maxX / tile)
        let firstRow = Int(dirtyRect.minY / tile), lastRow = Int(dirtyRect.maxY / tile)
        for row in firstRow...lastRow {
            for column in firstColumn...lastColumn where (row + column) % 2 == 0 {
                NSRect(x: CGFloat(column) * tile, y: CGFloat(row) * tile, width: tile, height: tile).fill()
            }
        }
        image.draw(in: bounds)
    }
}

final class ImageScrollView: NSScrollView {
    var onCancel: (() -> Void)?

    override var acceptsFirstResponder: Bool { true }

    override func cancelOperation(_ sender: Any?) {
        onCancel?()
    }
}

final class ImageViewer: NSView {
    static func canShow(_ url: URL) -> Bool {
        guard FileManager.default.fileExists(atPath: url.path), let type = UTType(filenameExtension: url.pathExtension) else { return false }
        return type.conforms(to: .image) || type.conforms(to: .pdf)
    }

    let scroll = ImageScrollView()
    private var image: NSImage?
    private var pixelSize = NSSize.zero
    private var fits = true

    init(url: URL) {
        super.init(frame: .zero)
        let info = NSTextField(labelWithString: "")
        info.font = .monospacedDigitSystemFont(ofSize: NSFont.smallSystemFontSize, weight: .regular)
        info.textColor = .secondaryLabelColor
        let fit = button("arrow.up.left.and.arrow.down.right", "Zoom to Fit", #selector(zoomToFit))
        let actual = button("1.magnifyingglass", "Actual Size", #selector(actualSize))
        let zoomOut = button("minus.magnifyingglass", "Zoom Out", #selector(zoomOut))
        let zoomIn = button("plus.magnifyingglass", "Zoom In", #selector(zoomIn))
        let bar = NSStackView(views: [info, NSView(), zoomOut, zoomIn, actual, fit])
        bar.spacing = 6
        bar.edgeInsets = NSEdgeInsets(top: 0, left: 12, bottom: 6, right: 12)
        scroll.contentView = CenteringClipView()
        scroll.drawsBackground = false
        scroll.hasVerticalScroller = true
        scroll.hasHorizontalScroller = true
        scroll.allowsMagnification = true
        scroll.minMagnification = 0.02
        scroll.maxMagnification = 32
        let stack = NSStackView(views: [bar, scroll])
        stack.orientation = .vertical
        stack.spacing = 0
        stack.translatesAutoresizingMaskIntoConstraints = false
        addSubview(stack)
        NSLayoutConstraint.activate([
            stack.topAnchor.constraint(equalTo: topAnchor),
            stack.bottomAnchor.constraint(equalTo: bottomAnchor),
            stack.leadingAnchor.constraint(equalTo: leadingAnchor),
            stack.trailingAnchor.constraint(equalTo: trailingAnchor),
            bar.widthAnchor.constraint(equalTo: stack.widthAnchor),
        ])
        let bytes = (try? url.resourceValues(forKeys: [.fileSizeKey]).fileSize).map { ByteCountFormatter.string(fromByteCount: Int64($0), countStyle: .file) } ?? ""
        guard let image = NSImage(contentsOf: url) else {
            info.stringValue = "Can't display this image · \(bytes)"
            return
        }
        self.image = image
        let pixels = image.representations.map { NSSize(width: $0.pixelsWide, height: $0.pixelsHigh) }.max { $0.width < $1.width }
        pixelSize = pixels.flatMap { $0.width > 0 ? $0 : nil } ?? image.size
        info.stringValue = "\(Int(pixelSize.width)) × \(Int(pixelSize.height)) px · \(bytes)"
        scroll.documentView = CheckerImageView(image: image, size: pixelSize)
        NotificationCenter.default.addObserver(forName: NSScrollView.didEndLiveMagnifyNotification, object: scroll, queue: .main) { [weak self] _ in
            MainActor.assumeIsolated {
                self?.fits = false
                self?.scroll.documentView?.needsDisplay = true
            }
        }
    }

    required init?(coder: NSCoder) { fatalError() }

    override func layout() {
        super.layout()
        if fits { applyFit() }
    }

    private func button(_ symbol: String, _ tip: String, _ action: Selector) -> NSButton {
        let button = NSButton(image: NSImage(systemSymbolName: symbol, accessibilityDescription: tip)!, target: self, action: action)
        button.isBordered = false
        button.toolTip = tip
        button.contentTintColor = .secondaryLabelColor
        return button
    }

    private func applyFit() {
        guard image != nil, pixelSize.width > 0, pixelSize.height > 0 else { return }
        let available = scroll.contentSize
        guard available.width > 0, available.height > 0 else { return }
        let scale = min(available.width / pixelSize.width, available.height / pixelSize.height, 1 / backingScale)
        setMagnification(scale)
    }

    private var backingScale: CGFloat { window?.backingScaleFactor ?? 2 }

    private func setMagnification(_ value: CGFloat) {
        scroll.magnification = value
        scroll.documentView?.needsDisplay = true
    }

    @objc private func zoomToFit() {
        fits = true
        applyFit()
    }

    @objc private func actualSize() {
        fits = false
        setMagnification(1 / backingScale)
    }

    @objc private func zoomIn() {
        fits = false
        setMagnification(scroll.magnification * 1.5)
    }

    @objc private func zoomOut() {
        fits = false
        setMagnification(scroll.magnification / 1.5)
    }
}
