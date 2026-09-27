import Foundation

enum ProviderKind: String, CaseIterable, Codable, Identifiable, Sendable {
    case amp
    case claude
    case codex
    case cursor
    case deepSeek
    case fx
    case openCode
    case openCode2
    case grok
    case kimi
    case devin
    case ohMyPi
    case pi

    var id: String { rawValue }

    var name: String {
        switch self {
        case .amp: "Amp"
        case .claude: "Claude Code"
        case .codex: "Codex"
        case .cursor: "Cursor"
        case .deepSeek: "DeepSeek"
        case .fx: "Factory"
        case .openCode: "OpenCode"
        case .openCode2: "OpenCode 2"
        case .grok: "Grok"
        case .kimi: "Kimi"
        case .devin: "Devin"
        case .ohMyPi: "Oh My Pi"
        case .pi: "Pi"
        }
    }

    var iconName: String {
        switch self {
        case .amp: "ProviderAmp"
        case .claude: "ProviderClaude"
        case .codex: "ProviderOpenai"
        case .cursor: "ProviderCursor"
        case .deepSeek: "ProviderDeepseek"
        case .fx: "ProviderFx"
        case .openCode: "ProviderOpencode"
        case .openCode2: "ProviderOpencode2"
        case .grok: "ProviderGrok"
        case .kimi: "ProviderKimi"
        case .devin: "ProviderDevin"
        case .ohMyPi: "ProviderOhmypi"
        case .pi: "ProviderPi"
        }
    }
}

enum RuntimeMode: String, CaseIterable, Codable, Identifiable, Sendable {
    case ask
    case autoAcceptEdits
    case auto
    case fullAccess

    var id: String { rawValue }

    var name: String {
        switch self {
        case .ask: "Ask"
        case .autoAcceptEdits: "Accept edits"
        case .auto: "Auto"
        case .fullAccess: "Full access"
        }
    }
}

enum SessionStatus: String, Codable, Sendable {
    case idle
    case connecting
    case working
    case waiting
    case background
    case failed

    var isBusy: Bool {
        switch self {
        case .connecting, .working, .waiting, .background: true
        case .idle, .failed: false
        }
    }
}

struct ProviderModelOption: Codable, Hashable, Identifiable, Sendable {
    let id: String
    let label: String
    let description: String?
}

struct ProviderModel: Codable, Hashable, Identifiable, Sendable {
    let id: String
    let name: String
    let subProvider: String?
    let isDefault: Bool
    let reasoningEfforts: [ProviderModelOption]
    let defaultReasoningEffort: String?
    let serviceTiers: [ProviderModelOption]
    let defaultServiceTier: String?
    let contextWindows: [ProviderModelOption]
    let defaultContextWindow: String?

    enum CodingKeys: String, CodingKey {
        case id, name
        case subProvider = "sub_provider"
        case isDefault = "is_default"
        case reasoningEfforts = "reasoning_efforts"
        case defaultReasoningEffort = "default_reasoning_effort"
        case serviceTiers = "service_tiers"
        case defaultServiceTier = "default_service_tier"
        case contextWindows = "context_windows"
        case defaultContextWindow = "default_context_window"
    }
}

struct ProviderProbe: Codable, Identifiable, Sendable {
    let provider: ProviderKind
    let installed: Bool
    let path: String?
    let models: [ProviderModel]
    let agentPresets: [ProviderAgentPreset]

    var id: ProviderKind { provider }

    enum CodingKeys: String, CodingKey {
        case provider, installed, path, models
        case agentPresets = "agent_presets"
    }
}

struct ProviderAgentPreset: Codable, Hashable, Identifiable, Sendable {
    let id: String
    let name: String
    let description: String?
    let isDefault: Bool
    let isCustom: Bool

    enum CodingKeys: String, CodingKey {
        case id, name, description
        case isDefault = "is_default"
        case isCustom = "is_custom"
    }
}

struct Project: Codable, Hashable, Identifiable, Sendable {
    let id: UUID
    let name: String
    let path: String
    let createdAt: UInt64

    enum CodingKeys: String, CodingKey {
        case id, name, path
        case createdAt = "created_at"
    }
}

enum MessageRole: String, Codable, Sendable {
    case user
    case assistant
    case system
    case tool
}

struct Message: Codable, Hashable, Identifiable, Sendable {
    let id: UUID
    let turnID: UUID?
    let role: MessageRole
    var content: String
    let displayContent: String?
    let createdAt: UInt64
    var streaming: Bool

    var visibleContent: String { displayContent ?? content }

    enum CodingKeys: String, CodingKey {
        case id, role, content, streaming
        case turnID = "turn_id"
        case displayContent = "display_content"
        case createdAt = "created_at"
    }
}

struct ReasoningBlock: Codable, Hashable, Sendable {
    let content: String
    let startedAt: UInt64
    let finishedAt: UInt64

    enum CodingKeys: String, CodingKey {
        case content
        case startedAt = "started_at_ms"
        case finishedAt = "finished_at_ms"
    }
}

struct ActivityFileChange: Codable, Hashable, Sendable {
    let path: String
    let additions: UInt64?
    let deletions: UInt64?
    let diff: String?
}

struct ActivityItem: Codable, Hashable, Identifiable, Sendable {
    let id: String
    let kind: String
    let title: String
    let detail: String?
    let arguments: String?
    let output: String?
    let failed: Bool
    let complete: Bool
    let fileChanges: [ActivityFileChange]?
    let displayTarget: String?
    let displayDescription: String?
    let reasoning: ReasoningBlock?

    enum CodingKeys: String, CodingKey {
        case id, kind, title, detail, arguments, output, failed, complete, reasoning
        case fileChanges = "file_changes"
        case displayTarget = "display_target"
        case displayDescription = "display_description"
    }

    init(
        id: String,
        kind: String,
        title: String,
        detail: String? = nil,
        arguments: String? = nil,
        output: String? = nil,
        failed: Bool = false,
        complete: Bool = true,
        fileChanges: [ActivityFileChange]? = nil,
        displayTarget: String? = nil,
        displayDescription: String? = nil,
        reasoning: ReasoningBlock? = nil
    ) {
        self.id = id
        self.kind = kind
        self.title = title
        self.detail = detail
        self.arguments = arguments
        self.output = output
        self.failed = failed
        self.complete = complete
        self.fileChanges = fileChanges
        self.displayTarget = displayTarget
        self.displayDescription = displayDescription
        self.reasoning = reasoning
    }
}

enum TranscriptContent: Codable, Hashable, Sendable {
    case reasoning(ReasoningBlock)
    case activities([ActivityItem])

    private enum CodingKeys: String, CodingKey { case kind, data }
    private enum Kind: String, Codable { case reasoning, activities }

    init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        switch try container.decode(Kind.self, forKey: .kind) {
        case .reasoning:
            self = .reasoning(try container.decode(ReasoningBlock.self, forKey: .data))
        case .activities:
            self = .activities(try container.decode([ActivityItem].self, forKey: .data))
        }
    }

    func encode(to encoder: Encoder) throws {
        var container = encoder.container(keyedBy: CodingKeys.self)
        switch self {
        case .reasoning(let block):
            try container.encode(Kind.reasoning, forKey: .kind)
            try container.encode(block, forKey: .data)
        case .activities(let activities):
            try container.encode(Kind.activities, forKey: .kind)
            try container.encode(activities, forKey: .data)
        }
    }
}

struct TranscriptBlock: Codable, Hashable, Identifiable, Sendable {
    let afterMessage: Int
    let turnID: UUID?
    let content: TranscriptContent

    var id: String {
        let contentID = switch content {
        case .reasoning(let reasoning): "reasoning-\(reasoning.startedAt)"
        case .activities(let activities): "activities-\(activities.first?.id ?? "empty")"
        }
        return "\(turnID?.uuidString ?? "none")-\(afterMessage)-\(contentID)"
    }

    enum CodingKeys: String, CodingKey {
        case content
        case afterMessage = "after_message"
        case turnID = "turn_id"
    }
}

struct AgentTurn: Codable, Hashable, Identifiable, Sendable {
    let id: UUID
    let turnCount: Int
    var status: String
    var providerTurnStarted: Bool
    let providerResumeAt: String?
    let startedAt: UInt64
    var completedAt: UInt64?
    let checkpoint: JSONValue?

    enum CodingKeys: String, CodingKey {
        case id, status, checkpoint
        case turnCount = "turn_count"
        case providerTurnStarted = "provider_turn_started"
        case providerResumeAt = "provider_resume_at"
        case startedAt = "started_at"
        case completedAt = "completed_at"
    }
}

struct AgentSession: Codable, Hashable, Identifiable, Sendable {
    let id: UUID
    var title: String
    var autoTitle: String?
    var projectID: UUID
    var provider: ProviderKind
    var model: String?
    var runtimeMode: RuntimeMode
    var reasoningEffort: String?
    var serviceTier: String?
    var contextWindow: String?
    var agentPreset: String?
    var status: SessionStatus
    let createdAt: UInt64
    var updatedAt: UInt64
    var lastReplyAt: UInt64?
    var providerCursor: JSONValue?
    var messages: [Message]
    var transcriptBlocks: [TranscriptBlock]
    var turns: [AgentTurn]

    var displayTitle: String {
        let explicit = title.trimmingCharacters(in: .whitespacesAndNewlines)
        if explicit != "New task", explicit != "New thread", !explicit.isEmpty { return explicit }
        return autoTitle?.trimmingCharacters(in: .whitespacesAndNewlines).nilIfEmpty ?? "New thread"
    }

    enum CodingKeys: String, CodingKey {
        case id, title, provider, model, status, messages
        case autoTitle = "auto_title"
        case projectID = "project_id"
        case runtimeMode = "runtime_mode"
        case reasoningEffort = "reasoning_effort"
        case serviceTier = "service_tier"
        case contextWindow = "context_window"
        case agentPreset = "agent_preset"
        case createdAt = "created_at"
        case updatedAt = "updated_at"
        case lastReplyAt = "last_reply_at"
        case providerCursor = "provider_cursor"
        case transcriptBlocks = "transcript_blocks"
        case turns
    }
}

struct PermissionOption: Codable, Hashable, Identifiable, Sendable {
    let id: String
    let label: String
    let allow: Bool
}

struct PendingPermission: Identifiable, Sendable {
    let id: String
    let sessionID: UUID
    let runtimeID: UUID
    let title: String
    let detail: String?
    let options: [PermissionOption]
}

struct PairingPayload: Codable, Equatable, Sendable {
    let url: URL
    let token: String

    init(url: URL, token: String) throws {
        guard ["ws", "wss"].contains(url.scheme?.lowercased() ?? ""), url.host != nil else {
            throw ConnectionError.invalidAddress
        }
        let trimmedToken = token.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmedToken.isEmpty else { throw ConnectionError.missingToken }
        self.url = Self.normalizedURL(url)
        self.token = trimmedToken
    }

    init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        let url = try container.decode(URL.self, forKey: .url)
        let token = try container.decode(String.self, forKey: .token)
        guard ["ws", "wss"].contains(url.scheme?.lowercased() ?? ""), url.host != nil else {
            throw ConnectionError.invalidAddress
        }
        let trimmedToken = token.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmedToken.isEmpty else { throw ConnectionError.missingToken }
        self.url = Self.normalizedURL(url)
        self.token = trimmedToken
    }

    /// The daemon only serves the `/v1` endpoint. Older saved payloads and
    /// hand-typed addresses often omit it, which the server answers with a
    /// plain HTTP 404 — surfaced by URLSession as "bad response from server".
    static func normalizedURL(_ url: URL) -> URL {
        guard var components = URLComponents(url: url, resolvingAgainstBaseURL: false) else {
            return url
        }
        let path = components.path
        if path.isEmpty || path == "/" {
            components.path = "/v1"
        }
        return components.url ?? url
    }

    private enum CodingKeys: String, CodingKey {
        case url, token
    }
}

enum ConnectionState: Equatable, Sendable {
    case disconnected
    case connecting
    case connected(version: String)
    case failed(String)
}

enum ConnectionError: LocalizedError, Sendable {
    case invalidAddress
    case missingToken
    case rejected(String)
    case protocolMismatch(Int)
    case disconnected
    case malformedResponse
    case daemon(String)

    var errorDescription: String? {
        switch self {
        case .invalidAddress: "Enter a ws:// or wss:// daemon address."
        case .missingToken: "Enter the daemon token from Insulator Desktop."
        case .rejected(let message), .daemon(let message): message
        case .protocolMismatch(let version): "The Mac uses protocol \(version), but this app supports protocol 8. Update Insulator on both devices."
        case .disconnected: "The connection to your Mac closed. Your work is still safe on the Mac."
        case .malformedResponse: "The Mac returned data this app could not read. Update Insulator on both devices."
        }
    }
}

private extension String {
    var nilIfEmpty: String? { isEmpty ? nil : self }
}
