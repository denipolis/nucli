import Foundation

@MainActor
final class ConversationCache {
    private var entries: [String: [Conversation]] = [:]
    private var inFlight: Set<String> = []
    private var waiting: [String: [([Conversation]) -> Void]] = [:]

    func cached(for path: String) -> [Conversation]? {
        entries[path]
    }

    func refresh(_ path: String, profiles: [AgentProfile], completion: (([Conversation]) -> Void)? = nil) {
        if let completion { waiting[path, default: []].append(completion) }
        guard inFlight.insert(path).inserted else { return }
        DispatchQueue.global(qos: .utility).async {
            let conversations = ConversationHistory.list(cwd: path, profiles: profiles)
            DispatchQueue.main.async { [self] in
                entries[path] = conversations
                inFlight.remove(path)
                waiting.removeValue(forKey: path)?.forEach { $0(conversations) }
                NotificationCenter.default.post(name: .conversationsChanged, object: path)
            }
        }
    }
}
