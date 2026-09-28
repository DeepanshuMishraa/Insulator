import SwiftUI
import UIKit

enum HomeTab: String, CaseIterable {
    case projects = "Projects"
    case chats = "Chats"
}

struct MainView: View {
    @Bindable var app: AppModel
    @State private var selectedTab: HomeTab = .projects
    @State private var search = ""
    @State private var showsSettings = false
    @State private var confirmsForget = false
    @State private var isCreatingChat = false
    @State private var expandedProjects: Set<UUID> = []
    @State private var path: [UUID] = []
    @State private var projectForRename: Project?
    @State private var renameProjectText = ""
    @State private var projectForRemoval: Project?
    @FocusState private var searchFocused: Bool

    private var filteredSessions: [AgentSession] {
        let query = search.trimmingCharacters(in: .whitespacesAndNewlines)
        let sorted = app.sessions.sorted { sessionRecency($0) > sessionRecency($1) }
        guard !query.isEmpty else { return sorted }
        return sorted.filter { session in
            session.displayTitle.localizedStandardContains(query)
                || app.project(for: session)?.name.localizedStandardContains(query) == true
                || session.provider.name.localizedStandardContains(query)
        }
    }

    private var filteredProjects: [Project] {
        let query = search.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !query.isEmpty else { return app.projects }
        return app.projects.filter { project in
            project.name.localizedStandardContains(query)
                || project.path.localizedStandardContains(query)
                || sessions(for: project).contains { session in
                    session.displayTitle.localizedStandardContains(query)
                        || session.provider.name.localizedStandardContains(query)
                }
        }
    }

    var body: some View {
        NavigationStack(path: $path) {
            VStack(spacing: 0) {
                // Custom Top Header Bar (No system navigation bar chrome = zero double buttons)
                HStack(alignment: .center) {
                    Button {
                        showsSettings = true
                    } label: {
                        ReiconIcon(.settings, size: 18)
                            .foregroundStyle(AppTheme.primary)
                            .frame(width: 44, height: 44)
                            .background(AppTheme.raised, in: Circle())
                    }
                    .buttonStyle(.plain)
                    .accessibilityLabel("Settings")

                    Spacer()

                    VStack(spacing: 3) {
                        Text("Insulator")
                            .font(AppTheme.font(.title3, weight: .bold))
                            .foregroundStyle(AppTheme.primary)
                        HStack(spacing: 5) {
                            Circle()
                                .fill(app.isConnected ? AppTheme.accent : Color.secondary)
                                .frame(width: 6, height: 6)
                                .accessibilityHidden(true)
                            ReiconIcon(.laptop, size: 11)
                                .foregroundStyle(AppTheme.secondary)
                                .accessibilityHidden(true)
                            Text(app.displayHost ?? "No Mac paired")
                                .font(AppTheme.font(.caption))
                                .foregroundStyle(AppTheme.secondary)
                                .lineLimit(1)
                        }
                    }

                    Spacer()

                    Menu {
                        Button {
                            Task { await app.reconnect() }
                        } label: {
                            Label("Reconnect", image: Reicon.refresh.rawValue)
                        }
                        Button(role: .destructive) {
                            confirmsForget = true
                        } label: {
                            Label("Forget Mac", image: Reicon.trash.rawValue)
                        }
                    } label: {
                        ReiconIcon(.more, size: 17)
                            .foregroundStyle(AppTheme.primary)
                            .frame(width: 44, height: 44)
                            .background(AppTheme.raised, in: Circle())
                    }
                    .buttonStyle(.plain)
                    .accessibilityLabel("More actions")
                }
                .padding(.horizontal, 16)
                .padding(.top, 4)
                .padding(.bottom, 8)
                .contentShape(Rectangle())
                .simultaneousGesture(TapGesture().onEnded { searchFocused = false })

                // Tab switcher (Projects / Chats)
                HStack(spacing: 8) {
                    ForEach(HomeTab.allCases, id: \.self) { tab in
                        Button {
                            UISelectionFeedbackGenerator().selectionChanged()
                            searchFocused = false
                            withAnimation(.spring(response: 0.24, dampingFraction: 0.78)) {
                                selectedTab = tab
                            }
                        } label: {
                            Text(tab.rawValue)
                                .font(AppTheme.font(size: 15, weight: .medium))
                                .padding(.horizontal, 18)
                                .padding(.vertical, 8)
                                .background(
                                    selectedTab == tab ? AppTheme.primary : AppTheme.raised,
                                    in: Capsule()
                                )
                                .foregroundStyle(selectedTab == tab ? AppTheme.background : AppTheme.primary)
                        }
                        .buttonStyle(BouncyButtonStyle(scale: 0.96))
                    }
                    Spacer()
                }
                .padding(.horizontal, 16)
                .padding(.bottom, 12)

                Group {
                    if app.isLoading && app.sessions.isEmpty && app.projects.isEmpty {
                        ProgressView("Loading…")
                            .frame(maxWidth: .infinity, maxHeight: .infinity)
                    } else if selectedTab == .projects {
                        projectsList
                    } else {
                        chatsList
                    }
                }
                .frame(maxWidth: .infinity, maxHeight: .infinity)
                .background(AppTheme.background)

                // Bottom Bar: Search Chats capsule + New Chat button
                HStack(spacing: 12) {
                    HStack(spacing: 10) {
                        ReiconIcon(.search, size: 16)
                            .foregroundStyle(AppTheme.secondary)
                        TextField(selectedTab == .projects ? "Search Projects" : "Search Chats", text: $search)
                            .focused($searchFocused)
                            .textInputAutocapitalization(.never)
                            .autocorrectionDisabled()
                            .submitLabel(.search)
                            .onSubmit { searchFocused = false }
                            .foregroundStyle(AppTheme.primary)
                    }
                    .padding(.horizontal, 16)
                    .frame(height: 52)
                    .background(AppTheme.raised, in: Capsule())

                    Button {
                        UIImpactFeedbackGenerator(style: .medium).impactOccurred()
                        searchFocused = false
                        createNewChat()
                    } label: {
                        if isCreatingChat {
                            ProgressView()
                                .tint(AppTheme.background)
                                .frame(width: 52, height: 52)
                                .background(AppTheme.primary, in: Circle())
                        } else {
                            ReiconIcon(.compose, size: 20)
                                .foregroundStyle(AppTheme.background)
                                .frame(width: 52, height: 52)
                                .background(AppTheme.primary, in: Circle())
                        }
                    }
                    .buttonStyle(BouncyButtonStyle(scale: 0.94))
                    .disabled(isCreatingChat)
                    .accessibilityLabel("New chat")
                }
                .padding(.horizontal, 16)
                .padding(.bottom, 8)
                .background(AppTheme.background)
            }
            .background(AppTheme.background)
            .toolbar(.hidden, for: .navigationBar)
            .alert("Rename Project", isPresented: Binding(
                get: { projectForRename != nil },
                set: { if !$0 { projectForRename = nil } }
            )) {
                TextField("Project name", text: $renameProjectText)
                Button("Save") {
                    guard let project = projectForRename else { return }
                    Task { await app.renameProject(project.id, name: renameProjectText) }
                    projectForRename = nil
                }
                Button("Cancel", role: .cancel) { projectForRename = nil }
            }
            .confirmationDialog(
                "Remove this project?",
                isPresented: Binding(
                    get: { projectForRemoval != nil },
                    set: { if !$0 { projectForRemoval = nil } }
                ),
                titleVisibility: .visible
            ) {
                Button("Remove Project", role: .destructive) {
                    guard let project = projectForRemoval else { return }
                    Task { await app.removeProject(project.id) }
                    projectForRemoval = nil
                }
                Button("Cancel", role: .cancel) { projectForRemoval = nil }
            } message: {
                Text("This removes the project and its chats from Insulator. Files on disk are not deleted.")
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
            .sheet(isPresented: $showsSettings) {
                SettingsView(app: app)
            }
        }
        .tint(AppTheme.primary)
    }

    @ViewBuilder
    private var projectsList: some View {
        if filteredProjects.isEmpty {
            emptyState(title: search.isEmpty ? "No projects" : "No matching projects")
        } else {
            List {
                ForEach(filteredProjects) { project in
                    Section {
                        Button {
                            searchFocused = false
                            withAnimation(.snappy) {
                                if expandedProjects.contains(project.id) {
                                    expandedProjects.remove(project.id)
                                } else {
                                    expandedProjects.insert(project.id)
                                }
                            }
                        } label: {
                            ProjectRow(
                                project: project,
                                chatCount: sessions(for: project).count,
                                isExpanded: expandedProjects.contains(project.id) || !searchQuery.isEmpty,
                                githubAvatar: app.githubAvatar
                            )
                        }
                        .buttonStyle(.plain)
                        .contextMenu {
                            if let url = app.projectGitHubURLs[project.id] {
                                Button {
                                    UIApplication.shared.open(url)
                                } label: {
                                    Text("Open in GitHub")
                                }
                            }
                            Button {
                                renameProjectText = project.name
                                projectForRename = project
                            } label: {
                                Label("Rename", image: Reicon.edit.rawValue)
                            }
                            Button(role: .destructive) {
                                projectForRemoval = project
                            } label: {
                                Label("Remove", image: Reicon.trash.rawValue)
                            }
                        }
                        .listRowBackground(AppTheme.background)
                        .accessibilityValue(expandedProjects.contains(project.id) || !searchQuery.isEmpty ? "Expanded" : "Collapsed")

                        if expandedProjects.contains(project.id) || !searchQuery.isEmpty {
                            ForEach(visibleSessions(for: project)) { session in
                                Button {
                                    searchFocused = false
                                    path.append(session.id)
                                } label: {
                                    SessionRow(session: session, project: project)
                                        .padding(.leading, 16)
                                }
                                .buttonStyle(.plain)
                                .listRowBackground(AppTheme.background)
                            }
                        }
                    }
                }
            }
            .listStyle(.plain)
            .scrollContentBackground(.hidden)
            .scrollDismissesKeyboard(.immediately)
            .scrollIndicators(.hidden)
            .refreshable { await app.refresh() }
        }
    }

    @ViewBuilder
    private var chatsList: some View {
        if filteredSessions.isEmpty {
            emptyState(title: search.isEmpty ? "No chats" : "No matching chats")
        } else {
            List(filteredSessions) { session in
                Button {
                    searchFocused = false
                    path.append(session.id)
                } label: {
                    SessionRow(session: session, project: app.project(for: session))
                }
                .buttonStyle(.plain)
                .listRowBackground(AppTheme.background)
            }
            .listStyle(.plain)
            .scrollContentBackground(.hidden)
            .scrollDismissesKeyboard(.immediately)
            .scrollIndicators(.hidden)
            .refreshable { await app.refresh() }
        }
    }

    private func emptyState(title: String) -> some View {
        Text(title)
            .font(AppTheme.font(size: 15))
            .foregroundStyle(AppTheme.secondary)
            .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
            .padding(16)
            .contentShape(Rectangle())
            .onTapGesture { searchFocused = false }
    }

    private var searchQuery: String {
        search.trimmingCharacters(in: .whitespacesAndNewlines)
    }

    private func visibleSessions(for project: Project) -> [AgentSession] {
        let projectSessions = sessions(for: project)
        guard !searchQuery.isEmpty,
              !project.name.localizedStandardContains(searchQuery),
              !project.path.localizedStandardContains(searchQuery) else { return projectSessions }
        return projectSessions.filter {
            $0.displayTitle.localizedStandardContains(searchQuery)
                || $0.provider.name.localizedStandardContains(searchQuery)
        }
    }

    private func sessions(for project: Project) -> [AgentSession] {
        app.sessions
            .filter { $0.projectID == project.id }
            .sorted { sessionRecency($0) > sessionRecency($1) }
    }

    private func sessionRecency(_ session: AgentSession) -> UInt64 {
        session.lastReplyAt ?? session.createdAt
    }

    private func createNewChat() {
        guard !isCreatingChat else { return }
        isCreatingChat = true
        Task {
            defer { isCreatingChat = false }
            do {
                let session = try await app.createNewSession()
                path.append(session.id)
            } catch {
                app.errorMessage = error.localizedDescription
            }
        }
    }
}

private struct ProjectRow: View {
    let project: Project
    let chatCount: Int
    let isExpanded: Bool
    let githubAvatar: UIImage?

    var body: some View {
        HStack(spacing: 13) {
            Group {
                if let githubAvatar {
                    Image(uiImage: githubAvatar)
                        .resizable()
                        .scaledToFill()
                } else {
                    Text(project.name.prefix(1).uppercased())
                        .font(AppTheme.font(size: 14, weight: .bold))
                        .foregroundStyle(AppTheme.background)
                        .frame(maxWidth: .infinity, maxHeight: .infinity)
                        .background(AppTheme.accent)
                }
            }
            .frame(width: 36, height: 36)
            .clipShape(Circle())
            .accessibilityHidden(true)

            VStack(alignment: .leading, spacing: 4) {
                Text(project.name)
                    .font(AppTheme.font(.body, weight: .semibold))
                    .lineLimit(1)
                Text(project.path)
                    .font(AppTheme.font(.caption))
                    .foregroundStyle(AppTheme.secondary)
                    .lineLimit(1)
            }

            Spacer(minLength: 8)

            Text("\(chatCount)")
                .font(AppTheme.font(.caption))
                .monospacedDigit()
                .foregroundStyle(AppTheme.secondary)
                .accessibilityLabel("\(chatCount) chats")

            ReiconIcon(.chevronRight, size: 12)
                .foregroundStyle(AppTheme.secondary)
                .rotationEffect(.degrees(isExpanded ? 90 : 0))
        }
        .padding(.vertical, 6)
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
                .foregroundStyle(AppTheme.primary)
                .frame(width: 36, height: 36)
                .background(AppTheme.raised, in: Circle())
                .accessibilityHidden(true)

            VStack(alignment: .leading, spacing: 4) {
                Text(session.displayTitle)
                    .font(AppTheme.font(.body, weight: .medium))
                    .lineLimit(1)
                HStack(spacing: 6) {
                    Text(project?.name ?? "Quick Chat")
                    Text("·")
                    Text(session.provider.name)
                }
                .font(AppTheme.font(.caption))
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
