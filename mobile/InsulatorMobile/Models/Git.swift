import Foundation

/// How a changed file differs from `HEAD`.
enum FileChangeStatus: String, Codable, Sendable {
    case modified
    case added
    case deleted
    case untracked

    var letter: String {
        switch self {
        case .modified: "M"
        case .added: "A"
        case .deleted: "D"
        case .untracked: "U"
        }
    }
}

/// One uncommitted file. A partly staged file is both staged and unstaged.
struct FileChange: Codable, Hashable, Identifiable, Sendable {
    let path: String
    let status: FileChangeStatus
    let staged: Bool
    let unstaged: Bool
    let additions: Int
    let deletions: Int

    var id: String { path }
    var isFullyStaged: Bool { staged && !unstaged }
    var isPartlyStaged: Bool { staged && unstaged }

    var name: String { (path as NSString).lastPathComponent }

    var directory: String {
        let directory = (path as NSString).deletingLastPathComponent
        return directory.isEmpty ? "/" : directory
    }

    func staging(_ staged: Bool) -> FileChange {
        FileChange(path: path, status: status, staged: staged, unstaged: !staged, additions: additions, deletions: deletions)
    }
}

/// A working copy's uncommitted files and where its branch stands.
struct WorkingTreeStatus: Codable, Hashable, Sendable {
    let branch: String
    let defaultBranch: String?
    let hasRemote: Bool
    let upstream: String?
    let ahead: Int
    let behind: Int
    let aheadOfDefault: Int
    let files: [FileChange]

    var onDefaultBranch: Bool { defaultBranch == branch }

    enum CodingKeys: String, CodingKey {
        case branch, upstream, ahead, behind, files
        case defaultBranch = "default_branch"
        case hasRemote = "has_remote"
        case aheadOfDefault = "ahead_of_default"
    }
}

/// A checkout of a repository.
struct WorktreeEntry: Codable, Hashable, Identifiable, Sendable {
    let path: String
    let branch: String?
    let isMain: Bool

    var id: String { path }
    var name: String { branch ?? (path as NSString).lastPathComponent }

    enum CodingKeys: String, CodingKey {
        case path, branch
        case isMain = "is_main"
    }
}

/// A provider plan's usage windows, as the desktop footer shows them.
struct PlanUsage: Codable, Hashable, Sendable {
    struct Window: Codable, Hashable, Sendable {
        let label: String
        let percent: Double
        let resetsAt: Int64?
    }

    let planLabel: String?
    let windows: [Window]
}

/// One line of a unified diff, numbered as the desktop's diff view numbers it.
struct DiffLine: Hashable, Sendable {
    enum Kind: Hashable, Sendable { case hunk, context, add, delete }

    let kind: Kind
    let text: String
    let oldNumber: Int?
    let newNumber: Int?
}

/// Splits a multi-file patch into each file's lines, keyed by its path in the
/// new tree (the old one for a deletion).
enum UnifiedDiff {
    static func files(in patch: String) -> [String: [DiffLine]] {
        var result: [String: [DiffLine]] = [:]
        var path: String?
        var lines: [DiffLine] = []
        var oldLine = 0
        var newLine = 0

        func flush() {
            if let path { result[path] = lines }
            lines = []
        }

        for raw in patch.split(separator: "\n", omittingEmptySubsequences: false) {
            let line = String(raw)
            if line.hasPrefix("diff --git ") {
                flush()
                path = Self.path(fromHeader: line)
                continue
            }
            if line.hasPrefix("+++ ") {
                let target = String(line.dropFirst(4))
                if target != "/dev/null" { path = Self.strip(target) }
                continue
            }
            if line.hasPrefix("--- ") || line.hasPrefix("index ") || line.hasPrefix("new file mode")
                || line.hasPrefix("deleted file mode") || line.hasPrefix("old mode") || line.hasPrefix("new mode")
                || line.hasPrefix("similarity index") || line.hasPrefix("rename ") || line.hasPrefix("Binary files") {
                continue
            }
            if line.hasPrefix("@@") {
                let (old, new) = hunkStarts(line)
                oldLine = old
                newLine = new
                lines.append(DiffLine(kind: .hunk, text: line, oldNumber: nil, newNumber: nil))
                continue
            }
            guard path != nil else { continue }
            if line.hasPrefix("+") {
                lines.append(DiffLine(kind: .add, text: String(line.dropFirst()), oldNumber: nil, newNumber: newLine))
                newLine += 1
            } else if line.hasPrefix("-") {
                lines.append(DiffLine(kind: .delete, text: String(line.dropFirst()), oldNumber: oldLine, newNumber: nil))
                oldLine += 1
            } else if line.hasPrefix(" ") {
                lines.append(DiffLine(kind: .context, text: String(line.dropFirst()), oldNumber: oldLine, newNumber: newLine))
                oldLine += 1
                newLine += 1
            }
        }
        flush()
        return result
    }

    /// `diff --git a/x b/x` names the file twice; the second half is its new path.
    private static func path(fromHeader header: String) -> String? {
        let rest = header.dropFirst("diff --git ".count)
        guard let range = rest.range(of: " b/", options: .backwards) else { return nil }
        return String(rest[range.upperBound...])
    }

    private static func strip(_ path: String) -> String {
        path.hasPrefix("b/") || path.hasPrefix("a/") ? String(path.dropFirst(2)) : path
    }

    /// `@@ -12,7 +12,9 @@` as its old and new starting lines.
    static func hunkStarts(_ header: String) -> (Int, Int) {
        let parts = header.split(separator: " ")
        func start(_ prefix: Character) -> Int {
            guard let part = parts.first(where: { $0.first == prefix }) else { return 1 }
            let number = part.dropFirst().split(separator: ",").first.flatMap { Int($0) } ?? 1
            return max(number, 1)
        }
        return (start("-"), start("+"))
    }
}
