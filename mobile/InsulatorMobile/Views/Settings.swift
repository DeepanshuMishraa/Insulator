import PhotosUI
import SwiftUI

/// Settings: a sheet with its own stack, so its pages push inside it.
struct SettingsScreen: View {
    @Bindable var app: AppModel
    let projectID: UUID?

    @Environment(\.dismiss) private var dismiss
    @State private var preferences = Preferences.shared
    @State private var appearance = AppearanceSettings.shared
    @State private var confirmsDisconnect = false

    var body: some View {
        NavigationStack {
            SettingsPage {
                desktopCard

                SettingsHeading("Notifications")
                SettingsCard {
                    SettingsRow(icon: .lock, label: "Needs your input", accessory: {
                        Toggle("Needs your input", isOn: $preferences.notifyOnInput).labelsHidden().tint(AppTheme.accent)
                    })
                    SettingsDivider()
                    SettingsRow(icon: .check, label: "Finished", accessory: {
                        Toggle("Finished", isOn: $preferences.notifyOnFinish).labelsHidden().tint(AppTheme.accent)
                    })
                }

                SettingsHeading("Preferences")
                SettingsCard {
                    NavigationLink { AppearanceSettingsPage(projectID: projectID) } label: {
                        SettingsRow(icon: .appearance, label: "Appearance", value: appearance.scheme == .system
                            ? appearance.theme.name
                            : "\(appearance.theme.name), \(appearance.scheme.name.lowercased())", chevron: true)
                    }
                    SettingsDivider()
                    NavigationLink { ProvidersPage(app: app) } label: {
                        SettingsRow(icon: .robot, label: "Providers", value: preferences.defaultChoice.flatMap {
                            app.modelName(provider: $0.provider, modelID: $0.modelID)
                        } ?? "Desktop default", chevron: true)
                    }
                    SettingsDivider()
                    NavigationLink { PermissionsPage() } label: {
                        SettingsRow(icon: .lock, label: "Permissions", value: preferences.defaultAccess?.label ?? "Desktop default", chevron: true)
                    }
                }

                SettingsHeading("New sessions")
                SettingsCard {
                    SettingsRow(icon: .chat, label: "Open after starting", accessory: {
                        Toggle("Open sessions after starting", isOn: $preferences.openAfterStarting).labelsHidden().tint(AppTheme.accent)
                    })
                }
                SettingsNote("Turn off to stay on the new session screen and start more sessions.")

                SettingsHeading("Workspace")
                SettingsCard {
                    NavigationLink { ProjectsSelectionPage(app: app) } label: {
                        SettingsRow(icon: .folder, label: "Projects", value: "\(app.visibleProjects.count)", chevron: true)
                    }
                    SettingsDivider()
                    Button { Task { await app.reconnect() } } label: {
                        SettingsRow(icon: .computer, label: "Reconnect")
                    }
                }

                SettingsCard {
                    Button { confirmsDisconnect = true } label: {
                        SettingsRow(icon: .computerOff, label: "Disconnect desktop", destructive: true)
                    }
                }
                .padding(.top, 24)

                if case .failed(let message) = app.connectionState {
                    Text(message)
                        .font(.system(size: 14))
                        .foregroundStyle(AppTheme.deletions)
                        .padding(.top, 16)
                }
            }
            .navigationTitle("Settings")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .topBarLeading) {
                    Button { dismiss() } label: { IconView(.close, size: 18) }
                        .accessibilityLabel("Close")
                }
            }
            .confirmationDialog("Disconnect desktop?", isPresented: $confirmsDisconnect, titleVisibility: .visible) {
                Button("Disconnect", role: .destructive) {
                    dismiss()
                    app.disconnect(forget: true)
                }
            } message: {
                Text("You can pair again by scanning a new code. Running sessions keep going on the Mac.")
            }
        }
        .presentationBackground(AppTheme.sheet)
    }

    private var desktopCard: some View {
        HStack(spacing: 14) {
            IconView(app.isConnected ? .computer : .computerOff, size: 22)
                .foregroundStyle(app.isConnected ? AppTheme.content : AppTheme.muted)
            VStack(alignment: .leading, spacing: 2) {
                Text(app.displayHost ?? "Your Mac")
                    .font(.system(size: 17, weight: .semibold))
                    .foregroundStyle(AppTheme.content)
                    .lineLimit(1)
                Text(app.isConnected ? "Connected to desktop" : "Desktop unavailable. Your workspace stays here.")
                    .font(.system(size: 14))
                    .foregroundStyle(AppTheme.muted)
            }
            Spacer(minLength: 0)
        }
        .padding(16)
        .background(AppTheme.surface, in: RoundedRectangle(cornerRadius: Radii.card, style: .continuous))
        .overlay(RoundedRectangle(cornerRadius: Radii.card, style: .continuous).stroke(AppTheme.stroke, lineWidth: 0.5))
    }
}

// MARK: - Settings UI kit

struct SettingsPage<Content: View>: View {
    @ViewBuilder let content: () -> Content

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 0) {
                content()
            }
            .padding(.horizontal, 20)
            .padding(.top, 12)
            .padding(.bottom, 40)
        }
        .scrollIndicators(.hidden)
        .background(AppTheme.sheet.ignoresSafeArea())
    }
}

struct SettingsHeading: View {
    let text: String
    init(_ text: String) { self.text = text }

    var body: some View {
        Text(text)
            .font(.system(size: 15))
            .foregroundStyle(AppTheme.muted)
            .padding(.top, 24)
            .padding(.bottom, 8)
            .padding(.leading, 16)
    }
}

struct SettingsNote: View {
    let text: String
    init(_ text: String) { self.text = text }

    var body: some View {
        Text(text)
            .font(.system(size: 13))
            .lineSpacing(3)
            .foregroundStyle(AppTheme.muted)
            .padding(.top, 8)
            .padding(.horizontal, 16)
    }
}

struct SettingsCard<Content: View>: View {
    @ViewBuilder let content: () -> Content

    var body: some View {
        VStack(spacing: 0) { content() }
            .buttonStyle(PressedOpacityStyle())
            .padding(.horizontal, 10)
            .padding(.vertical, 6)
            .background(AppTheme.surface, in: RoundedRectangle(cornerRadius: Radii.card, style: .continuous))
    }
}

struct SettingsDivider: View {
    var body: some View {
        Rectangle().fill(AppTheme.stroke).frame(height: 0.5).padding(.leading, 40)
    }
}

/// A row: an icon, its label, an optional value, and a chevron or an accessory.
struct SettingsRow<Accessory: View, Leading: View>: View {
    let leading: Leading?
    let label: String
    var value: String?
    var chevron = false
    var destructive = false
    @ViewBuilder var accessory: () -> Accessory

    var body: some View {
        let tint = destructive ? AppTheme.deletions : AppTheme.content
        HStack(spacing: 0) {
            if let leading {
                leading
                    .foregroundStyle(tint)
                    .frame(width: 36, height: 36)
                    .padding(.trailing, 4)
            }
            Text(label)
                .font(.system(size: 17))
                .foregroundStyle(tint)
                .lineLimit(1)
                .frame(maxWidth: .infinity, alignment: .leading)
            if let value {
                Text(value)
                    .font(.system(size: 14))
                    .foregroundStyle(AppTheme.muted)
                    .lineLimit(1)
                    .padding(.trailing, 8)
            }
            accessory()
            if chevron {
                IconView(.chevronRight, size: 18).foregroundStyle(AppTheme.faint)
            }
        }
        .frame(minHeight: 52)
        .padding(.vertical, 0)
        .contentShape(Rectangle())
    }
}

extension SettingsRow where Leading == IconView {
    init(
        icon: Icon?,
        label: String,
        value: String? = nil,
        chevron: Bool = false,
        destructive: Bool = false,
        @ViewBuilder accessory: @escaping () -> Accessory
    ) {
        self.leading = icon.map { IconView($0, size: 18) }
        self.label = label
        self.value = value
        self.chevron = chevron
        self.destructive = destructive
        self.accessory = accessory
    }
}

extension SettingsRow where Leading == IconView, Accessory == EmptyView {
    init(icon: Icon?, label: String, value: String? = nil, chevron: Bool = false, destructive: Bool = false) {
        self.init(icon: icon, label: label, value: value, chevron: chevron, destructive: destructive) { EmptyView() }
    }
}

// MARK: - Appearance

struct AppearanceSettingsPage: View {
    let projectID: UUID?

    @State private var appearance = AppearanceSettings.shared
    @State private var preferences = Preferences.shared

    var body: some View {
        SettingsPage {
            if let projectID {
                SettingsHeading("Background image").padding(.top, -20)
                BackgroundSection(projectID: projectID)
            }
            SettingsHeading("Color scheme").padding(.top, projectID == nil ? -20 : 0)
            Picker("Color scheme", selection: $appearance.scheme) {
                ForEach(AppSchemeChoice.allCases) { Text($0.name).tag($0) }
            }
            .pickerStyle(.segmented)
            .onChange(of: appearance.scheme) { Haptics.selection() }

            SettingsHeading("Theme")
            SettingsCard {
                ForEach(Array(AppThemeChoice.allCases.enumerated()), id: \.element) { index, theme in
                    if index > 0 { SettingsDivider() }
                    Button {
                        Haptics.selection()
                        appearance.theme = theme
                    } label: {
                        SettingsRow(leading: Swatch(theme: theme), label: theme.name) {
                            if theme == appearance.theme {
                                IconView(.check, size: 18).foregroundStyle(AppTheme.accent)
                            }
                        }
                    }
                }
            }

            SettingsHeading("Font")
            SettingsCard {
                Picker("Font", selection: $appearance.font) {
                    ForEach(AppFontChoice.allCases.filter(\.isAvailable)) { Text($0.name).tag($0) }
                }
                .pickerStyle(.menu)
                .tint(AppTheme.muted)
                .frame(maxWidth: .infinity, minHeight: 52, alignment: .trailing)
            }

            SettingsHeading("Transcript")
            SettingsCard {
                SettingsRow(icon: .wrench, label: "Collapse tool calls") {
                    Toggle("Collapse tool calls", isOn: $preferences.collapseToolCalls).labelsHidden().tint(AppTheme.accent)
                }
            }
            SettingsNote("Show only the label of the step the agent is on. Tap a step to see its tool calls.")
        }
        .navigationTitle("Appearance")
        .navigationBarTitleDisplayMode(.inline)
    }
}

/// A theme in both schemes at once: its light and dark surfaces split
/// across a circle, its accent at the centre.
private struct Swatch: View {
    let theme: AppThemeChoice

    var body: some View {
        let light = theme.palette(.light)
        let dark = theme.palette(.dark)
        HStack(spacing: 0) {
            Color(hex: light.surface)
            Color(hex: dark.surface)
        }
        .frame(width: 24, height: 24)
        .clipShape(Circle())
        .overlay(Circle().fill(Color(hex: dark.accent)).frame(width: 10, height: 10))
        .overlay(Circle().stroke(AppTheme.stroke, lineWidth: 0.5))
    }
}

/// The project's empty-session photo, shown as the session will show it,
/// picked, changed or removed here, and how strongly it shows.
private struct BackgroundSection: View {
    let projectID: UUID

    @State private var backgrounds = EmptyBackgrounds.shared
    @State private var item: PhotosPickerItem?
    @State private var picking = false
    @State private var opacity: Double = 0.45

    var body: some View {
        let image = backgrounds.image(for: projectID)
        VStack(spacing: 4) {
            Button { picking = true } label: {
                ZStack {
                    AppTheme.background
                    if let image {
                        Image(uiImage: image).resizable().scaledToFill().opacity(opacity)
                    }
                    if backgrounds.busy.contains(projectID) {
                        ProgressView()
                    } else if image == nil {
                        VStack(spacing: 8) {
                            IconView(.image, size: 26).foregroundStyle(AppTheme.muted)
                            Text("Choose a photo").font(.system(size: 15)).foregroundStyle(AppTheme.muted)
                        }
                    }
                }
                .frame(height: 220)
                .clipShape(RoundedRectangle(cornerRadius: Radii.card - 8, style: .continuous))
                .overlay(RoundedRectangle(cornerRadius: Radii.card - 8, style: .continuous).stroke(AppTheme.stroke, lineWidth: 0.5))
            }
            .buttonStyle(.plain)
            .accessibilityLabel(image == nil ? "Choose photo" : "Change photo")
            VStack(spacing: 0) {
                if image != nil {
                    Slider(value: $opacity, in: EmptyBackgrounds.opacityRange) { editing in
                        if !editing { backgrounds.setOpacity(opacity, for: projectID) }
                    }
                    .tint(AppTheme.accent)
                    .padding(.horizontal, 6)
                    .frame(height: 44)
                    .accessibilityLabel("Background visibility")
                    SettingsDivider()
                }
                Button { picking = true } label: {
                    SettingsRow(icon: .image, label: image == nil ? "Choose photo" : "Change photo", chevron: true)
                }
                if image != nil {
                    SettingsDivider()
                    Button { backgrounds.remove(for: projectID) } label: {
                        SettingsRow(icon: .trash, label: "Remove photo", destructive: true)
                    }
                }
            }
            .padding(.top, 6)
        }
        .padding(12)
        .background(AppTheme.surface, in: RoundedRectangle(cornerRadius: Radii.card, style: .continuous))
        .photosPicker(isPresented: $picking, selection: $item, matching: .images)
        .onAppear { opacity = backgrounds.opacity(for: projectID) }
        .onChange(of: item) { _, picked in
            guard let picked else { return }
            Task {
                if let data = try? await picked.loadTransferable(type: Data.self), let image = UIImage(data: data) {
                    await backgrounds.set(image, for: projectID, screen: UIScreen.main.bounds.size)
                }
                item = nil
            }
        }
    }
}

// MARK: - Providers and permissions

private struct ProvidersPage: View {
    @Bindable var app: AppModel
    @State private var preferences = Preferences.shared

    var body: some View {
        SettingsPage {
            SettingsHeading("New sessions start with").padding(.top, -20)
            SettingsCard {
                ForEach(Array(app.installedProbes.enumerated()), id: \.element.provider) { index, probe in
                    if index > 0 { SettingsDivider() }
                    let selected = preferences.defaultProvider == probe.provider
                    NavigationLink { ProviderModelsPage(app: app, provider: probe.provider) } label: {
                        SettingsRow(
                            leading: ProviderLogo(provider: probe.provider, size: 20),
                            label: probe.provider.name,
                            value: selected
                                ? preferences.defaultModels[probe.provider].flatMap { app.modelName(provider: probe.provider, modelID: $0.modelID) }
                                : "\(probe.models.count) \(probe.models.count == 1 ? "model" : "models")"
                        ) {
                            HStack(spacing: 4) {
                                if selected { IconView(.check, size: 18).foregroundStyle(AppTheme.accent) }
                                IconView(.chevronRight, size: 18).foregroundStyle(AppTheme.faint)
                            }
                        }
                    }
                }
                if app.installedProbes.isEmpty {
                    ProgressView().frame(maxWidth: .infinity, minHeight: 52)
                }
            }
            SettingsNote(preferences.defaultProvider == nil
                ? "New sessions use the first agent installed on the desktop and its default model."
                : "A model keeps its effort and other settings. Sessions you have already started keep their own.")
            if preferences.defaultProvider != nil {
                DesktopDefaultButton { preferences.defaultProvider = nil }
            }
        }
        .navigationTitle("Providers")
        .navigationBarTitleDisplayMode(.inline)
    }
}

private struct ProviderModelsPage: View {
    @Bindable var app: AppModel
    let provider: ProviderKind

    @State private var preferences = Preferences.shared
    @State private var query = ""

    var body: some View {
        let models = app.models(for: provider)
        let needle = query.trimmingCharacters(in: .whitespaces).lowercased()
        let shown = needle.isEmpty ? models : models.filter {
            [$0.name, $0.id, $0.subProvider ?? ""].contains { $0.lowercased().contains(needle) }
        }
        let saved = preferences.defaultModels[provider]
        let chosen = preferences.defaultProvider == provider ? saved : nil
        let current = saved ?? models.first.map { ModelChoice(provider: provider, model: $0) }
        let model = models.first { $0.id == current?.modelID }
        SettingsPage {
            if let current, let model, !model.reasoningEfforts.isEmpty || model.serviceTiers.contains(where: { $0.id == "fast" }) {
                SettingsCard {
                    if !model.reasoningEfforts.isEmpty {
                        EffortControl(label: "Effort", options: model.reasoningEfforts, value: current.reasoningEffort) { value in
                            var next = current
                            next.reasoningEffort = value
                            save(next)
                        }
                        .padding(.vertical, 8)
                        .padding(.horizontal, 6)
                    }
                    if model.serviceTiers.contains(where: { $0.id == "fast" }) {
                        SettingsRow(icon: .automation, label: "Fast mode") {
                            Toggle("Fast mode", isOn: Binding(get: { current.serviceTier == "fast" }, set: { on in
                                var next = current
                                next.serviceTier = on ? "fast" : nil
                                save(next)
                            }))
                            .labelsHidden()
                            .tint(AppTheme.accent)
                        }
                    }
                }
                SettingsHeading("Models")
            }
            if shown.isEmpty {
                Text(needle.isEmpty ? "No models from this provider yet." : "No models match.")
                    .font(.system(size: 15))
                    .foregroundStyle(AppTheme.muted)
                    .frame(maxWidth: .infinity)
                    .padding(.top, 24)
            } else {
                SettingsCard {
                    ForEach(Array(shown.enumerated()), id: \.element.id) { index, entry in
                        if index > 0 { SettingsDivider() }
                        Button {
                            guard entry.id != chosen?.modelID else { return }
                            Haptics.selection()
                            save((saved ?? ModelChoice(provider: provider, model: entry)).switching(to: entry, provider: provider))
                        } label: {
                            SettingsRow(icon: nil, label: entry.name, value: entry.subProvider) {
                                if entry.id == chosen?.modelID {
                                    IconView(.check, size: 18).foregroundStyle(AppTheme.accent)
                                }
                            }
                        }
                    }
                }
            }
        }
        .navigationTitle(provider.name)
        .navigationBarTitleDisplayMode(.inline)
        .searchable(text: $query, prompt: "Search models")
    }

    private func save(_ choice: ModelChoice) {
        preferences.defaultModels[provider] = choice
        preferences.defaultProvider = provider
    }
}

private struct PermissionsPage: View {
    @State private var preferences = Preferences.shared

    var body: some View {
        SettingsPage {
            SettingsHeading("New sessions start with").padding(.top, -20)
            SettingsCard {
                ForEach(Array(RuntimeMode.allCases.enumerated()), id: \.element) { index, mode in
                    if index > 0 { SettingsDivider() }
                    let tint = mode == .fullAccess ? AppTheme.caution : AppTheme.content
                    Button {
                        guard preferences.defaultAccess != mode else { return }
                        Haptics.selection()
                        preferences.defaultAccess = mode
                    } label: {
                        HStack(spacing: 0) {
                            IconView(mode.icon, size: 18)
                                .foregroundStyle(tint)
                                .frame(width: 36, height: 36)
                                .padding(.trailing, 4)
                            VStack(alignment: .leading, spacing: 2) {
                                Text(mode.label).font(.system(size: 17)).foregroundStyle(tint)
                                Text(mode.hint).font(.system(size: 13)).foregroundStyle(AppTheme.muted)
                            }
                            .frame(maxWidth: .infinity, alignment: .leading)
                            if preferences.defaultAccess == mode {
                                IconView(.check, size: 18).foregroundStyle(AppTheme.accent)
                            }
                        }
                        .padding(.vertical, 10)
                        .contentShape(Rectangle())
                    }
                    .buttonStyle(.plain)
                    .accessibilityHint(mode.hint)
                }
            }
            SettingsNote(preferences.defaultAccess == nil
                ? "New sessions use the permissions set on the desktop."
                : "Sessions you have already started keep their own permissions.")
            if preferences.defaultAccess != nil {
                DesktopDefaultButton { preferences.defaultAccess = nil }
            }
        }
        .navigationTitle("Permissions")
        .navigationBarTitleDisplayMode(.inline)
    }
}

private struct DesktopDefaultButton: View {
    let action: () -> Void

    var body: some View {
        Button {
            Haptics.selection()
            action()
        } label: {
            HStack(spacing: 8) {
                IconView(.computer, size: 18)
                Text("Use desktop default").font(.system(size: 16, weight: .semibold))
            }
            .foregroundStyle(AppTheme.content)
            .frame(maxWidth: .infinity, minHeight: 50)
            .background(AppTheme.surface, in: Capsule())
        }
        .buttonStyle(PressedOpacityStyle())
        .padding(.top, 24)
    }
}

/// Which of the Mac's projects show on this phone.
private struct ProjectsSelectionPage: View {
    @Bindable var app: AppModel
    @State private var preferences = Preferences.shared

    var body: some View {
        let visible = preferences.visibleProjectIDs ?? Set(app.projects.map(\.id))
        SettingsPage {
            SettingsCard {
                ForEach(Array(app.projects.enumerated()), id: \.element.id) { index, project in
                    if index > 0 { SettingsDivider() }
                    Button {
                        Haptics.selection()
                        var next = visible
                        if next.contains(project.id) { next.remove(project.id) } else { next.insert(project.id) }
                        preferences.visibleProjectIDs = next.count == app.projects.count ? nil : next
                    } label: {
                        SettingsRow(leading: ProjectMascot(path: project.path, size: 20), label: project.name) {
                            if visible.contains(project.id) {
                                IconView(.check, size: 18).foregroundStyle(AppTheme.accent)
                            }
                        }
                    }
                }
            }
            SettingsNote("Choose the projects you want on your phone. Selection controls what appears here; sessions keep running on the Mac.")
        }
        .navigationTitle("Projects")
        .navigationBarTitleDisplayMode(.inline)
    }
}

// MARK: - Project picker

/// The project switch: every project, the open one checked, narrowed by a search.
struct ProjectPicker: View {
    @Bindable var app: AppModel
    @Environment(\.dismiss) private var dismiss
    @State private var query = ""

    var body: some View {
        let needle = query.trimmingCharacters(in: .whitespaces).lowercased()
        let shown = needle.isEmpty ? app.visibleProjects : app.visibleProjects.filter { $0.name.lowercased().contains(needle) }
        NavigationStack {
            SettingsPage {
                SettingsCard {
                    ForEach(Array(shown.enumerated()), id: \.element.id) { index, project in
                        if index > 0 { SettingsDivider() }
                        Button {
                            Haptics.selection()
                            Preferences.shared.currentProjectID = project.id
                            dismiss()
                        } label: {
                            SettingsRow(leading: ProjectMascot(path: project.path, size: 20, active: app.sessions(in: project.id).contains { $0.status.isBusy }), label: project.name) {
                                if project.id == app.currentProject?.id {
                                    IconView(.check, size: 18).foregroundStyle(AppTheme.accent)
                                }
                            }
                        }
                    }
                }
                if !needle.isEmpty, shown.isEmpty {
                    Text("No projects match.")
                        .font(.system(size: 15))
                        .foregroundStyle(AppTheme.muted)
                        .frame(maxWidth: .infinity)
                        .padding(.top, 24)
                }
            }
            .navigationTitle("Projects")
            .navigationBarTitleDisplayMode(.inline)
            .searchable(text: $query, placement: .navigationBarDrawer(displayMode: .always), prompt: "Search projects")
            .toolbar {
                ToolbarItem(placement: .topBarLeading) {
                    Button { dismiss() } label: { IconView(.close, size: 18) }
                        .accessibilityLabel("Close")
                }
            }
        }
        .presentationBackground(AppTheme.sheet)
    }
}
