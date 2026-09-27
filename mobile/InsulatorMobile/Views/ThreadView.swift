import SwiftUI

struct ThreadView: View {
    @Bindable var app: AppModel
    let sessionID: UUID

    @State private var input = ""
    @State private var showsOptions = false

    private var session: AgentSession? {
        if app.selectedSession?.id == sessionID { return app.selectedSession }
        return app.sessions.first { $0.id == sessionID }
    }

    var body: some View {
        Group {
            if let session {
                ScrollViewReader { proxy in
                    ScrollView {
                        LazyVStack(alignment: .leading, spacing: 16) {
                            ForEach(timeline(for: session)) { entry in
                                TimelineEntryView(entry: entry)
                                    .id(entry.id)
                            }
                            if session.status.isBusy && session.messages.last?.streaming != true {
                                HStack(spacing: 8) {
                                    ProgressView().controlSize(.small)
                                    Text(session.status == .waiting ? "Waiting for you" : "Working")
                                        .font(.subheadline)
                                        .foregroundStyle(AppTheme.secondary)
                                }
                                .id("working")
                            }
                        }
                        .padding(.horizontal, 16)
                        .padding(.vertical, 20)
                    }
                    .onChange(of: session.messages.last?.content) { _, _ in
                        guard let id = timeline(for: session).last?.id else { return }
                        proxy.scrollTo(id, anchor: .bottom)
                    }
                }
                .safeAreaInset(edge: .bottom, spacing: 0) {
                    ComposerBar(
                        text: $input,
                        session: session,
                        onOptions: { showsOptions = true },
                        onSend: send,
                        onSteer: steer,
                        onCancel: { Task { await app.cancel() } }
                    )
                }
                .navigationTitle(session.displayTitle)
                .navigationBarTitleDisplayMode(.inline)
                .toolbar {
                    ToolbarItem(placement: .principal) {
                        VStack(spacing: 1) {
                            Text(session.displayTitle)
                                .font(.headline)
                                .lineLimit(1)
                            Text("\(session.provider.name) · \(session.model ?? "Default")")
                                .font(.caption2)
                                .foregroundStyle(AppTheme.secondary)
                                .lineLimit(1)
                        }
                    }
                }
                .sheet(isPresented: $showsOptions) {
                    SessionOptionsView(app: app, session: session)
                }
                .sheet(item: permissionBinding) { permission in
                    PermissionSheet(app: app, permission: permission)
                        .presentationDetents([.medium])
                }
            } else {
                ProgressView("Loading task…")
            }
        }
        .background(AppTheme.background)
    }

    private var permissionBinding: Binding<PendingPermission?> {
        Binding(
            get: {
                guard app.pendingPermission?.sessionID == sessionID else { return nil }
                return app.pendingPermission
            },
            set: { app.pendingPermission = $0 }
        )
    }

    private func send() {
        let text = input
        input = ""
        Task { await app.send(text) }
    }

    private func steer() {
        let text = input
        input = ""
        Task { await app.steer(text) }
    }

    private func timeline(for session: AgentSession) -> [TimelineEntry] {
        var result: [TimelineEntry] = []
        for block in session.transcriptBlocks where block.afterMessage == 0 {
            result.append(.block(block))
        }
        for (index, message) in session.messages.enumerated() {
            result.append(.message(message))
            for block in session.transcriptBlocks where block.afterMessage == index + 1 {
                result.append(.block(block))
            }
        }
        return result
    }
}

private enum TimelineEntry: Identifiable {
    case message(Message)
    case block(TranscriptBlock)

    var id: String {
        switch self {
        case .message(let message): "message-\(message.id.uuidString)"
        case .block(let block): "block-\(block.id)"
        }
    }
}

private struct TimelineEntryView: View {
    let entry: TimelineEntry

    var body: some View {
        switch entry {
        case .message(let message):
            MessageRow(message: message)
        case .block(let block):
            TranscriptBlockView(block: block)
        }
    }
}

private struct MessageRow: View {
    let message: Message

    var body: some View {
        if message.role == .user {
            HStack {
                Spacer(minLength: 44)
                Text(message.visibleContent)
                    .font(.body)
                    .textSelection(.enabled)
                    .padding(.horizontal, 15)
                    .padding(.vertical, 11)
                    .background(AppTheme.raised, in: RoundedRectangle(cornerRadius: 18))
            }
        } else {
            Text(message.visibleContent)
                .font(.body)
                .lineSpacing(4)
                .textSelection(.enabled)
                .frame(maxWidth: .infinity, alignment: .leading)
                .foregroundStyle(message.role == .system ? AppTheme.secondary : .primary)
        }
    }
}

private struct TranscriptBlockView: View {
    let block: TranscriptBlock
    @State private var expanded = false

    var body: some View {
        switch block.content {
        case .reasoning(let reasoning):
            DisclosureGroup("Reasoning", isExpanded: $expanded) {
                Text(reasoning.content)
                    .font(.subheadline)
                    .foregroundStyle(AppTheme.secondary)
                    .textSelection(.enabled)
                    .padding(.top, 8)
            }
            .tint(AppTheme.secondary)
            .padding(12)
            .background(AppTheme.surface, in: RoundedRectangle(cornerRadius: 14))
        case .activities(let activities):
            VStack(spacing: 8) {
                ForEach(activities) { activity in
                    ActivityRow(activity: activity)
                }
            }
        }
    }
}

private struct ActivityRow: View {
    let activity: ActivityItem
    @State private var expanded = false

    var body: some View {
        DisclosureGroup(isExpanded: $expanded) {
            if let detail = activity.output ?? activity.detail ?? activity.arguments {
                Text(detail)
                    .font(.caption.monospaced())
                    .foregroundStyle(AppTheme.secondary)
                    .textSelection(.enabled)
                    .padding(.top, 8)
            }
        } label: {
            HStack(spacing: 9) {
                Image(systemName: activity.failed ? "xmark.circle.fill" : activity.complete ? "checkmark.circle.fill" : "circle.dotted")
                    .foregroundStyle(activity.failed ? .red : activity.complete ? AppTheme.accent : AppTheme.secondary)
                VStack(alignment: .leading, spacing: 2) {
                    Text(activity.title)
                        .font(.subheadline.weight(.medium))
                    if let target = activity.displayTarget {
                        Text(target)
                            .font(.caption.monospaced())
                            .foregroundStyle(AppTheme.secondary)
                            .lineLimit(1)
                    }
                }
            }
        }
        .tint(AppTheme.secondary)
        .padding(11)
        .background(AppTheme.surface, in: RoundedRectangle(cornerRadius: 13))
        .overlay(RoundedRectangle(cornerRadius: 13).stroke(AppTheme.border))
    }
}

private struct ComposerBar: View {
    @Binding var text: String
    let session: AgentSession
    let onOptions: () -> Void
    let onSend: () -> Void
    let onSteer: () -> Void
    let onCancel: () -> Void

    @FocusState private var focused: Bool

    var body: some View {
        VStack(spacing: 9) {
            TextField("Ask Insulator", text: $text, axis: .vertical)
                .lineLimit(1...6)
                .focused($focused)
                .padding(.horizontal, 4)

            HStack(spacing: 10) {
                Button(action: onOptions) {
                    Label {
                        Text(session.model ?? session.provider.name)
                    } icon: {
                        Image(session.provider.iconName)
                            .resizable()
                            .aspectRatio(contentMode: .fit)
                            .frame(width: 13, height: 13)
                    }
                        .font(.caption.weight(.medium))
                        .lineLimit(1)
                        .padding(.horizontal, 9)
                        .frame(minHeight: 36)
                        .background(AppTheme.raised, in: Capsule())
                }
                .buttonStyle(.plain)
                .accessibilityLabel("Model and access settings")

                Spacer()

                if session.status.isBusy {
                    Button(action: onCancel) {
                        Image(systemName: "stop.fill")
                            .frame(width: 44, height: 44)
                            .background(AppTheme.raised, in: Circle())
                    }
                    .accessibilityLabel("Stop agent")

                    Button(action: onSteer) {
                        Image(systemName: "arrow.turn.up.right")
                            .frame(width: 44, height: 44)
                            .background(.white, in: Circle())
                            .foregroundStyle(.black)
                    }
                    .disabled(text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
                    .accessibilityLabel("Steer agent")
                } else {
                    Button(action: onSend) {
                        Image(systemName: "arrow.up")
                            .fontWeight(.bold)
                            .frame(width: 44, height: 44)
                            .background(.white, in: Circle())
                            .foregroundStyle(.black)
                    }
                    .disabled(text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
                    .accessibilityLabel("Send")
                }
            }
        }
        .padding(12)
        .background(.ultraThinMaterial, in: RoundedRectangle(cornerRadius: 24))
        .overlay(RoundedRectangle(cornerRadius: 24).stroke(AppTheme.border))
        .padding(.horizontal, 12)
        .padding(.bottom, 6)
    }
}

private struct SessionOptionsView: View {
    @Bindable var app: AppModel
    @Environment(\.dismiss) private var dismiss

    let session: AgentSession
    @State private var modelID: String?
    @State private var mode: RuntimeMode
    @State private var reasoning: String?
    @State private var tier: String?
    @State private var contextWindow: String?

    init(app: AppModel, session: AgentSession) {
        self.app = app
        self.session = session
        _modelID = State(initialValue: session.model)
        _mode = State(initialValue: session.runtimeMode)
        _reasoning = State(initialValue: session.reasoningEffort)
        _tier = State(initialValue: session.serviceTier)
        _contextWindow = State(initialValue: session.contextWindow)
    }

    private var models: [ProviderModel] { app.models(for: session.provider) }
    private var model: ProviderModel? { models.first { $0.id == modelID } }

    var body: some View {
        NavigationStack {
            Form {
                Section("Agent") {
                    LabeledContent("Provider", value: session.provider.name)
                    if !models.isEmpty {
                        Picker("Model", selection: $modelID) {
                            Text("Provider default").tag(String?.none)
                            ForEach(models) { model in
                                Text(model.name).tag(Optional(model.id))
                            }
                        }
                    }
                    Picker("Access", selection: $mode) {
                        ForEach(RuntimeMode.allCases) { mode in
                            Text(mode.name).tag(mode)
                        }
                    }
                }

                if let model, !model.reasoningEfforts.isEmpty {
                    Section("Reasoning") {
                        Picker("Effort", selection: $reasoning) {
                            ForEach(model.reasoningEfforts) { option in
                                Text(option.label).tag(Optional(option.id))
                            }
                        }
                    }
                }

                if let model, !model.serviceTiers.isEmpty {
                    Section("Speed") {
                        Picker("Tier", selection: $tier) {
                            Text("Default").tag(String?.none)
                            ForEach(model.serviceTiers) { option in
                                Text(option.label).tag(Optional(option.id))
                            }
                        }
                    }
                }

                if let model, !model.contextWindows.isEmpty {
                    Section("Context") {
                        Picker("Window", selection: $contextWindow) {
                            ForEach(model.contextWindows) { option in
                                Text(option.label).tag(Optional(option.id))
                            }
                        }
                    }
                }
            }
            .navigationTitle("Task settings")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("Cancel") { dismiss() }
                }
                ToolbarItem(placement: .confirmationAction) {
                    Button("Apply") {
                        let selected = model
                        Task {
                            await app.updateOptions(
                                model: selected,
                                mode: mode,
                                reasoningEffort: reasoning ?? selected?.defaultReasoningEffort,
                                serviceTier: tier ?? selected?.defaultServiceTier,
                                contextWindow: contextWindow ?? selected?.defaultContextWindow
                            )
                            dismiss()
                        }
                    }
                }
            }
            .onChange(of: modelID) { _, _ in
                reasoning = model?.defaultReasoningEffort
                tier = model?.defaultServiceTier
                contextWindow = model?.defaultContextWindow
            }
        }
    }
}

private struct PermissionSheet: View {
    @Bindable var app: AppModel
    let permission: PendingPermission

    var body: some View {
        NavigationStack {
            VStack(alignment: .leading, spacing: 18) {
                Label("Approval needed", systemImage: "hand.raised.fill")
                    .font(.title2.bold())
                Text(permission.title)
                    .font(.headline)
                if let detail = permission.detail {
                    Text(detail)
                        .font(.callout.monospaced())
                        .foregroundStyle(AppTheme.secondary)
                        .textSelection(.enabled)
                }
                Spacer()
                ForEach(permission.options) { option in
                    Button {
                        Task { await app.respond(to: permission, option: option) }
                    } label: {
                        Text(option.label)
                            .frame(maxWidth: .infinity, minHeight: 48)
                    }
                    .buttonStyle(.borderedProminent)
                    .tint(option.allow ? .white : .red)
                }
            }
            .padding(22)
        }
    }
}
