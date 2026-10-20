import Foundation

/// What a run of tool calls was for. Reads and searches are one thing,
/// looking around, so a grep and the file it turned up stay one group.
enum WorkKind: Hashable, Sendable {
    case research, edit, run, agent, other, think, note
}

struct WorkStep: Hashable, Identifiable, Sendable {
    let id: String
    let label: String
    var failed = false
}

/// One chunk of a turn's work: a run of calls and thinking, under the icon
/// for what it did most and a line saying what it added up to.
struct WorkPhase: Hashable, Identifiable, Sendable {
    let id: String
    let kind: WorkKind
    let title: String
    let steps: [WorkStep]
}

/// What an opened fold holds, in order: the agent's notes between its work.
enum FoldEntry: Hashable, Identifiable, Sendable {
    case prose(id: String, text: String)
    case phase(WorkPhase)

    var id: String {
        switch self {
        case .prose(let id, _): "prose-\(id)"
        case .phase(let phase): "phase-\(phase.id)"
        }
    }
}

enum TurnItem: Hashable, Identifiable, Sendable {
    case user(Message)
    /// The status line the turn's work folds behind.
    case header(TurnHeader)
    /// Work left open: the live run after the last thing the agent said.
    case work(id: String, phases: [WorkPhase])
    case reply(Message)
    case notice(id: String, text: String, error: Bool)

    var id: String {
        switch self {
        case .user(let message): "user-\(message.id)"
        case .header(let header): "header-\(header.id)"
        case .work(let id, _): "work-\(id)"
        case .reply(let message): "reply-\(message.id)"
        case .notice(let id, _, _): "notice-\(id)"
        }
    }
}

struct TurnHeader: Hashable, Sendable {
    let id: String
    let live: Bool
    let waiting: String?
    /// Unix seconds the turn started, for the live clock.
    let startedAt: UInt64?
    let durationSeconds: UInt64?
    let model: String?
    let provider: ProviderKind
    let entries: [FoldEntry]
}

struct TranscriptTurn: Hashable, Identifiable, Sendable {
    let id: String
    let items: [TurnItem]
    /// When the answer landed, for the line under a settled turn.
    let completedAt: UInt64?
    /// The turn's replies, for the footer's copy button.
    let copyText: String?
}

enum TranscriptTurns {
    /// A session's transcript as turns, laid out as the desktop's transcript:
    /// the prompt, the work folded behind one line, then the answer.
    static func build(_ session: AgentSession, modelName: String?) -> [TranscriptTurn] {
        var turns: [[Element]] = []
        var current: [Element] = []
        for entry in TranscriptTimeline(session).entries {
            switch entry {
            case .message(let message, _) where message.role == .user:
                if !current.isEmpty { turns.append(current) }
                current = [.user(message)]
            case .message(let message, _):
                guard message.role == .assistant,
                      !message.content.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty else { continue }
                current.append(.prose(message))
            case .block(let block):
                switch block.content {
                case .reasoning(let reasoning):
                    current.append(.reasoning(block.id, reasoning))
                case .activities(let activities):
                    current.append(contentsOf: activities.map(Element.activity))
                }
            }
        }
        if !current.isEmpty { turns.append(current) }

        let turnRecords = Dictionary(session.turns.map { ($0.id, $0) }, uniquingKeysWith: { _, last in last })
        return turns.enumerated().map { index, elements in
            let isLast = index == turns.count - 1
            let user = elements.compactMap { element -> Message? in
                if case .user(let message) = element { return message }
                return nil
            }.first
            let record = user?.turnID.flatMap { turnRecords[$0] }
            let live = isLast && session.status.isBusy
            return turn(
                elements: elements,
                record: record,
                live: live,
                waiting: live && session.status == .waiting ? "Waiting for approval" : nil,
                provider: session.provider,
                modelName: modelName
            )
        }
    }

    private enum Element {
        case user(Message)
        case prose(Message)
        case reasoning(String, ReasoningBlock)
        case activity(ActivityItem)
    }

    private static func turn(
        elements: [Element],
        record: AgentTurn?,
        live: Bool,
        waiting: String?,
        provider: ProviderKind,
        modelName: String?
    ) -> TranscriptTurn {
        var items: [TurnItem] = []
        var user: Message?
        var rest: [Element] = []
        for element in elements {
            if case .user(let message) = element { user = message } else { rest.append(element) }
        }
        if let user { items.append(.user(user)) }

        // The answer is the prose after the last piece of work; everything
        // before it folds away behind the status line.
        let lastWork = rest.lastIndex { element in
            if case .prose = element { return false }
            return true
        }
        let lastProse = rest.lastIndex { element in
            if case .prose = element { return true }
            return false
        }
        let foldEnd: Int
        var openWork: [Element] = []
        var replies: [Message] = []
        if let lastProse, lastProse > (lastWork ?? -1) {
            // Ends on prose: every message after the last work is the answer.
            let firstReply = (lastWork ?? -1) + 1
            foldEnd = firstReply
            replies = rest[firstReply...].compactMap { element in
                if case .prose(let message) = element { return message }
                return nil
            }
        } else if live, let lastWork {
            // Still working: the run since it last spoke stays open.
            let runStart = (rest[..<lastWork].lastIndex { element in
                if case .prose = element { return true }
                return false
            }).map { $0 + 1 } ?? 0
            foldEnd = runStart
            openWork = Array(rest[runStart...])
        } else {
            foldEnd = rest.count
        }

        let entries = foldEntries(Array(rest[..<foldEnd]))
        let turnID = record?.id.uuidString ?? user?.id.uuidString ?? UUID().uuidString
        let duration = record.flatMap { turn in turn.completedAt.map { $0 > turn.startedAt ? $0 - turn.startedAt : 0 } }
        if live || !entries.isEmpty || duration != nil {
            items.append(.header(TurnHeader(
                id: turnID,
                live: live,
                waiting: waiting,
                startedAt: record?.startedAt ?? user?.createdAt,
                durationSeconds: duration,
                model: modelName,
                provider: provider,
                entries: entries
            )))
        }
        if !openWork.isEmpty {
            items.append(.work(id: turnID, phases: phases(openWork, live: true)))
        }
        items.append(contentsOf: replies.map(TurnItem.reply))
        switch record?.status {
        case "failed": items.append(.notice(id: turnID, text: "The turn didn't finish.", error: true))
        case "interrupted": items.append(.notice(id: turnID, text: "Interrupted", error: false))
        default: break
        }

        let copyText = replies.map(\.content).joined(separator: "\n\n")
        return TranscriptTurn(
            id: turnID,
            items: items,
            completedAt: live ? nil : (record?.completedAt ?? replies.last?.createdAt),
            copyText: live || copyText.isEmpty ? nil : copyText
        )
    }

    /// The folded span as the agent's notes between phases of work.
    private static func foldEntries(_ elements: [Element]) -> [FoldEntry] {
        var entries: [FoldEntry] = []
        var run: [Element] = []
        func flushRun() {
            entries.append(contentsOf: phases(run, live: false).map(FoldEntry.phase))
            run = []
        }
        for element in elements {
            if case .prose(let message) = element {
                flushRun()
                entries.append(.prose(id: message.id.uuidString, text: message.content))
            } else {
                run.append(element)
            }
        }
        flushRun()
        return entries
    }

    /// A run of work as one phase: however many shapes it takes, it reads as
    /// one thing done rather than a row per call.
    private static func phases(_ elements: [Element], live: Bool) -> [WorkPhase] {
        var steps: [WorkStep] = []
        var activities: [ActivityItem] = []
        var thought = false
        for element in elements {
            switch element {
            case .reasoning(let id, let reasoning):
                thought = true
                let summary = summaryLine(reasoning.content)
                steps.append(WorkStep(id: id, label: summary.isEmpty ? "Thinking" : summary))
            case .activity(let activity):
                if activity.kind.lowercased() == "reasoning" {
                    thought = true
                    steps.append(WorkStep(id: activity.id, label: summaryLine(activity.reasoning?.content ?? activity.title)))
                    continue
                }
                activities.append(activity)
                steps.append(WorkStep(id: activity.id, label: stepLabel(activity), failed: activity.failed))
            default:
                break
            }
        }
        guard !steps.isEmpty else { return [] }
        let kind = dominantKind(activities) ?? (thought ? .think : .note)
        let title = activities.isEmpty
            ? (live ? "Thinking" : "Thought")
            : workSummary(activities, live: live)
        return [WorkPhase(id: steps[0].id, kind: kind, title: title, steps: steps)]
    }

    static func category(_ activity: ActivityItem) -> WorkKind {
        let kind = activity.kind.lowercased()
        switch kind {
        case "command", "bash": return .run
        case "file_change", "filechange", "edit": return .edit
        case "file_read", "fileread", "read", "file_search", "filesearch", "search", "file_list", "filelist", "list":
            return .research
        default:
            let title = activity.title.lowercased()
            return title.contains("agent") || title == "task" ? .agent : .other
        }
    }

    private static func isSearch(_ activity: ActivityItem) -> Bool {
        ["file_search", "filesearch", "search", "file_list", "filelist", "list"].contains(activity.kind.lowercased())
    }

    /// Ties break toward the kind that changed the most: an edit outranks a read.
    private static func dominantKind(_ activities: [ActivityItem]) -> WorkKind? {
        let order: [WorkKind] = [.edit, .run, .agent, .research, .other]
        var counts: [WorkKind: Int] = [:]
        for activity in activities { counts[category(activity), default: 0] += 1 }
        var best: WorkKind?
        for kind in order {
            let count = counts[kind] ?? 0
            if count > 0, count > (best.flatMap { counts[$0] } ?? 0) { best = kind }
        }
        return best
    }

    /// "Read 3 files · Edited 2 files · Ran a command", the clause for the
    /// call still running in the present tense.
    static func workSummary(_ activities: [ActivityItem], live: Bool) -> String {
        var order: [WorkKind] = []
        var reads: Set<String> = []
        var edits: Set<String> = []
        var searches = 0, runs = 0, agents = 0, others = 0
        for activity in activities {
            let kind = category(activity)
            if !order.contains(kind) { order.append(kind) }
            let target = activity.displayParam ?? activity.id
            switch kind {
            case .edit: edits.insert(target)
            case .research: if isSearch(activity) { searches += 1 } else { reads.insert(target) }
            case .run: runs += 1
            case .agent: agents += 1
            default: others += 1
            }
        }
        let running = live ? activities.last.map(category) : nil
        func files(_ paths: Set<String>) -> String {
            if paths.count == 1, let first = paths.first {
                let name = (first as NSString).lastPathComponent
                return name.isEmpty ? first : name
            }
            return "\(paths.count) files"
        }
        func counted(_ count: Int, _ one: String, _ many: String, present: Bool) -> String {
            count == 1 ? (present ? "Running \(one)" : "Ran \(one)") : "\(present ? "Running" : "Ran") \(count) \(many)"
        }
        return order.map { kind in
            let present = kind == running
            switch kind {
            case .edit: return "\(present ? "Editing" : "Edited") \(files(edits))"
            case .research:
                if !reads.isEmpty, searches == 0 { return "\(present ? "Reading" : "Read") \(files(reads))" }
                if reads.isEmpty { return present ? "Searching the project" : "Searched the project" }
                return present ? "Exploring the project" : "Explored the project"
            case .run: return counted(runs, "a command", "commands", present: present)
            case .agent: return counted(agents, "a subagent", "subagents", present: present)
            default: return counted(others, "a tool", "tools", present: present)
            }
        }.joined(separator: " · ")
    }

    private static func stepLabel(_ activity: ActivityItem) -> String {
        let verb = activity.complete ? activity.actionPastVerb : activity.actionActiveVerb
        guard let param = activity.displayParam else { return verb }
        let shown = category(activity) == .edit || category(activity) == .research && !isSearch(activity)
            ? (param as NSString).lastPathComponent
            : param
        return "\(verb) \(shown)"
    }

    /// The first line of some prose, without Markdown markers, kept short.
    static func summaryLine(_ text: String) -> String {
        let line = text.split(whereSeparator: \.isNewline)
            .map { $0.trimmingCharacters(in: .whitespaces) }
            .first { !$0.isEmpty } ?? ""
        let plain = line.replacingOccurrences(of: "**", with: "")
            .replacingOccurrences(of: "`", with: "")
            .trimmingCharacters(in: CharacterSet(charactersIn: "#*-> "))
        return plain.count > 120 ? "\(plain.prefix(119))…" : plain
    }

    /// "Claude Opus worked for 2m 4s", or "Working…" while the clock starts.
    static func workingLabel(seconds: UInt64?, model: String?, done: Bool) -> String {
        let who = model?.trimmingCharacters(in: .whitespaces).nilIfEmpty
        let verb = done ? (who == nil ? "Worked" : "worked") : (who == nil ? "Working" : "working")
        guard let seconds else {
            if done { return who.map { "\($0) \(verb)" } ?? verb }
            return who.map { "\($0) \(verb)…" } ?? "\(verb)…"
        }
        let elapsed = elapsedLabel(seconds)
        return who.map { "\($0) \(verb) for \(elapsed)" } ?? "\(verb) for \(elapsed)"
    }

    static func elapsedLabel(_ seconds: UInt64) -> String {
        let total = max(1, seconds)
        if total < 60 { return "\(total)s" }
        let minutes = total / 60
        let rest = total % 60
        return rest == 0 ? "\(minutes)m" : "\(minutes)m \(rest)s"
    }
}

private extension String {
    var nilIfEmpty: String? { isEmpty ? nil : self }
}
