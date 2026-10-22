import AppKit

final class GitPanelViewController: NSViewController {
    private let header = NSTextField(labelWithString: "")
    private let summary = NSTextField(labelWithString: "")
    private let rows = NSStackView()
    private let empty = NSTextField(labelWithString: "")
    private(set) var status: GitStatus?
    private let message = NSTextField()
    private let commitButton = NSButton(title: "Commit", target: nil, action: nil)
    private var commitBox: NSStackView!
    private var collapsed: Set<Bool> = []
    private var watcher: FileWatcher?
    private var isRefreshing = false
    private var needsRefresh = false

    var project: Project? {
        didSet {
            guard project !== oldValue else { return }
            status = nil
            rebuild()
            watch()
            refresh()
        }
    }

    override func loadView() {
        header.font = .systemFont(ofSize: NSFont.systemFontSize, weight: .semibold)
        header.lineBreakMode = .byTruncatingTail
        summary.font = .monospacedDigitSystemFont(ofSize: NSFont.smallSystemFontSize, weight: .regular)
        summary.textColor = .secondaryLabelColor
        message.target = self
        message.action = #selector(commit)
        message.usesSingleLineMode = false
        message.cell?.wraps = true
        message.cell?.isScrollable = false
        message.lineBreakMode = .byWordWrapping
        message.font = .systemFont(ofSize: NSFont.systemFontSize)
        commitButton.target = self
        commitButton.action = #selector(commit)
        commitButton.bezelColor = .controlAccentColor
        commitButton.controlSize = .large
        commitBox = NSStackView(views: [message, commitButton])
        commitBox.orientation = .vertical
        commitBox.alignment = .width
        commitBox.spacing = 6
        commitBox.edgeInsets = NSEdgeInsets(top: 2, left: 12, bottom: 10, right: 12)
        rows.orientation = .vertical
        rows.alignment = .width
        rows.spacing = 0
        rows.edgeInsets = NSEdgeInsets(top: 0, left: 6, bottom: 12, right: 6)
        rows.translatesAutoresizingMaskIntoConstraints = false
        empty.textColor = .tertiaryLabelColor
        empty.alignment = .center
        let document = FlippedView()
        document.translatesAutoresizingMaskIntoConstraints = false
        document.addSubview(rows)
        let scroll = NSScrollView()
        scroll.documentView = document
        scroll.hasVerticalScroller = false
        scroll.drawsBackground = false
        let titles = NSStackView(views: [header, summary])
        titles.orientation = .vertical
        titles.alignment = .leading
        titles.spacing = 2
        let top = NSStackView(views: [titles])
        top.edgeInsets = NSEdgeInsets(top: 8, left: 14, bottom: 8, right: 12)
        let stack = NSStackView(views: [top, commitBox, scroll])
        stack.orientation = .vertical
        stack.alignment = .leading
        stack.spacing = 0
        stack.translatesAutoresizingMaskIntoConstraints = false
        let root = NSView()
        root.addSubview(stack)
        NSLayoutConstraint.activate([
            stack.topAnchor.constraint(equalTo: root.topAnchor),
            stack.bottomAnchor.constraint(equalTo: root.bottomAnchor),
            stack.leadingAnchor.constraint(equalTo: root.leadingAnchor),
            stack.trailingAnchor.constraint(equalTo: root.trailingAnchor),
            scroll.widthAnchor.constraint(equalTo: stack.widthAnchor),
            top.widthAnchor.constraint(equalTo: stack.widthAnchor),
            commitBox.widthAnchor.constraint(equalTo: stack.widthAnchor),
            message.heightAnchor.constraint(greaterThanOrEqualToConstant: 44),
            message.widthAnchor.constraint(equalTo: commitBox.widthAnchor, constant: -24),
            commitButton.widthAnchor.constraint(equalTo: message.widthAnchor),
            document.topAnchor.constraint(equalTo: scroll.contentView.topAnchor),
            document.leadingAnchor.constraint(equalTo: scroll.contentView.leadingAnchor),
            document.widthAnchor.constraint(equalTo: scroll.contentView.widthAnchor),
            rows.topAnchor.constraint(equalTo: document.topAnchor),
            rows.leadingAnchor.constraint(equalTo: document.leadingAnchor),
            rows.trailingAnchor.constraint(equalTo: document.trailingAnchor),
            rows.bottomAnchor.constraint(equalTo: document.bottomAnchor),
        ])
        view = root
        NotificationCenter.default.addObserver(forName: NSApplication.didBecomeActiveNotification, object: nil, queue: .main) { [weak self] _ in self?.refresh() }
        NotificationCenter.default.addObserver(forName: .settingsChanged, object: nil, queue: .main) { [weak self] _ in MainActor.assumeIsolated { self?.rebuild() } }
        updateHeader()
    }

    private func watch() {
        watcher = nil
        guard let path = project?.path else { return }
        var paths = [path]
        if let gitDir = Git.gitDir(in: path), !gitDir.hasPrefix(path + "/") {
            paths.append(gitDir)
        }
        watcher = FileWatcher(paths: paths) { [weak self] in self?.refresh() }
    }

    func refresh() {
        guard let project else {
            updateHeader()
            return
        }
        if isRefreshing {
            needsRefresh = true
            return
        }
        isRefreshing = true
        let path = project.path
        DispatchQueue.global(qos: .userInitiated).async {
            let result = Git.status(in: path)
            DispatchQueue.main.async { [weak self] in
                guard let self else { return }
                isRefreshing = false
                if self.project === project, result != status {
                    status = result
                    updateHeader()
                    rebuild()
                    NotificationCenter.default.post(name: .gitChanged, object: project, userInfo: result.map { ["status": $0] } ?? [:])
                }
                if needsRefresh {
                    needsRefresh = false
                    refresh()
                }
            }
        }
        updateHeader()
    }

    private func updateHeader() {
        commitBox.isHidden = status == nil
        commitButton.isEnabled = status?.staged.isEmpty == false
        message.placeholderString = "Message (↩ to commit on \u{201C}\(status?.branch ?? "")\u{201D})"
        guard let project else {
            header.stringValue = "No project"
            summary.stringValue = ""
            return
        }
        guard let status else {
            header.stringValue = project.name
            summary.stringValue = isRefreshing ? "Loading…" : "Not a git repository"
            return
        }
        let added = status.changes.reduce(0) { $0 + $1.added }
        let removed = status.changes.reduce(0) { $0 + $1.removed }
        header.stringValue = status.branch
        summary.stringValue = status.changes.isEmpty ? "No changes" : "\(status.changes.count) files  +\(added)  −\(removed)"
    }

    private func rebuild() {
        rows.arrangedSubviews.forEach { $0.removeFromSuperview() }
        guard let project, let status else { return }
        for (staged, changes) in [(true, status.staged), (false, status.unstaged)] where !changes.isEmpty {
            let section = ChangeSectionRow(title: staged ? "Staged Changes" : "Changes", count: changes.count, isStaged: staged, isCollapsed: collapsed.contains(staged))
            section.onToggle = { [weak self] in
                guard let self else { return }
                if collapsed.contains(staged) { collapsed.remove(staged) } else { collapsed.insert(staged) }
                rebuild()
            }
            section.onStage = { [weak self] in if staged { self?.unstage(["."]) } else { self?.stage(["."]) } }
            section.onDiscard = staged ? nil : { [weak self] in self?.discard(changes) }
            rows.addArrangedSubview(section)
            guard !collapsed.contains(staged) else { continue }
            for change in changes {
                let row = ChangeFileRow(change: change, isStaged: staged)
                row.onOpen = { [weak self] in (self?.view.window?.windowController as? MainWindowController)?.show(change, in: project) }
                row.onStage = { [weak self] in if staged { self?.unstage([change.path]) } else { self?.stage([change.path]) } }
                row.onDiscard = staged ? nil : { [weak self] in self?.discard([change]) }
                rows.addArrangedSubview(row)
            }
        }
        if status.changes.isEmpty {
            empty.stringValue = "Working tree clean"
            rows.addArrangedSubview(empty)
            empty.widthAnchor.constraint(equalTo: rows.widthAnchor, constant: -12).isActive = true
        }
    }

    private func stage(_ paths: [String]) {
        perform { Git.stage(paths, in: $0) }
    }

    private func unstage(_ paths: [String]) {
        perform { Git.unstage(paths, in: $0) }
    }

    private func discard(_ changes: [FileChange]) {
        guard let path = project?.path, !changes.isEmpty else { return }
        let alert = NSAlert()
        alert.messageText = changes.count == 1 ? "Discard changes in \((changes[0].path as NSString).lastPathComponent)?" : "Discard all \(changes.count) changes?"
        alert.informativeText = "Unstaged changes are lost. New files are moved to the Trash."
        alert.addButton(withTitle: "Discard")
        alert.addButton(withTitle: "Cancel")
        alert.buttons.first?.hasDestructiveAction = true
        guard alert.runModal() == .alertFirstButtonReturn else { return }
        let untracked = changes.filter { $0.status == "?" }.map { URL(fileURLWithPath: path + "/" + $0.path) }
        let tracked = changes.filter { $0.status != "?" }.map(\.path)
        if !untracked.isEmpty { NSWorkspace.shared.recycle(untracked) { [weak self] _, _ in self?.refresh() } }
        if !tracked.isEmpty { perform { Git.discard(tracked, in: $0) } }
    }

    @objc private func commit() {
        let text = message.stringValue.trimmingCharacters(in: .whitespacesAndNewlines)
        guard status?.staged.isEmpty == false else { return NSSound.beep() }
        guard !text.isEmpty else {
            view.window?.makeFirstResponder(message)
            return NSSound.beep()
        }
        perform({ Git.commit(message: text, in: $0) }) { [weak self] in self?.message.stringValue = "" }
    }

    private func perform(_ action: @escaping @Sendable (String) -> String?, onSuccess: (() -> Void)? = nil) {
        guard let path = project?.path else { return }
        DispatchQueue.global(qos: .userInitiated).async {
            let error = action(path)
            DispatchQueue.main.async { [weak self] in
                guard let self else { return }
                if let error {
                    let alert = NSAlert()
                    alert.messageText = "Git failed"
                    alert.informativeText = error
                    if let window = view.window { alert.beginSheetModal(for: window) } else { alert.runModal() }
                } else {
                    onSuccess?()
                }
                refresh()
            }
        }
    }
}

class HoverRow: NSView {
    let actions = NSStackView()
    private var isHovered = false { didSet { actions.isHidden = !isHovered; needsDisplay = true } }

    override init(frame: NSRect) {
        super.init(frame: frame)
        actions.spacing = 2
        actions.isHidden = true
        addTrackingArea(NSTrackingArea(rect: .zero, options: [.mouseEnteredAndExited, .activeInActiveApp, .inVisibleRect], owner: self))
        heightAnchor.constraint(equalToConstant: 24).isActive = true
    }

    required init?(coder: NSCoder) { fatalError() }

    override func mouseDown(with event: NSEvent) {}
    override func mouseEntered(with event: NSEvent) { isHovered = true }
    override func mouseExited(with event: NSEvent) { isHovered = false }

    override func draw(_ dirtyRect: NSRect) {
        guard isHovered else { return }
        NSColor.labelColor.withAlphaComponent(0.06).setFill()
        NSBezierPath(roundedRect: bounds, xRadius: 5, yRadius: 5).fill()
    }

    func addAction(_ symbol: String, _ tip: String, _ action: Selector) {
        let button = NSButton(image: NSImage(systemSymbolName: symbol, accessibilityDescription: tip)!, target: self, action: action)
        button.isBordered = false
        button.symbolConfiguration = .init(pointSize: 11, weight: .regular)
        button.contentTintColor = .secondaryLabelColor
        button.toolTip = tip
        button.widthAnchor.constraint(equalToConstant: 20).isActive = true
        actions.addArrangedSubview(button)
    }

    func layout(_ views: [NSView]) {
        let stack = NSStackView(views: views)
        stack.spacing = 6
        stack.edgeInsets = NSEdgeInsets(top: 0, left: 6, bottom: 0, right: 8)
        stack.translatesAutoresizingMaskIntoConstraints = false
        addSubview(stack)
        NSLayoutConstraint.activate([
            stack.topAnchor.constraint(equalTo: topAnchor),
            stack.bottomAnchor.constraint(equalTo: bottomAnchor),
            stack.leadingAnchor.constraint(equalTo: leadingAnchor),
            stack.trailingAnchor.constraint(equalTo: trailingAnchor),
        ])
    }

    override func viewDidMoveToSuperview() {
        super.viewDidMoveToSuperview()
        guard let stack = superview as? NSStackView else { return }
        widthAnchor.constraint(equalTo: stack.widthAnchor, constant: -stack.edgeInsets.left - stack.edgeInsets.right).isActive = true
    }
}

final class ChangeSectionRow: HoverRow {
    var onToggle: (() -> Void)?
    var onStage: (() -> Void)?
    var onDiscard: (() -> Void)?

    init(title: String, count: Int, isStaged: Bool, isCollapsed: Bool) {
        super.init(frame: .zero)
        let chevron = NSImageView(image: NSImage(systemSymbolName: isCollapsed ? "chevron.right" : "chevron.down", accessibilityDescription: nil)!)
        chevron.symbolConfiguration = .init(pointSize: 9, weight: .semibold)
        chevron.contentTintColor = .secondaryLabelColor
        chevron.widthAnchor.constraint(equalToConstant: 12).isActive = true
        let label = NSTextField(labelWithString: title)
        label.font = .systemFont(ofSize: NSFont.smallSystemFontSize, weight: .semibold)
        label.setContentCompressionResistancePriority(.init(250), for: .horizontal)
        let badge = NSTextField(labelWithString: "\(count)")
        badge.font = .monospacedDigitSystemFont(ofSize: 10, weight: .semibold)
        badge.alignment = .center
        badge.wantsLayer = true
        badge.layer?.cornerRadius = 7
        badge.layer?.backgroundColor = NSColor.quaternaryLabelColor.cgColor
        badge.widthAnchor.constraint(greaterThanOrEqualToConstant: 20).isActive = true
        let spacer = NSView()
        spacer.setContentHuggingPriority(.init(1), for: .horizontal)
        if !isStaged, count > 0 { addAction("arrow.uturn.backward", "Discard All Changes", #selector(discardClicked)) }
        if count > 0 { addAction(isStaged ? "minus" : "plus", isStaged ? "Unstage All Changes" : "Stage All Changes", #selector(stageClicked)) }
        layout([chevron, label, spacer, actions, badge])
        setAccessibilityLabel("\(title), \(count)")
    }

    required init?(coder: NSCoder) { fatalError() }

    override func mouseUp(with event: NSEvent) {
        if bounds.contains(convert(event.locationInWindow, from: nil)) { onToggle?() }
    }

    @objc private func stageClicked() { onStage?() }
    @objc private func discardClicked() { onDiscard?() }
}

final class ChangeFileRow: HoverRow {
    var onOpen: (() -> Void)?
    var onStage: (() -> Void)?
    var onDiscard: (() -> Void)?

    init(change: FileChange, isStaged: Bool) {
        super.init(frame: .zero)
        let name = (change.path as NSString).lastPathComponent
        let (image, color) = FileIcon.icon(for: name, isDirectory: false)
        let icon = NSImageView(image: image ?? NSImage())
        icon.contentTintColor = color
        icon.symbolConfiguration = .init(pointSize: 11, weight: .regular)
        icon.widthAnchor.constraint(equalToConstant: 16).isActive = true
        let label = NSTextField(labelWithString: name)
        label.font = .systemFont(ofSize: NSFont.smallSystemFontSize)
        label.textColor = change.status == "D" ? .secondaryLabelColor : .labelColor
        label.lineBreakMode = .byTruncatingMiddle
        label.setContentCompressionResistancePriority(.init(260), for: .horizontal)
        let folder = NSTextField(labelWithString: (change.path as NSString).deletingLastPathComponent)
        folder.font = .systemFont(ofSize: 10)
        folder.textColor = .tertiaryLabelColor
        folder.lineBreakMode = .byTruncatingHead
        folder.setContentCompressionResistancePriority(.init(250), for: .horizontal)
        let spacer = NSView()
        spacer.setContentHuggingPriority(.init(1), for: .horizontal)
        let letter = NSTextField(labelWithString: change.status == "?" ? "U" : String(change.status))
        letter.font = .monospacedSystemFont(ofSize: NSFont.smallSystemFontSize, weight: .semibold)
        letter.textColor = FileChange.color(for: change.status)
        letter.alignment = .center
        letter.widthAnchor.constraint(equalToConstant: 14).isActive = true
        addAction("arrow.up.right.square", "Open Changes", #selector(openClicked))
        if !isStaged { addAction("arrow.uturn.backward", "Discard Changes", #selector(discardClicked)) }
        addAction(isStaged ? "minus" : "plus", isStaged ? "Unstage Changes" : "Stage Changes", #selector(stageClicked))
        layout([icon, label, folder, spacer, actions, letter])
        toolTip = change.path
        setAccessibilityLabel(change.path)
    }

    required init?(coder: NSCoder) { fatalError() }

    override func mouseUp(with event: NSEvent) {
        if bounds.contains(convert(event.locationInWindow, from: nil)) { onOpen?() }
    }

    @objc private func openClicked() { onOpen?() }
    @objc private func stageClicked() { onStage?() }
    @objc private func discardClicked() { onDiscard?() }
}

extension FileChange {
    static func color(for status: Character) -> NSColor {
        switch status {
        case "A", "?": .systemGreen
        case "D": .systemRed
        default: .systemOrange
        }
    }
}

extension Array {
    subscript(safe index: Int) -> Element? {
        indices.contains(index) ? self[index] : nil
    }
}
