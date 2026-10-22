import AppKit

final class ActionRow: NSView {
    private let action: () -> Void
    private var isHovered = false { didSet { needsDisplay = true } }
    private var isPressed = false { didSet { needsDisplay = true } }

    init(image: NSImage?, tint: NSColor? = nil, title: String, subtitle: String? = nil, detail: String? = nil, action: @escaping () -> Void) {
        self.action = action
        super.init(frame: .zero)
        let icon = NSImageView(image: image ?? NSImage())
        icon.symbolConfiguration = .init(pointSize: 15, weight: .regular)
        icon.contentTintColor = tint
        icon.setContentHuggingPriority(.required, for: .horizontal)
        let titleField = NSTextField(labelWithString: title)
        titleField.font = .systemFont(ofSize: NSFont.systemFontSize, weight: .medium)
        titleField.lineBreakMode = .byTruncatingTail
        let text = NSStackView(views: [titleField])
        text.orientation = .vertical
        text.alignment = .leading
        text.spacing = 1
        if let subtitle {
            let field = NSTextField(labelWithString: subtitle)
            field.font = .systemFont(ofSize: NSFont.smallSystemFontSize)
            field.textColor = .secondaryLabelColor
            field.lineBreakMode = .byTruncatingMiddle
            text.addArrangedSubview(field)
        }
        text.setContentCompressionResistancePriority(.defaultLow, for: .horizontal)
        var views: [NSView] = [icon, text, NSView()]
        if let detail {
            let field = NSTextField(labelWithString: detail)
            field.font = .monospacedDigitSystemFont(ofSize: NSFont.smallSystemFontSize, weight: .regular)
            field.textColor = .tertiaryLabelColor
            field.setContentCompressionResistancePriority(.required, for: .horizontal)
            views.append(field)
        }
        let row = NSStackView(views: views)
        row.spacing = 10
        row.edgeInsets = NSEdgeInsets(top: 8, left: 12, bottom: 8, right: 12)
        row.translatesAutoresizingMaskIntoConstraints = false
        addSubview(row)
        NSLayoutConstraint.activate([
            row.topAnchor.constraint(equalTo: topAnchor),
            row.bottomAnchor.constraint(equalTo: bottomAnchor),
            row.leadingAnchor.constraint(equalTo: leadingAnchor),
            row.trailingAnchor.constraint(equalTo: trailingAnchor),
            icon.widthAnchor.constraint(equalToConstant: 20),
            heightAnchor.constraint(greaterThanOrEqualToConstant: 40),
        ])
    }

    required init?(coder: NSCoder) { fatalError() }

    override func updateTrackingAreas() {
        trackingAreas.forEach(removeTrackingArea)
        addTrackingArea(NSTrackingArea(rect: bounds, options: [.mouseEnteredAndExited, .activeInKeyWindow], owner: self))
        super.updateTrackingAreas()
    }

    override func mouseEntered(with event: NSEvent) { isHovered = true }
    override func mouseExited(with event: NSEvent) { isHovered = false }
    override func mouseDown(with event: NSEvent) { isPressed = true }

    override func mouseUp(with event: NSEvent) {
        isPressed = false
        if bounds.contains(convert(event.locationInWindow, from: nil)) { action() }
    }

    override func resetCursorRects() {
        addCursorRect(bounds, cursor: .pointingHand)
    }

    override func draw(_ dirtyRect: NSRect) {
        guard isHovered || isPressed else { return }
        NSColor.labelColor.withAlphaComponent(isPressed ? 0.12 : 0.06).setFill()
        NSBezierPath(roundedRect: bounds, xRadius: 8, yRadius: 8).fill()
    }
}

@MainActor
class OverviewView: NSScrollView {
    let stack = NSStackView()

    init() {
        super.init(frame: .zero)
        drawsBackground = false
        hasVerticalScroller = true
        stack.orientation = .vertical
        stack.alignment = .leading
        stack.spacing = 6
        stack.edgeInsets = NSEdgeInsets(top: 36, left: 0, bottom: 36, right: 0)
        stack.translatesAutoresizingMaskIntoConstraints = false
        let document = FlippedView()
        document.translatesAutoresizingMaskIntoConstraints = false
        document.addSubview(stack)
        documentView = document
        NSLayoutConstraint.activate([
            document.widthAnchor.constraint(equalTo: contentView.widthAnchor),
            stack.topAnchor.constraint(equalTo: document.topAnchor),
            stack.bottomAnchor.constraint(equalTo: document.bottomAnchor),
            stack.centerXAnchor.constraint(equalTo: document.centerXAnchor),
            stack.widthAnchor.constraint(equalToConstant: 620),
        ])
        for name in [Notification.Name.projectsChanged, .conversationsChanged] {
            NotificationCenter.default.addObserver(forName: name, object: nil, queue: .main) { [weak self] _ in
                MainActor.assumeIsolated { self?.rebuild() }
            }
        }
    }

    required init?(coder: NSCoder) { fatalError() }

    func rebuild() {
        stack.arrangedSubviews.forEach { $0.removeFromSuperview() }
        build()
    }

    func build() {}

    func header(symbol: String, tint: NSColor?, title: String, subtitle: String) {
        let icon = NSImageView(image: NSImage(systemSymbolName: symbol, accessibilityDescription: nil) ?? NSImage())
        icon.symbolConfiguration = .init(pointSize: 30, weight: .regular)
        icon.contentTintColor = tint ?? .controlAccentColor
        let titleField = NSTextField(labelWithString: title)
        titleField.font = .systemFont(ofSize: 24, weight: .bold)
        let subtitleField = NSTextField(labelWithString: subtitle)
        subtitleField.textColor = .secondaryLabelColor
        subtitleField.lineBreakMode = .byTruncatingMiddle
        let text = NSStackView(views: [titleField, subtitleField])
        text.orientation = .vertical
        text.alignment = .leading
        text.spacing = 2
        let row = NSStackView(views: [icon, text])
        row.spacing = 14
        row.edgeInsets = NSEdgeInsets(top: 0, left: 12, bottom: 12, right: 12)
        stack.addArrangedSubview(row)
    }

    func section(_ title: String) {
        let label = NSTextField(labelWithString: title)
        label.font = .systemFont(ofSize: NSFont.smallSystemFontSize, weight: .semibold)
        label.textColor = .secondaryLabelColor
        let wrapper = NSStackView(views: [label])
        wrapper.edgeInsets = NSEdgeInsets(top: 16, left: 12, bottom: 2, right: 12)
        stack.addArrangedSubview(wrapper)
    }

    func note(_ text: String) {
        let label = NSTextField(labelWithString: text)
        label.textColor = .tertiaryLabelColor
        let wrapper = NSStackView(views: [label])
        wrapper.edgeInsets = NSEdgeInsets(top: 4, left: 12, bottom: 4, right: 12)
        stack.addArrangedSubview(wrapper)
    }

    func row(_ row: ActionRow) {
        stack.addArrangedSubview(row)
        row.widthAnchor.constraint(equalTo: stack.widthAnchor).isActive = true
    }

    static func symbol(_ name: String) -> NSImage? {
        NSImage(systemSymbolName: name, accessibilityDescription: nil)
    }
}

final class FlippedView: NSView {
    override var isFlipped: Bool { true }
}

final class ProjectOverview: OverviewView {
    private let project: Project

    init(project: Project) {
        self.project = project
        super.init()
        build()
    }

    required init?(coder: NSCoder) { fatalError() }

    override func build() {
        let app = AppController.shared
        let branch = ProcessInspector.gitBranch(at: project.path).map { " · \($0)" } ?? ""
        header(symbol: "folder.fill", tint: project.tint?.color, title: project.name, subtitle: project.displayPath + branch)

        section("New session")
        let profiles = app.state.profiles.filter(\.isLaunchable)
        let last = project.lastProfile ?? app.state.lastProfile
        let ordered = profiles.filter { $0.name == last } + profiles.filter { $0.name != last }
        for (index, profile) in ordered.enumerated() {
            row(ActionRow(image: AgentBadge.image(for: profile.kind?.rawValue ?? profile.executableName), title: "New \(profile.name) session", subtitle: profile.summary, detail: index == 0 ? MainMenu.shortcut(for: #selector(AppController.newAgent(_:))) + " ↩" : nil) { [project] in
                app.startAgent(profile, in: project)
            })
        }
        for shell in app.state.shells where shell.isLaunchable {
            row(ActionRow(image: Self.symbol("terminal"), tint: .secondaryLabelColor, title: "New \(shell.name) tab", subtitle: shell.path) { [project] in
                app.startShell(shell, in: project)
            })
        }
        row(ActionRow(image: Self.symbol("terminal"), tint: .secondaryLabelColor, title: "New terminal", subtitle: "\(app.state.terminalShell) in \(project.displayPath)", detail: MainMenu.shortcut(for: #selector(AppController.newTerminal(_:)))) { [project] in
            app.startTerminal(in: project)
        })

        let running = project.sessions
        if !running.isEmpty {
            section("Open tabs")
            for session in running {
                let image = session.runningAgent.map(AgentBadge.image(for:)) ?? Self.symbol(session.kind == .agent ? "sparkles" : "terminal")
                row(ActionRow(image: image, tint: .secondaryLabelColor, title: session.title, subtitle: session.isRunning ? session.subtitle : "exited") {
                    app.select(session)
                })
            }
        }

        section("Recent conversations")
        let openIds = Set(project.sessions.filter(\.isRunning).compactMap(\.conversationId))
        switch app.conversations.cached(for: project.path) {
        case nil:
            note("Loading…")
            app.conversations.refresh(project.path, profiles: app.state.profiles)
        case let list? where list.isEmpty:
            note("No conversations in this folder yet")
        case let list?:
            for conversation in list.prefix(12) {
                let isOpen = openIds.contains(conversation.id)
                row(ActionRow(image: AgentBadge.image(for: conversation.kind.rawValue), title: conversation.title, subtitle: "\(conversation.profile.name) · \(conversation.date.relativeDescription)", detail: isOpen ? "open" : nil) { [project] in
                    app.resume(conversation, in: project)
                })
            }
        }

        section("Folder")
        row(ActionRow(image: Self.symbol("finder"), tint: .secondaryLabelColor, title: "Reveal in Finder") { [project] in
            NSWorkspace.shared.selectFile(nil, inFileViewerRootedAtPath: project.path)
        })
    }
}
