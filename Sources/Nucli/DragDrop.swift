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
