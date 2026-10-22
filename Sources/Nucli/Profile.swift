import Foundation

enum AgentKind: String, CaseIterable {
    case claude
    case codex
    case pi

    var displayName: String {
        switch self {
        case .claude: "Claude Code"
        case .codex: "Codex"
        case .pi: "pi"
        }
    }

    var configVariable: String {
        switch self {
        case .claude: "CLAUDE_CONFIG_DIR"
        case .codex: "CODEX_HOME"
        case .pi: "PI_CODING_AGENT_DIR"
        }
    }

    var defaultConfigDirectory: String {
        switch self {
        case .claude: "~/.claude"
        case .codex: "~/.codex"
        case .pi: "~/.pi/agent"
        }
    }

    func dataDirectory(for profile: AgentProfile) -> String {
        let configured = profile.configDirectory.isEmpty
            ? profile.environment.first { $0.key == configVariable }?.value ?? defaultConfigDirectory
            : profile.configDirectory
        return configured.expandingHome
    }

    var assignsConversationId: Bool { self != .codex }

    func newArguments(conversationId: String?) -> [String] {
        guard assignsConversationId, let conversationId else { return [] }
        return ["--session-id", conversationId]
    }

    func resumeArguments(conversationId: String) -> [String] {
        switch self {
        case .claude: ["--resume", conversationId]
        case .codex: ["resume", conversationId]
        case .pi: ["--session-id", conversationId]
        }
    }
}

struct EnvironmentVariable: Equatable {
    var key: String
    var value: String
}

struct AgentProfile: Equatable {
    var name: String
    var executable: String
    var arguments = ""
    var configDirectory = ""
    var environment: [EnvironmentVariable] = []
    var theme = ""

    var executableName: String { (executable as NSString).lastPathComponent }
    var kind: AgentKind? { AgentKind(rawValue: executableName) }
    var isLaunchable: Bool { !executable.trimmingCharacters(in: .whitespaces).isEmpty }

    var summary: String {
        if let kind, !configDirectory.isEmpty { return "\(kind.displayName) · \(configDirectory)" }
        return ([executable] + (arguments.isEmpty ? [] : [arguments])).joined(separator: " ")
    }

    func launchCommand(extraArguments: [String] = []) -> String {
        var variables = environment.filter { !$0.key.isEmpty }
        if let kind, !configDirectory.isEmpty {
            variables.removeAll { $0.key == kind.configVariable }
            variables.append(EnvironmentVariable(key: kind.configVariable, value: configDirectory))
        }
        let assignments = variables.map { "\($0.key)=\($0.value.expandingHome.shellQuoted)" }
        let program = [executable.expandingHome.shellQuoted] + (arguments.isEmpty ? [] : [arguments]) + extraArguments.map(\.shellQuoted)
        return (assignments + ["exec"] + program).joined(separator: " ")
    }

    init(name: String, executable: String, arguments: String = "", configDirectory: String = "", environment: [EnvironmentVariable] = []) {
        self.name = name
        self.executable = executable
        self.arguments = arguments
        self.configDirectory = configDirectory
        self.environment = environment
    }

    init?(dictionary: [String: Any]) {
        guard let name = dictionary["name"] as? String else { return nil }
        if let executable = dictionary["executable"] as? String {
            self.init(
                name: name,
                executable: executable,
                arguments: dictionary["arguments"] as? String ?? "",
                configDirectory: dictionary["configDirectory"] as? String ?? "",
                environment: (dictionary["environment"] as? [[String]] ?? []).compactMap { pair in
                    pair.count == 2 ? EnvironmentVariable(key: pair[0], value: pair[1]) : nil
                }
            )
            theme = dictionary["theme"] as? String ?? ""
        } else if let command = dictionary["command"] as? String {
            let parts = CommandLineParts(command)
            let kind = AgentKind(rawValue: (parts.executable as NSString).lastPathComponent)
            var environment = parts.environment
            let configDirectory = kind.flatMap { kind in environment.firstIndex { $0.key == kind.configVariable }.map { environment.remove(at: $0).value } }
                .map { ($0.expandingHome as NSString).abbreviatingWithTildeInPath } ?? ""
            self.init(name: name, executable: parts.executable, arguments: parts.arguments, configDirectory: configDirectory, environment: environment)
            theme = dictionary["theme"] as? String ?? ""
        } else {
            return nil
        }
    }

    var dictionary: [String: Any] {
        [
            "name": name,
            "executable": executable,
            "arguments": arguments,
            "configDirectory": configDirectory,
            "environment": environment.map { [$0.key, $0.value] },
            "theme": theme,
        ]
    }
}

struct CommandLineParts {
    private static let wrappers: Set<String> = ["env", "command", "exec", "builtin", "nohup", "noglob", "time"]

    let environment: [EnvironmentVariable]
    let executable: String
    let arguments: String

    init(_ command: String) {
        var environment: [EnvironmentVariable] = []
        var executable = ""
        var arguments: [String] = []
        for word in Self.words(command) {
            if !executable.isEmpty {
                arguments.append(word.contains(" ") ? word.shellQuoted : word)
            } else if let eq = word.firstIndex(of: "="), !word.hasPrefix("-") {
                environment.append(EnvironmentVariable(key: String(word[..<eq]), value: String(word[word.index(after: eq)...])))
            } else if !Self.wrappers.contains(word) {
                executable = word
            }
        }
        self.environment = environment
        self.executable = executable
        self.arguments = arguments.joined(separator: " ")
    }

    private static func words(_ command: String) -> [String] {
        var words: [String] = []
        var current = ""
        var quote: Character?
        var hasWord = false
        for char in command {
            if let open = quote {
                if char == open { quote = nil } else { current.append(char) }
            } else if char == "\"" || char == "'" {
                quote = char
                hasWord = true
            } else if char == " " || char == "\t" {
                if hasWord || !current.isEmpty { words.append(current) }
                current = ""
                hasWord = false
            } else {
                current.append(char)
            }
        }
        if hasWord || !current.isEmpty { words.append(current) }
        return words
    }
}

extension String {
    var expandingHome: String {
        let home = NSHomeDirectory()
        let expanded = replacingOccurrences(of: "${HOME}", with: home).replacingOccurrences(of: "$HOME", with: home)
        return (expanded as NSString).expandingTildeInPath
    }

    var shellQuoted: String {
        "'" + replacingOccurrences(of: "'", with: "'\\''") + "'"
    }
}

struct ShellProfile: Equatable {
    var name: String
    var path: String

    var isLaunchable: Bool { FileManager.default.isExecutableFile(atPath: path.expandingHome) }

    init(name: String, path: String) {
        self.name = name
        self.path = path
    }

    init?(dictionary: [String: Any]) {
        guard let name = dictionary["name"] as? String, let path = dictionary["path"] as? String else { return nil }
        self.init(name: name, path: path)
    }

    var dictionary: [String: Any] { ["name": name, "path": path] }

    static func detect(loginShell: String) -> [ShellProfile] {
        let listed = (try? String(contentsOfFile: "/etc/shells", encoding: .utf8))?
            .split(separator: "\n")
            .map { $0.trimmingCharacters(in: .whitespaces) }
            .filter { $0.hasPrefix("/") } ?? []
        var shells: [ShellProfile] = []
        for path in [loginShell] + listed where FileManager.default.isExecutableFile(atPath: path) && !shells.contains(where: { $0.path == path }) {
            let name = (path as NSString).lastPathComponent
            shells.append(ShellProfile(name: shells.contains { $0.name == name } ? path : name, path: path))
        }
        return shells
    }
}
