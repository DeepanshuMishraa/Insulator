import Foundation
import UIKit
import UserNotifications

/// A file picked in the composer, read whole to upload with the message.
struct PickedFile: Hashable, Identifiable, Sendable {
    let id = UUID()
    let name: String
    let data: Data
}

struct FinishedSession: Hashable, Identifiable, Sendable {
    let sessionID: UUID
    let projectID: UUID
    /// Unix seconds it finished.
    let at: UInt64

    var id: UUID { sessionID }
}

/// Where a new session will work, as the composer's workspace line picks it.
enum WorkspaceChoice: Hashable, Sendable {
    case current
    case newWorktree
    case existing(WorktreeEntry)

    var label: String {
        switch self {
        case .current: "Current checkout"
        case .newWorktree: "New worktree"
        case .existing(let worktree): worktree.name
        }
    }
}

extension AppModel {
    // MARK: - Projects

    /// The projects shown on this phone, as chosen in settings.
    var visibleProjects: [Project] {
        guard let visible = Preferences.shared.visibleProjectIDs else { return projects }
        let shown = projects.filter { visible.contains($0.id) }
        return shown.isEmpty ? projects : shown
    }

    var currentProject: Project? {
        let shown = visibleProjects
        return shown.first { $0.id == Preferences.shared.currentProjectID } ?? shown.first
    }

    func sessions(in projectID: UUID) -> [AgentSession] {
        sessions
            .filter { $0.projectID == projectID }
            .sorted { ($0.lastReplyAt ?? $0.createdAt) > ($1.lastReplyAt ?? $1.createdAt) }
    }

    /// The working copy a session's files and changes are read from.
    func workingDirectory(for session: AgentSession) -> String? {
        session.worktreePath ?? project(for: session)?.path
    }

    func modelName(provider: ProviderKind, modelID: String?) -> String? {
        guard let modelID else { return nil }
        return models(for: provider).first { $0.id == modelID }?.name ?? modelID
    }

    // MARK: - Model choice

    /// What a new session in `project` runs with: a pick in the model sheet,
    /// else this phone's default, else the first installed provider's default.
    func newSessionChoice(for projectID: UUID) -> ModelChoice? {
        if let draft = draftChoices[projectID] { return draft }
        if let choice = Preferences.shared.defaultChoice,
           installedProbes.contains(where: { $0.provider == choice.provider }) {
            return choice
        }
        guard let provider = installedProbes.first?.provider else { return nil }
        let available = models(for: provider)
        return ModelChoice(provider: provider, model: available.first(where: \.isDefault) ?? available.first)
    }

    func newSessionAccess(for projectID: UUID) -> RuntimeMode {
        draftAccess[projectID] ?? Preferences.shared.defaultAccess ?? .ask
    }

    func choice(of session: AgentSession) -> ModelChoice {
        var choice = ModelChoice(provider: session.provider, model: nil)
        choice.modelID = session.model
        choice.reasoningEffort = session.reasoningEffort
        choice.serviceTier = session.serviceTier
        choice.contextWindow = session.contextWindow
        return choice
    }

    /// Applies a pick to a running session, or remembers it for the next new one.
    func apply(_ choice: ModelChoice, projectID: UUID, sessionID: UUID?) async {
        guard let sessionID else {
            draftChoices[projectID] = choice
            return
        }
        guard let session = session(withID: sessionID) else { return }
        if session.provider != choice.provider {
            await updateSessionProvider(sessionID, provider: choice.provider)
        }
        let model = models(for: choice.provider).first { $0.id == choice.modelID }
        selectSessionIfNeeded(sessionID)
        await updateOptions(
            model: model,
            mode: self.session(withID: sessionID)?.runtimeMode ?? session.runtimeMode,
            reasoningEffort: choice.reasoningEffort,
            serviceTier: choice.serviceTier,
            contextWindow: choice.contextWindow
        )
    }

    func apply(access: RuntimeMode, projectID: UUID, sessionID: UUID?) async {
        guard let sessionID else {
            draftAccess[projectID] = access
            return
        }
        selectSessionIfNeeded(sessionID)
        await updateSessionMode(sessionID, mode: access)
    }

    private func selectSessionIfNeeded(_ sessionID: UUID) {
        if selectedSession?.id != sessionID, let session = session(withID: sessionID) {
            selectedSession = session
        }
    }

    // MARK: - Sessions

    /// Starts a session in `project` with its first message, in the working
    /// copy `workspace` names. A new worktree is made on the Mac first.
    func startSession(
        in project: Project,
        text: String,
        images: [UIImage],
        files: [PickedFile],
        workspace: WorkspaceChoice
    ) async throws -> AgentSession {
        guard let choice = newSessionChoice(for: project.id) else {
            throw ConnectionError.daemon("No coding agent is installed on the connected Mac. Install one there, then reconnect.")
        }
        let id = UUID()
        let sessionWorkspace: SessionWorkspace?
        switch workspace {
        case .current:
            sessionWorkspace = nil
        case .existing(let worktree):
            sessionWorkspace = worktree.isMain ? nil : .worktree(path: worktree.path, branch: worktree.name)
        case .newWorktree:
            let result = try await workspaceRequest(.object([
                "type": .string("createWorktree"),
                "project_path": .string(project.path),
                "project_id": .uuid(project.id),
                "session_id": .uuid(id),
                "prompt": .string(text),
                "base_branch": .null,
            ]))
            guard let path = result["worktree"]?["path"]?.string,
                  let branch = result["worktree"]?["branch"]?.string else {
                throw ConnectionError.malformedResponse
            }
            sessionWorkspace = .worktree(path: path, branch: branch)
        }
        let model = models(for: choice.provider).first { $0.id == choice.modelID }
        let session = try await createSession(
            project: project,
            provider: choice.provider,
            model: model,
            mode: newSessionAccess(for: project.id),
            reasoningEffort: choice.reasoningEffort,
            serviceTier: choice.serviceTier,
            contextWindow: choice.contextWindow,
            agentPreset: probes[choice.provider]?.agentPresets.first(where: \.isDefault)?.id,
            workspace: sessionWorkspace,
            id: id
        )
        await send(text, images: images, files: files)
        return session
    }

    func rename(_ sessionID: UUID, to title: String) async {
        let title = title.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !title.isEmpty else { return }
        await renameSession(sessionID, title: title)
    }

    // MARK: - Workspace operations

    /// Runs a daemon workspace operation and returns its result object.
    func workspaceRequest(_ operation: JSONValue) async throws -> JSONValue {
        let payload = try await client.request(command: .object([
            "type": .string("workspace"),
            "operation": operation,
        ]))
        guard let result = payload["result"] else { throw ConnectionError.malformedResponse }
        return result
    }

    func inspectWorkingTree(_ cwd: String) async throws -> WorkingTreeStatus {
        let result = try await workspaceRequest(.object([
            "type": .string("inspectWorkingTree"),
            "cwd": .string(cwd),
        ]))
        guard let status = result["status"] else { throw ConnectionError.malformedResponse }
        return try status.decode(WorkingTreeStatus.self)
    }

    func stage(_ paths: [String], staged: Bool, in cwd: String) async throws {
        _ = try await workspaceRequest(.object([
            "type": .string("stagePaths"),
            "cwd": .string(cwd),
            "paths": .array(paths.map(JSONValue.string)),
            "staged": .bool(staged),
        ]))
    }

    /// Each changed file's unified diff, read from the working copy as it is now.
    func uncommittedDiffs(_ cwd: String) async throws -> [String: [DiffLine]] {
        let result = try await workspaceRequest(.object([
            "type": .string("collectReviewDiff"),
            "cwd": .string(cwd),
            "source": .string("uncommitted"),
        ]))
        guard let patch = result["data"]?["patch"]?.string else { throw ConnectionError.malformedResponse }
        return await Task.detached(priority: .userInitiated) { UnifiedDiff.files(in: patch) }.value
    }

    func commit(_ message: String, in cwd: String) async throws {
        _ = try await workspaceRequest(.object([
            "type": .string("commit"),
            "cwd": .string(cwd),
            "message": .string(message),
            "include_unstaged": .bool(false),
            "push": .bool(false),
        ]))
    }

    func push(_ cwd: String) async throws {
        _ = try await workspaceRequest(.object([
            "type": .string("push"),
            "cwd": .string(cwd),
        ]))
    }

    /// The agent that writes commit messages and pull requests: the
    /// session's, else the first installed one.
    func gitInvocation(provider: ProviderKind?) -> JSONValue? {
        let probe = provider.flatMap { probes[$0] }.flatMap { $0.installed ? $0 : nil } ?? installedProbes.first
        guard let probe, let binary = probe.path else { return nil }
        return .object([
            "provider": .string(probe.provider.rawValue),
            "binary": .string(binary),
            "model": .null,
            "reasoning_effort": .null,
        ])
    }

    func generateCommitMessage(in cwd: String, provider: ProviderKind?) async throws -> String {
        guard let invocation = gitInvocation(provider: provider) else {
            throw ConnectionError.daemon("No coding agent is installed on the Mac to write the message.")
        }
        let result = try await workspaceRequest(.object([
            "type": .string("generateCommitMessage"),
            "cwd": .string(cwd),
            "include_unstaged": .bool(false),
            "invocation": invocation,
        ]))
        guard let message = result["message"]?.string else { throw ConnectionError.malformedResponse }
        return message
    }

    func createPullRequest(in cwd: String, provider: ProviderKind?) async throws -> URL? {
        guard let invocation = gitInvocation(provider: provider) else {
            throw ConnectionError.daemon("No coding agent is installed on the Mac to write the pull request.")
        }
        let result = try await workspaceRequest(.object([
            "type": .string("createPullRequest"),
            "cwd": .string(cwd),
            "commit_message": .null,
            "include_unstaged": .bool(false),
            "invocation": invocation,
        ]))
        return result["url"]?.string.flatMap(URL.init(string:))
    }

    func worktrees(of project: Project) async throws -> [WorktreeEntry] {
        let result = try await workspaceRequest(.object([
            "type": .string("listWorktrees"),
            "cwd": .string(project.path),
        ]))
        guard let values = result["worktrees"] else { throw ConnectionError.malformedResponse }
        return try values.decode([WorktreeEntry].self)
    }

    /// A file's text, its path relative to `root` or absolute beneath it.
    func readFile(_ path: String, root: String) async throws -> String {
        var relative = path
        if path.hasPrefix("/") {
            let prefix = root.hasSuffix("/") ? root : root + "/"
            guard path.hasPrefix(prefix) else {
                throw ConnectionError.daemon("\(path) is outside this project, so the phone can't open it.")
            }
            relative = String(path.dropFirst(prefix.count))
        }
        relative = relative.replacingOccurrences(of: #"(:\d+(:\d+)?|#L\d+(-L\d+)?)$"#, with: "", options: .regularExpression)
        if relative.hasPrefix("./") { relative.removeFirst(2) }
        let result = try await workspaceRequest(.object([
            "type": .string("readTextFile"),
            "root": .string(root),
            "relative_path": .string(relative),
        ]))
        guard let content = result["content"]?.string else { throw ConnectionError.malformedResponse }
        return content
    }

    /// The provider plan's usage windows, when the provider reports any.
    func planUsage(for provider: ProviderKind) async throws -> PlanUsage? {
        let payload = try await client.request(command: .object([
            "type": .string("fetchPlanUsage"),
            "provider": .string(provider.rawValue),
            "binaryOverride": .optional(binaryOverrides[provider]),
            "cliVersion": .null,
        ]))
        guard let usage = payload["usage"], usage != .null else { return nil }
        return try usage.decode(PlanUsage.self)
    }
}

/// Local notifications for sessions that finish or need the user while the
/// app is in the background.
@MainActor
enum SessionNotifier {
    static func requestPermission() {
        UNUserNotificationCenter.current().requestAuthorization(options: [.alert, .sound]) { _, _ in }
    }

    static func finished(_ session: AgentSession) {
        guard Preferences.shared.notifyOnFinish else { return }
        post(title: session.displayTitle, body: "Finished", id: session.id)
    }

    static func needsInput(_ session: AgentSession) {
        guard Preferences.shared.notifyOnInput else { return }
        post(title: session.displayTitle, body: "Needs your approval", id: session.id)
    }

    private static func post(title: String, body: String, id: UUID) {
        guard UIApplication.shared.applicationState != .active else { return }
        let content = UNMutableNotificationContent()
        content.title = title
        content.body = body
        content.sound = .default
        content.userInfo = ["sessionID": id.uuidString]
        let request = UNNotificationRequest(identifier: "\(id.uuidString)-\(body)", content: content, trigger: nil)
        UNUserNotificationCenter.current().add(request)
    }
}
