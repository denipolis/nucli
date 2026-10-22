import AppKit

final class FolderDropView: NSView {
    var onDrop: (([String]) -> Void)?

    override init(frame: NSRect) {
        super.init(frame: frame)
        registerForDraggedTypes([.fileURL])
    }

    required init?(coder: NSCoder) { fatalError() }

    private func folders(_ info: NSDraggingInfo) -> [String] {
        let urls = info.draggingPasteboard.readObjects(forClasses: [NSURL.self], options: [.urlReadingFileURLsOnly: true]) as? [URL] ?? []
        return urls.filter { (try? $0.resourceValues(forKeys: [.isDirectoryKey]).isDirectory) == true }.map(\.path)
    }

    override func draggingEntered(_ sender: NSDraggingInfo) -> NSDragOperation {
        folders(sender).isEmpty ? [] : .copy
    }

    override func performDragOperation(_ sender: NSDraggingInfo) -> Bool {
        let list = folders(sender)
        guard !list.isEmpty else { return false }
        onDrop?(list)
        return true
    }
}

final class ContentViewController: NSViewController, NSSplitViewDelegate {
    private let placeholder = NSTextField(labelWithString: "")
    private var current: NSView?
    private weak var focus: NSView?
    private var fileDiff: FileDiffViewController?
    private var fileViews: [String: FileDiffViewController] = [:]
    private let fileContainer = NSView()
    private let fileHolder = NSView()
    private let tabBar = FileTabBar()
    private weak var fileProject: Project?
    let layout = SplitLayout()
    let terminals = TerminalPanel()
    private let main = NSView()
    private let panelSplit = NSSplitView()
    private var restoredHeight: CGFloat?

    override func loadView() {
        let dropView = FolderDropView()
        dropView.onDrop = { [weak self] paths in
            guard let id = (self?.view.window?.windowController as? MainWindowController)?.id else { return }
            AppController.shared.openFolders(paths, windowId: id)
        }
        view = dropView
        view.wantsLayer = true
        panelSplit.isVertical = false
        panelSplit.dividerStyle = .thin
        panelSplit.delegate = self
        panelSplit.addArrangedSubview(main)
        panelSplit.addArrangedSubview(terminals)
        panelSplit.setHoldingPriority(.init(260), forSubviewAt: 0)
        panelSplit.setHoldingPriority(.init(270), forSubviewAt: 1)
        terminals.isHidden = true
        panelSplit.translatesAutoresizingMaskIntoConstraints = false
        view.addSubview(panelSplit)
        NSLayoutConstraint.activate([
            panelSplit.topAnchor.constraint(equalTo: view.topAnchor),
            panelSplit.bottomAnchor.constraint(equalTo: view.bottomAnchor),
            panelSplit.leadingAnchor.constraint(equalTo: view.leadingAnchor),
            panelSplit.trailingAnchor.constraint(equalTo: view.trailingAnchor),
        ])
        applyTheme()
        updatePlaceholder()
        NotificationCenter.default.addObserver(forName: .settingsChanged, object: nil, queue: .main) { [weak self] _ in MainActor.assumeIsolated { self?.updatePlaceholder() } }
        placeholder.alignment = .center
        placeholder.textColor = .tertiaryLabelColor
        placeholder.font = .systemFont(ofSize: 15)
        placeholder.translatesAutoresizingMaskIntoConstraints = false
        main.addSubview(placeholder)
        layout.host.isHidden = true
        pin(layout.host)
        fileContainer.wantsLayer = true
        for child in [tabBar, fileHolder] {
            child.translatesAutoresizingMaskIntoConstraints = false
            fileContainer.addSubview(child)
        }
        NSLayoutConstraint.activate([
            tabBar.topAnchor.constraint(equalTo: fileContainer.topAnchor),
            tabBar.leadingAnchor.constraint(equalTo: fileContainer.leadingAnchor),
            tabBar.trailingAnchor.constraint(equalTo: fileContainer.trailingAnchor),
            fileHolder.topAnchor.constraint(equalTo: tabBar.bottomAnchor),
            fileHolder.leadingAnchor.constraint(equalTo: fileContainer.leadingAnchor),
            fileHolder.trailingAnchor.constraint(equalTo: fileContainer.trailingAnchor),
            fileHolder.bottomAnchor.constraint(equalTo: fileContainer.bottomAnchor),
        ])
        tabBar.onSelect = { [weak self] path in self?.openTab(path) }
        tabBar.onClose = { [weak self] path in self?.closeFileTab(path) }
        tabBar.onHome = { [weak self] in
            guard let self, let project = fileProject, let owner = view.window?.windowController as? MainWindowController else { return }
            owner.showOverview(of: project)
        }
        NSLayoutConstraint.activate([
            placeholder.centerXAnchor.constraint(equalTo: main.centerXAnchor),
            placeholder.centerYAnchor.constraint(equalTo: main.centerYAnchor),
        ])
    }

    private func updatePlaceholder() {
        let lines = [(#selector(AppController.newAgentWithLastProfile(_:)), "New Agent"), (#selector(AppController.newAgent(_:)), "Choose Agent"), (#selector(AppController.toggleTerminalPanel(_:)), "Terminal"), (#selector(AppController.openProject(_:)), "Open Project"), (#selector(AppController.commandPalette(_:)), "Command Palette")]
        placeholder.stringValue = lines.map { "\(MainMenu.shortcut(for: $0.0))  \($0.1)" }.joined(separator: "\n") + "\n\nor drop a folder here"
    }

    func show(_ newView: NSView?) {
        focus = nil
        closeFileDiff()
        layout.host.isHidden = true
        guard newView !== current else { return }
        current?.removeFromSuperview()
        current = newView
        placeholder.isHidden = newView != nil
        guard let newView else { return }
        pin(newView, inset: 10)
    }

    func show(_ session: Session) {
        focus = session.terminal
        closeFileDiff()
        current?.removeFromSuperview()
        current = nil
        layout.show(session)
        layout.host.isHidden = false
        placeholder.isHidden = true
    }

    func showFileDiff(_ change: FileChange, in project: Project, line: Int? = nil) {
        project.recentFiles = Array(([change.path] + project.recentFiles.filter { $0 != change.path }).prefix(20))
        if !project.openFiles.contains(change.path) {
            let index = project.activeFile.flatMap { project.openFiles.firstIndex(of: $0) }.map { $0 + 1 } ?? project.openFiles.count
            project.openFiles.insert(change.path, at: index)
        }
        project.activeFile = change.path
        AppState.shared.save()
        AppController.shared.noteUsed(.files(project))
        let key = project.path + "\u{0}" + change.path
        var diff = fileViews[key]
        if diff == nil || diff?.change != change && diff?.isDirty != true {
            diff?.removeFromParent()
            diff = FileDiffViewController(change: change, project: project)
            fileViews[key] = diff
            addChild(diff!)
        }
        guard let diff else { return }
        fileDiff?.view.removeFromSuperview()
        fileDiff = diff
        fileProject = project
        diff.view.translatesAutoresizingMaskIntoConstraints = false
        fileHolder.addSubview(diff.view)
        NSLayoutConstraint.activate([
            diff.view.topAnchor.constraint(equalTo: fileHolder.topAnchor),
            diff.view.bottomAnchor.constraint(equalTo: fileHolder.bottomAnchor),
            diff.view.leadingAnchor.constraint(equalTo: fileHolder.leadingAnchor),
            diff.view.trailingAnchor.constraint(equalTo: fileHolder.trailingAnchor),
        ])
        if fileContainer.superview == nil { pin(fileContainer) }
        tabBar.show(project.openFiles, active: change.path)
        view.window?.makeFirstResponder(diff.focusView)
        if let line { diff.reveal(line: line) }
    }

    var isShowingFile: Bool { fileDiff != nil }
    var hasUnsavedFiles: Bool { fileViews.values.contains(where: \.isDirty) }
    var fileView: FileDiffViewController? { fileDiff }

    func closeFileDiff() {
        guard let fileDiff else { return }
        fileDiff.view.removeFromSuperview()
        fileContainer.removeFromSuperview()
        self.fileDiff = nil
        if let focus { view.window?.makeFirstResponder(focus) }
    }

    private func openTab(_ path: String) {
        guard let project = fileProject else { return }
        (view.window?.windowController as? MainWindowController)?.openFile(path, in: project)
    }

    func closeFileTab(_ path: String? = nil) {
        guard let project = fileProject, let path = path ?? project.activeFile, let index = project.openFiles.firstIndex(of: path) else {
            closeFileDiff()
            return
        }
        if fileViews[project.path + "\u{0}" + path]?.confirmClose() == false { return }
        project.openFiles.remove(at: index)
        fileViews.removeValue(forKey: project.path + "\u{0}" + path)?.removeFromParent()
        guard path == project.activeFile else {
            AppState.shared.save()
            tabBar.show(project.openFiles, active: project.activeFile)
            return
        }
        project.activeFile = project.openFiles[safe: min(index, project.openFiles.count - 1)]
        AppState.shared.save()
        if let next = project.activeFile {
            openTab(next)
        } else {
            closeFileDiff()
        }
    }

    func applyTheme() {
        fileContainer.layer?.backgroundColor = TerminalTheme.current.backgroundColor.cgColor
        view.layer?.backgroundColor = TerminalTheme.current.backgroundColor.cgColor
        view.window?.backgroundColor = TerminalTheme.current.backgroundColor
    }

    private func pin(_ child: NSView, inset: CGFloat = 0) {
        child.translatesAutoresizingMaskIntoConstraints = false
        main.addSubview(child)
        NSLayoutConstraint.activate([
            child.topAnchor.constraint(equalTo: main.safeAreaLayoutGuide.topAnchor, constant: inset),
            child.bottomAnchor.constraint(equalTo: main.bottomAnchor, constant: -inset),
            child.leadingAnchor.constraint(equalTo: main.leadingAnchor, constant: inset),
            child.trailingAnchor.constraint(equalTo: main.trailingAnchor, constant: -inset),
        ])
    }

    func showTerminals(of project: Project?) {
        let visible = project?.showsTerminals == true && project?.terminals.isEmpty == false
        terminals.show(visible ? project?.terminals ?? [] : [], active: visible ? project?.activeTerminal ?? project?.terminals.last : nil)
        guard terminals.isHidden == visible else { return }
        terminals.isHidden = !visible
        panelSplit.adjustSubviews()
        if visible {
            let height = CGFloat(UserDefaults.standard.double(forKey: "terminalPanelHeight"))
            panelSplit.setPosition(panelSplit.bounds.height - (height >= 120 ? height : panelSplit.bounds.height * 0.35), ofDividerAt: 0)
            restoredHeight = nil
            terminals.setMaximized(false)
        }
    }

    func toggleMaximizedTerminals() {
        guard !terminals.isHidden else { return }
        if let restoredHeight {
            panelSplit.setPosition(panelSplit.bounds.height - restoredHeight, ofDividerAt: 0)
            self.restoredHeight = nil
        } else {
            restoredHeight = terminals.frame.height
            panelSplit.setPosition(main.safeAreaInsets.top + 40, ofDividerAt: 0)
        }
        terminals.setMaximized(restoredHeight != nil)
    }

    func splitView(_ splitView: NSSplitView, constrainMinCoordinate proposedMinimumPosition: CGFloat, ofSubviewAt dividerIndex: Int) -> CGFloat {
        max(proposedMinimumPosition, main.safeAreaInsets.top + 40)
    }

    func splitView(_ splitView: NSSplitView, constrainMaxCoordinate proposedMaximumPosition: CGFloat, ofSubviewAt dividerIndex: Int) -> CGFloat {
        min(proposedMaximumPosition, splitView.bounds.height - 120)
    }

    func splitView(_ splitView: NSSplitView, shouldHideDividerAt dividerIndex: Int) -> Bool {
        terminals.isHidden
    }

    func splitViewDidResizeSubviews(_ notification: Notification) {
        guard !terminals.isHidden, restoredHeight == nil, terminals.frame.height >= 120, view.window != nil else { return }
        UserDefaults.standard.set(Double(terminals.frame.height), forKey: "terminalPanelHeight")
    }
}

extension NSAttributedString.Key {
    static let diffKind = NSAttributedString.Key("NucliDiffKind")
    static let gutter = NSAttributedString.Key("NucliGutter")
}

final class DiffLayoutManager: NSLayoutManager {
    override func drawBackground(forGlyphRange glyphsToShow: NSRange, at origin: NSPoint) {
        super.drawBackground(forGlyphRange: glyphsToShow, at: origin)
        guard let storage = textStorage else { return }
        enumerateLineFragments(forGlyphRange: glyphsToShow) { rect, _, _, glyphRange, _ in
            let index = self.characterIndexForGlyph(at: glyphRange.location)
            guard index < storage.length, let kind = storage.attribute(.diffKind, at: index, effectiveRange: nil) as? String else { return }
            (kind == "+" ? NSColor.systemGreen : NSColor.systemRed).withAlphaComponent(0.16).setFill()
            NSRect(x: 0, y: rect.minY + origin.y, width: max(rect.maxX + origin.x, 100000), height: rect.height).fill()
        }
    }
}

final class DiffGutterView: NSRulerView {
    override var isFlipped: Bool { true }

    init(scrollView: NSScrollView, textView: NSTextView) {
        super.init(scrollView: scrollView, orientation: .verticalRuler)
        clientView = textView
        NotificationCenter.default.addObserver(self, selector: #selector(redraw), name: NSView.boundsDidChangeNotification, object: scrollView.contentView)
        NotificationCenter.default.addObserver(self, selector: #selector(redraw), name: NSText.didChangeNotification, object: textView)
    }

    required init(coder: NSCoder) { fatalError() }

    @objc private func redraw() { needsDisplay = true }

    override func draw(_ dirtyRect: NSRect) {
        guard let tv = clientView as? NSTextView, let layoutManager = tv.layoutManager, let container = tv.textContainer, let storage = tv.textStorage else { return }
        let text = storage.string as NSString
        let glyphs = layoutManager.glyphRange(forBoundingRect: tv.visibleRect, in: container)
        layoutManager.enumerateLineFragments(forGlyphRange: glyphs) { rect, _, _, glyphRange, _ in
            let index = layoutManager.characterIndexForGlyph(at: glyphRange.location)
            guard index < storage.length else { return }
            let y = self.convert(NSPoint(x: 0, y: rect.minY + tv.textContainerOrigin.y), from: tv).y
            if let kind = storage.attribute(.diffKind, at: index, effectiveRange: nil) as? String {
                (kind == "+" ? NSColor.systemGreen : NSColor.systemRed).withAlphaComponent(0.16).setFill()
                NSRect(x: 0, y: y, width: self.bounds.width, height: rect.height).fill()
            }
            guard let label = storage.attribute(.gutter, at: index, effectiveRange: nil) as? String else { return }
            if index > 0, storage.attribute(.gutter, at: text.lineRange(for: NSRange(location: index - 1, length: 0)).location, effectiveRange: nil) as? String == label { return }
            let font = storage.attribute(.font, at: index, effectiveRange: nil) as? NSFont ?? AppState.shared.font
            NSAttributedString(string: label, attributes: [.font: font, .foregroundColor: NSColor.tertiaryLabelColor]).draw(at: NSPoint(x: 0, y: y))
        }
    }
}

final class DiffTextView: NSTextView {
    var onCancel: (() -> Void)?
    var onSave: (() -> Void)?
    private var words: [String] = []
    private var wordsDate = Date.distantPast

    override func performKeyEquivalent(with event: NSEvent) -> Bool {
        if isEditable, window?.firstResponder === self, event.modifierFlags.intersection([.command, .shift, .option, .control]) == .command, event.charactersIgnoringModifiers == "s" {
            onSave?()
            return true
        }
        return super.performKeyEquivalent(with: event)
    }

    override func cancelOperation(_ sender: Any?) {
        onCancel?()
    }

    override func insertNewline(_ sender: Any?) {
        guard isEditable else { return super.insertNewline(sender) }
        let text = string as NSString
        let line = text.substring(with: text.lineRange(for: NSRange(location: selectedRange().location, length: 0)))
        insertText("\n" + line.prefix { $0 == " " || $0 == "\t" }, replacementRange: selectedRange())
    }

    override func insertText(_ string: Any, replacementRange: NSRange) {
        super.insertText(string, replacementRange: replacementRange)
        guard isEditable, !hasMarkedText(), let typed = (string as? String)?.first, (string as? String)?.count == 1, typed.isLetter || typed.isNumber || typed == "_" else { return }
        var index = 0
        let range = rangeForUserCompletion
        guard range.length >= 2, completions(forPartialWordRange: range, indexOfSelectedItem: &index)?.isEmpty == false else { return }
        complete(nil)
    }

    override func completions(forPartialWordRange charRange: NSRange, indexOfSelectedItem index: UnsafeMutablePointer<Int>) -> [String]? {
        if Date().timeIntervalSince(wordsDate) > 2 {
            let text = string as NSString
            let regex = try! NSRegularExpression(pattern: "[A-Za-z_$][A-Za-z0-9_$]{2,}")
            words = Set(regex.matches(in: string, range: NSRange(location: 0, length: text.length)).map { text.substring(with: $0.range) }).sorted()
            wordsDate = Date()
        }
        let prefix = (string as NSString).substring(with: charRange).lowercased()
        return Array(words.filter { $0.count > prefix.count && $0.lowercased().hasPrefix(prefix) }.prefix(30))
    }

    override func insertCompletion(_ word: String, forPartialWordRange charRange: NSRange, movement: Int, isFinal flag: Bool) {
        guard flag, movement == NSTextMovement.return.rawValue || movement == NSTextMovement.tab.rawValue else { return }
        super.insertCompletion(word, forPartialWordRange: charRange, movement: movement, isFinal: flag)
    }
}

final class FileDiffViewController: NSViewController, NSTextViewDelegate {
    let change: FileChange
    let project: Project
    private(set) var textView: DiffTextView!
    private var gutter: DiffGutterView!
    private var imageViewer: ImageViewer?
    var focusView: NSView { imageViewer?.scroll ?? textView }
    private let header = NSTextField(labelWithString: "")
    private var changeOffsets: [Int] = []
    private var lineOffsets: [Int: Int] = [:]
    private var pendingLine: Int?
    private let undo = UndoManager()
    private var lastEdit: NSRange?
    private var loadedText: NSString = ""
    private(set) var isDirty = false {
        didSet { header.stringValue = (isDirty ? "● " : "") + change.path }
    }

    init(change: FileChange, project: Project) {
        self.change = change
        self.project = project
        super.init(nibName: nil, bundle: nil)
    }

    required init?(coder: NSCoder) { fatalError() }

    private var fileURL: URL { URL(fileURLWithPath: project.path).appendingPathComponent(change.path) }
    private var canEdit: Bool { change.status != "D" && change.commit == nil && imageViewer == nil }
    private var baseAttributes: [NSAttributedString.Key: Any] { [.font: AppState.shared.font, .foregroundColor: NSColor.labelColor] }

    override func loadView() {
        let root = NSView()
        header.stringValue = change.path + (change.commit.map { " @ " + $0.prefix(7) } ?? "")
        header.font = .systemFont(ofSize: NSFont.systemFontSize, weight: .semibold)
        header.lineBreakMode = .byTruncatingMiddle
        header.setContentCompressionResistancePriority(.defaultLow, for: .horizontal)
        let stats = NSTextField(labelWithString: change.status == " " ? "" : "+\(change.added)  −\(change.removed)")
        stats.font = .monospacedDigitSystemFont(ofSize: NSFont.smallSystemFontSize, weight: .regular)
        stats.textColor = .secondaryLabelColor
        let previous = button("chevron.up", "Previous Change (⇧F7)", #selector(previousChange))
        let next = button("chevron.down", "Next Change (F7)", #selector(nextChange))
        let open = button("arrow.up.forward.app", "Open with Default App", #selector(openExternally))
        let reveal = button("folder", "Reveal in Finder", #selector(revealInFinder))
        let isDiff = change.status != " "
        previous.isHidden = !isDiff
        next.isHidden = !isDiff
        let bar = NSStackView(views: [header, stats, NSView(), previous, next, reveal, open])
        bar.spacing = 6
        bar.edgeInsets = NSEdgeInsets(top: 8, left: 12, bottom: 8, right: 12)
        let scroll = NSTextView.scrollableTextView()
        scroll.hasHorizontalScroller = true
        let storage = NSTextStorage()
        let layoutManager = DiffLayoutManager()
        storage.addLayoutManager(layoutManager)
        let container = NSTextContainer(size: NSSize(width: CGFloat.greatestFiniteMagnitude, height: CGFloat.greatestFiniteMagnitude))
        container.widthTracksTextView = false
        layoutManager.addTextContainer(container)
        let tv = DiffTextView(frame: .zero, textContainer: container)
        tv.minSize = .zero
        tv.maxSize = NSSize(width: CGFloat.greatestFiniteMagnitude, height: CGFloat.greatestFiniteMagnitude)
        tv.autoresizingMask = [.width]
        tv.isVerticallyResizable = true
        tv.isEditable = false
        tv.isRichText = false
        tv.usesFindBar = true
        tv.isIncrementalSearchingEnabled = true
        tv.isHorizontallyResizable = true
        tv.textContainerInset = NSSize(width: 4, height: 6)
        tv.drawsBackground = false
        tv.onCancel = { [weak self] in (self?.parent as? ContentViewController)?.closeFileDiff() }
        tv.onSave = { [weak self] in self?.save() }
        tv.isAutomaticQuoteSubstitutionEnabled = false
        tv.isAutomaticDashSubstitutionEnabled = false
        tv.isAutomaticTextReplacementEnabled = false
        tv.isAutomaticSpellingCorrectionEnabled = false
        tv.isContinuousSpellCheckingEnabled = false
        tv.allowsUndo = true
        tv.delegate = self
        scroll.documentView = tv
        textView = tv
        gutter = DiffGutterView(scrollView: scroll, textView: tv)
        scroll.verticalRulerView = gutter
        scroll.hasVerticalRuler = true
        scroll.rulersVisible = true
        if ImageViewer.canShow(fileURL) {
            imageViewer = ImageViewer(url: fileURL)
            imageViewer?.scroll.onCancel = tv.onCancel
        }
        let stack = NSStackView(views: [bar, imageViewer ?? scroll])
        stack.orientation = .vertical
        stack.spacing = 0
        stack.translatesAutoresizingMaskIntoConstraints = false
        root.addSubview(stack)
        NSLayoutConstraint.activate([
            stack.topAnchor.constraint(equalTo: root.topAnchor),
            stack.bottomAnchor.constraint(equalTo: root.bottomAnchor),
            stack.leadingAnchor.constraint(equalTo: root.leadingAnchor),
            stack.trailingAnchor.constraint(equalTo: root.trailingAnchor),
            bar.widthAnchor.constraint(equalTo: stack.widthAnchor),
        ])
        imageViewer?.widthAnchor.constraint(equalTo: stack.widthAnchor).isActive = true
        view = root
        NotificationCenter.default.addObserver(forName: .gitChanged, object: project, queue: .main) { [weak self] _ in self?.reload() }
        NotificationCenter.default.addObserver(forName: .settingsChanged, object: nil, queue: .main) { [weak self] _ in MainActor.assumeIsolated { self?.reload() } }
        reload()
    }

    private func button(_ symbol: String, _ tip: String, _ action: Selector) -> NSButton {
        let button = NSButton(image: NSImage(systemSymbolName: symbol, accessibilityDescription: tip)!, target: self, action: action)
        button.bezelStyle = .accessoryBarAction
        button.isBordered = false
        button.toolTip = tip
        button.contentTintColor = .secondaryLabelColor
        return button
    }

    func undoManager(for view: NSTextView) -> UndoManager? { undo }

    func textView(_ textView: NSTextView, shouldChangeTextIn range: NSRange, replacementString text: String?) -> Bool {
        guard let storage = textView.textStorage else { return true }
        if let text, !text.isEmpty {
            func isRemoved(_ index: Int) -> Bool { storage.attribute(.diffKind, at: index, effectiveRange: nil) as? String == "-" }
            let string = storage.string as NSString
            if range.location > 0, string.character(at: range.location - 1) != 10, isRemoved(range.location - 1) { return false }
            if (range.location..<NSMaxRange(range)).contains(where: isRemoved) { return false }
        }
        lastEdit = NSRange(location: range.location, length: text?.utf16.count ?? 0)
        return true
    }

    func textView(_ textView: NSTextView, shouldChangeTypingAttributes oldTypingAttributes: [String: Any] = [:], toAttributes newTypingAttributes: [NSAttributedString.Key: Any] = [:]) -> [NSAttributedString.Key: Any] {
        baseAttributes
    }

    func textDidChange(_ notification: Notification) {
        isDirty = !((textView.textStorage?.string ?? "") as NSString).isEqual(to: loadedText as String)
        guard let edit = lastEdit, let storage = textView.textStorage, var highlighter = Language.forPath(change.path).map({ Highlighter(language: $0, palette: TerminalTheme.current.syntax) }) else { return }
        let string = storage.string as NSString
        let range = string.lineRange(for: NSRange(location: min(edit.location, string.length), length: min(edit.length, string.length - min(edit.location, string.length))))
        storage.beginEditing()
        storage.addAttribute(.foregroundColor, value: NSColor.labelColor, range: range)
        string.enumerateSubstrings(in: range, options: [.byLines, .substringNotRequired]) { _, line, _, _ in
            for span in highlighter.spans(in: Substring(string.substring(with: line))) {
                storage.addAttribute(.foregroundColor, value: span.color, range: NSRange(location: line.location + span.range.location, length: span.range.length))
            }
        }
        storage.endEditing()
    }

    private func fileText(upTo end: Int? = nil) -> String {
        guard let storage = textView?.textStorage else { return "" }
        var text = ""
        storage.enumerateAttribute(.diffKind, in: NSRange(location: 0, length: end ?? storage.length)) { value, range, _ in
            if value as? String != "-" { text += storage.attributedSubstring(from: range).string }
        }
        return text
    }

    @discardableResult
    func save() -> Bool {
        guard isDirty else { return true }
        var text = fileText()
        if let last = (try? Data(contentsOf: fileURL))?.last, last != 10, text.hasSuffix("\n") { text.removeLast() }
        do {
            try text.write(to: fileURL, atomically: true, encoding: .utf8)
            isDirty = false
            reload()
            return true
        } catch {
            presentError(error)
            return false
        }
    }

    func confirmClose() -> Bool {
        guard isDirty else { return true }
        let alert = NSAlert()
        alert.messageText = "Save changes to \((change.path as NSString).lastPathComponent)?"
        alert.informativeText = "Your changes will be lost if you don't save them."
        alert.addButton(withTitle: "Save")
        alert.addButton(withTitle: "Cancel")
        alert.addButton(withTitle: "Don't Save")
        switch alert.runModal() {
        case .alertFirstButtonReturn: return save()
        case .alertThirdButtonReturn: return true
        default: return false
        }
    }

    @objc private func openExternally() {
        NSWorkspace.shared.open(fileURL)
    }

    @objc private func revealInFinder() {
        NSWorkspace.shared.activateFileViewerSelecting([fileURL])
    }

    func reveal(line: Int) {
        guard let tv = textView, let layoutManager = tv.layoutManager, let start = lineOffsets[line] ?? lineOffsets[lineOffsets.keys.max() ?? 0] else {
            pendingLine = line
            return
        }
        pendingLine = nil
        let range = (tv.string as NSString).lineRange(for: NSRange(location: start, length: 0))
        tv.setSelectedRange(range)
        let rect = layoutManager.lineFragmentRect(forGlyphAt: layoutManager.glyphIndexForCharacter(at: start), effectiveRange: nil)
        let visible = tv.enclosingScrollView?.contentView.bounds.height ?? 0
        tv.scroll(NSPoint(x: 0, y: max(0, rect.minY + tv.textContainerInset.height - visible / 3)))
        tv.showFindIndicator(for: range)
    }

    @objc func nextChange() { jump(1) }
    @objc func previousChange() { jump(-1) }

    private func jump(_ step: Int) {
        guard let tv = textView, let layoutManager = tv.layoutManager, !changeOffsets.isEmpty else { return }
        let current = tv.selectedRange().location
        let target = step > 0 ? changeOffsets.first { $0 > current } ?? changeOffsets.first! : changeOffsets.last { $0 < current } ?? changeOffsets.last!
        tv.setSelectedRange(NSRange(location: target, length: 0))
        let glyph = layoutManager.glyphIndexForCharacter(at: target)
        let rect = layoutManager.lineFragmentRect(forGlyphAt: glyph, effectiveRange: nil)
        let visible = tv.enclosingScrollView?.contentView.bounds.height ?? 0
        tv.scroll(NSPoint(x: 0, y: max(0, rect.minY + tv.textContainerInset.height - visible / 4)))
    }

    private func reload() {
        guard imageViewer == nil, !isDirty else { return }
        let change = change
        let path = project.path
        let font = AppState.shared.font
        let palette = TerminalTheme.current.syntax
        DispatchQueue.global(qos: .userInitiated).async {
            let lines = Git.fullDiff(of: change, in: path)
            let (text, offsets, lineStarts, gutterWidth) = Self.render(lines, font: font, language: Language.forPath(change.path), palette: palette)
            DispatchQueue.main.async { [weak self] in
                guard let self, let tv = textView, !isDirty else { return }
                let origin = tv.enclosingScrollView?.contentView.bounds.origin
                let isFirst = changeOffsets.isEmpty && tv.string.isEmpty
                let before = fileText(upTo: min(tv.selectedRange().location, tv.string.utf16.count)).components(separatedBy: "\n")
                tv.textStorage?.setAttributedString(text)
                loadedText = text.string as NSString
                undo.removeAllActions()
                tv.isEditable = canEdit && lines != nil
                tv.typingAttributes = baseAttributes
                gutter.ruleThickness = ceil(("0" as NSString).size(withAttributes: [.font: font]).width * CGFloat(gutterWidth)) + 6
                gutter.needsDisplay = true
                changeOffsets = offsets
                lineOffsets = lineStarts
                if let line = pendingLine {
                    reveal(line: line)
                } else if isFirst, change.status != "?", !offsets.isEmpty {
                    jump(1)
                } else {
                    if !isFirst, let start = lineStarts[before.count] {
                        let length = (tv.string as NSString).lineRange(for: NSRange(location: start, length: 0)).length
                        tv.setSelectedRange(NSRange(location: start + min(before.last?.utf16.count ?? 0, max(0, length - 1)), length: 0))
                    }
                    if let origin { tv.scroll(origin) }
                }
            }
        }
    }

    private static func render(_ lines: [DiffLine]?, font: NSFont, language: Language?, palette: SyntaxPalette) -> (NSAttributedString, [Int], [Int: Int], Int) {
        guard let lines else {
            return (NSAttributedString(string: "Binary, unreadable or too large file", attributes: [.font: font, .foregroundColor: NSColor.secondaryLabelColor]), [], [:], 0)
        }
        var lineStarts: [Int: Int] = [:]
        let result = NSMutableAttributedString()
        let isDiff = lines.contains { $0.kind != " " }
        let twoColumns = lines.contains { $0.kind == "-" }
        let width = max(3, String(lines.count).count)
        func number(_ n: Int?) -> String {
            let s = n.map(String.init) ?? ""
            return String(repeating: " ", count: max(0, width - s.count)) + s
        }
        var highlighter = language.map { Highlighter(language: $0, palette: palette) }
        var offsets: [Int] = []
        var previous: Character = " "
        var gutterWidth = 0
        for line in lines {
            if line.kind != " ", previous == " " { offsets.append(result.length) }
            previous = line.kind
            let start = result.length
            if let number = line.newNumber { lineStarts[number] = start }
            let numbers = twoColumns ? "\(number(line.oldNumber)) \(number(line.newNumber))" : number(line.newNumber ?? line.oldNumber)
            let label = " \(numbers) " + (isDiff ? "\(line.kind)" : "")
            gutterWidth = max(gutterWidth, label.count + 1)
            let text = NSMutableAttributedString(string: "\(line.text)\n", attributes: [.font: font, .foregroundColor: NSColor.labelColor, .gutter: label])
            for span in highlighter?.spans(in: line.text) ?? [] {
                text.addAttribute(.foregroundColor, value: span.color, range: span.range)
            }
            result.append(text)
            if line.kind != " " {
                result.addAttribute(.diffKind, value: String(line.kind), range: NSRange(location: start, length: result.length - start))
            }
        }
        return (result, offsets, lineStarts, gutterWidth)
    }
}
