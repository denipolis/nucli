import Foundation

enum AgentDetector {
    static let knownCommands = ["claude", "codex", "gemini", "opencode", "pi", "amp", "aider", "cursor-agent", "goose", "crush", "qwen", "copilot", "droid"]

    static func detect(shell: String) -> [AgentProfile] {
        let dirs = searchDirectories(shell: shell)
        let shellPath = Set(loginPath(shell: shell))
        return knownCommands.flatMap { name -> [AgentProfile] in
            guard let dir = dirs.first(where: { FileManager.default.isExecutableFile(atPath: $0 + "/" + name) }) else { return [] }
            let command = shellPath.contains(dir) ? name : dir + "/" + name
            return variants(of: name, command: command)
        }
    }

    private static let configVariants: [String: (variable: String, markers: [String])] = [
        "claude": ("CLAUDE_CONFIG_DIR", ["projects", "history.jsonl", "settings.json", ".credentials.json"]),
        "codex": ("CODEX_HOME", ["sessions", "config.toml", "auth.json"]),
    ]

    private static let configMarkers: [AgentKind: [String]] = [
        .claude: ["projects", "history.jsonl", "settings.json", ".credentials.json"],
        .codex: ["sessions", "config.toml", "auth.json"],
    ]

    private static func variants(of name: String, command: String) -> [AgentProfile] {
        guard let kind = AgentKind(rawValue: name), let markers = configMarkers[kind] else {
            return [AgentProfile(name: name, executable: command)]
        }
        let home = NSHomeDirectory()
        let entries = (try? FileManager.default.contentsOfDirectory(atPath: home)) ?? []
        let dirs = entries.filter { $0 == "." + name || $0.hasPrefix("." + name + "-") }.sorted().filter { entry in
            var isDir: ObjCBool = false
            return FileManager.default.fileExists(atPath: home + "/" + entry, isDirectory: &isDir) && isDir.boolValue
                && markers.contains { FileManager.default.fileExists(atPath: home + "/" + entry + "/" + $0) }
        }
        let extra = dirs.filter { $0 != "." + name }.map { entry in
            AgentProfile(name: String(entry.dropFirst()), executable: command, configDirectory: "~/" + entry)
        }
        let plain = AgentProfile(name: name, executable: command)
        return dirs.contains("." + name) || extra.isEmpty ? [plain] + extra : extra
    }

    private static func searchDirectories(shell: String) -> [String] {
        let home = NSHomeDirectory()
        let nvm = home + "/.nvm/versions/node"
        let nodeBins = ((try? FileManager.default.contentsOfDirectory(atPath: nvm)) ?? []).sorted(by: >).map { "\(nvm)/\($0)/bin" }
        let known = [
            "/opt/homebrew/bin", "/usr/local/bin",
            home + "/.local/bin", home + "/.claude/local", home + "/.bun/bin", home + "/.npm-global/bin",
            home + "/.volta/bin", home + "/Library/pnpm", home + "/.deno/bin", home + "/.cargo/bin",
            home + "/go/bin", home + "/.opencode/bin", home + "/.amp/bin",
        ] + nodeBins
        var seen = Set<String>()
        return (loginPath(shell: shell) + known).filter { seen.insert($0).inserted }
    }

    private static func loginPath(shell: String) -> [String] {
        let process = Process()
        process.executableURL = URL(fileURLWithPath: shell)
        process.arguments = ["-l", "-c", "printf %s \"$PATH\""]
        process.environment = ["HOME": NSHomeDirectory(), "USER": NSUserName(), "PATH": "/usr/bin:/bin:/usr/sbin:/sbin"]
        let out = Pipe()
        process.standardOutput = out
        process.standardError = FileHandle.nullDevice
        process.standardInput = FileHandle.nullDevice
        guard (try? process.run()) != nil else { return [] }
        let data = out.fileHandleForReading.readDataToEndOfFile()
        process.waitUntilExit()
        return String(decoding: data, as: UTF8.self).split(separator: ":").map(String.init)
    }
}
