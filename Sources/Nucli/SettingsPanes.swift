import AppKit

@MainActor
private func formGrid(_ rows: [[NSView]]) -> NSGridView {
    let grid = NSGridView(views: rows)
    grid.column(at: 0).xPlacement = .trailing
    grid.rowAlignment = .firstBaseline
    grid.rowSpacing = 12
    grid.columnSpacing = 10
    return grid
}

@MainActor
private func pinned(_ child: NSView, inset: CGFloat = 20) -> NSView {
    let root = NSView()
    child.translatesAutoresizingMaskIntoConstraints = false
    root.addSubview(child)
    NSLayoutConstraint.activate([
        child.topAnchor.constraint(equalTo: root.topAnchor, constant: inset),
        child.leadingAnchor.constraint(equalTo: root.leadingAnchor, constant: inset),
        child.trailingAnchor.constraint(equalTo: root.trailingAnchor, constant: -inset),
        child.bottomAnchor.constraint(lessThanOrEqualTo: root.bottomAnchor, constant: -inset),
    ])
    return root
}

@MainActor
private func note(_ text: String) -> NSTextField {
    let field = NSTextField(wrappingLabelWithString: text)
    field.font = .systemFont(ofSize: NSFont.smallSystemFontSize)
    field.textColor = .secondaryLabelColor
    return field
}

final class GeneralSettingsPane: NSViewController {
    private var state: AppState { .shared }

    override func loadView() {
        title = "General"
        let restore = NSButton(checkboxWithTitle: "Reopen tabs and agent conversations from the last session", target: self, action: #selector(toggleRestore(_:)))
        restore.state = state.restoresTabs ? .on : .off
        let notify = NSButton(checkboxWithTitle: "Notify when an agent finishes or a tab rings in the background", target: self, action: #selector(toggleNotifications(_:)))
        notify.state = state.notifications ? .on : .off
        let grid = formGrid([
            [NSTextField(labelWithString: "Startup:"), restore],
            [NSTextField(labelWithString: "Notifications:"), notify],
        ])
        view = pinned(grid)
        preferredContentSize = NSSize(width: 620, height: 120)
    }

    @objc private func toggleRestore(_ sender: NSButton) {
        state.restoresTabs = sender.state == .on
        state.save()
    }

    @objc private func toggleNotifications(_ sender: NSButton) {
        state.notifications = sender.state == .on
        state.save()
    }
}

final class ShellsSettingsPane: NSViewController, NSTableViewDataSource, NSTableViewDelegate, NSTextFieldDelegate {
    private let table = NSTableView()
    private let status = NSTextField(labelWithString: "")
    private var state: AppState { .shared }

    override func loadView() {
        title = "Shells"
        for (id, title, width) in [("name", "Name", 150.0), ("path", "Path", 380.0)] {
            let column = NSTableColumn(identifier: .init(id))
            column.title = title
            column.width = width
            table.addTableColumn(column)
        }
        table.style = .plain
        table.usesAlternatingRowBackgroundColors = true
        table.dataSource = self
        table.delegate = self
        let scroll = NSScrollView()
        scroll.documentView = table
        scroll.hasVerticalScroller = true
        scroll.borderType = .bezelBorder
        let buttons = NSSegmentedControl(images: [NSImage(named: NSImage.addTemplateName)!, NSImage(named: NSImage.removeTemplateName)!], trackingMode: .momentary, target: self, action: #selector(edit(_:)))
        let detect = NSButton(title: "Detect Shells", target: self, action: #selector(detect))
        detect.controlSize = .small
        status.font = .systemFont(ofSize: NSFont.smallSystemFontSize)
        status.textColor = .secondaryLabelColor
        let stack = NSStackView(views: [
            note("Shells open as tabs in the sidebar, next to agents. Start one with ⇧⌘T, or ⌘T after using it once. The terminal panel (⌃`) opens the shell you used last."),
            scroll,
            NSStackView(views: [buttons, detect, status]),
        ])
        stack.orientation = .vertical
        stack.alignment = .leading
        NSLayoutConstraint.activate([
            scroll.widthAnchor.constraint(equalTo: stack.widthAnchor),
            scroll.heightAnchor.constraint(equalToConstant: 300),
        ])
        view = pinned(stack)
        preferredContentSize = NSSize(width: 620, height: 420)
    }

    @objc private func edit(_ sender: NSSegmentedControl) {
        view.window?.makeFirstResponder(nil)
        if sender.selectedSegment == 0 {
            state.shells.append(ShellProfile(name: "shell", path: ""))
            table.reloadData()
            table.editColumn(1, row: state.shells.count - 1, with: nil, select: true)
        } else if state.shells.indices.contains(table.selectedRow) {
            state.shells.remove(at: table.selectedRow)
            table.reloadData()
        }
        state.save()
    }

    @objc private func detect() {
        let added = ShellProfile.detect(loginShell: state.shell).filter { shell in !state.shells.contains { $0.path == shell.path } }
        state.shells += added
        state.save()
        table.reloadData()
        status.stringValue = added.isEmpty ? "No new shells found" : "Added \(added.count)"
    }

    func numberOfRows(in tableView: NSTableView) -> Int {
        state.shells.count
    }

    func tableView(_ tableView: NSTableView, viewFor tableColumn: NSTableColumn?, row: Int) -> NSView? {
        let shell = state.shells[row]
        let isName = tableColumn?.identifier.rawValue == "name"
        let field = NSTextField(string: isName ? shell.name : shell.path)
        field.isBordered = false
        field.drawsBackground = false
        field.placeholderString = isName ? "Name" : "/bin/zsh"
        if !isName { field.font = .monospacedSystemFont(ofSize: NSFont.systemFontSize, weight: .regular) }
        field.tag = isName ? 0 : 1
        field.delegate = self
        return field
    }

    func controlTextDidChange(_ obj: Notification) {
        guard let field = obj.object as? NSTextField else { return }
        let row = table.row(for: field)
        guard state.shells.indices.contains(row) else { return }
        let value = field.stringValue.trimmingCharacters(in: .whitespaces)
        if field.tag == 0 { state.shells[row].name = value } else { state.shells[row].path = value }
        state.save()
    }
}

final class ThemeSwatch: NSView {
    var theme: TerminalTheme? { didSet { needsDisplay = true } }

    override func draw(_ dirtyRect: NSRect) {
        guard let theme else { return }
        let path = NSBezierPath(roundedRect: bounds, xRadius: 8, yRadius: 8)
        theme.backgroundColor.setFill()
        path.fill()
        NSColor.separatorColor.setStroke()
        path.stroke()
        let text = NSAttributedString(string: "~/project % ls", attributes: [.font: NSFont.monospacedSystemFont(ofSize: 12, weight: .regular), .foregroundColor: theme.foregroundColor])
        text.draw(at: NSPoint(x: 12, y: bounds.height - 26))
        let size: CGFloat = 18
        for (index, color) in theme.ansiColors.enumerated() {
            let row = CGFloat(index / 8)
            let rect = NSRect(x: 12 + CGFloat(index % 8) * (size + 6), y: bounds.height - 56 - row * (size + 6), width: size, height: size)
            color.setFill()
            NSBezierPath(roundedRect: rect, xRadius: 4, yRadius: 4).fill()
        }
    }
}

@MainActor
enum ThemeMenu {
    static func fill(_ popup: NSPopUpButton, selected: String, defaultTitle: String? = nil) {
        popup.removeAllItems()
        func add(_ title: String, _ id: String) {
            popup.addItem(withTitle: title)
            popup.lastItem?.representedObject = id
        }
        if let defaultTitle {
            add(defaultTitle, "")
            popup.menu?.addItem(.separator())
        }
        TerminalTheme.automatic.forEach { add($0.name, $0.id) }
        popup.menu?.addItem(.separator())
        TerminalTheme.builtIn.forEach { add($0.name, $0.id) }
        popup.selectItem(at: max(0, popup.itemArray.firstIndex { $0.representedObject as? String == selected } ?? 0))
    }
}

final class AppearanceSettingsPane: NSViewController, NSTextFieldDelegate {
    private let popup = NSPopUpButton()
    private let themePopup = NSPopUpButton()
    private let swatch = ThemeSwatch()
    private let appearanceNote = NSTextField(wrappingLabelWithString: "")
    private let fontPopup = NSPopUpButton()
    private let sizeField = NSTextField()
    private let sizeStepper = NSStepper()
    private let spacingPopup = NSPopUpButton()
    private let cursorPopup = NSPopUpButton()
    private let blink = NSButton(checkboxWithTitle: "Blink", target: nil, action: nil)
    private let ligatures = NSButton(checkboxWithTitle: "Ligatures", target: nil, action: nil)
    private let textSizePopup = NSPopUpButton()
    private let spacings: [CGFloat] = [1, 1.1, 1.2, 1.3, 1.4, 1.5]
    private var state: AppState { .shared }

    override func loadView() {
        title = "Appearance"
        for mode in AppearanceMode.allCases { popup.addItem(withTitle: mode == .system ? "Automatic (follow macOS)" : mode.title) }
        popup.target = self
        popup.action = #selector(changed)
        themePopup.target = self
        themePopup.action = #selector(themeChanged)
        appearanceNote.font = .systemFont(ofSize: NSFont.smallSystemFontSize)
        appearanceNote.textColor = .secondaryLabelColor
        appearanceNote.preferredMaxLayoutWidth = 360
        swatch.widthAnchor.constraint(equalToConstant: 220).isActive = true
        swatch.heightAnchor.constraint(equalToConstant: 92).isActive = true
        fontPopup.addItem(withTitle: "SF Mono (System)")
        fontPopup.menu?.addItem(.separator())
        let families = NSFontManager.shared.availableFontFamilies.filter { family in
            NSFontManager.shared.availableMembers(ofFontFamily: family)?.contains { ($0[3] as? UInt).map { NSFontTraitMask(rawValue: $0).contains(.fixedPitchFontMask) } ?? false } ?? false
        }
        fontPopup.addItems(withTitles: families.filter { !$0.hasPrefix(".") })
        fontPopup.target = self
        fontPopup.action = #selector(fontChanged)
        sizeField.formatter = NumberFormatter()
        sizeField.delegate = self
        sizeField.widthAnchor.constraint(equalToConstant: 44).isActive = true
        sizeStepper.minValue = 8
        sizeStepper.maxValue = 32
        sizeStepper.target = self
        sizeStepper.action = #selector(stepped)
        ligatures.target = self
        ligatures.action = #selector(terminalOptionsChanged)
        ligatures.toolTip = "Only fonts with programming ligatures (JetBrains Mono, Fira Code, Cascadia Code…) have them"
        let size = NSStackView(views: [sizeField, sizeStepper, NSTextField(labelWithString: "pt")])
        let fontRow = NSStackView(views: [fontPopup, ligatures])
        fontRow.spacing = 12
        size.spacing = 4
        spacings.forEach { spacingPopup.addItem(withTitle: String(format: "%.1f", $0)) }
        spacingPopup.target = self
        spacingPopup.action = #selector(terminalOptionsChanged)
        CursorShape.allCases.forEach { cursorPopup.addItem(withTitle: $0.title) }
        cursorPopup.target = self
        cursorPopup.action = #selector(terminalOptionsChanged)
        blink.target = self
        blink.action = #selector(terminalOptionsChanged)
        let cursor = NSStackView(views: [cursorPopup, blink])
        cursor.spacing = 12
        TextSize.allCases.forEach { textSizePopup.addItem(withTitle: $0.title) }
        textSizePopup.target = self
        textSizePopup.action = #selector(textSizeChanged)
        let grid = formGrid([
            [NSTextField(labelWithString: "Theme:"), themePopup],
            [NSGridCell.emptyContentView, swatch],
            [NSTextField(labelWithString: "Appearance:"), popup],
            [NSGridCell.emptyContentView, appearanceNote],
            [NSTextField(labelWithString: "Font:"), fontRow],
            [NSTextField(labelWithString: "Size:"), size],
            [NSTextField(labelWithString: "Line spacing:"), spacingPopup],
            [NSTextField(labelWithString: "Cursor:"), cursor],
            [NSTextField(labelWithString: "Sidebar text:"), textSizePopup],
            [NSGridCell.emptyContentView, note("Text size of the sidebar and the Files view.")],
        ])
        grid.row(at: 4).topPadding = 8
        view = pinned(grid)
        preferredContentSize = NSSize(width: 620, height: 520)
        reload()
    }

    private func reload() {
        ThemeMenu.fill(themePopup, selected: state.theme)
        swatch.theme = TerminalTheme.resolve(state.theme)
        popup.selectItem(at: AppearanceMode.allCases.firstIndex(of: state.appearance) ?? 0)
        let automatic = TerminalTheme.isAutomatic(state.theme)
        popup.isEnabled = automatic
        appearanceNote.stringValue = automatic ? "Light or dark for the whole app. Automatic themes pick their light or dark variant to match." : "Set by the theme. Choose an automatic theme to follow macOS."
        if let name = state.fontName, let family = NSFont(name: name, size: 12)?.familyName, fontPopup.itemTitles.contains(family) {
            fontPopup.selectItem(withTitle: family)
        } else {
            fontPopup.selectItem(at: 0)
        }
        sizeField.integerValue = Int(state.fontSize)
        sizeStepper.integerValue = Int(state.fontSize)
        spacingPopup.selectItem(at: spacings.firstIndex { abs($0 - state.lineSpacing) < 0.01 } ?? 0)
        cursorPopup.selectItem(at: CursorShape.allCases.firstIndex(of: state.cursorShape) ?? 0)
        blink.state = state.cursorBlinks ? .on : .off
        ligatures.state = state.ligatures ? .on : .off
        textSizePopup.selectItem(at: TextSize.allCases.firstIndex(of: state.sidebarTextSize) ?? 1)
    }

    @objc private func changed() {
        state.appearance = AppearanceMode.allCases[popup.indexOfSelectedItem]
        AppController.shared.applyAppearance()
        reload()
    }

    @objc private func themeChanged() {
        state.theme = themePopup.selectedItem?.representedObject as? String ?? "nucli"
        AppController.shared.applyAppearance()
        reload()
    }

    @objc private func fontChanged() {
        let family = fontPopup.indexOfSelectedItem == 0 ? nil : fontPopup.titleOfSelectedItem
        state.fontName = family.flatMap { NSFontManager.shared.font(withFamily: $0, traits: [], weight: 5, size: state.fontSize)?.fontName }
        AppController.shared.applyFont()
    }

    @objc private func stepped() {
        AppController.shared.setFontSize(CGFloat(sizeStepper.integerValue))
        reload()
    }

    @objc private func terminalOptionsChanged() {
        state.lineSpacing = spacings[safe: spacingPopup.indexOfSelectedItem] ?? 1
        state.cursorShape = CursorShape.allCases[safe: cursorPopup.indexOfSelectedItem] ?? .block
        state.cursorBlinks = blink.state == .on
        state.ligatures = ligatures.state == .on
        AppController.shared.applyFont()
    }

    @objc private func textSizeChanged() {
        state.sidebarTextSize = TextSize.allCases[safe: textSizePopup.indexOfSelectedItem] ?? .medium
        AppController.shared.applyFont()
    }

    func controlTextDidEndEditing(_ obj: Notification) {
        guard sizeField.integerValue > 0 else { return }
        AppController.shared.setFontSize(CGFloat(sizeField.integerValue))
        reload()
    }
}

final class KeybindingsSettingsPane: NSViewController, NSTableViewDataSource, NSTableViewDelegate, NSSearchFieldDelegate {
    private let table = NSTableView()
    private let search = NSSearchField()
    private var rows: [(id: String, title: String, item: NSMenuItem)] = []
    private var recording: String?
    private var monitor: Any?
    private var state: AppState { .shared }

    override func loadView() {
        title = "Keybindings"
        for (id, title, width) in [("command", "Command", 300.0), ("shortcut", "Shortcut", 160.0)] {
            let column = NSTableColumn(identifier: .init(id))
            column.title = title
            column.width = width
            table.addTableColumn(column)
        }
        table.style = .inset
        table.usesAlternatingRowBackgroundColors = true
        table.dataSource = self
        table.delegate = self
        table.target = self
        table.doubleAction = #selector(startRecording)
        let scroll = NSScrollView()
        scroll.documentView = table
        scroll.hasVerticalScroller = true
        scroll.borderType = .bezelBorder
        search.placeholderString = "Search commands or shortcuts"
        search.delegate = self
        let reset = NSButton(title: "Reset Selected", target: self, action: #selector(resetSelected))
        let resetAll = NSButton(title: "Reset All", target: self, action: #selector(resetAll))
        let buttons = NSStackView(views: [note("Double-click a shortcut and press the new keys. ⌫ removes it, Esc cancels."), NSView(), reset, resetAll])
        let stack = NSStackView(views: [search, scroll, buttons])
        stack.orientation = .vertical
        stack.alignment = .leading
        stack.spacing = 10
        let root = pinned(stack)
        NSLayoutConstraint.activate([
            scroll.widthAnchor.constraint(equalTo: stack.widthAnchor),
            search.widthAnchor.constraint(equalTo: stack.widthAnchor),
            buttons.widthAnchor.constraint(equalTo: stack.widthAnchor),
            scroll.heightAnchor.constraint(greaterThanOrEqualToConstant: 360),
            stack.bottomAnchor.constraint(equalTo: root.bottomAnchor, constant: -20),
        ])
        view = root
        preferredContentSize = NSSize(width: 620, height: 520)
        reload()
    }

    override func viewWillDisappear() {
        super.viewWillDisappear()
        stopRecording()
    }

    func reload() {
        guard let menu = NSApp.mainMenu else { return }
        let query = search.stringValue.trimmingCharacters(in: .whitespaces)
        rows = MainMenu.commands(in: menu).compactMap { id, item in
            let title = id.split(separator: "/").joined(separator: " › ").replacingOccurrences(of: "…", with: "")
            guard query.isEmpty || title.localizedCaseInsensitiveContains(query) || shortcut(of: item).display.contains(query.uppercased()) else { return nil }
            return (id, title, item)
        }
        table.reloadData()
    }

    private func shortcut(of item: NSMenuItem) -> Shortcut {
        Shortcut(key: item.keyEquivalent, modifiers: item.keyEquivalentModifierMask)
    }

    func controlTextDidChange(_ obj: Notification) {
        reload()
    }

    func numberOfRows(in tableView: NSTableView) -> Int {
        rows.count
    }

    func tableView(_ tableView: NSTableView, viewFor tableColumn: NSTableColumn?, row: Int) -> NSView? {
        let entry = rows[row]
        if tableColumn?.identifier.rawValue == "command" {
            let label = NSTextField(labelWithString: entry.title)
            label.lineBreakMode = .byTruncatingMiddle
            label.textColor = state.keyBindings[entry.id] == nil ? .labelColor : .controlAccentColor
            return label
        }
        if recording == entry.id {
            let label = NSTextField(labelWithString: "Press keys…")
            label.textColor = .controlAccentColor
            return label
        }
        let current = shortcut(of: entry.item)
        let label = NSTextField(labelWithString: current.key.isEmpty ? "—" : current.display)
        label.font = .systemFont(ofSize: NSFont.systemFontSize, weight: .medium)
        let conflicts = !current.key.isEmpty && rows.contains { $0.id != entry.id && shortcut(of: $0.item) == current }
        label.textColor = conflicts ? .systemRed : current.key.isEmpty ? .tertiaryLabelColor : .labelColor
        label.toolTip = conflicts ? "Used by another command" : nil
        return label
    }

    @objc private func startRecording() {
        guard let entry = rows[safe: table.clickedRow] else { return }
        stopRecording()
        recording = entry.id
        table.reloadData()
        monitor = NSEvent.addLocalMonitorForEvents(matching: .keyDown) { [weak self] event in
            guard let self, event.window === view.window else { return event }
            record(event)
            return nil
        }
    }

    private func record(_ event: NSEvent) {
        guard let id = recording else { return }
        let modifiers = event.modifierFlags.intersection([.command, .shift, .option, .control])
        switch event.keyCode {
        case 53 where modifiers.isEmpty:
            break
        case 51 where modifiers.isEmpty, 117 where modifiers.isEmpty:
            set(id, Shortcut(key: "", modifiers: []))
        default:
            guard let key = event.characters(byApplyingModifiers: [])?.lowercased(), !key.isEmpty else { return }
            let isFunctionKey = key.unicodeScalars.first.map { $0.value >= 0xF700 && $0.value <= 0xF8FF } ?? false
            guard !modifiers.isEmpty || isFunctionKey else { return }
            set(id, Shortcut(key: key, modifiers: modifiers))
        }
        stopRecording()
    }

    private func set(_ id: String, _ shortcut: Shortcut?) {
        if let shortcut, shortcut != MainMenu.defaults[id] {
            state.keyBindings[id] = shortcut.dictionary
        } else {
            state.keyBindings.removeValue(forKey: id)
        }
        AppController.shared.applyKeyBindings()
    }

    private func stopRecording() {
        if let monitor { NSEvent.removeMonitor(monitor) }
        monitor = nil
        recording = nil
        reload()
    }

    @objc private func resetSelected() {
        guard let entry = rows[safe: table.selectedRow] else { return }
        set(entry.id, nil)
        reload()
    }

    @objc private func resetAll() {
        state.keyBindings = [:]
        AppController.shared.applyKeyBindings()
        reload()
    }
}
