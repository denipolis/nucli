import Foundation

struct Conversation {
    let id: String
    let title: String
    let date: Date
    let kind: AgentKind
    let profile: AgentProfile
}

struct HistoryEntry {
    let conversation: Conversation
    let cwd: String
}

enum ConversationHistory {
    static func list(cwd: String, profiles: [AgentProfile], limit: Int = 15) -> [Conversation] {
        var seenDirs = Set<String>()
        var result: [Conversation] = []
        for profile in profiles {
            guard let kind = profile.kind else { continue }
            let dir = kind.dataDirectory(for: profile)
            guard seenDirs.insert(dir).inserted else { continue }
            let found: [(id: String, title: String, date: Date)] = switch kind {
            case .claude: claudeConversations(dataDir: dir, cwd: cwd, limit: limit)
            case .codex: codexConversations(dataDir: dir, cwd: cwd, limit: limit)
            case .pi: piConversations(dataDir: dir, cwd: cwd, limit: limit)
            }
            result += found.map { Conversation(id: $0.id, title: $0.title, date: $0.date, kind: kind, profile: profile) }
        }
        saveCache()
        return Array(result.sorted { $0.date > $1.date }.prefix(limit))
    }

    static func all(profiles: [AgentProfile], limit: Int = 150) -> [HistoryEntry] {
        var seenDirs = Set<String>()
        var result: [HistoryEntry] = []
        for profile in profiles {
            guard let kind = profile.kind else { continue }
            let dir = kind.dataDirectory(for: profile)
            guard seenDirs.insert(dir).inserted else { continue }
            let found: [(id: String, title: String, date: Date, cwd: String)] = switch kind {
            case .claude:
                newestFiles(in: subdirectories(of: dir + "/projects"), limit: limit).compactMap { url, date in
                    let meta = meta(url, date: date, read: claudeMeta)
                    guard let title = meta.title, let cwd = meta.cwd else { return nil }
                    return (url.deletingPathExtension().lastPathComponent, title, date, cwd)
                }
            case .codex:
                codexFiles(dataDir: dir).prefix(limit).compactMap { url, date in
                    let meta = meta(url, date: date, read: codexMeta)
                    guard let id = meta.id, let cwd = meta.cwd else { return nil }
                    return (id, meta.title ?? "Codex session", date, cwd)
                }
            case .pi:
                newestFiles(in: subdirectories(of: dir + "/sessions"), limit: limit).compactMap { url, date in
                    let meta = meta(url, date: date, read: piMeta)
                    guard let id = meta.id, let cwd = meta.cwd, let title = meta.title else { return nil }
                    return (id, title, date, cwd)
                }
            }
            result += found.map { HistoryEntry(conversation: Conversation(id: $0.id, title: $0.title, date: $0.date, kind: kind, profile: profile), cwd: $0.cwd) }
        }
        saveCache()
        return Array(result.sorted { $0.conversation.date > $1.conversation.date }.prefix(limit))
    }

    private static func subdirectories(of path: String) -> [String] {
        ((try? FileManager.default.contentsOfDirectory(atPath: path)) ?? []).map { path + "/" + $0 }
    }

    static func claudeProjectDirectory(dataDir: String, cwd: String) -> String {
        let encoded = String(cwd.unicodeScalars.map { CharacterSet.alphanumerics.contains($0) && $0.isASCII ? Character($0) : "-" })
        return dataDir + "/projects/" + encoded
    }

    static func claudeProcess(pid: pid_t, dataDirs: [String]) -> (sessionId: String, status: String, subagents: String?)? {
        for dir in dataDirs {
            guard let data = FileManager.default.contents(atPath: "\(dir)/sessions/\(pid).json"),
                  let json = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
                  let id = json["sessionId"] as? String else { continue }
            return (id, json["status"] as? String ?? "", (json["cwd"] as? String).map { claudeProjectDirectory(dataDir: dir, cwd: $0) + "/\(id)/subagents" })
        }
        return nil
    }

    static func claudeConversationExists(dataDir: String, cwd: String, id: String) -> Bool {
        FileManager.default.fileExists(atPath: claudeProjectDirectory(dataDir: dataDir, cwd: cwd) + "/\(id).jsonl")
    }

    private static func claudeConversations(dataDir: String, cwd: String, limit: Int) -> [(id: String, title: String, date: Date)] {
        let dir = claudeProjectDirectory(dataDir: dataDir, cwd: cwd)
        return newestFiles(in: [dir], limit: limit * 2).compactMap { url, date in
            guard let title = meta(url, date: date, read: claudeMeta).title else { return nil }
            return (url.deletingPathExtension().lastPathComponent, title, date)
        }.prefix(limit).map { $0 }
    }

    private struct Meta: Codable {
        var date: Date
        var id: String?
        var cwd: String?
        var title: String?
    }

    private static let cacheLock = NSLock()
    private static let cacheURL = FileManager.default.urls(for: .cachesDirectory, in: .userDomainMask)[0].appendingPathComponent((Bundle.main.object(forInfoDictionaryKey: "CFBundleName") as? String ?? "Nucli") + "/conversations.json")
    nonisolated(unsafe) private static var cache: [String: Meta]?
    nonisolated(unsafe) private static var cacheChanged = false

    private static func meta(_ url: URL, date: Date, read: (URL) -> Meta?) -> Meta {
        cacheLock.lock()
        if cache == nil {
            cache = (try? Data(contentsOf: cacheURL)).flatMap { try? JSONDecoder().decode([String: Meta].self, from: $0) } ?? [:]
        }
        let cached = cache?[url.path]
        cacheLock.unlock()
        if let cached, abs(cached.date.timeIntervalSince(date)) < 0.001 { return cached }
        var meta = read(url) ?? Meta(date: date)
        meta.date = date
        cacheLock.lock()
        cache?[url.path] = meta
        cacheChanged = true
        cacheLock.unlock()
        return meta
    }

    private static func saveCache() {
        cacheLock.lock()
        defer { cacheLock.unlock() }
        guard cacheChanged, let cache, let data = try? JSONEncoder().encode(cache) else { return }
        cacheChanged = false
        try? FileManager.default.createDirectory(at: cacheURL.deletingLastPathComponent(), withIntermediateDirectories: true)
        try? data.write(to: cacheURL, options: .atomic)
    }

    private static func claudeMeta(_ url: URL) -> Meta? {
        var named: String?
        var aiTitle: String?
        var firstPrompt: String?
        var cwd: String?
        for line in jsonLines(url) {
            if cwd == nil, line.contains("\"cwd\""), let value = decode(line)?["cwd"] as? String {
                cwd = value
            }
            if line.contains("\"custom-title\""), let json = decode(line), json["type"] as? String == "custom-title" {
                named = json["customTitle"] as? String ?? named
            } else if line.contains("\"ai-title\""), let json = decode(line), json["type"] as? String == "ai-title" {
                aiTitle = json["aiTitle"] as? String ?? aiTitle
            } else if firstPrompt == nil, line.contains("\"type\":\"user\""), let json = decode(line),
                      json["isMeta"] as? Bool != true, json["isCompactSummary"] as? Bool != true,
                      let message = json["message"] as? [String: Any] {
                firstPrompt = promptText(message["content"])
            }
        }
        return Meta(date: .distantPast, cwd: cwd, title: named ?? aiTitle ?? firstPrompt)
    }

    private static func codexConversations(dataDir: String, cwd: String, limit: Int) -> [(id: String, title: String, date: Date)] {
        var result: [(id: String, title: String, date: Date)] = []
        for (url, date) in codexFiles(dataDir: dataDir) {
            let meta = meta(url, date: date, read: codexMeta)
            guard let id = meta.id, meta.cwd == cwd else { continue }
            result.append((id, meta.title ?? "Codex session", date))
            if result.count == limit { break }
        }
        return result
    }

    static func piSessionDirectory(dataDir: String, cwd: String) -> String {
        dataDir + "/sessions/--" + cwd.dropFirst().replacingOccurrences(of: "/", with: "-") + "--"
    }

    private static func piConversations(dataDir: String, cwd: String, limit: Int) -> [(id: String, title: String, date: Date)] {
        newestFiles(in: [piSessionDirectory(dataDir: dataDir, cwd: cwd)], limit: limit * 2).compactMap { url, date in
            let meta = meta(url, date: date, read: piMeta)
            return meta.id.flatMap { id in meta.title.map { (id, $0, date) } }
        }.prefix(limit).map { $0 }
    }

    private static func piMeta(_ url: URL) -> Meta? {
        var id: String?
        var cwd: String?
        var prompt: String?
        for line in jsonLines(url, maxBytes: 1048576) {
            guard let json = decode(line) else { continue }
            switch json["type"] as? String {
            case "session":
                id = json["id"] as? String
                cwd = json["cwd"] as? String
            case "message" where prompt == nil:
                let message = json["message"] as? [String: Any]
                if message?["role"] as? String == "user" { prompt = promptText(message?["content"]) }
            default: break
            }
            if id != nil, prompt != nil { break }
        }
        guard let id, let cwd, let prompt else { return nil }
        return Meta(date: .distantPast, id: id, cwd: cwd, title: prompt)
    }

    static func newestCodexConversation(dataDir: String, cwd: String, startedAfter: Date, excluding: Set<String>) -> String? {
        codexFiles(dataDir: dataDir).lazy
            .filter { $0.date >= startedAfter }
            .map { meta($0.url, date: $0.date, read: codexMeta) }
            .first { $0.cwd == cwd && $0.id.map(excluding.contains) == false }?.id
    }

    private static func codexFiles(dataDir: String) -> [(url: URL, date: Date)] {
        let root = URL(fileURLWithPath: dataDir + "/sessions")
        guard let enumerator = FileManager.default.enumerator(at: root, includingPropertiesForKeys: [.contentModificationDateKey]) else { return [] }
        let urls = enumerator.compactMap { $0 as? URL }.filter { $0.pathExtension == "jsonl" }
        return newest(urls, limit: 300)
    }

    private static func codexMeta(_ url: URL) -> Meta? {
        var id: String?
        var cwd: String?
        for line in jsonLines(url, maxBytes: 262144) {
            guard let json = decode(line), let payload = json["payload"] as? [String: Any] else { continue }
            if json["type"] as? String == "session_meta" {
                id = payload["id"] as? String
                cwd = payload["cwd"] as? String
            } else if json["type"] as? String == "event_msg", payload["type"] as? String == "user_message",
                      let text = promptText(payload["message"]), let id, let cwd {
                return Meta(date: .distantPast, id: id, cwd: cwd, title: text)
            }
        }
        guard let id, let cwd else { return nil }
        return Meta(date: .distantPast, id: id, cwd: cwd)
    }

    private static func promptText(_ content: Any?) -> String? {
        let text: String? = if let string = content as? String {
            string
        } else if let parts = content as? [[String: Any]] {
            parts.first { $0["type"] as? String == "text" }?["text"] as? String
        } else {
            nil
        }
        guard let trimmed = text?.trimmingCharacters(in: .whitespacesAndNewlines), !trimmed.isEmpty, !trimmed.hasPrefix("<") else { return nil }
        return String(trimmed.prefix(200)).replacingOccurrences(of: "\n", with: " ")
    }

    private static func newestFiles(in dirs: [String], limit: Int) -> [(url: URL, date: Date)] {
        let urls = dirs.flatMap { dir -> [URL] in
            let contents = (try? FileManager.default.contentsOfDirectory(at: URL(fileURLWithPath: dir), includingPropertiesForKeys: [.contentModificationDateKey])) ?? []
            return contents.filter { $0.pathExtension == "jsonl" }
        }
        return newest(urls, limit: limit)
    }

    private static func newest(_ urls: [URL], limit: Int) -> [(url: URL, date: Date)] {
        let dated: [(url: URL, date: Date)] = urls.map { (url: $0, date: modificationDate($0)) }
        let sorted = dated.sorted { (a: (url: URL, date: Date), b: (url: URL, date: Date)) -> Bool in a.date > b.date }
        return Array(sorted.prefix(limit))
    }

    private static func modificationDate(_ url: URL) -> Date {
        (try? url.resourceValues(forKeys: [.contentModificationDateKey]).contentModificationDate) ?? .distantPast
    }

    private static func jsonLines(_ url: URL, maxBytes: Int = 33554432) -> [Substring] {
        guard let handle = try? FileHandle(forReadingFrom: url) else { return [] }
        defer { try? handle.close() }
        guard let data = try? handle.read(upToCount: maxBytes) else { return [] }
        return String(decoding: data, as: UTF8.self).split(separator: "\n")
    }

    private static func decode(_ line: Substring) -> [String: Any]? {
        try? JSONSerialization.jsonObject(with: Data(line.utf8)) as? [String: Any]
    }
}

extension Date {
    var relativeDescription: String {
        let formatter = RelativeDateTimeFormatter()
        formatter.unitsStyle = .short
        return formatter.localizedString(for: self, relativeTo: Date())
    }
}
