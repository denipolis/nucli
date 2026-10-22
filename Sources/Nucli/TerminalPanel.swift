import AppKit

final class TerminalPanel: NSView {
    var onSelect: ((Session) -> Void)?
    var onClose: ((Session) -> Void)?
    var onNew: (() -> Void)?
    var onHide: (() -> Void)?
    var onMaximize: (() -> Void)?
    private(set) weak var shown: Session?
    private let tabs = NSStackView()
    private let holder = NSView()
    private let maximize = NSButton()

    override init(frame: NSRect) {
        super.init(frame: frame)
        wantsLayer = true
        tabs.spacing = 2
        let title = NSTextField(labelWithString: "Terminal")
        title.font = .systemFont(ofSize: NSFont.smallSystemFontSize, weight: .semibold)
        title.textColor = .secondaryLabelColor
        let scroll = NSScrollView()
        scroll.drawsBackground = false
        scroll.hasHorizontalScroller = false
        scroll.verticalScrollElasticity = .none
        let document = FlippedView()
        document.addSubview(tabs)
        scroll.documentView = document
        let new = button("plus", "New Terminal (\(MainMenu.shortcut(for: #selector(AppController.newTerminal(_:)))))", #selector(newClicked))
        configure(maximize, "chevron.up", "Maximize Panel", #selector(maximizeClicked))
        let hide = button("xmark", "Hide Panel (\(MainMenu.shortcut(for: #selector(AppController.toggleTerminalPanel(_:)))))", #selector(hideClicked))
        let buttons = NSStackView(views: [new, maximize, hide])
        buttons.spacing = 10
        let separator = NSBox()
        separator.boxType = .separator
        for view in [separator, title, scroll, buttons, holder] as [NSView] {
            view.translatesAutoresizingMaskIntoConstraints = false
            addSubview(view)
        }
        tabs.translatesAutoresizingMaskIntoConstraints = false
        document.translatesAutoresizingMaskIntoConstraints = false
        NSLayoutConstraint.activate([
            separator.topAnchor.constraint(equalTo: topAnchor),
            separator.leadingAnchor.constraint(equalTo: leadingAnchor),
            separator.trailingAnchor.constraint(equalTo: trailingAnchor),
            title.leadingAnchor.constraint(equalTo: leadingAnchor, constant: 14),
            title.centerYAnchor.constraint(equalTo: scroll.centerYAnchor),
            scroll.topAnchor.constraint(equalTo: separator.bottomAnchor, constant: 2),
            scroll.heightAnchor.constraint(equalToConstant: 32),
            scroll.leadingAnchor.constraint(equalTo: title.trailingAnchor, constant: 12),
            scroll.trailingAnchor.constraint(equalTo: buttons.leadingAnchor, constant: -8),
            buttons.trailingAnchor.constraint(equalTo: trailingAnchor, constant: -14),
            buttons.centerYAnchor.constraint(equalTo: scroll.centerYAnchor),
            document.topAnchor.constraint(equalTo: scroll.contentView.topAnchor),
            document.bottomAnchor.constraint(equalTo: scroll.contentView.bottomAnchor),
            document.leadingAnchor.constraint(equalTo: scroll.contentView.leadingAnchor),
            tabs.leadingAnchor.constraint(equalTo: document.leadingAnchor),
            tabs.trailingAnchor.constraint(equalTo: document.trailingAnchor),
            tabs.centerYAnchor.constraint(equalTo: document.centerYAnchor),
            holder.topAnchor.constraint(equalTo: scroll.bottomAnchor),
            holder.leadingAnchor.constraint(equalTo: leadingAnchor, constant: 10),
            holder.trailingAnchor.constraint(equalTo: trailingAnchor, constant: -10),
            holder.bottomAnchor.constraint(equalTo: bottomAnchor, constant: -6),
        ])
    }

    required init?(coder: NSCoder) { fatalError() }

    var containsFocus: Bool {
        (window?.firstResponder as? NSView)?.isDescendant(of: self) == true
    }

    func setMaximized(_ maximized: Bool) {
        maximize.image = NSImage(systemSymbolName: maximized ? "chevron.down" : "chevron.up", accessibilityDescription: maximized ? "Restore Panel Size" : "Maximize Panel")
        maximize.toolTip = maximized ? "Restore Panel Size" : "Maximize Panel"
    }

    func show(_ terminals: [Session], active: Session?) {
        tabs.arrangedSubviews.forEach { $0.removeFromSuperview() }
        for (index, session) in terminals.enumerated() {
            let title = session.customName ?? session.foreground ?? (session.shell as NSString).lastPathComponent
            let tab = FileTabView(title: terminals.count > 1 && session.customName == nil ? "\(index + 1): \(title)" : title, image: NSImage(systemSymbolName: "terminal", accessibilityDescription: nil), color: session.needsAttention ? .systemGreen : session.color?.color ?? .secondaryLabelColor, toolTip: session.directory ?? session.project.displayPath, isActive: session === active)
            tab.onSelect = { [weak self] in self?.onSelect?(session) }
            tab.onClose = { [weak self] in self?.onClose?(session) }
            tab.menu = NSMenu()
            tab.menu?.addItem(AppController.colorMenu(for: session, current: session.color))
            tab.menu?.items.forEach { $0.submenu?.items.forEach { $0.target = AppController.shared } }
            tabs.addArrangedSubview(tab)
        }
        guard shown !== active else { return }
        shown?.container.removeFromSuperview()
        shown = active
        guard let active else { return }
        active.container.translatesAutoresizingMaskIntoConstraints = false
        holder.addSubview(active.container)
        NSLayoutConstraint.activate([
            active.container.topAnchor.constraint(equalTo: holder.topAnchor),
            active.container.bottomAnchor.constraint(equalTo: holder.bottomAnchor),
            active.container.leadingAnchor.constraint(equalTo: holder.leadingAnchor),
            active.container.trailingAnchor.constraint(equalTo: holder.trailingAnchor),
        ])
    }

    private func button(_ symbol: String, _ tip: String, _ action: Selector) -> NSButton {
        let button = NSButton()
        configure(button, symbol, tip, action)
        return button
    }

    private func configure(_ button: NSButton, _ symbol: String, _ tip: String, _ action: Selector) {
        button.image = NSImage(systemSymbolName: symbol, accessibilityDescription: tip)
        button.symbolConfiguration = .init(pointSize: 11, weight: .regular)
        button.isBordered = false
        button.contentTintColor = .secondaryLabelColor
        button.toolTip = tip
        button.target = self
        button.action = action
    }

    @objc private func newClicked() { onNew?() }
    @objc private func hideClicked() { onHide?() }
    @objc private func maximizeClicked() { onMaximize?() }
}
