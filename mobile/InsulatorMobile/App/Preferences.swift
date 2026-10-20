import Foundation
import Observation

/// What a session runs with: the provider, its model, and that model's
/// options. Access is chosen apart, as the desktop keeps it.
struct ModelChoice: Codable, Hashable, Sendable {
    var provider: ProviderKind
    var modelID: String?
    var reasoningEffort: String?
    var serviceTier: String?
    var contextWindow: String?

    /// The model's own defaults, as a fresh pick of it starts with.
    init(provider: ProviderKind, model: ProviderModel?) {
        self.provider = provider
        modelID = model?.id
        reasoningEffort = model?.defaultReasoningEffort
        serviceTier = model?.defaultServiceTier
        contextWindow = model?.defaultContextWindow
    }

    /// Another model, keeping each option the new one also offers.
    func switching(to model: ProviderModel, provider: ProviderKind) -> ModelChoice {
        var next = ModelChoice(provider: provider, model: model)
        if provider == self.provider {
            if let effort = reasoningEffort, model.reasoningEfforts.contains(where: { $0.id == effort }) {
                next.reasoningEffort = effort
            }
            if let tier = serviceTier, model.serviceTiers.contains(where: { $0.id == tier }) {
                next.serviceTier = tier
            }
            if let window = contextWindow, model.contextWindows.contains(where: { $0.id == window }) {
                next.contextWindow = window
            }
        }
        return next
    }
}

/// Settings kept on this phone, apart from the desktop's.
@MainActor
@Observable
final class Preferences {
    static let shared = Preferences()

    /// Open a session as soon as it starts, rather than staying ready for another.
    var openAfterStarting: Bool { didSet { save(openAfterStarting, "openAfterStarting") } }
    /// Show only a step's label until it is tapped.
    var collapseToolCalls: Bool { didSet { save(collapseToolCalls, "collapseToolCalls") } }
    var notifyOnInput: Bool { didSet { save(notifyOnInput, "notifyOnInput") } }
    var notifyOnFinish: Bool { didSet { save(notifyOnFinish, "notifyOnFinish") } }
    /// The provider new sessions start with; `nil` follows the desktop.
    var defaultProvider: ProviderKind? { didSet { save(defaultProvider?.rawValue, "defaultProvider") } }
    /// Each provider's own pick, remembered while another is the default.
    var defaultModels: [ProviderKind: ModelChoice] { didSet { saveCodable(defaultModels, "defaultModels") } }
    /// The access new sessions start with; `nil` follows the desktop.
    var defaultAccess: RuntimeMode? { didSet { save(defaultAccess?.rawValue, "defaultAccess") } }
    /// The projects shown on this phone; `nil` shows them all.
    var visibleProjectIDs: Set<UUID>? { didSet { saveCodable(visibleProjectIDs, "visibleProjectIDs") } }
    var currentProjectID: UUID? { didSet { save(currentProjectID?.uuidString, "currentProjectID") } }

    var defaultChoice: ModelChoice? {
        defaultProvider.flatMap { defaultModels[$0] }
    }

    private let defaults = UserDefaults.standard
    private static let prefix = "preferences."

    private init() {
        let defaults = UserDefaults.standard
        func bool(_ key: String, _ fallback: Bool) -> Bool {
            defaults.object(forKey: Self.prefix + key) as? Bool ?? fallback
        }
        func decode<T: Decodable>(_ key: String) -> T? {
            defaults.data(forKey: Self.prefix + key).flatMap { try? JSONDecoder().decode(T.self, from: $0) }
        }
        openAfterStarting = bool("openAfterStarting", true)
        collapseToolCalls = bool("collapseToolCalls", true)
        notifyOnInput = bool("notifyOnInput", true)
        notifyOnFinish = bool("notifyOnFinish", true)
        defaultProvider = defaults.string(forKey: Self.prefix + "defaultProvider").flatMap(ProviderKind.init(rawValue:))
        defaultModels = decode("defaultModels") ?? [:]
        defaultAccess = defaults.string(forKey: Self.prefix + "defaultAccess").flatMap(RuntimeMode.init(rawValue:))
        visibleProjectIDs = decode("visibleProjectIDs")
        currentProjectID = defaults.string(forKey: Self.prefix + "currentProjectID").flatMap(UUID.init(uuidString:))
    }

    private func save(_ value: Bool, _ key: String) {
        defaults.set(value, forKey: Self.prefix + key)
    }

    private func save(_ value: String?, _ key: String) {
        defaults.set(value, forKey: Self.prefix + key)
    }

    private func saveCodable<T: Encodable>(_ value: T?, _ key: String) {
        if let value, let data = try? JSONEncoder().encode(value) {
            defaults.set(data, forKey: Self.prefix + key)
        } else {
            defaults.removeObject(forKey: Self.prefix + key)
        }
    }
}

extension RuntimeMode {
    /// The desktop's permission modes, with their labels and hints.
    var label: String {
        switch self {
        case .ask: "Supervised"
        case .autoAcceptEdits: "Auto-accept edits"
        case .auto: "Auto"
        case .fullAccess: "Full access"
        }
    }

    var hint: String {
        switch self {
        case .ask: "Ask before commands and file changes."
        case .autoAcceptEdits: "Auto-approve edits, ask before other actions."
        case .auto: "An AI reviewer can approve or deny actions."
        case .fullAccess: "Allow commands, edits, and supported MCP confirmations in non-plan turns without prompts."
        }
    }

    var icon: Icon {
        switch self {
        case .ask: .lock
        case .autoAcceptEdits: .edit
        case .auto: .sparkle
        case .fullAccess: .shield
        }
    }
}
