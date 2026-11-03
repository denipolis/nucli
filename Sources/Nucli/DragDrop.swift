import AppKit
import SwiftTerm

extension NSPasteboard.PasteboardType {
    static let nucliItem = NSPasteboard.PasteboardType("uk.polishch.nucli.sidebar-item")
}

final class DropTerminalView: TerminalView {
    override init(frame: CGRect, font: NSFont?) {
        super.init(frame: frame, font: font)
        registerForDraggedTypes([.fileURL, .string])
    }

    required init?(coder: NSCoder) { fatalError() }

    var themeSelection = (background: NSColor.selectedTextBackgroundColor, foreground: NSColor.selectedTextColor) {
        didSet { updateFindHighlight() }
    }
    private var isFinding = false
    private var lastMatch: NSRange?

    private var isFindBarVisible: Bool {
        subviews.contains { $0 is NSVisualEffectView && !$0.isHidden }
    }

    nonisolated override func selectionChanged(source: Terminal) {
        super.selectionChanged(source: source)
        DispatchQueue.main.async { [weak self] in
            MainActor.assumeIsolated { self?.updateFindHighlight() }
        }
    }

    private func updateFindHighlight() {
        let finding = isFindBarVisible && selectionActive
        selectedTextForegroundColor = finding ? .black : themeSelection.foreground
        guard finding else {
            isFinding = false
            lastMatch = nil
            selectedTextBackgroundColor = themeSelection.background
            return
        }
        let match = selectedRange()
        guard !isFinding || match != lastMatch else { return }
        isFinding = true
        lastMatch = match
        flashMatch()
    }

    private func flashMatch() {
        for (step, color) in [NSColor.systemOrange, .systemYellow, .systemOrange, .systemYellow].enumerated() {
            DispatchQueue.main.asyncAfter(deadline: .now() + Double(step) * 0.12) { [weak self] in
                guard let self, self.isFinding else { return }
                self.selectedTextBackgroundColor = color
            }
        }
    }

    override func draggingEntered(_ sender: NSDraggingInfo) -> NSDragOperation {
        .copy
    }

    override func performDragOperation(_ sender: NSDraggingInfo) -> Bool {
        let pasteboard = sender.draggingPasteboard
        let text: String
        if let urls = pasteboard.readObjects(forClasses: [NSURL.self], options: [.urlReadingFileURLsOnly: true]) as? [URL], !urls.isEmpty {
            text = urls.map { $0.path.shellQuoted }.joined(separator: " ") + " "
        } else if let string = pasteboard.string(forType: .string) {
            text = string
        } else {
            return false
        }
        send(txt: text)
        window?.makeFirstResponder(self)
        return true
    }
}
