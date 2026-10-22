import AppKit
import SwiftTerm

enum SessionKind: String {
    case terminal
    case shell
    case agent
}

enum AgentActivity {
    case idle
    case working
    case waiting
}

final class Session: NSObject, TerminalViewDelegate, LocalProcessDelegate {
    let kind: SessionKind
    let profile: AgentProfile?
    unowned var project: Project
    let id = UUID().uuidString
    let terminal: TerminalView
    private var process: LocalProcess!
    private var isAnchored = false
    private var isAltScreen = false
    let container = NSView()
    private let statusBar = StatusBar()
    private var terminalTitle: String
    var customName: String?
    var color: ItemColor?
    var title: String { customName ?? terminalTitle }
    var directory: String?
    private(set) var foreground: String?
    private(set) var runningAgent: String?
    let shell: String
    private let shellName: String
    var conversationId: String?
    var scrollbackId: String?
    let startedAt = Date()
    private(set) var isRunning = true
    var onExit: ((Session, Int32?) -> Void)?
    var onAttention: ((Session, String?) -> Void)?
    var onOpenFile: ((Session, String, Int?) -> Void)?
    var needsAttention = false {
        didSet { if needsAttention != oldValue { changed() } }
    }
    private(set) var activity = AgentActivity.idle {
        didSet { if activity != oldValue { changed() } }
    }
    private(set) var exitCode: Int32?
    private var reportsStatus = false
    private(set) var lastOutput = Date.distantPast
    private(set) var subagents: [Subagent] = []
    private var subagentsDir: String?
    private var scanningSubagents = false
    private var busySince: Date?

    var subtitle: String {
        if let profile { return profile.name }
        if let foreground { return foreground }
        return directory.map { ($0 as NSString).abbreviatingWithTildeInPath } ?? "terminal"
    }

    init(kind: SessionKind, profile: AgentProfile?, command: String?, conversationId: String?, project: Project, directory: String? = nil, shell: String, font: NSFont, size: NSSize) {
        self.terminal = DropTerminalView(frame: NSRect(origin: .zero, size: size), font: font)
        self.kind = kind
        self.profile = profile
        self.conversationId = conversationId
        self.directory = directory
        self.project = project
        self.terminalTitle = profile?.name ?? (shell as NSString).lastPathComponent
        self.shell = shell
        self.shellName = (shell as NSString).lastPathComponent
        super.init()
        layout()
        terminal.terminalDelegate = self
        process = LocalProcess(delegate: self, dispatchQueue: .main)
        TerminalTheme.forProfile(profile).apply(to: terminal)
        applyTerminalOptions()
        terminal.optionAsMetaKey = true
        let inherited = ProcessInfo.processInfo.environment
        var env = inherited.filter { ["HOME", "USER", "LOGNAME", "TMPDIR", "SSH_AUTH_SOCK", "__CF_USER_TEXT_ENCODING", "LANG"].contains($0.key) }
        env["PATH"] = "/usr/bin:/bin:/usr/sbin:/sbin"
        env["TERM"] = "xterm-256color"
        env["COLORTERM"] = "truecolor"
        env["TERM_PROGRAM"] = "Nucli"
        env["SHELL"] = shell
        env["CLICOLOR"] = "1"
        env["FORCE_HYPERLINK"] = "1"
        env["LANG"] = env["LANG"] ?? "en_US.UTF-8"
        let args = command.map { ["-l", "-i", "-c", $0] } ?? ["-l"]
        process.startProcess(
            executable: shell,
            args: args,
            environment: env.map { "\($0.key)=\($0.value)" },
            currentDirectory: directory ?? project.path
        )
    }

    func applyTerminalOptions() {
        let state = AppState.shared
        terminal.lineSpacing = state.lineSpacing
        let style: CursorStyle = switch (state.cursorShape, state.cursorBlinks) {
        case (.block, true): .blinkBlock
        case (.block, false): .steadyBlock
        case (.bar, true): .blinkBar
        case (.bar, false): .steadyBar
        case (.underline, true): .blinkUnderline
        case (.underline, false): .steadyUnderline
        }
        terminal.setCursorStyle(style)
    }

    private static let restoredMarker = "── restored ──"

    static let scrollbackDirectory = FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask)[0].appendingPathComponent("Nucli/Scrollback")

    func saveScrollback() {
        let lines = String(decoding: terminal.getBufferAsData(kind: .normal), as: UTF8.self).split(separator: "\n", omittingEmptySubsequences: false).filter { $0.trimmingCharacters(in: .whitespaces) != Self.restoredMarker }
        let trimmed = lines.reversed().drop { $0.trimmingCharacters(in: .whitespaces).isEmpty }.reversed().suffix(5000)
        guard !trimmed.isEmpty else { return }
        let id = UUID().uuidString
        try? FileManager.default.createDirectory(at: Self.scrollbackDirectory, withIntermediateDirectories: true)
        guard (try? trimmed.joined(separator: "\n").write(to: Self.scrollbackDirectory.appendingPathComponent(id + ".txt"), atomically: true, encoding: .utf8)) != nil else { return }
        scrollbackId = id
    }

    func restoreScrollback(_ id: String) {
        guard let text = try? String(contentsOf: Self.scrollbackDirectory.appendingPathComponent(id + ".txt"), encoding: .utf8) else { return }
        let body = text.split(separator: "\n", omittingEmptySubsequences: false).joined(separator: "\r\n")
        terminal.feed(text: "\u{1b}[2m" + body + "\u{1b}[0m\r\n\u{1b}[2m\(Self.restoredMarker)\u{1b}[0m\r\n")
    }

    func kill() {
        onExit = nil
        guard isRunning else { return }
        let pid = process.shellPid
        if pid > 0 { Darwin.kill(pid, SIGHUP) }
        process.terminate()
    }

    private func layout() {
        terminal.translatesAutoresizingMaskIntoConstraints = false
        statusBar.translatesAutoresizingMaskIntoConstraints = false
        container.addSubview(terminal)
        container.addSubview(statusBar)
        NSLayoutConstraint.activate([
            terminal.topAnchor.constraint(equalTo: container.topAnchor),
            terminal.leadingAnchor.constraint(equalTo: container.leadingAnchor),
            terminal.trailingAnchor.constraint(equalTo: container.trailingAnchor),
            statusBar.topAnchor.constraint(equalTo: terminal.bottomAnchor, constant: 6),
            statusBar.leadingAnchor.constraint(equalTo: container.leadingAnchor),
            statusBar.trailingAnchor.constraint(equalTo: container.trailingAnchor),
            statusBar.bottomAnchor.constraint(equalTo: container.bottomAnchor),
        ])
        statusBar.directory.text = project.displayPath
        statusBar.directory.onClick = { [weak self] chip in self?.showDirectoryMenu(chip) }
        statusBar.branch.onClick = { [weak self] chip in self?.showBranchMenu(chip) }
        statusBar.diff.onClick = { [weak self] _ in
            guard let self else { return }
            AppController.shared.showChanges(of: self.project)
        }
        NotificationCenter.default.addObserver(forName: .gitChanged, object: nil, queue: .main) { [weak self] note in
            MainActor.assumeIsolated {
                guard let self, note.object as? Project === self.project else { return }
                let status = note.userInfo?["status"] as? GitStatus
                self.statusBar.setDiff(added: status?.changes.reduce(0) { $0 + $1.added } ?? 0, removed: status?.changes.reduce(0) { $0 + $1.removed } ?? 0, files: status?.changes.count ?? 0)
            }
        }
    }

    private var workingDirectory: String { directory ?? project.path }

    private func showDirectoryMenu(_ chip: Chip) {
        let menu = NSMenu()
        let path = workingDirectory
        menu.addItem(MenuAction.item("Reveal in Finder") { NSWorkspace.shared.selectFile(nil, inFileViewerRootedAtPath: path) })
        menu.addItem(MenuAction.item("Copy Path") {
            NSPasteboard.general.clearContents()
            NSPasteboard.general.setString(path, forType: .string)
        })
        chip.popUp(menu)
    }

    private func showBranchMenu(_ chip: Chip) {
        let dir = workingDirectory
        let current = statusBar.branch.text
        let menu = NSMenu()
        for branch in Git.branches(in: dir) {
            let item = MenuAction.item(branch) { [weak self] in self?.runGit(["switch", branch], in: dir) }
            item.state = branch == current ? .on : .off
            menu.addItem(item)
        }
        menu.addItem(.separator())
        menu.addItem(MenuAction.item("New Branch…") { [weak self] in
            guard let name = AppController.shared.ask("New Branch", "Creates the branch from the current commit and switches to it.", value: "", placeholder: "feature/name"), !name.isEmpty else { return }
            self?.runGit(["switch", "-c", name], in: dir)
        })
        chip.popUp(menu)
    }

    private func runGit(_ args: [String], in dir: String) {
        DispatchQueue.global(qos: .userInitiated).async {
            let error = Git.runReportingError(args, in: dir)
            DispatchQueue.main.async { [weak self] in
                guard let self else { return }
                if let error {
                    let alert = NSAlert()
                    alert.messageText = "git \(args.joined(separator: " ")) failed"
                    alert.informativeText = error
                    alert.runModal()
                }
                statusBar.branch.text = ProcessInspector.gitBranch(at: dir)
            }
        }
    }

    func refreshForeground(knownAgents: Set<String>) {
        let pid = isRunning ? ProcessInspector.foregroundPID(ptyFD: process.childfd) : nil
        let name = pid.flatMap(ProcessInspector.commandName)
        let command = name == shellName ? nil : name
        let agent = command.flatMap { knownAgents.contains($0) ? $0 : nil }
        if let cwd = pid.flatMap(ProcessInspector.currentDirectory), cwd != directory {
            directory = cwd
            statusBar.directory.text = (cwd as NSString).abbreviatingWithTildeInPath
            AppState.shared.save()
        }
        statusBar.branch.text = directory.flatMap(ProcessInspector.gitBranch)
        statusBar.process.text = command
        readAgentStatus(pid: agent == "claude" ? pid : nil)
        refreshSubagents()
        checkFinished()
        guard command != foreground || agent != runningAgent else { return }
        foreground = command
        runningAgent = agent
        changed()
    }

    private func trackOutput(_ slice: ArraySlice<UInt8>) {
        let now = Date()
        if busySince == nil || now.timeIntervalSince(lastOutput) > 1.5 { busySince = now }
        lastOutput = now
        if let message = Self.notification(in: slice) { onAttention?(self, message) }
    }

    private var claudeDirectories: [String] {
        if let profile, profile.kind == .claude { return [AgentKind.claude.dataDirectory(for: profile)] }
        let dirs = AppState.shared.profiles.filter { $0.kind == .claude }.map { AgentKind.claude.dataDirectory(for: $0) }
        return Array(Set(dirs + [AgentKind.claude.defaultConfigDirectory.expandingHome]))
    }

    private func readAgentStatus(pid: pid_t?) {
        guard let pid, let status = ConversationHistory.claudeProcess(pid: pid, dataDirs: claudeDirectories) else {
            subagentsDir = nil
            if reportsStatus {
                reportsStatus = false
                activity = .idle
            }
            return
        }
        reportsStatus = true
        subagentsDir = status.subagents
        if kind == .agent, status.sessionId != conversationId {
            conversationId = status.sessionId
            AppState.shared.save()
        }
        let new: AgentActivity = switch status.status {
        case "busy", "shell": .working
        case "waiting": .waiting
        default: .idle
        }
        let old = activity
        activity = new
        if old == .working, new == .idle { onAttention?(self, nil) }
        if old != .waiting, new == .waiting { onAttention?(self, "Waiting for input") }
    }

    private func refreshSubagents() {
        guard let dir = subagentsDir else {
            if !subagents.isEmpty {
                subagents = []
                changed()
            }
            return
        }
        guard !scanningSubagents else { return }
        scanningSubagents = true
        DispatchQueue.global(qos: .utility).async {
            let found = Subagent.running(in: dir)
            DispatchQueue.main.async { [weak self] in
                MainActor.assumeIsolated {
                    guard let self else { return }
                    self.scanningSubagents = false
                    let old = self.subagents
                    self.subagents = found.map { item in
                        let agent = old.first { $0.id == item.id } ?? Subagent(id: item.id, session: self, title: item.title, type: item.type)
                        agent.title = item.title
                        agent.type = item.type
                        return agent
                    }
                    if old.map(\.id) != self.subagents.map(\.id) { self.changed() }
                }
            }
        }
    }

    private func checkFinished() {
        guard !reportsStatus, let busySince else { return }
        if runningAgent != nil, Date().timeIntervalSince(lastOutput) < 1.5, lastOutput.timeIntervalSince(busySince) >= 2 { activity = .working }
        guard Date().timeIntervalSince(lastOutput) > 2 else { return }
        self.busySince = nil
        activity = .idle
        if runningAgent != nil, lastOutput.timeIntervalSince(busySince) >= 5 { onAttention?(self, nil) }
    }

    private static let oscPrefixes = ["\u{1b}]9;", "\u{1b}]777;notify;"].map { Array($0.utf8) }

    private static func notification(in slice: ArraySlice<UInt8>) -> String? {
        for prefix in oscPrefixes {
            guard let range = slice.firstRange(of: prefix) else { continue }
            let rest = slice[range.upperBound...]
            let end = rest.firstIndex { $0 == 0x07 || $0 == 0x1b } ?? rest.endIndex
            let text = String(decoding: rest[..<end], as: UTF8.self)
            if prefix.count == 4, text.hasPrefix("4;") { continue }
            let parts = text.split(separator: ";", omittingEmptySubsequences: false)
            return parts.last.map(String.init) ?? ""
        }
        return nil
    }

    private func changed() {
        NotificationCenter.default.post(name: .sessionChanged, object: self)
    }

    private static let clearScreen: [UInt8] = Array("\u{1b}[2J".utf8)
    private static let cursorToBottom: [UInt8] = Array("\u{1b}[999;1H".utf8)
    private static let altScreenModes = ["\u{1b}[?1049", "\u{1b}[?1047", "\u{1b}[?47"].map { Array($0.utf8) }

    func dataReceived(slice: ArraySlice<UInt8>) {
        trackOutput(slice)
        guard kind != .agent else {
            terminal.feed(byteArray: slice)
            return
        }
        var bytes: [UInt8] = []
        bytes.reserveCapacity(slice.count + 16)
        if !isAnchored {
            isAnchored = true
            bytes += Self.cursorToBottom
        }
        for byte in slice {
            bytes.append(byte)
            switch byte {
            case 0x68, 0x6c:
                let prefix = bytes.dropLast()
                if Self.altScreenModes.contains(where: { prefix.count >= $0.count && Array(prefix.suffix($0.count)) == $0 }) {
                    isAltScreen = byte == 0x68
                }
            case 0x4a:
                if !isAltScreen, runningAgent == nil, bytes.count >= 4, Array(bytes.suffix(4)) == Self.clearScreen {
                    bytes += Self.cursorToBottom
                }
            default:
                break
            }
        }
        terminal.feed(byteArray: bytes[...])
    }

    func getWindowSize() -> winsize {
        let dimensions = terminal.terminalDimensions
        return winsize(ws_row: UInt16(dimensions.rows), ws_col: UInt16(dimensions.cols), ws_xpixel: 0, ws_ypixel: 0)
    }

    func processTerminated(_ source: LocalProcess, exitCode: Int32?) {
        guard isRunning else { return }
        isRunning = false
        self.exitCode = exitCode
        activity = .idle
        changed()
        onExit?(self, exitCode)
    }

    func sizeChanged(source: TerminalView, newCols: Int, newRows: Int) {
        anchorToBottom()
        var size = getWindowSize()
        _ = process.updateWindowSize(&size)
    }

    private func anchorToBottom() {
        guard kind != .agent, isAnchored, !isAltScreen, runningAgent == nil else { return }
        let snapshot = terminal.terminalStateSnapshot()
        let gap = snapshot.dimensions.rows - 1 - snapshot.cursor.row
        guard gap > 0, snapshot.visibleRows.filter({ $0.row > snapshot.cursor.row }).allSatisfy({ $0.text.trimmingCharacters(in: .whitespaces).isEmpty }) else { return }
        terminal.feed(text: "\u{1b}[\(gap)T\u{1b}[\(gap)B")
    }

    func send(source: TerminalView, data: ArraySlice<UInt8>) {
        process.send(data: data)
    }

    func scrolled(source: TerminalView, position: Double) {}

    func requestOpenLink(source: TerminalView, link: String, params: [String: String]) {
        if let url = URL(string: link), let scheme = url.scheme {
            if scheme == "file" { openPath(url.path) } else { NSWorkspace.shared.open(url) }
            return
        }
        openPath(link)
    }

    private func openPath(_ link: String) {
        var path = (link as NSString).expandingTildeInPath
        var line: Int?
        if let range = path.range(of: #":[0-9]+(:[0-9]+)?$"#, options: .regularExpression) {
            line = path[range].split(separator: ":").first.flatMap { Int($0) }
            path = String(path[..<range.lowerBound])
        }
        let relative = [path] + (path.hasPrefix("a/") || path.hasPrefix("b/") ? [String(path.dropFirst(2))] : [])
        let candidates = path.hasPrefix("/") ? [path] : [directory, project.path].compactMap { $0 }.flatMap { base in relative.map { (base as NSString).appendingPathComponent($0) } }
        for candidate in candidates.map({ URL(fileURLWithPath: $0).standardizedFileURL.path }) {
            var isDirectory: ObjCBool = false
            guard FileManager.default.fileExists(atPath: candidate, isDirectory: &isDirectory) else { continue }
            if isDirectory.boolValue {
                NSWorkspace.shared.open(URL(fileURLWithPath: candidate))
            } else {
                onOpenFile?(self, candidate, line)
            }
            return
        }
    }

    func bell(source: TerminalView) {
        onAttention?(self, nil)
    }

    func clipboardCopy(source: TerminalView, content: Data) {
        guard let text = String(data: content, encoding: .utf8) else { return }
        NSPasteboard.general.clearContents()
        NSPasteboard.general.setString(text, forType: .string)
    }

    func clipboardRead(source: TerminalView) -> Data? {
        NSPasteboard.general.string(forType: .string)?.data(using: .utf8)
    }

    func kittyClipboardCapabilities(source: TerminalView) -> KittyClipboardCapabilities { [] }

    func kittyClipboardAvailableMimeTypes(source: TerminalView, location: KittyClipboardLocation) -> [String]? { nil }

    func kittyClipboardRead(source: TerminalView, location: KittyClipboardLocation, mimeType: String) -> KittyClipboardReadResult? { nil }

    func kittyClipboardWrite(source: TerminalView, location: KittyClipboardLocation, content: KittyClipboardWriteContent) -> KittyClipboardWriteResult { .unsupported }

    func kittyClipboardRequestPermission(source: TerminalView, request: KittyClipboardPermissionRequest) -> KittyClipboardPermissionResult { .deny }

    func iTermContent(source: TerminalView, content: ArraySlice<UInt8>) {}

    func rangeChanged(source: TerminalView, startY: Int, endY: Int) {}

    func setTerminalTitle(source: TerminalView, title: String) {
        let trimmed = title.trimmingCharacters(in: .whitespaces)
        guard !trimmed.isEmpty, trimmed != terminalTitle else { return }
        terminalTitle = trimmed
        changed()
    }

    func hostCurrentDirectoryUpdate(source: TerminalView, directory: String?) {
        self.directory = directory.flatMap { URL(string: $0)?.path } ?? directory
        AppState.shared.save()
        changed()
    }
}
