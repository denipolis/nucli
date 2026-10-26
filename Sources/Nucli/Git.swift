import Foundation

struct FileChange: Equatable {
    let path: String
    let status: Character
    let added: Int
    let removed: Int
    var commit: String? = nil
    var parent: String? = nil
}

struct GitStatus: Equatable {
    let branch: String
    let changes: [FileChange]
    var staged: [FileChange] = []
    var unstaged: [FileChange] = []
}

struct DiffLine: Equatable {
    let kind: Character
    let text: Substring
    let oldNumber: Int?
    let newNumber: Int?
}

enum Git {
    static let emptyTree = "4b825dc642cb6eb9a060e54bf8d69288fbee4904"

    static func run(_ args: [String], in dir: String) -> String? {
        let process = Process()
        process.executableURL = URL(fileURLWithPath: "/usr/bin/git")
        process.arguments = ["-c", "core.quotepath=off", "--no-pager"] + args
        process.currentDirectoryURL = URL(fileURLWithPath: dir)
        let out = Pipe()
        process.standardOutput = out
        process.standardError = FileHandle.nullDevice
        do { try process.run() } catch { return nil }
        let data = out.fileHandleForReading.readDataToEndOfFile()
        process.waitUntilExit()
        return process.terminationStatus == 0 ? String(decoding: data, as: UTF8.self) : nil
    }

    static func runReportingError(_ args: [String], in dir: String) -> String? {
        let process = Process()
        process.executableURL = URL(fileURLWithPath: "/usr/bin/git")
        process.arguments = ["-c", "core.quotepath=off", "--no-pager"] + args
        process.currentDirectoryURL = URL(fileURLWithPath: dir)
        let err = Pipe()
        process.standardOutput = FileHandle.nullDevice
        process.standardError = err
        do { try process.run() } catch { return error.localizedDescription }
        let data = err.fileHandleForReading.readDataToEndOfFile()
        process.waitUntilExit()
        return process.terminationStatus == 0 ? nil : String(decoding: data, as: UTF8.self).trimmingCharacters(in: .whitespacesAndNewlines)
    }

    static func branches(in dir: String) -> [String] {
        lines(run(["for-each-ref", "--sort=-committerdate", "--count=30", "--format=%(refname:short)", "refs/heads"], in: dir)).map(String.init)
    }

    static func gitDir(in dir: String) -> String? {
        run(["rev-parse", "--absolute-git-dir"], in: dir)?.trimmingCharacters(in: .whitespacesAndNewlines)
    }

    private static func base(in dir: String) -> String {
        run(["rev-parse", "--verify", "-q", "HEAD"], in: dir) != nil ? "HEAD" : emptyTree
    }

    private static func lines(_ output: String?) -> [Substring] {
        output?.split(separator: "\n") ?? []
    }

    static func status(in dir: String) -> GitStatus? {
        guard run(["rev-parse", "--is-inside-work-tree"], in: dir) != nil else { return nil }
        let base = base(in: dir)
        let branch = run(["branch", "--show-current"], in: dir)?.trimmingCharacters(in: .whitespacesAndNewlines) ?? ""
        let untracked = lines(run(["ls-files", "--others", "--exclude-standard"], in: dir)).map {
            FileChange(path: String($0), status: "?", added: lineCount(of: dir + "/" + $0), removed: 0)
        }
        let changes = diff([base], in: dir) + untracked
        let staged = diff(["--cached", base], in: dir)
        let unstaged = diff([], in: dir) + untracked
        return GitStatus(branch: branch.isEmpty ? "detached" : branch, changes: changes.sorted { $0.path < $1.path }, staged: staged.sorted { $0.path < $1.path }, unstaged: unstaged.sorted { $0.path < $1.path })
    }

    static func diffStat(in dir: String) -> (added: Int, removed: Int)? {
        guard run(["rev-parse", "--is-inside-work-tree"], in: dir) != nil else { return nil }
        var added = lines(run(["ls-files", "--others", "--exclude-standard"], in: dir)).reduce(0) { $0 + lineCount(of: dir + "/" + $1) }
        var removed = 0
        for line in lines(run(["diff", base(in: dir), "--numstat", "--relative", "--no-renames"], in: dir)) {
            let parts = line.split(separator: "\t", maxSplits: 2)
            guard parts.count == 3 else { continue }
            added += Int(parts[0]) ?? 0
            removed += Int(parts[1]) ?? 0
        }
        return (added, removed)
    }

    static func changes(in commit: Commit, dir: String) -> [FileChange] {
        let parent = commit.parents.first ?? emptyTree
        return diff([parent, commit.hash], in: dir).map {
            FileChange(path: $0.path, status: $0.status, added: $0.added, removed: $0.removed, commit: commit.hash, parent: parent)
        }
    }

    private static func diff(_ args: [String], in dir: String) -> [FileChange] {
        var statuses: [Substring: Character] = [:]
        for line in lines(run(["diff"] + args + ["--name-status", "--relative", "--no-renames"], in: dir)) {
            let parts = line.split(separator: "\t", maxSplits: 1)
            if parts.count == 2, let letter = parts[0].first { statuses[parts[1]] = letter }
        }
        return lines(run(["diff"] + args + ["--numstat", "--relative", "--no-renames"], in: dir)).compactMap { line in
            let parts = line.split(separator: "\t", maxSplits: 2)
            guard parts.count == 3 else { return nil }
            return FileChange(path: String(parts[2]), status: statuses[parts[2]] ?? "M", added: Int(parts[0]) ?? 0, removed: Int(parts[1]) ?? 0)
        }
    }

    private static func lineCount(of path: String) -> Int {
        autoreleasepool {
            guard let attrs = try? FileManager.default.attributesOfItem(atPath: path),
                  (attrs[.size] as? Int ?? 0) < 4000000,
                  let data = FileManager.default.contents(atPath: path) else { return 0 }
            let count = data.reduce(0) { $1 == 10 ? $0 + 1 : $0 }
            return data.last == 10 || data.isEmpty ? count : count + 1
        }
    }

    static func stage(_ paths: [String], in dir: String) -> String? {
        runReportingError(["add", "-A", "--"] + paths, in: dir)
    }

    static func unstage(_ paths: [String], in dir: String) -> String? {
        runReportingError(["reset", "-q", "--"] + paths, in: dir)
    }

    static func discard(_ paths: [String], in dir: String) -> String? {
        runReportingError(["restore", "--worktree", "--"] + paths, in: dir)
    }

    static func commit(message: String, in dir: String) -> String? {
        runReportingError(["commit", "-q", "-m", message], in: dir)
    }

    static func fullDiff(of change: FileChange, in dir: String, cached: Bool? = nil) -> [DiffLine]? {
        if change.commit == nil, change.status == "?" || change.status == " " {
            let path = dir + "/" + change.path
            guard ((try? FileManager.default.attributesOfItem(atPath: path))?[.size] as? Int ?? 0) < 4000000,
                  let data = FileManager.default.contents(atPath: path),
                  !data.contains(0) else { return nil }
            var lines = String(decoding: data, as: UTF8.self).split(separator: "\n", omittingEmptySubsequences: false)
            if lines.last?.isEmpty == true { lines.removeLast() }
            return lines.enumerated().map {
                DiffLine(kind: change.status == "?" ? "+" : " ", text: $0.element, oldNumber: change.status == "?" ? nil : $0.offset + 1, newNumber: $0.offset + 1)
            }
        }
        let revisions = change.commit.map { [change.parent ?? emptyTree, $0] } ?? cached.map { $0 ? ["--cached", base(in: dir)] : [] } ?? [base(in: dir)]
        guard let output = run(["diff"] + revisions + ["--no-color", "--no-ext-diff", "-U100000000", "--relative", "--", change.path], in: dir),
              !output.contains("\nBinary files ") else { return nil }
        var result: [DiffLine] = []
        var old = 1
        var new = 1
        var inHunk = false
        for line in output.split(separator: "\n", omittingEmptySubsequences: false) {
            if line.hasPrefix("@@") {
                inHunk = true
                continue
            }
            guard inHunk, let kind = line.first else { continue }
            let text = line.dropFirst()
            switch kind {
            case "+":
                result.append(DiffLine(kind: "+", text: text, oldNumber: nil, newNumber: new))
                new += 1
            case "-":
                result.append(DiffLine(kind: "-", text: text, oldNumber: old, newNumber: nil))
                old += 1
            case " ":
                result.append(DiffLine(kind: " ", text: text, oldNumber: old, newNumber: new))
                old += 1
                new += 1
            default:
                continue
            }
        }
        return result
    }
}

final class FileWatcher {
    private var stream: FSEventStreamRef?
    private let handler: () -> Void

    init(paths: [String], handler: @escaping () -> Void) {
        self.handler = handler
        var context = FSEventStreamContext(version: 0, info: Unmanaged.passUnretained(self).toOpaque(), retain: nil, release: nil, copyDescription: nil)
        let callback: FSEventStreamCallback = { _, info, _, _, _, _ in
            guard let info else { return }
            Unmanaged<FileWatcher>.fromOpaque(info).takeUnretainedValue().handler()
        }
        guard let stream = FSEventStreamCreate(nil, callback, &context, paths as CFArray, FSEventStreamEventId(kFSEventStreamEventIdSinceNow), 1.0, FSEventStreamCreateFlags(kFSEventStreamCreateFlagNone)) else { return }
        self.stream = stream
        FSEventStreamSetDispatchQueue(stream, .main)
        FSEventStreamStart(stream)
    }

    deinit {
        guard let stream else { return }
        FSEventStreamStop(stream)
        FSEventStreamInvalidate(stream)
        FSEventStreamRelease(stream)
    }
}
