import Foundation

struct FuzzyMatch {
    let score: Int
    let positions: [Int]
}

enum Fuzzy {
    private static let separators: Set<Character> = ["/", "\\", "_", "-", " ", ".", ":"]

    static func match(_ query: String, in candidate: String) -> FuzzyMatch? {
        let q = Array(query.lowercased())
        let original = Array(candidate)
        let c = original.map { Character($0.lowercased()) }
        let n = q.count
        let m = c.count
        guard n > 0 else { return FuzzyMatch(score: 0, positions: []) }
        guard n <= m else { return nil }
        var index = 0
        for char in c where index < n && char == q[index] { index += 1 }
        guard index == n else { return nil }

        var bonus = [Int](repeating: 0, count: m)
        for j in 0..<m {
            if j == 0 {
                bonus[j] = 12
            } else if separators.contains(original[j - 1]) {
                bonus[j] = 10
            } else if original[j - 1].isLowercase && original[j].isUppercase {
                bonus[j] = 9
            } else if !original[j - 1].isLetter && original[j].isLetter {
                bonus[j] = 6
            }
        }
        let minimum = Int.min / 4
        var matched = [Int](repeating: minimum, count: n * m)
        var best = [Int](repeating: minimum, count: n * m)
        var consecutive = [Bool](repeating: false, count: n * m)
        for i in 0..<n {
            var running = minimum
            for j in i..<m {
                let k = i * m + j
                if q[i] == c[j] {
                    var score = minimum
                    if i == 0 {
                        score = bonus[j] - j / 4
                    } else if j > 0 {
                        let afterGap = best[(i - 1) * m + j - 1] + bonus[j]
                        let afterRun = matched[(i - 1) * m + j - 1] + 8 + bonus[j] / 2
                        if afterRun >= afterGap, matched[(i - 1) * m + j - 1] > minimum {
                            score = afterRun
                            consecutive[k] = true
                        } else {
                            score = afterGap
                        }
                    }
                    if score > minimum { matched[k] = score + 1 }
                }
                running = max(running > minimum ? running - 1 : minimum, matched[k])
                best[k] = running
            }
        }
        let total = best[(n - 1) * m + m - 1]
        guard total > minimum else { return nil }
        var positions: [Int] = []
        var j = m - 1
        var i = n - 1
        var forced = false
        while i >= 0, j >= 0 {
            let k = i * m + j
            if matched[k] > minimum && (forced || matched[k] == best[k]) {
                positions.append(j)
                forced = consecutive[k]
                i -= 1
            }
            j -= 1
        }
        return FuzzyMatch(score: total, positions: positions.reversed())
    }

    static func matchPath(_ query: String, path: String) -> FuzzyMatch? {
        let name = (path as NSString).lastPathComponent
        let offset = path.count - name.count
        if !query.contains("/"), let match = match(query, in: name) {
            return FuzzyMatch(score: match.score + 40 - path.count / 20, positions: match.positions.map { $0 + offset })
        }
        guard let match = match(query, in: path) else { return nil }
        return FuzzyMatch(score: match.score - path.count / 20, positions: match.positions)
    }
}
