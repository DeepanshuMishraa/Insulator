import SwiftUI

/// What the Sessions page narrows its list to.
struct SessionFilter: Equatable {
    var working = false
    var provider: ProviderKind?

    static let none = SessionFilter()
}

/// A project's sessions, newest first. Each swipes left to rename or delete,
/// as on the desktop sidebar. Searched and filtered from the bar where the
/// composer rests on the first page, and narrowed to a worktree when the
/// workspace menu picks one.
struct SessionsPage: View {
    @Bindable var app: AppModel
    let project: Project
    let worktree: WorktreeEntry?
    let active: Bool

    @Environment(Router.self) private var router
    @State private var query = ""
    @State private var filter = SessionFilter.none
    @State private var renaming: AgentSession?
    @State private var renameText = ""
    @State private var deleting: AgentSession?

    private var listed: [AgentSession] {
        let needle = query.trimmingCharacters(in: .whitespacesAndNewlines).lowercased()
        return app.sessions(in: project.id).filter { session in
            (needle.isEmpty || session.displayTitle.lowercased().contains(needle)
                || (session.branch?.lowercased().contains(needle) ?? false))
                && inWorktree(session)
        }
    }

    private var shown: [AgentSession] {
        listed.filter { session in
            (!filter.working || session.status.isBusy) && (filter.provider == nil || session.provider == filter.provider)
        }
    }

    var body: some View {
        let sessions = shown
        List {
            if sessions.isEmpty {
                Text(query.isEmpty && filter == .none && worktree == nil ? "No sessions yet." : "No matching sessions.")
                    .font(AppTheme.font(size: 15))
                    .foregroundStyle(AppTheme.muted)
                    .listRowBackground(Color.clear)
                    .listRowSeparator(.hidden)
            }
            ForEach(sessions) { session in
                Button {
                    router.open(session.id)
                } label: {
                    SessionCard(session: session, model: app.modelName(provider: session.provider, modelID: session.model), paused: !active)
                }
                .buttonStyle(PressedHighlightStyle())
                .listRowInsets(EdgeInsets(top: 0, leading: 10, bottom: 0, trailing: 10))
                .listRowBackground(Color.clear)
                .listRowSeparatorTint(AppTheme.stroke)
                .alignmentGuide(.listRowSeparatorLeading) { _ in 14 }
                .alignmentGuide(.listRowSeparatorTrailing) { dimensions in dimensions.width - 4 }
                .swipeActions(edge: .trailing, allowsFullSwipe: false) {
                    Button(role: .destructive) { deleting = session } label: {
                        Label("Delete", image: Reicon.trash.rawValue)
                    }
                    .tint(AppTheme.deletions)
                    Button { startRename(session) } label: {
                        Label("Rename", image: Reicon.edit.rawValue)
                    }
                    .tint(AppTheme.surfaceRaised)
                }
                .contextMenu {
                    Button { startRename(session) } label: { Label("Rename", systemImage: "pencil") }
                    Button(role: .destructive) { deleting = session } label: { Label("Delete", systemImage: "trash") }
                }
                .transition(.opacity)
            }
        }
        .listStyle(.plain)
        .scrollContentBackground(.hidden)
        .scrollIndicators(.hidden)
        .scrollDismissesKeyboard(.interactively)
        .contentMargins(.top, 30, for: .scrollContent)
        .animation(.easeInOut(duration: 0.25), value: sessions.map(\.id))
        .refreshable { await app.refresh() }
        .safeAreaInset(edge: .bottom, spacing: 0) {
            SearchBar(query: $query, placeholder: "Search sessions") {
                FilterMenu(filter: $filter, providers: providers)
            }
            .padding(.bottom, 8)
        }
        .alert("Rename session", isPresented: Binding(get: { renaming != nil }, set: { if !$0 { renaming = nil } })) {
            TextField("Session name", text: $renameText)
            Button("Cancel", role: .cancel) { renaming = nil }
            Button("Rename") {
                if let session = renaming {
                    Task { await app.rename(session.id, to: renameText) }
                }
                renaming = nil
            }
        }
        .confirmationDialog(
            deleting.map { "Delete “\($0.displayTitle)”?" } ?? "",
            isPresented: Binding(get: { deleting != nil }, set: { if !$0 { deleting = nil } }),
            titleVisibility: .visible
        ) {
            Button("Delete", role: .destructive) {
                if let session = deleting {
                    Task { await app.deleteSession(session.id) }
                }
                deleting = nil
            }
        } message: {
            Text("This can’t be undone.")
        }
    }

    /// The agents to filter by: those in the list, in the order first seen.
    private var providers: [ProviderKind] {
        var seen: [ProviderKind] = []
        for session in listed where !seen.contains(session.provider) { seen.append(session.provider) }
        if let picked = filter.provider, !seen.contains(picked) { seen.append(picked) }
        return seen
    }

    /// Whether a session works in the picked worktree; any does without one.
    private func inWorktree(_ session: AgentSession) -> Bool {
        guard let worktree else { return true }
        if worktree.isMain { return session.worktreePath == nil }
        return session.worktreePath.map { $0.trimmingTrailingSlashes == worktree.path.trimmingTrailingSlashes } ?? false
    }

    private func startRename(_ session: AgentSession) {
        renameText = session.displayTitle
        renaming = session
    }
}

/// A session as the desktop's session card lays it out: the title with the
/// time or its status on the right, then the branch with the model and its
/// provider's mark.
struct SessionCard: View {
    let session: AgentSession
    let model: String?
    var paused = false

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            HStack(spacing: 8) {
                Text(session.displayTitle)
                    .font(AppTheme.font(size: 16, weight: .semibold))
                    .foregroundStyle(AppTheme.content)
                    .lineLimit(1)
                    .frame(maxWidth: .infinity, alignment: .leading)
                if session.status.isBusy {
                    HStack(spacing: 4) {
                        TerminalSpinner(paused: paused)
                        Text("Working...")
                    }
                    .font(AppTheme.font(size: 13))
                    .foregroundStyle(AppTheme.accent)
                } else {
                    Text(relativeTime(session.lastReplyAt ?? session.updatedAt))
                        .font(AppTheme.font(size: 13).monospacedDigit())
                        .foregroundStyle(AppTheme.faint)
                }
            }
            HStack(spacing: 8) {
                HStack(spacing: 5) {
                    IconView(.branch, size: 13).foregroundStyle(AppTheme.faint)
                    Text(session.branch ?? "No branch")
                        .foregroundStyle(session.branch == nil ? AppTheme.faint : AppTheme.muted)
                        .lineLimit(1)
                }
                .frame(maxWidth: .infinity, alignment: .leading)
                HStack(spacing: 6) {
                    if let model {
                        Text(model).foregroundStyle(AppTheme.muted).lineLimit(1)
                    }
                    ProviderLogo(provider: session.provider, size: 14).foregroundStyle(AppTheme.muted)
                }
            }
            .font(AppTheme.font(size: 13))
        }
        .padding(.horizontal, 14)
        .padding(.vertical, 14)
        .contentShape(Rectangle())
        .accessibilityElement(children: .combine)
        .accessibilityLabel(session.status.isBusy ? "\(session.displayTitle), working" : session.displayTitle)
        .accessibilityHint("Swipe left or long press to rename or delete")
    }
}

/// The Sessions page's search field where the composer rests on the first
/// page. While it has focus, a round button beside it clears the search and
/// lets go of the keyboard.
struct SearchBar<Accessory: View>: View {
    @Binding var query: String
    let placeholder: String
    @ViewBuilder var accessory: () -> Accessory

    @FocusState private var focused: Bool

    static var height: CGFloat { 50 }

    var body: some View {
        HStack(spacing: 10) {
            HStack(spacing: 10) {
                IconView(.search, size: 20).foregroundStyle(AppTheme.muted)
                TextField(placeholder, text: $query)
                    .font(AppTheme.font(size: 17))
                    .focused($focused)
                    .submitLabel(.search)
                    .autocorrectionDisabled()
                    .textInputAutocapitalization(.never)
                    .tint(AppTheme.accent)
                if !query.isEmpty {
                    Button { query = "" } label: {
                        IconView(.close, size: 12)
                            .foregroundStyle(AppTheme.inverse)
                            .frame(width: 20, height: 20)
                            .background(AppTheme.muted, in: Circle())
                    }
                    .buttonStyle(.plain)
                    .accessibilityLabel("Clear search")
                }
            }
            .padding(.horizontal, 18)
            .frame(height: Self.height)
            .glass(in: Capsule())
            if focused {
                CircleButton(icon: .close, label: "Close search", size: Self.height) {
                    query = ""
                    focused = false
                }
                .transition(.scale.combined(with: .opacity))
            } else {
                accessory()
            }
        }
        .padding(.horizontal, 20)
        .animation(.snappy, value: focused)
    }
}

/// The round button beside the session search: show only those working now,
/// or one agent's. Lit while a filter is on.
private struct FilterMenu: View {
    @Binding var filter: SessionFilter
    let providers: [ProviderKind]

    var body: some View {
        let on = filter != .none
        Menu {
            Picker("Status", selection: $filter.working) {
                Label("All sessions", systemImage: "tray.full").tag(false)
                Label("Working now", systemImage: "bolt").tag(true)
            }
            .pickerStyle(.inline)
            if providers.count > 1 || filter.provider != nil {
                Picker("Agent", selection: $filter.provider) {
                    Text("Any agent").tag(ProviderKind?.none)
                    ForEach(providers) { provider in
                        Text(provider.name).tag(ProviderKind?.some(provider))
                    }
                }
                .pickerStyle(.inline)
            }
        } label: {
            IconView(.filter, size: 20)
                .foregroundStyle(on ? AppTheme.accent : AppTheme.content)
                .frame(width: SearchBar<EmptyView>.height, height: SearchBar<EmptyView>.height)
                .glass(in: Circle(), interactive: true)
        }
        .accessibilityLabel(on ? "Filter sessions, on" : "Filter sessions")
    }
}

private extension String {
    var trimmingTrailingSlashes: String {
        var value = self
        while value.hasSuffix("/") { value.removeLast() }
        return value
    }
}
