import AppKit

enum ProcessInspector {
    private static let interpreters: Set<String> = ["node", "bun", "deno", "python", "python3", "ruby"]

    static func foregroundPID(ptyFD: Int32) -> pid_t? {
        guard ptyFD >= 0 else { return nil }
        let pgid = tcgetpgrp(ptyFD)
        return pgid > 0 ? pgid : nil
    }

    static func currentDirectory(pid: pid_t) -> String? {
        var info = proc_vnodepathinfo()
        let size = Int32(MemoryLayout<proc_vnodepathinfo>.size)
        guard proc_pidinfo(pid, PROC_PIDVNODEPATHINFO, 0, &info, size) == size else { return nil }
        let path = withUnsafeBytes(of: info.pvi_cdir.vip_path) { String(decoding: $0.prefix { $0 != 0 }, as: UTF8.self) }
        return path.isEmpty ? nil : path
    }

    static func gitBranch(at directory: String) -> String? {
        var dir = URL(fileURLWithPath: directory)
        while true {
            let dotGit = dir.appendingPathComponent(".git")
            var gitDir: URL?
            var isDir: ObjCBool = false
            if FileManager.default.fileExists(atPath: dotGit.path, isDirectory: &isDir) {
                if isDir.boolValue {
                    gitDir = dotGit
                } else if let text = try? String(contentsOf: dotGit, encoding: .utf8), text.hasPrefix("gitdir: ") {
                    let path = text.dropFirst(8).trimmingCharacters(in: .whitespacesAndNewlines)
                    gitDir = URL(fileURLWithPath: path, relativeTo: dir)
                }
            }
            if let gitDir {
                guard let head = try? String(contentsOf: gitDir.appendingPathComponent("HEAD"), encoding: .utf8) else { return nil }
                let trimmed = head.trimmingCharacters(in: .whitespacesAndNewlines)
                return trimmed.hasPrefix("ref: refs/heads/") ? String(trimmed.dropFirst(16)) : String(trimmed.prefix(7))
            }
            let parent = dir.deletingLastPathComponent()
            if parent.path == dir.path { return nil }
            dir = parent
        }
    }

    static func commandName(pid: pid_t) -> String? {
        var mib: [Int32] = [CTL_KERN, KERN_PROCARGS2, pid]
        var size = 0
        guard sysctl(&mib, 3, nil, &size, nil, 0) == 0, size > 4 else { return nil }
        var buffer = [UInt8](repeating: 0, count: size)
        guard sysctl(&mib, 3, &buffer, &size, nil, 0) == 0, size > 4 else { return nil }
        let argc = Int(buffer.withUnsafeBytes { $0.load(as: Int32.self) })
        var index = 4
        while index < size, buffer[index] != 0 { index += 1 }
        while index < size, buffer[index] == 0 { index += 1 }
        var args: [String] = []
        while args.count < min(argc, 2), index < size {
            let start = index
            while index < size, buffer[index] != 0 { index += 1 }
            args.append(String(decoding: buffer[start..<index], as: UTF8.self))
            index += 1
        }
        let names = args.map { name -> String in
            let base = ((name as NSString).lastPathComponent as NSString).deletingPathExtension
            return base.hasPrefix("-") ? String(base.dropFirst()) : base
        }
        guard let first = names.first else { return nil }
        return interpreters.contains(first) && names.count > 1 ? names[1] : first
    }
}

enum AgentBadge {
    private static let colors: [String: UInt32] = [
        "claude": 0xD97757, "codex": 0x10A37F, "gemini": 0x4285F4, "opencode": 0x6E6E73,
        "pi": 0xAF52DE, "amp": 0xFF9F0A, "aider": 0x30B0C7, "cursor-agent": 0x5E5CE6,
        "goose": 0x8E8E93, "crush": 0xFF375F, "qwen": 0x7D5CFF, "copilot": 0x0A84FF, "droid": 0x32D74B,
    ]
    private static let fallback: [UInt32] = [0xFF453A, 0xFF9F0A, 0x32D74B, 0x64D2FF, 0x0A84FF, 0xBF5AF2, 0xFF375F]
    private static var cache: [String: NSImage] = [:]

    private static let appBundleIds: [String: [String]] = [
        "claude": ["com.anthropic.claudefordesktop"],
        "codex": ["com.openai.codex", "com.openai.chat"],
        "cursor-agent": ["com.todesktop.230313mzl4w4u92"],
    ]

    static func image(for agent: String) -> NSImage {
        if let cached = cache[agent] { return cached }
        if let url = appBundleIds[agent]?.lazy.compactMap({ NSWorkspace.shared.urlForApplication(withBundleIdentifier: $0) }).first {
            let icon = NSWorkspace.shared.icon(forFile: url.path)
            icon.size = NSSize(width: 18, height: 18)
            cache[agent] = icon
            return icon
        }
        let hex = colors[agent] ?? fallback[Int(agent.unicodeScalars.reduce(0) { $0 &+ $1.value }) % fallback.count]
        let color = NSColor(srgbRed: CGFloat(hex >> 16 & 0xFF) / 255, green: CGFloat(hex >> 8 & 0xFF) / 255, blue: CGFloat(hex & 0xFF) / 255, alpha: 1)
        let letter = String(agent.prefix(1)).uppercased() as NSString
        let image = NSImage(size: NSSize(width: 18, height: 18), flipped: false) { rect in
            color.setFill()
            NSBezierPath(roundedRect: rect.insetBy(dx: 1, dy: 1), xRadius: 4.5, yRadius: 4.5).fill()
            let attrs: [NSAttributedString.Key: Any] = [.font: NSFont.systemFont(ofSize: 11, weight: .bold), .foregroundColor: NSColor.white]
            let size = letter.size(withAttributes: attrs)
            letter.draw(at: NSPoint(x: rect.midX - size.width / 2, y: rect.midY - size.height / 2), withAttributes: attrs)
            return true
        }
        cache[agent] = image
        return image
    }
}
