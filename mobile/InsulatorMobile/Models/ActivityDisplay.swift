import Foundation

extension ActivityItem {
    private var normalizedKind: String { kind.lowercased() }

    var actionPastVerb: String {
        switch normalizedKind {
        case "command", "bash": "Ran"
        case "file_read", "fileread", "read": "Read"
        case "file_change", "filechange", "edit": "Edited"
        case "file_search", "filesearch", "search": "Searched"
        case "file_list", "filelist", "list": "Listed"
        case "reasoning", "think": "Thought"
        case "plan": "Updated plan"
        default: !title.isEmpty && title != "Tool" && title != "Activity" ? title : "Tool"
        }
    }

    var actionActiveVerb: String {
        switch normalizedKind {
        case "file_read", "fileread", "read": "Reading"
        case "file_change", "filechange", "edit": "Editing"
        case "file_search", "filesearch", "search": "Searching"
        case "file_list", "filelist", "list": "Listing"
        case "reasoning", "think": "Thinking"
        case "plan": "Planning"
        default: "Running"
        }
    }

    /// What the call acted on: its target, description, first changed file,
    /// or the first line of its arguments or detail.
    var displayParam: String? {
        if let target = displayTarget?.trimmed, !target.isEmpty { return target }
        if let description = displayDescription?.trimmed, !description.isEmpty { return description }
        if let path = fileChanges?.first?.path, !path.isEmpty { return path }
        if let arguments, let line = arguments.firstNonEmptyLine { return line }
        if let detail, let line = detail.firstNonEmptyLine,
           !["completed", "failed"].contains(detail.lowercased()) {
            return line
        }
        return nil
    }
}

private extension String {
    var trimmed: String { trimmingCharacters(in: .whitespacesAndNewlines) }

    var firstNonEmptyLine: String? {
        components(separatedBy: .newlines)
            .map { $0.trimmingCharacters(in: .whitespacesAndNewlines) }
            .first { !$0.isEmpty }
    }
}
