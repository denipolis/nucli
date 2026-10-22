import Foundation

final class Subagent {
    let id: String
    unowned let session: Session
    var title: String
    var type: String

    init(id: String, session: Session, title: String, type: String) {
        self.id = id
        self.session = session
        self.title = title
        self.type = type
    }

    nonisolated static func running(in dir: String) -> [(id: String, title: String, type: String)] {
        let files = (try? FileManager.default.contentsOfDirectory(atPath: dir)) ?? []
        return files.filter { $0.hasSuffix(".jsonl") }.compactMap { file -> (Date, String, String, String)? in
            let path = dir + "/" + file
            guard let modified = (try? FileManager.default.attributesOfItem(atPath: path))?[.modificationDate] as? Date,
                  Date().timeIntervalSince(modified) < 600, !isFinished(path) else { return nil }
            let base = String(file.dropLast(6))
            let meta = FileManager.default.contents(atPath: dir + "/" + base + ".meta.json").flatMap { try? JSONSerialization.jsonObject(with: $0) as? [String: Any] } ?? [:]
            let created = (try? FileManager.default.attributesOfItem(atPath: path))?[.creationDate] as? Date ?? modified
            return (created, base, meta["description"] as? String ?? base, meta["agentType"] as? String ?? "")
        }.sorted { $0.0 < $1.0 }.map { ($0.1, $0.2, $0.3) }
    }

    private nonisolated static func isFinished(_ path: String) -> Bool {
        guard let handle = FileHandle(forReadingAtPath: path) else { return true }
        defer { try? handle.close() }
        let size = (try? handle.seekToEnd()) ?? 0
        try? handle.seek(toOffset: size > 65536 ? size - 65536 : 0)
        guard let data = try? handle.readToEnd(),
              let last = String(decoding: data, as: UTF8.self).split(separator: "\n").last,
              let json = try? JSONSerialization.jsonObject(with: Data(last.utf8)) as? [String: Any] else { return false }
        if json["toolEndsTurn"] as? Bool == true { return true }
        return json["type"] as? String == "assistant" && (json["message"] as? [String: Any])?["stop_reason"] as? String == "end_turn"
    }
}
