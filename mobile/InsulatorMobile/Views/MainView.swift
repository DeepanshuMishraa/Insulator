import SwiftUI

struct MainView: View {
    @Bindable var app: AppModel
    @State private var search = ""
    @State private var showsNewTask = false
    @State private var showsSettings = false
    @State private var confirmsForget = false
    @State private var path: [UUID] = []

    private var filteredSessions: [AgentSession] {
        let query = search.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !query.isEmpty else { return app.sessions }
        return app.sessions.filter { session in
            session.displayTitle.localizedStandardContains(query)
                || app.project(for: session)?.name.localizedStandardContains(query) == true
                || session.provider.name.localizedStandardContains(query)
        }
    }

    var body: some View {
        NavigationStack(path: $path) {
            VStack(spacing: 0) {
                Group {
                    if app.isLoading && app.sessions.isEmpty {
                        ProgressView("Loading tasks…")
                            .frame(maxWidth: .infinity, maxHeight: .infinity)
                    } else if filteredSessions.isEmpty && search.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
                        ContentUnavailableView {
                            Label("No tasks", systemImage: "bubble.left.and.text.bubble.right")
                        } description: {
                            Text("Start a task on this iPhone or in Insulator Desktop.")
                        } actions: {
                            Button("New task") { showsNewTask = true }
                                .buttonStyle(.borderedProminent)
                        }
                    } else if filteredSessions.isEmpty {
                        ContentUnavailableView.search
                    } else {
                        List(filteredSessions) { session in
                            NavigationLink(value: session.id) {
                                SessionRow(session: session, project: app.project(for: session))
                            }
                            .listRowBackground(AppTheme.background)
                        }
                        .listStyle(.plain)
                        .refreshable { await app.refresh() }
                    }
                }
                .frame(maxWidth: .infinity, maxHeight: .infinity)
                .background(AppTheme.background)

                HStack(spacing: 10) {
                    Image(systemName: "magnifyingglass")
                        .foregroundStyle(AppTheme.secondary)
                    TextField("Search tasks", text: $search)
                        .textInputAutocapitalization(.never)
                        .autocorrectionDisabled()
                        .foregroundStyle(.white)
                }
                .padding(.horizontal, 16)
                .frame(minHeight: 52)
                .background(AppTheme.raised, in: Capsule())
                .padding(.horizontal, 16)
                .padding(.bottom, 8)
                .background(AppTheme.background)
            }
            .background(AppTheme.background)
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                HomeToolbar(
                    host: app.displayHost,
                    isConnected: app.isConnected,
                    onGear: { showsSettings = true }
                ) {
                    Button {
                        showsNewTask = true
                    } label: {
                        Label("New task", systemImage: "square.and.pencil")
                    }
                    Button {
                        Task { await app.reconnect() }
                    } label: {
                        Label("Reconnect", systemImage: "arrow.clockwise")
                    }
                    Button(role: .destructive) {
                        confirmsForget = true
                    } label: {
                        Label("Forget Mac", systemImage: "trash")
                    }
                }
            }
            .confirmationDialog("Forget this Mac?", isPresented: $confirmsForget, titleVisibility: .visible) {
                Button("Forget Mac", role: .destructive) {
                    app.disconnect(forget: true)
                }
                Button("Cancel", role: .cancel) {}
            } message: {
                Text("You will need its Tailscale address and daemon token to connect again. Running tasks will continue on the Mac.")
            }
            .navigationDestination(for: UUID.self) { sessionID in
                ThreadView(app: app, sessionID: sessionID)
                    .task {
                        guard let session = app.sessions.first(where: { $0.id == sessionID }) else { return }
                        await app.open(session)
                    }
            }
            .sheet(isPresented: $showsNewTask) {
                NewTaskView(app: app) { session in
                    showsNewTask = false
                    path.append(session.id)
                }
            }
            .sheet(isPresented: $showsSettings) {
                SettingsView(app: app)
            }
        }
        .tint(.white)
    }
}

private struct SessionRow: View {
    let session: AgentSession
    let project: Project?

    var body: some View {
        HStack(spacing: 13) {
            Image(session.provider.iconName)
                .resizable()
                .aspectRatio(contentMode: .fit)
                .frame(width: 17, height: 17)
                .foregroundStyle(.white)
                .frame(width: 36, height: 36)
                .background(AppTheme.raised, in: Circle())
                .accessibilityHidden(true)

            VStack(alignment: .leading, spacing: 4) {
                Text(session.displayTitle)
                    .font(.body.weight(.medium))
                    .lineLimit(1)
                HStack(spacing: 6) {
                    Text(project?.name ?? "Unknown project")
                    Text("·")
                    Text(session.provider.name)
                }
                .font(.caption)
                .foregroundStyle(AppTheme.secondary)
                .lineLimit(1)
            }

            Spacer(minLength: 8)

            if session.status.isBusy {
                ProgressView()
                    .controlSize(.small)
                    .accessibilityLabel("Running")
            }
        }
        .padding(.vertical, 6)
    }
}

private struct NewTaskView: View {
    @Bindable var app: AppModel
    @Environment(\.dismiss) private var dismiss
    let onCreate: (AgentSession) -> Void

    @State private var projectID: UUID?
    @State private var provider: ProviderKind = .pi
    @State private var modelID: String?
    @State private var mode: RuntimeMode = .ask
    @State private var reasoningEffort: String?
    @State private var isCreating = false

    private var selectedProject: Project? {
        app.projects.first { $0.id == projectID }
    }

    private var availableModels: [ProviderModel] { app.models(for: provider) }

    private var selectedModel: ProviderModel? {
        availableModels.first { $0.id == modelID }
    }

    var body: some View {
        NavigationStack {
            Form {
                Section("Project") {
                    Picker("Project", selection: $projectID) {
                        Text("Choose a project").tag(UUID?.none)
                        ForEach(app.projects) { project in
                            Text(project.name).tag(Optional(project.id))
                        }
                    }
                }

                Section("Agent") {
                    Picker("Provider", selection: $provider) {
                        ForEach(app.installedProbes) { probe in
                            Label {
                                Text(probe.provider.name)
                            } icon: {
                                Image(probe.provider.iconName)
                                    .resizable()
                                    .aspectRatio(contentMode: .fit)
                                    .frame(width: 18, height: 18)
                            }
                            .tag(probe.provider)
                        }
                    }
                    .onChange(of: provider) { _, newProvider in selectDefaults(for: newProvider) }

                    if !availableModels.isEmpty {
                        Picker("Model", selection: $modelID) {
                            Text("Provider default").tag(String?.none)
                            ForEach(availableModels) { model in
                                Text(model.name).tag(Optional(model.id))
                            }
                        }
                    }

                    Picker("Access", selection: $mode) {
                        ForEach(RuntimeMode.allCases) { mode in
                            Text(mode.name).tag(mode)
                        }
                    }

                    if let model = selectedModel, !model.reasoningEfforts.isEmpty {
                        Picker("Reasoning", selection: $reasoningEffort) {
                            ForEach(model.reasoningEfforts) { effort in
                                Text(effort.label).tag(Optional(effort.id))
                            }
                        }
                    }
                }
            }
            .navigationTitle("New task")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("Cancel") { dismiss() }
                }
                ToolbarItem(placement: .confirmationAction) {
                    Button(isCreating ? "Creating…" : "Create") {
                        create()
                    }
                    .disabled(selectedProject == nil || isCreating || app.probes[provider]?.installed != true)
                }
            }
            .task {
                projectID = projectID ?? app.projects.first?.id
                if app.probes[provider]?.installed != true,
                   let first = app.installedProbes.first?.provider {
                    provider = first
                }
                selectDefaults(for: provider)
            }
        }
    }

    private func selectDefaults(for provider: ProviderKind) {
        let models = app.models(for: provider)
        let model = models.first(where: \.isDefault) ?? models.first
        modelID = model?.id
        reasoningEffort = model?.defaultReasoningEffort
    }

    private func create() {
        guard let project = selectedProject else { return }
        isCreating = true
        Task {
            do {
                let session = try await app.createSession(
                    project: project,
                    provider: provider,
                    model: selectedModel,
                    mode: mode,
                    reasoningEffort: reasoningEffort,
                    serviceTier: selectedModel?.defaultServiceTier,
                    contextWindow: selectedModel?.defaultContextWindow,
                    agentPreset: app.probes[provider]?.agentPresets.first(where: \.isDefault)?.id
                )
                onCreate(session)
                dismiss()
            } catch {
                app.errorMessage = error.localizedDescription
                isCreating = false
            }
        }
    }
}
