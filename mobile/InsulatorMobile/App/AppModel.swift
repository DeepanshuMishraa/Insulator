import Foundation
import Observation

@MainActor
@Observable
final class AppModel {
    var connectionState: ConnectionState = .disconnected
    var projects: [Project] = []
    var sessions: [AgentSession] = []
    var selectedSession: AgentSession?
    var probes: [ProviderKind: ProviderProbe] = [:]
    var pendingPermission: PendingPermission?
    var errorMessage: String?
    var isLoading = false
    var connectedAddress: String?
    /// Host of the Mac saved in the Keychain. Non-nil means this iPhone has
    /// paired before, so a disconnect shows the reconnect screen instead of
    /// the first-time entry form.
    var rememberedHost: String?

    @ObservationIgnored private let client = DaemonClient()
    @ObservationIgnored private var runtimes: [UUID: UUID] = [:]
    @ObservationIgnored private var supportsSteer: [UUID: Bool] = [:]
    @ObservationIgnored private var binaryOverrides: [ProviderKind: String] = [:]
    @ObservationIgnored private var disabledProviders: Set<ProviderKind> = []
    @ObservationIgnored private var refreshTask: Task<Void, Never>?
    @ObservationIgnored private var savedPayload: PairingPayload?

    init() {
        client.onNotice = { [weak self] notice in self?.handle(notice) }
        rememberedHost = CredentialStore.load()?.url.host
    }

    var installedProbes: [ProviderProbe] {
        ProviderKind.allCases.compactMap { probes[$0] }.filter { $0.installed && !disabledProviders.contains($0.provider) }
    }

    var isConnected: Bool {
        if case .connected = connectionState { return true }
        return false
    }

    /// Host shown under the home header: the live connection first, the
    /// remembered Mac second.
    var displayHost: String? {
        if let connectedAddress, let host = URL(string: connectedAddress)?.host {
            return host
        }
        return rememberedHost
    }

    func connectSaved() async {
        guard let payload = CredentialStore.load() else { return }
        rememberedHost = payload.url.host
        await connect(payload, remember: false)
    }

    /// Reconnect to the remembered Mac from the reconnect screen.
    func reconnect() async {
        await connectSaved()
    }

    func connect(address: String, token: String) async {
        let normalized = Self.normalizedAddress(address)
        guard let url = URL(string: normalized) else {
            errorMessage = ConnectionError.invalidAddress.localizedDescription
            return
        }
        do {
            await connect(try PairingPayload(url: url, token: token), remember: true)
        } catch {
            errorMessage = error.localizedDescription
        }
    }

    func disconnect(forget: Bool = false) {
        client.disconnect()
        connectionState = .disconnected
        projects = []
        sessions = []
        probes = [:]
        selectedSession = nil
        runtimes = [:]
        connectedAddress = nil
        if forget {
            CredentialStore.clear()
            savedPayload = nil
            rememberedHost = nil
        }
    }

    func refresh() async {
        guard isConnected else { return }
        do {
            try await loadTaskState()
        } catch {
            errorMessage = error.localizedDescription
        }
    }

    func open(_ session: AgentSession) async {
        selectedSession = session
        do {
            try await hydrate(session.id)
            try await attach(session.id)
        } catch {
            errorMessage = error.localizedDescription
        }
    }

    func createSession(
        project: Project,
        provider: ProviderKind,
        model: ProviderModel?,
        mode: RuntimeMode,
        reasoningEffort: String?,
        serviceTier: String?,
        contextWindow: String?,
        agentPreset: String?
    ) async throws -> AgentSession {
        let now = UInt64(Date().timeIntervalSince1970)
        let session = AgentSession(
            id: UUID(),
            title: "New task",
            autoTitle: nil,
            projectID: project.id,
            provider: provider,
            model: model?.id,
            runtimeMode: mode,
            reasoningEffort: reasoningEffort ?? model?.defaultReasoningEffort,
            serviceTier: serviceTier ?? model?.defaultServiceTier,
            contextWindow: contextWindow ?? model?.defaultContextWindow,
            agentPreset: agentPreset,
            status: .idle,
            createdAt: now,
            updatedAt: now,
            lastReplyAt: nil,
            providerCursor: nil,
            messages: [],
            transcriptBlocks: []
        )
        let encodedSession = try JSONValue.decode(session)
        _ = try await client.request(command: .object([
            "type": .string("saveTaskState"),
            "projects": .array(try projects.map { try JSONValue.decode($0) }),
            "liveSessionIds": .array([]),
            "sessions": .array([encodedSession]),
        ]))
        sessions.insert(session, at: 0)
        selectedSession = session
        return session
    }

    func send(_ text: String) async {
        let prompt = text.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !prompt.isEmpty, let session = selectedSession else { return }
        do {
            let runtimeID = try await ensureRuntime(for: session)
            let messageID = UUID()
            let turnID = UUID()
            appendOptimisticMessage(Message(
                id: messageID,
                turnID: turnID,
                role: .user,
                content: prompt,
                displayContent: nil,
                createdAt: UInt64(Date().timeIntervalSince1970),
                streaming: false
            ))
            _ = try await client.request(
                sessionID: session.id,
                runtimeID: runtimeID,
                command: .object([
                    "type": .string("prompt"),
                    "prompt": .string(prompt),
                    "turnId": .uuid(turnID),
                    "messageId": .uuid(messageID),
                ])
            )
        } catch {
            errorMessage = error.localizedDescription
            scheduleHydrate(session.id, delay: 0)
        }
    }

    func steer(_ text: String) async {
        let prompt = text.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !prompt.isEmpty,
              let session = selectedSession,
              let runtimeID = runtimes[session.id],
              supportsSteer[session.id] == true else { return }
        do {
            _ = try await client.request(
                sessionID: session.id,
                runtimeID: runtimeID,
                command: .object(["type": .string("steer"), "prompt": .string(prompt)])
            )
        } catch {
            errorMessage = error.localizedDescription
        }
    }

    func cancel() async {
        guard let session = selectedSession, let runtimeID = runtimes[session.id] else { return }
        do {
            _ = try await client.request(
                sessionID: session.id,
                runtimeID: runtimeID,
                command: .object(["type": .string("cancel")])
            )
        } catch {
            errorMessage = error.localizedDescription
        }
    }

    func respond(to permission: PendingPermission, option: PermissionOption) async {
        do {
            _ = try await client.request(
                sessionID: permission.sessionID,
                runtimeID: permission.runtimeID,
                command: .object([
                    "type": .string("respond"),
                    "requestId": .string(permission.id),
                    "optionId": .string(option.id),
                ])
            )
            pendingPermission = nil
        } catch {
            errorMessage = error.localizedDescription
        }
    }

    func updateOptions(
        model: ProviderModel?,
        mode: RuntimeMode,
        reasoningEffort: String?,
        serviceTier: String?,
        contextWindow: String?
    ) async {
        guard var session = selectedSession else { return }
        session.model = model?.id
        session.runtimeMode = mode
        session.reasoningEffort = reasoningEffort
        session.serviceTier = serviceTier
        session.contextWindow = contextWindow
        selectedSession = session
        replaceInSessionList(session)

        guard let runtimeID = runtimes[session.id] else { return }
        do {
            _ = try await client.request(
                sessionID: session.id,
                runtimeID: runtimeID,
                command: .object([
                    "type": .string("applyOptions"),
                    "options": .object([
                        "mode": .string(mode.rawValue),
                        "model": .optional(model?.id),
                        "reasoningEffort": .optional(reasoningEffort),
                        "serviceTier": .optional(serviceTier),
                        "contextWindow": .optional(contextWindow),
                    ]),
                ])
            )
        } catch {
            errorMessage = error.localizedDescription
            scheduleHydrate(session.id, delay: 0)
        }
    }

    func models(for provider: ProviderKind) -> [ProviderModel] {
        probes[provider]?.models ?? []
    }

    func project(for session: AgentSession) -> Project? {
        projects.first { $0.id == session.projectID }
    }

    private func connect(_ payload: PairingPayload, remember: Bool) async {
        connectionState = .connecting
        errorMessage = nil
        do {
            let version = try await client.connect(to: payload)
            savedPayload = payload
            connectedAddress = payload.url.absoluteString
            if remember { try CredentialStore.save(payload) }
            rememberedHost = payload.url.host
            connectionState = .connected(version: version)
            try await loadInitialState()
        } catch {
            client.disconnect()
            let message = Self.describeConnectionError(error, url: payload.url)
            connectionState = .failed(message)
            errorMessage = message
        }
    }

    /// URLSession errors are terse ("Could not connect to the server"), so
    /// translate the common ones into the setting the user should check.
    private static func describeConnectionError(_ error: Error, url: URL) -> String {
        let code = (error as NSError).code
        let host = url.host ?? url.absoluteString
        let port = url.port.map(String.init) ?? "the shown"
        switch code {
        case NSURLErrorCannotConnectToHost, NSURLErrorTimedOut:
            return "Could not reach \(host):\(port). Check the Mac is awake with exposure on (Desktop Settings → Daemon), the port matches, and this iPhone is on the same Tailnet."
        case NSURLErrorBadServerResponse:
            return "The Mac rejected the handshake. Check the Tailscale IP and port in Desktop Settings → Daemon, then update Insulator on both devices."
        default:
            return error.localizedDescription
        }
    }

    private func loadInitialState() async throws {
        isLoading = true
        defer { isLoading = false }
        try await loadSettings()
        try await loadTaskState()
        try await probeProviders()
    }

    private func loadSettings() async throws {
        let payload = try await client.request(command: .object(["type": .string("getSettings")]))
        guard let settings = payload["settings"] else { throw ConnectionError.malformedResponse }
        let disabledValues = settings["disabled_providers"]?.array ?? settings["disabledProviders"]?.array ?? []
        disabledProviders = Set(disabledValues.compactMap { $0.string.flatMap(ProviderKind.init(rawValue:)) })
        let overrides = settings["provider_binary_overrides"]?.object ?? settings["providerBinaryOverrides"]?.object ?? [:]
        binaryOverrides = overrides.reduce(into: [:]) { result, entry in
            guard let provider = ProviderKind(rawValue: entry.key), let path = entry.value.string else { return }
            result[provider] = path
        }
    }

    private func loadTaskState() async throws {
        let payload = try await client.request(command: .object(["type": .string("loadTaskState")]))
        guard let projectValues = payload["projects"]?.array,
              let sessionValues = payload["sessions"]?.array else {
            throw ConnectionError.malformedResponse
        }
        projects = try projectValues.map { try $0.decode(Project.self) }
        sessions = try sessionValues
            .map { try $0.decode(AgentSession.self) }
            .filter { $0.title != "__removed__" }
            .sorted { ($0.lastReplyAt ?? $0.createdAt) > ($1.lastReplyAt ?? $1.createdAt) }

        if let selected = selectedSession,
           let projection = sessions.first(where: { $0.id == selected.id }) {
            // List projections deliberately carry no messages or transcript
            // blocks. A catalog refresh arriving mid-stream (e.g. right after
            // our own prompt) must update status and titles but must never
            // wipe the open thread's detail — that blanked the whole view.
            var merged = projection
            if !selected.messages.isEmpty || !selected.transcriptBlocks.isEmpty {
                merged.messages = selected.messages
                merged.transcriptBlocks = selected.transcriptBlocks
            }
            selectedSession = merged
        }
    }

    private func probeProviders() async throws {
        var loaded: [ProviderKind: ProviderProbe] = [:]
        for provider in ProviderKind.allCases {
            let payload = try await client.request(command: .object([
                "type": .string("probeProvider"),
                "provider": .string(provider.rawValue),
                "binaryOverride": .optional(binaryOverrides[provider]),
                "discoverModels": .bool(true),
                "probeVersion": .bool(true),
            ]))
            guard let probeValue = payload["probe"] else { continue }
            let probe = try probeValue.decode(ProviderProbe.self)
            loaded[provider] = probe
        }
        probes = loaded
    }

    private func hydrate(_ sessionID: UUID) async throws {
        let payload = try await client.request(
            sessionID: sessionID,
            command: .object(["type": .string("hydrateSession"), "sessionId": .uuid(sessionID)])
        )
        guard let sessionValue = payload["session"], sessionValue != .null else { return }
        let session = try sessionValue.decode(AgentSession.self)
        selectedSession = session
        replaceInSessionList(session)
    }

    private func attach(_ sessionID: UUID) async throws {
        let payload = try await client.request(
            sessionID: sessionID,
            command: .object(["type": .string("attachSession")])
        )
        if let runtimeID = payload["runtimeId"]?.string.flatMap(UUID.init(uuidString:)) {
            runtimes[sessionID] = runtimeID
        }
        if case .bool(let value)? = payload["supportsSteer"] {
            supportsSteer[sessionID] = value
        }
    }

    private func ensureRuntime(for session: AgentSession) async throws -> UUID {
        if let runtimeID = runtimes[session.id] { return runtimeID }
        try await attach(session.id)
        if let runtimeID = runtimes[session.id] { return runtimeID }

        guard let project = project(for: session),
              let binary = probes[session.provider]?.path else {
            throw ConnectionError.daemon("\(session.provider.name) is not installed on the connected Mac.")
        }
        let runtimeID = UUID()
        let payload = try await client.request(
            sessionID: session.id,
            runtimeID: runtimeID,
            command: .object([
                "type": .string("start"),
                "options": .object([
                    "provider": .string(session.provider.rawValue),
                    "binary": .string(binary),
                    "cwd": .string(project.path),
                    "mode": .string(session.runtimeMode.rawValue),
                    "model": .optional(session.model),
                    "reasoningEffort": .optional(session.reasoningEffort),
                    "serviceTier": .optional(session.serviceTier),
                    "contextWindow": .optional(session.contextWindow),
                    "agentPreset": .optional(session.agentPreset),
                    "computerUseEnabled": .bool(false),
                    "providerCursor": session.providerCursor ?? .null,
                ]),
            ])
        )
        runtimes[session.id] = runtimeID
        if case .bool(let value)? = payload["supportsSteer"] {
            supportsSteer[session.id] = value
        }
        return runtimeID
    }

    private func handle(_ notice: DaemonNotice) {
        switch notice {
        case .taskStateChanged:
            Task { await refresh() }
        case .disconnected(let message):
            connectionState = .failed(message)
            errorMessage = message
        case .event(let sessionID, let runtimeID, let kind, let payload):
            runtimes[sessionID] = runtimeID
            handleEvent(sessionID: sessionID, runtimeID: runtimeID, kind: kind, payload: payload)
        }
    }

    private func handleEvent(sessionID: UUID, runtimeID: UUID, kind: String, payload: JSONValue) {
        switch kind {
        case "textDelta":
            guard let text = payload.string else { return }
            appendTextDelta(text, to: sessionID)
        case "permission":
            guard let requestID = payload["requestId"]?.string,
                  let title = payload["title"]?.string,
                  let optionValues = payload["options"]?.array,
                  let options = try? optionValues.map({ try $0.decode(PermissionOption.self) }) else { return }
            pendingPermission = PendingPermission(
                id: requestID,
                sessionID: sessionID,
                runtimeID: runtimeID,
                title: title,
                detail: payload["detail"]?.string,
                options: options
            )
        case "error":
            errorMessage = payload.string ?? "The agent reported an error."
            scheduleHydrate(sessionID)
        case "turnFinished", "turnParked", "connected", "autoTitleUpdated":
            scheduleHydrate(sessionID)
        default:
            scheduleHydrate(sessionID, delay: 0.35)
        }
    }

    private func appendOptimisticMessage(_ message: Message) {
        guard var session = selectedSession else { return }
        session.messages.append(message)
        session.status = .working
        selectedSession = session
        replaceInSessionList(session)
    }

    private func appendTextDelta(_ delta: String, to sessionID: UUID) {
        guard var session = selectedSession, session.id == sessionID else { return }
        if let last = session.messages.indices.last,
           session.messages[last].role == .assistant,
           session.messages[last].streaming {
            session.messages[last].content += delta
        } else {
            session.messages.append(Message(
                id: UUID(),
                turnID: nil,
                role: .assistant,
                content: delta,
                displayContent: nil,
                createdAt: UInt64(Date().timeIntervalSince1970),
                streaming: true
            ))
        }
        session.status = .working
        selectedSession = session
    }

    private func scheduleHydrate(_ sessionID: UUID, delay: TimeInterval = 0.2) {
        guard selectedSession?.id == sessionID else { return }
        refreshTask?.cancel()
        refreshTask = Task { [weak self] in
            if delay > 0 {
                try? await Task.sleep(for: .seconds(delay))
            }
            guard !Task.isCancelled else { return }
            do {
                try await self?.hydrate(sessionID)
            } catch {
                self?.errorMessage = error.localizedDescription
            }
        }
    }

    private func replaceInSessionList(_ session: AgentSession) {
        if let index = sessions.firstIndex(where: { $0.id == session.id }) {
            sessions[index] = session
        } else {
            sessions.insert(session, at: 0)
        }
    }

    private static func normalizedAddress(_ address: String) -> String {
        let trimmed = address.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty else { return trimmed }
        let withScheme = trimmed.contains("://") ? trimmed : "ws://\(trimmed)"
        guard var components = URLComponents(string: withScheme) else { return withScheme }
        // The daemon only serves /v1; without it the handshake gets a plain
        // HTTP 404 that URLSession reports as "bad response from server".
        if components.path.isEmpty || components.path == "/" {
            components.path = "/v1"
        }
        return components.string ?? withScheme
    }
}

private extension JSONValue {
    static func decode<T: Encodable>(_ value: T) throws -> JSONValue {
        try JSONDecoder().decode(JSONValue.self, from: JSONEncoder().encode(value))
    }
}
