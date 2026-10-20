import SwiftUI

/// Efforts past "high" draw in amber, as the desktop's effort tiles warm up.
private func isHotEffort(_ id: String?) -> Bool {
    ["xhigh", "max", "ultra", "ultracode", "ultrathink"].contains(id ?? "")
}

/// The model sheet: the account, a button to the chosen model's options, then
/// every provider's models in sections that fold away, the chosen provider's
/// open. Each change goes straight to the session, or to the next new one.
struct ModelPicker: View {
    @Bindable var app: AppModel
    let projectID: UUID
    let sessionID: UUID?

    @Environment(\.dismiss) private var dismiss
    @State private var query = ""
    @State private var folded: [ProviderKind: Bool] = [:]
    @State private var usage: PlanUsage?
    @State private var usageLoading = false
    @State private var usageError: String?

    private var choice: ModelChoice? {
        if let sessionID, let session = app.session(withID: sessionID) { return app.choice(of: session) }
        return app.newSessionChoice(for: projectID)
    }

    var body: some View {
        NavigationStack {
            ScrollView {
                if let choice {
                    list(choice)
                } else {
                    ProgressView().padding(.top, 40)
                }
            }
            .scrollIndicators(.hidden)
            .background(AppTheme.sheet.ignoresSafeArea())
            .navigationTitle("Model")
            .navigationBarTitleDisplayMode(.inline)
            .searchable(text: $query, placement: .navigationBarDrawer(displayMode: .always), prompt: "Find a model")
            .toolbar {
                ToolbarItem(placement: .topBarLeading) {
                    Button { dismiss() } label: { IconView(.close, size: 18) }
                        .accessibilityLabel("Close")
                }
            }
            .refreshable { await loadUsage() }
            .task(id: choice?.provider) { await loadUsage() }
        }
        .presentationBackground(AppTheme.sheet)
    }

    private func list(_ choice: ModelChoice) -> some View {
        let needle = query.trimmingCharacters(in: .whitespaces).lowercased()
        let searching = !needle.isEmpty
        let model = app.models(for: choice.provider).first { $0.id == choice.modelID }
        let sections = app.installedProbes.map { probe in
            (provider: probe.provider, models: probe.models.filter { entry in
                !searching || [entry.name, entry.id, entry.subProvider ?? ""].contains { $0.lowercased().contains(needle) }
            })
        }.filter { !searching || !$0.models.isEmpty }

        return VStack(alignment: .leading, spacing: 20) {
            if !searching {
                VStack(alignment: .leading, spacing: 8) {
                    heading("Account")
                    AccountCard(
                        provider: choice.provider,
                        usage: usage,
                        loading: usageLoading,
                        error: usageError
                    ) { Task { await loadUsage() } }
                }
                VStack(alignment: .leading, spacing: 8) {
                    heading("Selected provider")
                    NavigationLink {
                        ModelOptions(app: app, projectID: projectID, sessionID: sessionID)
                    } label: {
                        HStack(spacing: 12) {
                            ProviderLogo(provider: choice.provider, size: 20).foregroundStyle(AppTheme.content)
                            VStack(alignment: .leading, spacing: 2) {
                                Text(model?.name ?? choice.modelID ?? "Desktop default")
                                    .font(.system(size: 17, weight: .semibold))
                                    .foregroundStyle(AppTheme.content)
                                Text(summary(choice, model: model))
                                    .font(.system(size: 13))
                                    .foregroundStyle(access == .fullAccess ? AppTheme.caution : AppTheme.muted)
                            }
                            .lineLimit(1)
                            .frame(maxWidth: .infinity, alignment: .leading)
                            Text("Options").font(.system(size: 15)).foregroundStyle(AppTheme.muted)
                            IconView(.chevronRight, size: 14).foregroundStyle(AppTheme.muted)
                        }
                        .padding(12)
                        .frame(minHeight: 64)
                        .background(AppTheme.content.opacity(0.07), in: RoundedRectangle(cornerRadius: 18, style: .continuous))
                    }
                    .buttonStyle(PressedOpacityStyle(opacity: 0.5))
                }
            }
            if sections.isEmpty {
                Text("No matching models.")
                    .font(.system(size: 15))
                    .foregroundStyle(AppTheme.muted)
                    .frame(maxWidth: .infinity)
                    .padding(.top, 24)
            }
            VStack(spacing: 0) {
                ForEach(Array(sections.enumerated()), id: \.element.provider) { index, section in
                    let open = searching || !(folded[section.provider] ?? (section.provider != choice.provider))
                    if index > 0, !open, !(searching || !(folded[sections[index - 1].provider] ?? (sections[index - 1].provider != choice.provider))) {
                        Rectangle().fill(AppTheme.stroke).frame(height: 0.5).padding(.horizontal, 12)
                    }
                    providerSection(section.provider, models: section.models, open: open, searching: searching, choice: choice)
                }
            }
        }
        .padding(.horizontal, 20)
        .padding(.top, 12)
        .padding(.bottom, 40)
    }

    private var access: RuntimeMode {
        if let sessionID, let session = app.session(withID: sessionID) { return session.runtimeMode }
        return app.newSessionAccess(for: projectID)
    }

    private func summary(_ choice: ModelChoice, model: ProviderModel?) -> String {
        var parts: [String] = []
        if let effort = model?.reasoningEfforts.first(where: { $0.id == choice.reasoningEffort }) { parts.append(effort.label) }
        if choice.serviceTier == "fast" { parts.append("Fast") }
        if let window = model?.contextWindows.first(where: { $0.id == choice.contextWindow }) { parts.append(window.label) }
        if choice.provider != .fx { parts.append(access.label) }
        return parts.isEmpty ? "Options" : parts.joined(separator: " · ")
    }

    private func heading(_ text: String) -> some View {
        Text(text)
            .font(.system(size: 13, weight: .semibold))
            .foregroundStyle(AppTheme.muted)
            .padding(.horizontal, 12)
    }

    private func providerSection(
        _ provider: ProviderKind,
        models: [ProviderModel],
        open: Bool,
        searching: Bool,
        choice: ModelChoice
    ) -> some View {
        VStack(spacing: 0) {
            Button {
                Haptics.selection()
                withAnimation(.snappy) { folded[provider] = open }
            } label: {
                HStack(spacing: 8) {
                    ProviderLogo(provider: provider, size: 18).foregroundStyle(AppTheme.content)
                    Text(provider.name)
                        .font(.system(size: 15, weight: .semibold))
                        .foregroundStyle(AppTheme.content)
                    if !open {
                        Text("\(models.count)")
                            .font(.system(size: 13).monospacedDigit())
                            .foregroundStyle(AppTheme.muted)
                    }
                    Spacer()
                    if !searching {
                        IconView(.chevronDown, size: 16)
                            .foregroundStyle(AppTheme.muted)
                            .rotationEffect(.degrees(open ? 180 : 0))
                    }
                }
                .frame(minHeight: 52)
                .padding(.horizontal, 4)
                .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .disabled(searching)
            .accessibilityAddTraits(open ? .isSelected : [])
            if open {
                Rectangle().fill(AppTheme.stroke).frame(height: 0.5).padding(.horizontal, 4)
                if models.isEmpty {
                    Text("No models from this provider yet.")
                        .font(.system(size: 15))
                        .foregroundStyle(AppTheme.muted)
                        .padding(.vertical, 12)
                        .padding(.horizontal, 4)
                        .frame(maxWidth: .infinity, alignment: .leading)
                }
                ForEach(Array(models.enumerated()), id: \.element.id) { index, entry in
                    if index > 0 { Rectangle().fill(AppTheme.stroke).frame(height: 0.5).padding(.horizontal, 4) }
                    modelRow(entry, provider: provider, choice: choice)
                }
            }
        }
        .padding(.horizontal, 8)
        .background(
            RoundedRectangle(cornerRadius: 18, style: .continuous)
                .fill(open ? AppTheme.content.opacity(0.07) : .clear)
        )
        .padding(.vertical, 4)
    }

    private func modelRow(_ entry: ProviderModel, provider: ProviderKind, choice: ModelChoice) -> some View {
        let selected = entry.id == choice.modelID && provider == choice.provider
        let isDefault = Preferences.shared.defaultModels[provider]?.modelID == entry.id && Preferences.shared.defaultProvider == provider
        return Button {
            guard !selected else { return }
            Haptics.selection()
            let next = choice.switching(to: entry, provider: provider)
            Task { await app.apply(next, projectID: projectID, sessionID: sessionID) }
        } label: {
            HStack(spacing: 12) {
                VStack(alignment: .leading, spacing: 2) {
                    HStack(spacing: 8) {
                        Text(entry.name)
                            .font(.system(size: 16, weight: selected ? .semibold : .regular))
                            .foregroundStyle(AppTheme.content)
                            .lineLimit(1)
                        if isDefault {
                            Text("Default")
                                .font(.system(size: 12, weight: .semibold))
                                .foregroundStyle(AppTheme.accent)
                                .padding(.horizontal, 7)
                                .padding(.vertical, 2)
                                .background(AppTheme.accent.opacity(0.16), in: RoundedRectangle(cornerRadius: 6))
                        }
                    }
                    if let sub = entry.subProvider {
                        Text(sub).font(.system(size: 13)).foregroundStyle(AppTheme.muted).lineLimit(1)
                    }
                }
                .frame(maxWidth: .infinity, alignment: .leading)
                if selected {
                    IconView(.check, size: 18).foregroundStyle(AppTheme.accent)
                }
            }
            .padding(.horizontal, 4)
            .padding(.vertical, 10)
            .frame(minHeight: 52)
            .contentShape(Rectangle())
        }
        .buttonStyle(PressedOpacityStyle(opacity: 0.5))
        .accessibilityAddTraits(selected ? .isSelected : [])
    }

    private func loadUsage() async {
        guard let provider = choice?.provider, app.isConnected else { return }
        usageLoading = true
        defer { usageLoading = false }
        do {
            usage = try await app.planUsage(for: provider)
            usageError = nil
        } catch {
            usageError = error.localizedDescription
        }
    }
}

/// The provider's plan on the Mac, with its usage windows, percentages and
/// reset times, as the desktop footer shows them.
private struct AccountCard: View {
    let provider: ProviderKind
    let usage: PlanUsage?
    let loading: Bool
    let error: String?
    let onRefresh: () -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            HStack(spacing: 8) {
                Text(usage?.planLabel ?? "\(provider.name) on your Mac")
                    .font(.system(size: 16, weight: .semibold))
                    .foregroundStyle(AppTheme.content)
                    .lineLimit(1)
                if let warning {
                    Text(warning.label)
                        .font(.system(size: 13, weight: .semibold))
                        .foregroundStyle(warning.color)
                }
                Spacer()
                Button(action: onRefresh) {
                    Group {
                        if loading { ProgressView().controlSize(.small) } else { IconView(.refresh, size: 17) }
                    }
                    .foregroundStyle(AppTheme.muted)
                    .frame(width: 32, height: 32)
                }
                .buttonStyle(PressedOpacityStyle())
                .disabled(loading)
                .accessibilityLabel("Refresh account usage")
            }
            if let error {
                Text(error).font(.system(size: 13)).foregroundStyle(AppTheme.muted)
            } else if let usage, !usage.windows.isEmpty {
                ForEach(usage.windows, id: \.label) { window in
                    UsageMeter(window: window)
                }
            } else if !loading {
                Text("Usage isn't reported for this provider. Add accounts in desktop Settings → Accounts.")
                    .font(.system(size: 13))
                    .foregroundStyle(AppTheme.muted)
            }
        }
        .padding(.horizontal, 14)
        .padding(.vertical, 12)
        .background(AppTheme.content.opacity(0.07), in: RoundedRectangle(cornerRadius: 18, style: .continuous))
    }

    private var warning: (label: String, color: Color)? {
        guard let peak = usage?.windows.map(\.percent).max() else { return nil }
        if peak >= 100 { return ("Exhausted", AppTheme.deletions) }
        if peak >= 80 { return ("Low", AppTheme.caution) }
        return nil
    }
}

private struct UsageMeter: View {
    let window: PlanUsage.Window

    var body: some View {
        let percent = min(100, max(0, window.percent))
        VStack(alignment: .leading, spacing: 5) {
            HStack {
                Text(window.label).foregroundStyle(AppTheme.content)
                if let resetsAt = window.resetsAt {
                    Text("resets \(Date(timeIntervalSince1970: TimeInterval(resetsAt)), format: .relative(presentation: .named))")
                        .foregroundStyle(AppTheme.muted)
                }
                Spacer()
                Text("\(Int(percent.rounded()))%")
                    .monospacedDigit()
                    .foregroundStyle(percent >= 80 ? AppTheme.caution : AppTheme.muted)
            }
            .font(.system(size: 13))
            GeometryReader { proxy in
                ZStack(alignment: .leading) {
                    Capsule().fill(AppTheme.content.opacity(0.1))
                    Capsule()
                        .fill(percent >= 100 ? AppTheme.deletions : percent >= 80 ? AppTheme.caution : AppTheme.accent)
                        .frame(width: proxy.size.width * percent / 100)
                }
            }
            .frame(height: 4)
        }
    }
}

/// The chosen model's options and the session's access, each with its choices.
struct ModelOptions: View {
    @Bindable var app: AppModel
    let projectID: UUID
    let sessionID: UUID?

    var body: some View {
        let choice = sessionID.flatMap { app.session(withID: $0) }.map(app.choice(of:)) ?? app.newSessionChoice(for: projectID)
        let model = choice.flatMap { choice in app.models(for: choice.provider).first { $0.id == choice.modelID } }
        let access = sessionID.flatMap { app.session(withID: $0)?.runtimeMode } ?? app.newSessionAccess(for: projectID)
        ScrollView {
            VStack(alignment: .leading, spacing: 20) {
                if let choice, let model {
                    if !model.reasoningEfforts.isEmpty {
                        EffortControl(label: "Effort", options: model.reasoningEfforts, value: choice.reasoningEffort) { value in
                            var next = choice
                            next.reasoningEffort = value
                            Task { await app.apply(next, projectID: projectID, sessionID: sessionID) }
                        }
                        .padding(.horizontal, 16)
                        .padding(.vertical, 14)
                        .background(AppTheme.content.opacity(0.07), in: RoundedRectangle(cornerRadius: 18, style: .continuous))
                    }
                    if model.serviceTiers.contains(where: { $0.id == "fast" }) {
                        HStack(spacing: 12) {
                            IconView(.automation, size: 18).foregroundStyle(AppTheme.content)
                            VStack(alignment: .leading, spacing: 2) {
                                Text("Fast mode").font(.system(size: 16)).foregroundStyle(AppTheme.content)
                                Text("Quicker replies, at a higher cost.").font(.system(size: 13)).foregroundStyle(AppTheme.muted)
                            }
                            Spacer()
                            Toggle("Fast mode", isOn: Binding(
                                get: { choice.serviceTier == "fast" },
                                set: { on in
                                    var next = choice
                                    next.serviceTier = on ? "fast" : nil
                                    Task { await app.apply(next, projectID: projectID, sessionID: sessionID) }
                                }
                            ))
                            .labelsHidden()
                            .tint(AppTheme.accent)
                        }
                        .padding(.horizontal, 16)
                        .padding(.vertical, 14)
                        .background(AppTheme.content.opacity(0.07), in: RoundedRectangle(cornerRadius: 18, style: .continuous))
                    }
                    if model.contextWindows.count > 1 {
                        OptionList(
                            title: "Context window",
                            choices: model.contextWindows.map { OptionChoice(value: $0.id, label: $0.label, hint: $0.description) },
                            value: choice.contextWindow
                        ) { value in
                            var next = choice
                            next.contextWindow = value
                            Task { await app.apply(next, projectID: projectID, sessionID: sessionID) }
                        }
                    }
                }
                if choice?.provider != .fx {
                    OptionList(
                        title: "Access",
                        choices: RuntimeMode.allCases.map {
                            OptionChoice(value: $0.rawValue, label: $0.label, hint: $0.hint, icon: $0.icon, warm: $0 == .fullAccess)
                        },
                        value: access.rawValue
                    ) { value in
                        guard let mode = RuntimeMode(rawValue: value) else { return }
                        Task { await app.apply(access: mode, projectID: projectID, sessionID: sessionID) }
                    }
                }
            }
            .padding(.horizontal, 20)
            .padding(.top, 12)
            .padding(.bottom, 40)
        }
        .scrollIndicators(.hidden)
        .background(AppTheme.sheet.ignoresSafeArea())
        .navigationTitle("Options")
        .navigationBarTitleDisplayMode(.inline)
    }
}

struct OptionChoice: Identifiable {
    let value: String
    let label: String
    var hint: String?
    var icon: Icon?
    var warm = false

    var id: String { value }
}

/// An option's choices in a card, the chosen one checked.
struct OptionList: View {
    let title: String
    let choices: [OptionChoice]
    let value: String?
    let onPick: (String) -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            Text(title)
                .font(.system(size: 13, weight: .semibold))
                .foregroundStyle(AppTheme.muted)
                .padding(.horizontal, 4)
            VStack(spacing: 0) {
                ForEach(Array(choices.enumerated()), id: \.element.id) { index, entry in
                    if index > 0 { Rectangle().fill(AppTheme.stroke).frame(height: 0.5).padding(.horizontal, 4) }
                    let selected = entry.value == value
                    let tone = entry.warm ? AppTheme.caution : AppTheme.content
                    Button {
                        guard !selected else { return }
                        Haptics.selection()
                        onPick(entry.value)
                    } label: {
                        HStack(spacing: 12) {
                            if let icon = entry.icon {
                                IconView(icon, size: 18).foregroundStyle(tone)
                            }
                            VStack(alignment: .leading, spacing: 2) {
                                Text(entry.label)
                                    .font(.system(size: 16, weight: selected ? .semibold : .regular))
                                    .foregroundStyle(tone)
                                if let hint = entry.hint {
                                    Text(hint).font(.system(size: 13)).foregroundStyle(AppTheme.muted)
                                }
                            }
                            .frame(maxWidth: .infinity, alignment: .leading)
                            if selected {
                                IconView(.check, size: 18).foregroundStyle(AppTheme.accent)
                            }
                        }
                        .padding(.horizontal, 4)
                        .padding(.vertical, 10)
                        .frame(minHeight: 52)
                        .contentShape(Rectangle())
                    }
                    .buttonStyle(PressedOpacityStyle(opacity: 0.5))
                    .accessibilityAddTraits(selected ? .isSelected : [])
                }
            }
            .padding(.horizontal, 12)
            .background(AppTheme.content.opacity(0.07), in: RoundedRectangle(cornerRadius: 18, style: .continuous))
        }
    }
}

/// The effort slider, its level named on the right and warming past high.
struct EffortControl: View {
    let label: String
    let options: [ProviderModelOption]
    let value: String?
    let onChange: (String) -> Void

    @State private var dragged: Double?

    var body: some View {
        let index = options.firstIndex { $0.id == value } ?? 0
        let shown = Int((dragged ?? Double(index)).rounded())
        let hot = isHotEffort(options[safe: shown]?.id)
        VStack(alignment: .leading, spacing: 8) {
            HStack {
                Text(label).font(.system(size: 16)).foregroundStyle(AppTheme.content)
                Spacer()
                Text(options[safe: shown]?.label ?? "")
                    .font(.system(size: 15, weight: .semibold))
                    .foregroundStyle(hot ? AppTheme.caution : AppTheme.accent)
            }
            if options.count > 1 {
                Slider(
                    value: Binding(get: { dragged ?? Double(index) }, set: { next in
                        if Int(next.rounded()) != Int((dragged ?? Double(index)).rounded()) { Haptics.selection() }
                        dragged = next
                    }),
                    in: 0...Double(options.count - 1),
                    step: 1
                ) { editing in
                    if !editing, let dragged, let option = options[safe: Int(dragged.rounded())] {
                        self.dragged = nil
                        if option.id != value { onChange(option.id) }
                    }
                }
                .tint(hot ? AppTheme.caution : AppTheme.accent)
                .frame(height: 44)
                .accessibilityLabel(label)
            }
        }
    }
}

extension Array {
    subscript(safe index: Int) -> Element? {
        indices.contains(index) ? self[index] : nil
    }
}
