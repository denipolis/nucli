import AppKit

final class SettingsWindowController: NSWindowController, NSTableViewDataSource, NSTableViewDelegate, NSTextFieldDelegate {
    private let profileTable = NSTableView()
    private let environmentTable = NSTableView()
    private let nameField = NSTextField()
    private let executableField = NSTextField()
    private let argumentsField = NSTextField()
    private let configField = NSTextField()
    private let themePopup = NSPopUpButton()
    private let configRow = NSStackView()
    private var configHint = NSTextField()
    private let preview = NSTextField(wrappingLabelWithString: "")
    private let status = NSTextField(labelWithString: "")
    private let form = NSGridView()
    private var state: AppState { .shared }

    private var selectedIndex: Int? {
        state.profiles.indices.contains(profileTable.selectedRow) ? profileTable.selectedRow : nil
    }

    convenience init() {
        let window = NSWindow(contentRect: NSRect(x: 0, y: 0, width: 760, height: 520), styleMask: [.titled, .closable], backing: .buffered, defer: true)
        window.title = "Settings"
        self.init(window: window)
        let tabs = NSTabViewController()
        tabs.tabStyle = .toolbar
        let agents = NSViewController()
        agents.title = "Agents"
        agents.view = build()
        agents.preferredContentSize = NSSize(width: 620, height: 500)
        let keybindings = KeybindingsSettingsPane()
        self.keybindings = keybindings
        for (controller, symbol) in [(GeneralSettingsPane(), "gearshape"), (agents, "sparkles"), (ShellsSettingsPane(), "terminal"), (keybindings, "keyboard"), (AppearanceSettingsPane(), "circle.lefthalf.filled")] as [(NSViewController, String)] {
            _ = controller.view
            let item = NSTabViewItem(viewController: controller)
            item.label = controller.title ?? ""
            item.image = NSImage(systemSymbolName: symbol, accessibilityDescription: item.label)
            tabs.addTabViewItem(item)
        }
        window.contentViewController = tabs
        reload()
    }

    private var keybindings: KeybindingsSettingsPane?

    private func build() -> NSView {
        profileTable.addTableColumn(NSTableColumn(identifier: .init("name")))
        profileTable.headerView = nil
        profileTable.style = .sourceList
        profileTable.rowHeight = 28
        profileTable.dataSource = self
        profileTable.delegate = self
        let profileScroll = scroll(profileTable)
        let listButtons = NSSegmentedControl(images: [NSImage(named: NSImage.addTemplateName)!, NSImage(named: NSImage.removeTemplateName)!], trackingMode: .momentary, target: self, action: #selector(editProfiles(_:)))
        let duplicate = NSButton(title: "Duplicate", target: self, action: #selector(duplicateProfile))
        duplicate.controlSize = .small
        let detect = NSButton(title: "Detect Agents", target: self, action: #selector(detectAgents))
        detect.controlSize = .small
        status.font = .systemFont(ofSize: NSFont.smallSystemFontSize)
        status.textColor = .secondaryLabelColor
        let list = NSStackView(views: [profileScroll, NSStackView(views: [listButtons, duplicate]), detect, status])
        list.orientation = .vertical
        list.alignment = .leading
        NSLayoutConstraint.activate([
            list.widthAnchor.constraint(equalToConstant: 150),
            profileScroll.widthAnchor.constraint(equalTo: list.widthAnchor),
        ])

        for field in [nameField, executableField, argumentsField, configField] {
            field.delegate = self
        }
        executableField.placeholderString = "claude"
        argumentsField.placeholderString = "--model opus"
        executableField.font = .monospacedSystemFont(ofSize: NSFont.systemFontSize, weight: .regular)
        argumentsField.font = executableField.font
        configField.font = executableField.font

        let configButton = NSButton(title: "Choose…", target: self, action: #selector(chooseConfigDirectory))
        configRow.setViews([configField, configButton], in: .leading)
        configHint = hint("")
        themePopup.target = self
        themePopup.action = #selector(themeChanged)

        for (id, title, width) in [("key", "Variable", 110.0), ("value", "Value", 150.0)] {
            let column = NSTableColumn(identifier: .init(id))
            column.title = title
            column.width = width
            environmentTable.addTableColumn(column)
        }
        environmentTable.style = .plain
        environmentTable.usesAlternatingRowBackgroundColors = true
        environmentTable.dataSource = self
        environmentTable.delegate = self
        let environmentScroll = scroll(environmentTable)
        environmentScroll.borderType = .bezelBorder
        let environmentButtons = NSSegmentedControl(images: [NSImage(named: NSImage.addTemplateName)!, NSImage(named: NSImage.removeTemplateName)!], trackingMode: .momentary, target: self, action: #selector(editEnvironment(_:)))

        preview.font = .monospacedSystemFont(ofSize: NSFont.smallSystemFontSize, weight: .regular)
        preview.textColor = .secondaryLabelColor
        preview.isSelectable = true

        let fieldWidth: CGFloat = 310
        configHint.preferredMaxLayoutWidth = fieldWidth
        preview.preferredMaxLayoutWidth = fieldWidth
        form.addRow(with: [label("Name:"), nameField])
        form.addRow(with: [label("Executable:"), executableField])
        form.addRow(with: [label("Arguments:"), argumentsField])
        form.addRow(with: [label("Config folder:"), configRow])
        form.addRow(with: [NSGridCell.emptyContentView, configHint])
        form.addRow(with: [label("Theme:"), themePopup])
        form.addRow(with: [label("Environment:"), environmentScroll])
        form.addRow(with: [NSGridCell.emptyContentView, environmentButtons])
        form.addRow(with: [label("Launch:"), preview])
        form.column(at: 0).xPlacement = .trailing
        form.rowAlignment = .firstBaseline
        form.rowSpacing = 10
        form.columnSpacing = 8
        form.row(at: 4).topPadding = -4
        form.row(at: 6).rowAlignment = .none
        form.row(at: 6).yPlacement = .top
        form.row(at: 7).topPadding = -4
        form.setContentHuggingPriority(.required, for: .vertical)

        let columns = NSStackView(views: [list, form])
        columns.alignment = .top
        columns.spacing = 20
        let root = NSStackView(views: [columns])
        root.orientation = .vertical
        root.alignment = .leading
        root.spacing = 16
        root.edgeInsets = NSEdgeInsets(top: 20, left: 20, bottom: 20, right: 20)
        root.translatesAutoresizingMaskIntoConstraints = false
        let content = NSView()
        content.addSubview(root)
        NSLayoutConstraint.activate([
            root.topAnchor.constraint(equalTo: content.topAnchor),
            root.bottomAnchor.constraint(equalTo: content.bottomAnchor),
            root.leadingAnchor.constraint(equalTo: content.leadingAnchor),
            root.trailingAnchor.constraint(equalTo: content.trailingAnchor),
            columns.widthAnchor.constraint(equalTo: root.widthAnchor, constant: -40),
            columns.heightAnchor.constraint(greaterThanOrEqualToConstant: 380),
            profileScroll.heightAnchor.constraint(greaterThanOrEqualToConstant: 260),
            nameField.widthAnchor.constraint(equalToConstant: fieldWidth),
            executableField.widthAnchor.constraint(equalTo: nameField.widthAnchor),
            argumentsField.widthAnchor.constraint(equalTo: nameField.widthAnchor),
            configRow.widthAnchor.constraint(equalTo: nameField.widthAnchor),
            configHint.widthAnchor.constraint(equalTo: nameField.widthAnchor),
            environmentScroll.widthAnchor.constraint(equalTo: nameField.widthAnchor),
            environmentScroll.heightAnchor.constraint(equalToConstant: 120),
            preview.widthAnchor.constraint(equalTo: nameField.widthAnchor),
        ])
        return content
    }

    private func label(_ text: String) -> NSTextField {
        NSTextField(labelWithString: text)
    }

    private func hint(_ text: String) -> NSTextField {
        let field = NSTextField(wrappingLabelWithString: text)
        field.font = .systemFont(ofSize: NSFont.smallSystemFontSize)
        field.textColor = .secondaryLabelColor
        return field
    }

    private func scroll(_ table: NSTableView) -> NSScrollView {
        let scroll = NSScrollView()
        scroll.documentView = table
        scroll.hasVerticalScroller = true
        return scroll
    }

    func reload() {
        let selected = profileTable.selectedRow
        profileTable.reloadData()
        keybindings?.reload()
        let row = state.profiles.indices.contains(selected) ? selected : (state.profiles.isEmpty ? -1 : 0)
        if row >= 0 { profileTable.selectRowIndexes([row], byExtendingSelection: false) }
        showSelectedProfile()
    }

    private func showSelectedProfile() {
        guard let index = selectedIndex else {
            form.isHidden = true
            return
        }
        form.isHidden = false
        let profile = state.profiles[index]
        nameField.stringValue = profile.name
        executableField.stringValue = profile.executable
        argumentsField.stringValue = profile.arguments
        configField.stringValue = profile.configDirectory
        environmentTable.reloadData()
        ThemeMenu.fill(themePopup, selected: profile.theme, defaultTitle: "Same as the app")
        updateDerived(profile)
    }

    private func updateDerived(_ profile: AgentProfile) {
        form.row(at: 3).isHidden = profile.kind == nil
        configHint.stringValue = profile.kind.map { "Detected \($0.displayName). Sets \($0.configVariable): where \($0.displayName) keeps its account, settings and conversation history. Nucli reads history from here." } ?? "Nucli doesn't know this agent yet. To use a custom folder, set the agent's own variable in Environment variables below."
        configField.placeholderString = profile.kind.map { "\($0.defaultConfigDirectory) (default)" }
        preview.stringValue = profile.isLaunchable ? profile.launchCommand() : "Set an executable to use this profile."
    }

    private func update(_ change: (inout AgentProfile) -> Void) {
        guard let index = selectedIndex else { return }
        change(&state.profiles[index])
        state.save()
        updateDerived(state.profiles[index])
    }

    @objc private func formChanged() {
        update { profile in
            profile.executable = executableField.stringValue.trimmingCharacters(in: .whitespaces)
            profile.arguments = argumentsField.stringValue.trimmingCharacters(in: .whitespaces)
            profile.configDirectory = configField.stringValue.trimmingCharacters(in: .whitespaces)
            let name = nameField.stringValue.trimmingCharacters(in: .whitespaces)
            if !name.isEmpty { profile.name = name }
        }
        if let index = selectedIndex {
            profileTable.reloadData(forRowIndexes: [index], columnIndexes: [0])
        }
    }

    @objc private func themeChanged() {
        update { $0.theme = themePopup.selectedItem?.representedObject as? String ?? "" }
        AppController.shared.applyTheme()
    }

    @objc private func chooseConfigDirectory() {
        guard let window else { return }
        let panel = NSOpenPanel()
        panel.canChooseFiles = false
        panel.canChooseDirectories = true
        panel.showsHiddenFiles = true
        panel.directoryURL = URL(fileURLWithPath: NSHomeDirectory())
        panel.beginSheetModal(for: window) { [weak self] response in
            guard let self, response == .OK, let url = panel.url else { return }
            configField.stringValue = (url.path as NSString).abbreviatingWithTildeInPath
            formChanged()
        }
    }

    private func add(_ profile: AgentProfile) {
        state.profiles.append(profile)
        state.save()
        profileTable.reloadData()
        profileTable.selectRowIndexes([state.profiles.count - 1], byExtendingSelection: false)
        showSelectedProfile()
        window?.makeFirstResponder(nameField)
    }

    private func uniqueName(_ base: String) -> String {
        let names = Set(state.profiles.map(\.name))
        guard names.contains(base) else { return base }
        return (2...).lazy.map { "\(base)-\($0)" }.first { !names.contains($0) }!
    }

    @objc private func duplicateProfile() {
        guard let index = selectedIndex else { return }
        var copy = state.profiles[index]
        copy.name = uniqueName(copy.name)
        add(copy)
    }

    @objc private func detectAgents() {
        status.stringValue = "Searching…"
        state.detectAgents { [weak self] added in
            self?.status.stringValue = added == 0 ? "No new agents found" : "Added \(added)"
            self?.reload()
        }
    }

    @objc private func editProfiles(_ sender: NSSegmentedControl) {
        window?.makeFirstResponder(nil)
        if sender.selectedSegment == 0 {
            add(AgentProfile(name: uniqueName("agent"), executable: "claude"))
        } else if let index = selectedIndex {
            state.profiles.remove(at: index)
            state.save()
            reload()
        }
    }

    @objc private func editEnvironment(_ sender: NSSegmentedControl) {
        window?.makeFirstResponder(nil)
        if sender.selectedSegment == 0 {
            update { $0.environment.append(EnvironmentVariable(key: "", value: "")) }
            environmentTable.reloadData()
            if let count = selectedIndex.map({ state.profiles[$0].environment.count }) {
                environmentTable.editColumn(0, row: count - 1, with: nil, select: true)
            }
        } else {
            let row = environmentTable.selectedRow
            update { if $0.environment.indices.contains(row) { $0.environment.remove(at: row) } }
            environmentTable.reloadData()
        }
    }

    func numberOfRows(in tableView: NSTableView) -> Int {
        if tableView === profileTable { return state.profiles.count }
        return selectedIndex.map { state.profiles[$0].environment.count } ?? 0
    }

    func tableView(_ tableView: NSTableView, viewFor tableColumn: NSTableColumn?, row: Int) -> NSView? {
        if tableView === profileTable {
            let profile = state.profiles[row]
            let badge = NSImageView(image: AgentBadge.image(for: profile.kind?.rawValue ?? profile.executableName))
            let name = NSTextField(labelWithString: profile.name)
            name.lineBreakMode = .byTruncatingTail
            name.setContentCompressionResistancePriority(.defaultLow, for: .horizontal)
            let stack = NSStackView(views: [badge, name])
            stack.spacing = 6
            return stack
        }
        guard let index = selectedIndex else { return nil }
        let variable = state.profiles[index].environment[row]
        let isKey = tableColumn?.identifier.rawValue == "key"
        let field = NSTextField(string: isKey ? variable.key : variable.value)
        field.isBordered = false
        field.drawsBackground = false
        field.font = .monospacedSystemFont(ofSize: NSFont.systemFontSize, weight: .regular)
        field.placeholderString = isKey ? "NAME" : "value"
        field.tag = isKey ? 0 : 1
        field.delegate = self
        return field
    }

    func tableViewSelectionDidChange(_ notification: Notification) {
        guard notification.object as? NSTableView === profileTable else { return }
        showSelectedProfile()
    }

    func controlTextDidChange(_ obj: Notification) {
        guard let field = obj.object as? NSTextField else { return }
        if [nameField, executableField, argumentsField, configField].contains(field) {
            formChanged()
        } else {
            let row = environmentTable.row(for: field)
            let value = field.stringValue
            let isKey = field.tag == 0
            update { profile in
                guard profile.environment.indices.contains(row) else { return }
                if isKey { profile.environment[row].key = value.trimmingCharacters(in: .whitespaces) } else { profile.environment[row].value = value }
            }
        }
    }
}
