import Foundation
import Observation
import UIKit

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
    var transcriptRevision = 0
    var connectedAddress: String?
    /// Host of the Mac saved in the Keychain. Non-nil means this iPhone has
    /// paired before, so a disconnect shows the reconnect screen instead of the
    /// first-time entry form.
    var rememberedHost: String?
    /// Transcript images live on the Mac. Previews are fetched by blob reference
    /// and cached here so a thumbnail and the full-screen viewer share one read.
    let attachmentImages: AttachmentImageStore

    @ObservationIgnored private let client: DaemonClient
    @ObservationIgnored private var runtimes: [UUID: UUID] = [:]
    @ObservationIgnored private var supportsSteer: [UUID: Bool] = [:]
    @ObservationIgnored private var activityIDs: [UUID: [String: String]] = [:]
    @ObservationIgnored private var binaryOverrides: [ProviderKind: String] = [:]
    @ObservationIgnored private var disabledProviders: Set<ProviderKind> = []
    @ObservationIgnored private var refreshTask: Task<Void, Never>?
    @ObservationIgnored private var savedPayload: PairingPayload?
    @ObservationIgnored private var pendingTextDeltas: [String] = []
    @ObservationIgnored private var pendingReasoningDeltas: [String] = []
    @ObservationIgnored private var pendingDeltasSessionID: UUID?
    @ObservationIgnored private var streamCommitTask: Task<Void, Never>?

    init() {
        let client = DaemonClient()
        self.client = client
        // The store reads blobs through the client, captured directly so this
        // does not need a fully initialized `self`.
        attachmentImages = AttachmentImageStore { reference in
            try await Self.readBlobData(reference: reference, using: client)
        }
        client.onNotice = { [weak self] notice in self?.handle(notice) }
        rememberedHost = CredentialStore.load()?.url.host
    }

    /// The Mac forwards one event per provider delta, and every delta re-rendered
    /// the whole transcript — a Markdown re-parse per token. Deltas are committed
    /// at a fixed cadence instead, which reads as a faster stream rather than a
    /// stuttering one.
    private static let streamCommitInterval = Duration.milliseconds(80)

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
        flushStreamDeltas()
        streamCommitTask?.cancel()
        streamCommitTask = nil
        client.disconnect()
        connectionState = .disconnected
        projects = []
        sessions = []
        probes = [:]
        selectedSession = nil
        runtimes = [:]
        activityIDs = [:]
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
        flushStreamDeltas()
        selectedSession = session
        do {
            try await hydrate(session.id)
            try await attach(session.id)
        } catch {
            errorMessage = error.localizedDescription
        }
    }

    func createNewSession(project: Project? = nil) async throws -> AgentSession {
        let resolvedProject: Project
        if let project {
            resolvedProject = project
        } else if let first = projects.first {
            resolvedProject = first
        } else {
            let defaultProject = Project(
                id: UUID(),
                name: "Quick Chat",
                path: "~",
                createdAt: UInt64(Date().timeIntervalSince1970)
            )
            projects = [defaultProject]
            resolvedProject = defaultProject
        }

        let provider = installedProbes.first?.provider ?? .pi
        let availableModels = models(for: provider)
        let model = availableModels.first(where: \.isDefault) ?? availableModels.first
        let preset = probes[provider]?.agentPresets.first(where: \.isDefault)?.id

        return try await createSession(
            project: resolvedProject,
            provider: provider,
            model: model,
            mode: .ask,
            reasoningEffort: model?.defaultReasoningEffort,
            serviceTier: model?.defaultServiceTier,
            contextWindow: model?.defaultContextWindow,
            agentPreset: preset
        )
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
            title: "New thread",
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
            transcriptBlocks: [],
            turns: []
        )
        var updatedProjects = projects
        if !updatedProjects.contains(where: { $0.id == project.id }) {
            updatedProjects.append(project)
            projects = updatedProjects
        }
        let encodedSession = try JSONValue.decode(session)
        _ = try await client.request(command: .object([
            "type": .string("saveTaskState"),
            "projects": .array(try updatedProjects.map { try JSONValue.decode($0) }),
            "liveSessionIds": .array([]),
            "sessions": .array([encodedSession]),
        ]))
        sessions.insert(session, at: 0)
        selectedSession = session
        return session
    }

    func send(_ text: String, images: [UIImage] = []) async {
        let prompt = text.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !prompt.isEmpty || !images.isEmpty, let session = selectedSession else { return }
        do {
            let runtimeID = try await ensureRuntime(for: session)
            let attachments = try await upload(images: images)
            guard let submission = Self.submission(prompt: prompt, attachments: attachments) else { return }
            let messageID = UUID()
            let turnID = UUID()
            appendOptimisticMessage(Message(
                id: messageID,
                turnID: turnID,
                role: .user,
                content: submission.transport,
                displayContent: submission.display,
                attachments: attachments,
                createdAt: UInt64(Date().timeIntervalSince1970),
                streaming: false
            ))
            try await saveSession(session.id)
            _ = try await client.request(
                sessionID: session.id,
                runtimeID: runtimeID,
                command: .object([
                    "type": .string("prompt"),
                    "prompt": .string(submission.transport),
                    "turnId": .uuid(turnID),
                    "messageId": .uuid(messageID),
                ])
            )
        } catch {
            errorMessage = error.localizedDescription
            scheduleHydrate(session.id, delay: 0)
        }
    }

    func steer(_ text: String, images: [UIImage] = []) async {
        let prompt = text.trimmingCharacters(in: .whitespacesAndNewlines)
        guard let session = selectedSession,
              let runtimeID = runtimes[session.id],
              supportsSteer[session.id] == true else { return }
        do {
            let attachments = try await upload(images: images)
            guard let submission = Self.submission(prompt: prompt, attachments: attachments) else { return }
            _ = try await client.request(
                sessionID: session.id,
                runtimeID: runtimeID,
                command: .object([
                    "type": .string("steer"),
                    "prompt": .string(submission.transport),
                ])
            )
        } catch {
            errorMessage = error.localizedDescription
        }
    }

    /// Splits a submission into the text the provider receives and the text the
    /// user sees. Attachment mentions are transport-only, exactly as on the Mac,
    /// so the user bubble shows what was typed and nothing else.
    private static func submission(
        prompt: String,
        attachments: [MessageAttachment]
    ) -> (transport: String, display: String?)? {
        let mentions = attachments.map { "@\($0.mention)" }.joined(separator: " ")
        let transport: String
        switch (prompt.isEmpty, mentions.isEmpty) {
        case (true, true): return nil
        case (false, true): transport = prompt
        case (true, false): transport = mentions
        case (false, false): transport = "\(prompt) \(mentions)"
        }
        return (transport, attachments.isEmpty ? nil : prompt)
    }

    /// Uploads picked images into the daemon's blob store and returns the
    /// attachments the provider will see. The local pixels are cached against
    /// the new reference so the transcript row draws without another round trip.
    private func upload(images: [UIImage]) async throws -> [MessageAttachment] {
        var attachments: [MessageAttachment] = []
        for image in images {
            guard let prepared = await ComposerImage.prepare(image) else { continue }
            let payload = try await client.request(command: .object([
                "type": .string("storeBlob"),
                "mimeType": .string(prepared.mimeType),
                "bytes": .string(prepared.data.base64EncodedString()),
            ]))
            guard let reference = payload["reference"]?.string, let path = payload["path"]?.string else {
                throw ConnectionError.malformedResponse
            }
            attachmentImages.adopt(prepared.preview, for: reference)
            attachments.append(MessageAttachment(
                path: path,
                mention: path,
                name: "image\(attachments.count == 0 ? "" : "-\(attachments.count + 1)").\(prepared.fileExtension)",
                isDir: false,
                isImage: true,
                blobReference: reference
            ))
        }
        return attachments
    }

    /// Reads blob bytes the daemon owns. The transcript keeps only a reference,
    /// so this is the only way back to a picture the user sent earlier.
    private static func readBlobData(reference: String, using client: DaemonClient) async throws -> Data {
        let payload = try await client.request(command: .object([
            "type": .string("readBlob"),
            "reference": .string(reference),
        ]))
        guard let encoded = payload["bytes"]?.string else { throw ConnectionError.malformedResponse }
        // A photo is megabytes of base64; decoding it on the main actor would
        // drop frames while the user is scrolling.
        let data = await Task.detached(priority: .userInitiated) {
            Data(base64Encoded: encoded)
        }.value
        guard let data else { throw ConnectionError.malformedResponse }
        return data
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
            updateSession(permission.sessionID) { $0.status = .working }
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

        if let runtimeID = runtimes[session.id] {
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
                try await saveSession(session.id)
            } catch {
                errorMessage = error.localizedDescription
                scheduleHydrate(session.id, delay: 0)
            }
        } else {
            do {
                _ = try await client.request(command: .object([
                    "type": .string("saveTaskState"),
                    "projects": .array(try projects.map { try JSONValue.decode($0) }),
                    "liveSessionIds": .array([]),
                    "sessions": .array([try JSONValue.decode(session)]),
                ]))
            } catch {
                errorMessage = error.localizedDescription
            }
        }
    }

    func updateSessionProject(_ sessionID: UUID, projectID: UUID) async {
        guard var session = session(withID: sessionID) else { return }
        session.projectID = projectID
        if selectedSession?.id == sessionID {
            selectedSession = session
        }
        replaceInSessionList(session)
        do {
            _ = try await client.request(command: .object([
                "type": .string("saveTaskState"),
                "projects": .array(try projects.map { try JSONValue.decode($0) }),
                "liveSessionIds": .array([]),
                "sessions": .array([try JSONValue.decode(session)]),
            ]))
        } catch {
            errorMessage = error.localizedDescription
        }
    }

    func updateSessionProvider(_ sessionID: UUID, provider: ProviderKind) async {
        guard var session = session(withID: sessionID) else { return }
        let availableModels = models(for: provider)
        let defaultModel = availableModels.first(where: \.isDefault) ?? availableModels.first
        session.provider = provider
        session.model = defaultModel?.id
        session.reasoningEffort = defaultModel?.defaultReasoningEffort
        session.serviceTier = defaultModel?.defaultServiceTier
        session.contextWindow = defaultModel?.defaultContextWindow
        session.agentPreset = probes[provider]?.agentPresets.first(where: \.isDefault)?.id
        session.updatedAt = UInt64(Date().timeIntervalSince1970)
        if selectedSession?.id == sessionID {
            selectedSession = session
        }
        replaceInSessionList(session)
        do {
            _ = try await client.request(command: .object([
                "type": .string("saveTaskState"),
                "projects": .array(try projects.map { try JSONValue.decode($0) }),
                "liveSessionIds": .array([]),
                "sessions": .array([try JSONValue.decode(session)]),
            ]))
        } catch {
            errorMessage = error.localizedDescription
        }
    }

    func updateSessionMode(_ sessionID: UUID, mode: RuntimeMode) async {
        guard let session = session(withID: sessionID) else { return }
        let currentModel = models(for: session.provider).first(where: { $0.id == session.model })
        await updateOptions(
            model: currentModel,
            mode: mode,
            reasoningEffort: session.reasoningEffort,
            serviceTier: session.serviceTier,
            contextWindow: session.contextWindow
        )
    }

    func updateSessionModel(_ sessionID: UUID, model: ProviderModel) async {
        guard let session = session(withID: sessionID) else { return }
        await updateOptions(
            model: model,
            mode: session.runtimeMode,
            reasoningEffort: model.defaultReasoningEffort ?? session.reasoningEffort,
            serviceTier: model.defaultServiceTier ?? session.serviceTier,
            contextWindow: model.defaultContextWindow ?? session.contextWindow
        )
    }

    func updateSessionReasoning(_ sessionID: UUID, reasoningEffort: String?) async {
        guard let session = session(withID: sessionID) else { return }
        let currentModel = models(for: session.provider).first(where: { $0.id == session.model })
        await updateOptions(
            model: currentModel,
            mode: session.runtimeMode,
            reasoningEffort: reasoningEffort,
            serviceTier: session.serviceTier,
            contextWindow: session.contextWindow
        )
    }

    func toggleFastMode(_ sessionID: UUID) async {
        guard let session = session(withID: sessionID) else { return }
        let newTier: String? = session.serviceTier == "fast" ? nil : "fast"
        let currentModel = models(for: session.provider).first(where: { $0.id == session.model })
        await updateOptions(
            model: currentModel,
            mode: session.runtimeMode,
            reasoningEffort: session.reasoningEffort,
            serviceTier: newTier,
            contextWindow: session.contextWindow
        )
    }

    func togglePlanMode(_ sessionID: UUID) async {
        guard var session = session(withID: sessionID) else { return }
        let currentPreset = session.agentPreset
        let newPreset: String? = currentPreset == "plan" ? nil : "plan"
        session.agentPreset = newPreset
        if selectedSession?.id == sessionID {
            selectedSession = session
        }
        replaceInSessionList(session)
        do {
            _ = try await client.request(command: .object([
                "type": .string("saveTaskState"),
                "projects": .array(try projects.map { try JSONValue.decode($0) }),
                "liveSessionIds": .array([]),
                "sessions": .array([try JSONValue.decode(session)]),
            ]))
        } catch {
            errorMessage = error.localizedDescription
        }
    }

    func deleteSession(_ sessionID: UUID) async {
        guard let session = session(withID: sessionID) else { return }
        var tombstone = session
        tombstone.title = "__removed__"
        sessions.removeAll { $0.id == sessionID }
        if selectedSession?.id == sessionID {
            selectedSession = nil
        }
        do {
            _ = try await client.request(command: .object([
                "type": .string("saveTaskState"),
                "projects": .array(try projects.map { try JSONValue.decode($0) }),
                "liveSessionIds": .array([]),
                "sessions": .array([try JSONValue.decode(tombstone)]),
            ]))
        } catch {
            errorMessage = error.localizedDescription
        }
    }

    func renameSession(_ sessionID: UUID, title: String) async {
        guard var session = session(withID: sessionID) else { return }
        session.title = title
        if selectedSession?.id == sessionID {
            selectedSession = session
        }
        replaceInSessionList(session)
        do {
            _ = try await client.request(command: .object([
                "type": .string("saveTaskState"),
                "projects": .array(try projects.map { try JSONValue.decode($0) }),
                "liveSessionIds": .array([]),
                "sessions": .array([try JSONValue.decode(session)]),
            ]))
        } catch {
            errorMessage = error.localizedDescription
        }
    }

    var allAvailableProviders: [ProviderKind] {
        var providers: [ProviderKind] = []
        for probe in installedProbes {
            providers.append(probe.provider)
        }
        for kind in [ProviderKind.claude, .codex, .deepSeek, .grok, .pi, .cursor, .kimi, .devin, .openCode, .openCode2, .fx, .ohMyPi, .amp] {
            if !providers.contains(kind) {
                providers.append(kind)
            }
        }
        return providers
    }

    func models(for provider: ProviderKind) -> [ProviderModel] {
        let discovered = probes[provider]?.models ?? []
        if !discovered.isEmpty {
            return discovered
        }
        return Self.fallbackModels(for: provider)
    }

    static let standardReasoning: [ProviderModelOption] = [
        ProviderModelOption(id: "low", label: "Low", description: nil),
        ProviderModelOption(id: "medium", label: "Medium", description: nil),
        ProviderModelOption(id: "high", label: "High", description: nil),
        ProviderModelOption(id: "max", label: "Max", description: nil)
    ]

    static func fallbackModels(for provider: ProviderKind) -> [ProviderModel] {
        switch provider {
        case .codex:
            return [
                ProviderModel(id: "gpt-5.6-sol", name: "GPT-5.6 Sol", subProvider: "OpenAI", isDefault: true, reasoningEfforts: standardReasoning, defaultReasoningEffort: "medium", serviceTiers: [], defaultServiceTier: nil, contextWindows: [], defaultContextWindow: nil),
                ProviderModel(id: "gpt-5.6-terra", name: "GPT-5.6 Terra", subProvider: "OpenAI", isDefault: false, reasoningEfforts: standardReasoning, defaultReasoningEffort: "medium", serviceTiers: [], defaultServiceTier: nil, contextWindows: [], defaultContextWindow: nil),
                ProviderModel(id: "gpt-5.6-luna", name: "GPT-5.6 Luna", subProvider: "OpenAI", isDefault: false, reasoningEfforts: standardReasoning, defaultReasoningEffort: "medium", serviceTiers: [], defaultServiceTier: nil, contextWindows: [], defaultContextWindow: nil),
                ProviderModel(id: "gpt-5.5", name: "GPT-5.5", subProvider: "OpenAI", isDefault: false, reasoningEfforts: standardReasoning, defaultReasoningEffort: "medium", serviceTiers: [], defaultServiceTier: nil, contextWindows: [], defaultContextWindow: nil),
                ProviderModel(id: "gpt-5.4", name: "GPT-5.4", subProvider: "OpenAI", isDefault: false, reasoningEfforts: standardReasoning, defaultReasoningEffort: "medium", serviceTiers: [], defaultServiceTier: nil, contextWindows: [], defaultContextWindow: nil),
                ProviderModel(id: "gpt-4o", name: "GPT-4o", subProvider: "OpenAI", isDefault: false, reasoningEfforts: [], defaultReasoningEffort: nil, serviceTiers: [], defaultServiceTier: nil, contextWindows: [], defaultContextWindow: nil),
                ProviderModel(id: "o3-mini", name: "o3-mini", subProvider: "OpenAI", isDefault: false, reasoningEfforts: standardReasoning, defaultReasoningEffort: "medium", serviceTiers: [], defaultServiceTier: nil, contextWindows: [], defaultContextWindow: nil),
                ProviderModel(id: "o1", name: "o1", subProvider: "OpenAI", isDefault: false, reasoningEfforts: standardReasoning, defaultReasoningEffort: "high", serviceTiers: [], defaultServiceTier: nil, contextWindows: [], defaultContextWindow: nil)
            ]
        case .claude:
            return [
                ProviderModel(id: "claude-sonnet-5", name: "Claude Sonnet 5", subProvider: "Anthropic", isDefault: true, reasoningEfforts: standardReasoning, defaultReasoningEffort: "medium", serviceTiers: [], defaultServiceTier: nil, contextWindows: [], defaultContextWindow: nil),
                ProviderModel(id: "claude-opus-5", name: "Claude Opus 5", subProvider: "Anthropic", isDefault: false, reasoningEfforts: standardReasoning, defaultReasoningEffort: "high", serviceTiers: [], defaultServiceTier: nil, contextWindows: [], defaultContextWindow: nil),
                ProviderModel(id: "claude-fable-5", name: "Claude Fable 5", subProvider: "Anthropic", isDefault: false, reasoningEfforts: standardReasoning, defaultReasoningEffort: "medium", serviceTiers: [], defaultServiceTier: nil, contextWindows: [], defaultContextWindow: nil),
                ProviderModel(id: "claude-3-7-sonnet", name: "Claude 3.7 Sonnet", subProvider: "Anthropic", isDefault: false, reasoningEfforts: standardReasoning, defaultReasoningEffort: "medium", serviceTiers: [], defaultServiceTier: nil, contextWindows: [], defaultContextWindow: nil),
                ProviderModel(id: "claude-3-5-sonnet", name: "Claude 3.5 Sonnet", subProvider: "Anthropic", isDefault: false, reasoningEfforts: [], defaultReasoningEffort: nil, serviceTiers: [], defaultServiceTier: nil, contextWindows: [], defaultContextWindow: nil),
                ProviderModel(id: "claude-haiku-4-5", name: "Claude Haiku 4.5", subProvider: "Anthropic", isDefault: false, reasoningEfforts: [], defaultReasoningEffort: nil, serviceTiers: [], defaultServiceTier: nil, contextWindows: [], defaultContextWindow: nil)
            ]
        case .deepSeek:
            return [
                ProviderModel(id: "deepseek-chat", name: "DeepSeek V3", subProvider: "DeepSeek", isDefault: true, reasoningEfforts: [], defaultReasoningEffort: nil, serviceTiers: [], defaultServiceTier: nil, contextWindows: [], defaultContextWindow: nil),
                ProviderModel(id: "deepseek-reasoner", name: "DeepSeek R1", subProvider: "DeepSeek", isDefault: false, reasoningEfforts: standardReasoning, defaultReasoningEffort: "medium", serviceTiers: [], defaultServiceTier: nil, contextWindows: [], defaultContextWindow: nil)
            ]
        case .grok:
            return [
                ProviderModel(id: "grok-3", name: "Grok 3", subProvider: "xAI", isDefault: true, reasoningEfforts: standardReasoning, defaultReasoningEffort: "medium", serviceTiers: [], defaultServiceTier: nil, contextWindows: [], defaultContextWindow: nil),
                ProviderModel(id: "grok-3-mini", name: "Grok 3 Mini", subProvider: "xAI", isDefault: false, reasoningEfforts: standardReasoning, defaultReasoningEffort: "low", serviceTiers: [], defaultServiceTier: nil, contextWindows: [], defaultContextWindow: nil),
                ProviderModel(id: "grok-2", name: "Grok 2", subProvider: "xAI", isDefault: false, reasoningEfforts: [], defaultReasoningEffort: nil, serviceTiers: [], defaultServiceTier: nil, contextWindows: [], defaultContextWindow: nil)
            ]
        case .cursor:
            return [
                ProviderModel(id: "auto", name: "Auto", subProvider: "Cursor", isDefault: true, reasoningEfforts: [], defaultReasoningEffort: nil, serviceTiers: [], defaultServiceTier: nil, contextWindows: [], defaultContextWindow: nil),
                ProviderModel(id: "cursor-small", name: "Cursor Small", subProvider: "Cursor", isDefault: false, reasoningEfforts: [], defaultReasoningEffort: nil, serviceTiers: [], defaultServiceTier: nil, contextWindows: [], defaultContextWindow: nil)
            ]
        case .pi:
            return [
                ProviderModel(id: "pi-default", name: "Pi Agent", subProvider: "Inflection", isDefault: true, reasoningEfforts: standardReasoning, defaultReasoningEffort: "medium", serviceTiers: [], defaultServiceTier: nil, contextWindows: [], defaultContextWindow: nil),
                ProviderModel(id: "pi-fast", name: "Pi Fast", subProvider: "Inflection", isDefault: false, reasoningEfforts: [], defaultReasoningEffort: nil, serviceTiers: [], defaultServiceTier: nil, contextWindows: [], defaultContextWindow: nil)
            ]
        case .ohMyPi:
            return [
                ProviderModel(id: "ohmypi-default", name: "Oh My Pi Agent", subProvider: "OhMyPi", isDefault: true, reasoningEfforts: standardReasoning, defaultReasoningEffort: "medium", serviceTiers: [], defaultServiceTier: nil, contextWindows: [], defaultContextWindow: nil)
            ]
        case .kimi:
            return [
                ProviderModel(id: "moonshot-v1-auto", name: "Kimi Auto", subProvider: "Moonshot", isDefault: true, reasoningEfforts: [], defaultReasoningEffort: nil, serviceTiers: [], defaultServiceTier: nil, contextWindows: [], defaultContextWindow: nil),
                ProviderModel(id: "k1", name: "Kimi k1", subProvider: "Moonshot", isDefault: false, reasoningEfforts: standardReasoning, defaultReasoningEffort: "medium", serviceTiers: [], defaultServiceTier: nil, contextWindows: [], defaultContextWindow: nil)
            ]
        case .devin:
            return [
                ProviderModel(id: "devin-default", name: "Devin Agent", subProvider: "Cognition", isDefault: true, reasoningEfforts: standardReasoning, defaultReasoningEffort: "medium", serviceTiers: [], defaultServiceTier: nil, contextWindows: [], defaultContextWindow: nil)
            ]
        case .openCode:
            return [
                ProviderModel(id: "opencode-default", name: "OpenCode Default", subProvider: "OpenCode", isDefault: true, reasoningEfforts: [], defaultReasoningEffort: nil, serviceTiers: [], defaultServiceTier: nil, contextWindows: [], defaultContextWindow: nil)
            ]
        case .openCode2:
            return [
                ProviderModel(id: "opencode2-default", name: "OpenCode 2 Default", subProvider: "OpenCode", isDefault: true, reasoningEfforts: [], defaultReasoningEffort: nil, serviceTiers: [], defaultServiceTier: nil, contextWindows: [], defaultContextWindow: nil)
            ]
        case .fx:
            return [
                ProviderModel(id: "factory-default", name: "Factory Default", subProvider: "Factory", isDefault: true, reasoningEfforts: [], defaultReasoningEffort: nil, serviceTiers: [], defaultServiceTier: nil, contextWindows: [], defaultContextWindow: nil)
            ]
        case .amp:
            return [
                ProviderModel(id: "medium", name: "Medium", subProvider: "Amp", isDefault: true, reasoningEfforts: [], defaultReasoningEffort: nil, serviceTiers: [], defaultServiceTier: nil, contextWindows: [], defaultContextWindow: nil),
                ProviderModel(id: "high", name: "High", subProvider: "Amp", isDefault: false, reasoningEfforts: [], defaultReasoningEffort: nil, serviceTiers: [], defaultServiceTier: nil, contextWindows: [], defaultContextWindow: nil),
                ProviderModel(id: "ultra", name: "Ultra", subProvider: "Amp", isDefault: false, reasoningEfforts: [], defaultReasoningEffort: nil, serviceTiers: [], defaultServiceTier: nil, contextWindows: [], defaultContextWindow: nil),
                ProviderModel(id: "low", name: "Low", subProvider: "Amp", isDefault: false, reasoningEfforts: [], defaultReasoningEffort: nil, serviceTiers: [], defaultServiceTier: nil, contextWindows: [], defaultContextWindow: nil)
            ]
        }
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
            merged.runtimeMode = selected.runtimeMode
            merged.reasoningEffort = selected.reasoningEffort
            merged.serviceTier = selected.serviceTier
            merged.contextWindow = selected.contextWindow
            merged.agentPreset = selected.agentPreset
            merged.providerCursor = selected.providerCursor
            if !selected.messages.isEmpty || !selected.transcriptBlocks.isEmpty || !selected.turns.isEmpty {
                merged.messages = selected.messages
                merged.transcriptBlocks = selected.transcriptBlocks
                merged.turns = selected.turns
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
        var session = try sessionValue.decode(AgentSession.self)
        if let local = selectedSession, local.id == sessionID, !local.messages.isEmpty {
            let localMessages = Dictionary(uniqueKeysWithValues: local.messages.map { ($0.id, $0) })
            let hydratedIDs = Set(session.messages.map(\.id))
            session.messages = session.messages.map { localMessages[$0.id] ?? $0 }
                + local.messages.filter { !hydratedIDs.contains($0.id) }
            if session.transcriptBlocks.isEmpty {
                session.transcriptBlocks = local.transcriptBlocks
            }
            let hydratedTurnIDs = Set(session.turns.map(\.id))
            session.turns += local.turns.filter { !hydratedTurnIDs.contains($0.id) }
        }
        updateSelectedSession(session)
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
        case "promptSubmitted":
            guard var session = selectedSession, session.id == sessionID else { return }
            if let messageID = payload["messageId"]?.string.flatMap(UUID.init(uuidString:)),
               !session.messages.contains(where: { $0.id == messageID }),
               let content = payload["message"]?.string {
                let turnID = payload["turnId"]?.string.flatMap(UUID.init(uuidString:))
                let now = UInt64(Date().timeIntervalSince1970)
                session.messages.append(Message(
                    id: messageID,
                    turnID: turnID,
                    role: .user,
                    content: content,
                    displayContent: nil,
                    createdAt: now,
                    streaming: false
                ))
                if let turnID {
                    session.turns.append(AgentTurn(
                        id: turnID,
                        turnCount: session.turns.count + 1,
                        status: "running",
                        providerTurnStarted: false,
                        providerResumeAt: nil,
                        startedAt: now,
                        completedAt: nil,
                        checkpoint: nil
                    ))
                }
            }
            session.status = .working
            updateSelectedSession(session)
        case "connected":
            updateSession(sessionID) { session in
                session.providerCursor = payload == .null ? nil : payload
                if session.status == .connecting {
                    session.status = .working
                }
            }
        case "agentPresetSelected":
            updateSession(sessionID) { $0.agentPreset = payload.string }
        case "turnStarted":
            updateSession(sessionID) { session in
                session.status = .working
                if let index = session.turns.indices.last,
                   session.turns[index].status == "running" {
                    session.turns[index].providerTurnStarted = true
                }
            }
        case "textDelta":
            guard let text = payload.string else { return }
            enqueueTextDelta(text, from: sessionID)
        case "reasoningDelta":
            guard let text = payload.string else { return }
            enqueueReasoningDelta(text, from: sessionID)
        case "activity":
            guard let activity = activity(from: payload, sessionID: sessionID) else { return }
            appendActivity(activity, to: sessionID)
        case "richActivity":
            guard let activity = try? payload.decode(ActivityItem.self) else { return }
            appendActivity(activity, to: sessionID)
        case "turnParked":
            updateSession(sessionID) { $0.status = .waiting }
            persistSession(sessionID)
        case "turnFinished":
            flushStreamDeltas()
            updateSession(sessionID) { session in
                session.status = .idle
                session.messages.indices.forEach { session.messages[$0].streaming = false }
                if let index = session.turns.indices.last,
                   session.turns[index].status == "running" {
                    session.turns[index].status = "completed"
                    session.turns[index].completedAt = UInt64(Date().timeIntervalSince1970)
                }
                session.transcriptBlocks = session.transcriptBlocks.map { block in
                    guard case .reasoning(let reasoning) = block.content,
                          reasoning.finishedAt == 0 else { return block }
                    return TranscriptBlock(
                        afterMessage: block.afterMessage,
                        turnID: block.turnID,
                        content: .reasoning(ReasoningBlock(
                            content: reasoning.content,
                            startedAt: reasoning.startedAt,
                            finishedAt: UInt64(Date().timeIntervalSince1970 * 1_000)
                        ))
                    )
                }
                let now = UInt64(Date().timeIntervalSince1970)
                session.updatedAt = now
                session.lastReplyAt = now
            }
            persistSession(sessionID)
        case "autoTitleUpdated":
            updateSession(sessionID) { $0.autoTitle = payload.string }
            persistSession(sessionID)
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
            updateSession(sessionID) { $0.status = .waiting }
        case "error":
            flushStreamDeltas()
            errorMessage = payload.string ?? "The agent reported an error."
            updateSession(sessionID) { session in
                session.status = .failed
                if let index = session.turns.indices.last,
                   session.turns[index].status == "running" {
                    session.turns[index].status = "failed"
                    session.turns[index].completedAt = UInt64(Date().timeIntervalSince1970)
                }
            }
            persistSession(sessionID)
        case "processExited":
            updateSession(sessionID) { session in
                session.status = .idle
                if let index = session.turns.indices.last,
                   session.turns[index].status == "running" {
                    session.turns[index].status = "interrupted"
                    session.turns[index].completedAt = UInt64(Date().timeIntervalSince1970)
                }
            }
            persistSession(sessionID)
        default:
            break
        }
    }

    private func appendOptimisticMessage(_ message: Message) {
        guard var session = selectedSession else { return }
        session.messages.append(message)
        if let turnID = message.turnID,
           !session.turns.contains(where: { $0.id == turnID }) {
            session.turns.append(AgentTurn(
                id: turnID,
                turnCount: session.turns.count + 1,
                status: "running",
                providerTurnStarted: false,
                providerResumeAt: nil,
                startedAt: message.createdAt,
                completedAt: nil,
                checkpoint: nil
            ))
        }
        session.status = .working
        session.updatedAt = message.createdAt
        session.lastReplyAt = message.createdAt
        updateSelectedSession(session)
    }

    private func enqueueTextDelta(_ delta: String, from sessionID: UUID) {
        pendingTextDeltas.append(delta)
        noteDeltas(from: sessionID)
        scheduleStreamCommit()
    }

    private func enqueueReasoningDelta(_ delta: String, from sessionID: UUID) {
        pendingReasoningDeltas.append(delta)
        noteDeltas(from: sessionID)
        scheduleStreamCommit()
    }

    /// A run of deltas belongs to one session. Switching mid-run commits what is
    /// buffered so two turns never merge into the same message.
    private func noteDeltas(from sessionID: UUID) {
        if pendingDeltasSessionID != sessionID {
            flushStreamDeltas()
        }
        pendingDeltasSessionID = sessionID
    }

    private func scheduleStreamCommit() {
        guard streamCommitTask == nil else { return }
        streamCommitTask = Task { [weak self] in
            try? await Task.sleep(for: Self.streamCommitInterval)
            guard !Task.isCancelled else { return }
            self?.flushStreamDeltas()
        }
    }

    /// Applies buffered deltas in arrival order. Both buffers drain on every
    /// commit, so a reasoning burst never waits behind a text burst.
    private func flushStreamDeltas() {
        streamCommitTask = nil
        let sessionID = pendingDeltasSessionID
        let text = pendingTextDeltas.joined()
        let reasoning = pendingReasoningDeltas.joined()
        pendingTextDeltas.removeAll()
        pendingReasoningDeltas.removeAll()
        pendingDeltasSessionID = nil
        guard let sessionID else { return }
        if !text.isEmpty { appendTextDelta(text, to: sessionID) }
        if !reasoning.isEmpty { appendReasoningDelta(reasoning, to: sessionID) }
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
                turnID: session.turns.last(where: { $0.status == "running" })?.id
                    ?? session.messages.last(where: { $0.role == .user })?.turnID,
                role: .assistant,
                content: delta,
                displayContent: nil,
                createdAt: UInt64(Date().timeIntervalSince1970),
                streaming: true
            ))
        }
        session.status = .working
        updateSelectedSession(session)
    }

    private func appendReasoningDelta(_ delta: String, to sessionID: UUID) {
        updateSession(sessionID) { session in
            let turnID = session.messages.last(where: { $0.role == .user })?.turnID
            if let index = session.transcriptBlocks.indices.last,
               session.transcriptBlocks[index].turnID == turnID,
               case .reasoning(let reasoning) = session.transcriptBlocks[index].content {
                session.transcriptBlocks[index] = TranscriptBlock(
                    afterMessage: session.transcriptBlocks[index].afterMessage,
                    turnID: turnID,
                    content: .reasoning(ReasoningBlock(
                        content: reasoning.content + delta,
                        startedAt: reasoning.startedAt,
                        finishedAt: 0
                    ))
                )
            } else {
                session.transcriptBlocks.append(TranscriptBlock(
                    afterMessage: session.messages.count,
                    turnID: turnID,
                    content: .reasoning(ReasoningBlock(
                        content: delta,
                        startedAt: UInt64(Date().timeIntervalSince1970 * 1_000),
                        finishedAt: 0
                    ))
                ))
            }
        }
    }

    private func appendActivity(_ activity: ActivityItem, to sessionID: UUID) {
        updateSession(sessionID) { session in
            for blockIndex in session.transcriptBlocks.indices.reversed() {
                guard case .activities(var activities) = session.transcriptBlocks[blockIndex].content,
                      let activityIndex = activities.firstIndex(where: { $0.id == activity.id }) else { continue }
                activities[activityIndex] = activity
                let block = session.transcriptBlocks[blockIndex]
                session.transcriptBlocks[blockIndex] = TranscriptBlock(
                    afterMessage: block.afterMessage,
                    turnID: block.turnID,
                    content: .activities(activities)
                )
                return
            }

            let turnID = session.messages.last(where: { $0.role == .user })?.turnID
            if let index = session.transcriptBlocks.indices.last,
               session.transcriptBlocks[index].turnID == turnID,
               session.transcriptBlocks[index].afterMessage == session.messages.count,
               case .activities(var activities) = session.transcriptBlocks[index].content {
                activities.append(activity)
                session.transcriptBlocks[index] = TranscriptBlock(
                    afterMessage: session.messages.count,
                    turnID: turnID,
                    content: .activities(activities)
                )
            } else {
                session.transcriptBlocks.append(TranscriptBlock(
                    afterMessage: session.messages.count,
                    turnID: turnID,
                    content: .activities([activity])
                ))
            }
        }
    }

    private func activity(from payload: JSONValue, sessionID: UUID) -> ActivityItem? {
        guard let kind = payload["kind"]?.string,
              let title = payload["title"]?.string,
              case .bool(let complete)? = payload["complete"] else { return nil }
        let sourceID = payload["id"]?.string ?? "\(kind):\(title)"
        let turnID = selectedSession?.messages.last(where: { $0.role == .user })?.turnID?.uuidString ?? "none"
        let scopedSourceID = "\(turnID):\(sourceID)"
        let id = activityIDs[sessionID]?[scopedSourceID] ?? UUID().uuidString.lowercased()
        activityIDs[sessionID, default: [:]][scopedSourceID] = id
        return ActivityItem(
            id: id,
            kind: kind,
            title: title,
            detail: payload["detail"]?.string,
            arguments: payload["arguments"]?.string,
            output: payload["output"]?.string,
            failed: payload["failed"]?.boolValue ?? false,
            complete: complete,
            fileChanges: nil,
            displayTarget: payload["display_target"]?.string ?? payload["displayTarget"]?.string,
            displayDescription: payload["display_description"]?.string ?? payload["displayDescription"]?.string,
            reasoning: nil
        )
    }

    private func updateSession(_ sessionID: UUID, mutation: (inout AgentSession) -> Void) {
        guard var session = selectedSession, session.id == sessionID else { return }
        mutation(&session)
        updateSelectedSession(session)
    }

    private func updateSelectedSession(_ session: AgentSession) {
        selectedSession = session
        replaceInSessionList(session)
        transcriptRevision &+= 1
    }

    private func persistSession(_ sessionID: UUID) {
        Task { [weak self] in
            do {
                try await self?.saveSession(sessionID)
            } catch {
                self?.errorMessage = error.localizedDescription
            }
        }
    }

    private func session(withID sessionID: UUID) -> AgentSession? {
        if let selectedSession, selectedSession.id == sessionID {
            return selectedSession
        }
        return sessions.first { $0.id == sessionID }
    }

    private func saveSession(_ sessionID: UUID) async throws {
        guard let session = session(withID: sessionID) else { return }
        _ = try await client.request(command: .object([
            "type": .string("saveTaskState"),
            "projects": .array(try projects.map { try JSONValue.decode($0) }),
            "liveSessionIds": .array(runtimes.keys.map(JSONValue.uuid)),
            "sessions": .array([try JSONValue.decode(session)]),
        ]))
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

// MARK: - Attachment Images

/// An image picked in the composer, encoded for upload and kept locally so the
/// transcript can show it without waiting for a round trip to the Mac.
struct PreparedAttachmentImage: @unchecked Sendable {
    let data: Data
    let mimeType: String
    let fileExtension: String
    /// Immutable, decoded once and only read on the main actor.
    let preview: UIImage
}

enum ComposerImage {
    /// A photo straight off the camera is far larger than any provider needs,
    /// and a megabyte of base64 inside a websocket frame is a visible stall, so
    /// uploads are downscaled and re-encoded off the main actor first.
    static let maxDimension: CGFloat = 2048
    /// Transparent images keep their alpha as PNG, but only while they are small
    /// enough that the larger file is worth it.
    private static let maxPNGDimension: CGFloat = 1024

    static func prepare(_ image: UIImage) async -> PreparedAttachmentImage? {
        let box = UncheckedImageBox(image)
        return await Task.detached(priority: .userInitiated) {
            guard box.image.size.width > 0, box.image.size.height > 0 else { return nil }
            let normalized = redraw(box.image)
            if hasAlpha(box.image), max(normalized.size.width, normalized.size.height) <= Self.maxPNGDimension,
               let data = normalized.pngData() {
                return PreparedAttachmentImage(
                    data: data,
                    mimeType: "image/png",
                    fileExtension: "png",
                    preview: normalized
                )
            }
            guard let data = normalized.jpegData(compressionQuality: 0.82) else { return nil }
            return PreparedAttachmentImage(
                data: data,
                mimeType: "image/jpeg",
                fileExtension: "jpg",
                preview: normalized
            )
        }.value
    }

    /// Redraws at the upload size, which also bakes in the orientation the
    /// picker handed us so the Mac never sees a sideways photo.
    private static func redraw(_ image: UIImage) -> UIImage {
        let longestSide = max(image.size.width, image.size.height)
        let scale = longestSide > maxDimension ? maxDimension / longestSide : 1
        let size = CGSize(
            width: max(1, (image.size.width * scale).rounded()),
            height: max(1, (image.size.height * scale).rounded())
        )
        let format = UIGraphicsImageRendererFormat.default()
        format.scale = 1
        format.opaque = true
        return UIGraphicsImageRenderer(size: size, format: format).image { _ in
            image.draw(in: CGRect(origin: .zero, size: size))
        }
    }

    private static func hasAlpha(_ image: UIImage) -> Bool {
        switch image.cgImage?.alphaInfo {
        case .first, .last, .premultipliedFirst, .premultipliedLast: true
        default: false
        }
    }
}

/// `UIImage` is immutable once created, so a background executor may redraw and
/// encode it. UIKit does not declare that, hence the box.
private struct UncheckedImageBox: @unchecked Sendable {
    let image: UIImage

    init(_ image: UIImage) {
        self.image = image
    }
}

/// Image bytes for transcript attachments live on the Mac, so a preview is
/// fetched once per blob reference and kept for the life of the process.
/// Each thumbnail observes the store itself, so a decode only invalidates the
/// cell that needed it rather than the whole transcript.
@MainActor
@Observable
final class AttachmentImageStore {
    private(set) var images: [String: UIImage] = [:]
    @ObservationIgnored private var inFlight: Set<String> = []
    @ObservationIgnored private var unreadable: Set<String> = []
    @ObservationIgnored private let fetchData: (String) async throws -> Data

    init(fetchData: @escaping (String) async throws -> Data) {
        self.fetchData = fetchData
    }

    func image(for reference: String) -> UIImage? {
        images[reference]
    }

    /// Seeds the cache with the image the user just picked, so the row it will
    /// appear in draws immediately instead of after an upload round trip.
    func adopt(_ image: UIImage, for reference: String) {
        images[reference] = image
    }

    func load(_ reference: String) {
        guard images[reference] == nil, !inFlight.contains(reference), !unreadable.contains(reference) else { return }
        inFlight.insert(reference)
        Task {
            defer { inFlight.remove(reference) }
            guard let data = try? await fetchData(reference), let image = UIImage(data: data) else {
                unreadable.insert(reference)
                return
            }
            images[reference] = image
        }
    }
}
