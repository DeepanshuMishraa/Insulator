import SwiftUI

enum TranscriptScrollPosition {
    static func distanceFromBottom(contentHeight: CGFloat, visibleMaxY: CGFloat) -> CGFloat {
        max(0, contentHeight - visibleMaxY)
    }
}

enum TranscriptScrollOwnership: Equatable {
    case app
    case user

    static func afterUserInteraction(isAtBottom: Bool) -> Self {
        isAtBottom ? .app : .user
    }
}

/// One of a project's sessions, pushed over the project: its chat under the
/// native header, its title beside the back button, its changes and actions
/// on the right, and its own composer.
struct SessionScreen: View {
    @Bindable var app: AppModel
    let sessionID: UUID

    @Environment(Router.self) private var router
    @Environment(\.dismiss) private var dismiss
    @State private var status: WorkingTreeStatus?
    @State private var renaming = false
    @State private var renameText = ""
    @State private var deleting = false
    @State private var copied = false

    private var session: AgentSession? {
        app.selectedSession?.id == sessionID ? app.selectedSession : app.session(withID: sessionID)
    }

    var body: some View {
        Group {
            if let session, let project = app.project(for: session) {
                content(session, project: project)
            } else {
                ProgressView().frame(maxWidth: .infinity, maxHeight: .infinity)
            }
        }
        .background(AppTheme.background.ignoresSafeArea())
        .task {
            app.viewingSessionID = sessionID
            app.finished.removeAll { $0.sessionID == sessionID }
            if app.selectedSession?.id != sessionID, let session = app.session(withID: sessionID) {
                await app.open(session)
            }
        }
        .task(id: "\(session?.worktreePath ?? "")-\(session?.status.isBusy ?? false)-\(app.isConnected)") {
            guard app.isConnected, let session, let cwd = app.workingDirectory(for: session) else { return }
            status = try? await app.inspectWorkingTree(cwd)
        }
        .onDisappear {
            if app.viewingSessionID == sessionID { app.viewingSessionID = nil }
        }
    }

    private func content(_ session: AgentSession, project: Project) -> some View {
        let branch = status?.branch ?? session.branch ?? ""
        let changed = status?.files ?? []
        return Transcript(app: app, session: session)
            .environment(\.fileOpener, app.workingDirectory(for: session).map { root in
                FileOpener { path in router.sheet = .file(root: root, path: path) }
            })
            .safeAreaInset(edge: .bottom, spacing: 0) {
                Composer(
                    app: app,
                    context: .session(session, project: project, branch: branch, changed: changed),
                    busy: session.status.isBusy,
                    onStop: { try await app.stop(session.id) },
                    onSend: { text, images, files in
                        if session.status.isBusy, app.canSteer(session.id) {
                            await app.steer(text, images: images, files: files)
                        } else {
                            await app.send(text, images: images, files: files)
                        }
                    }
                )
                .padding(.bottom, 8)
            }
            .navigationBarTitleDisplayMode(.inline)
            .toolbarBackground(.hidden, for: .navigationBar)
            .modifier(SessionTitle(title: session.displayTitle, subtitle: [project.name, branch].filter { !$0.isEmpty }.joined(separator: " · ")))
            .toolbar {
                ToolbarItem(placement: .topBarTrailing) {
                    Button {
                        guard let cwd = app.workingDirectory(for: session) else { return }
                        router.sheet = .changes(ChangesTarget(cwd: cwd, provider: session.provider))
                    } label: {
                        Image(systemName: "plus.forwardslash.minus")
                            .overlay(alignment: .topTrailing) {
                                if !changed.isEmpty {
                                    Text("\(changed.count)")
                                        .font(AppTheme.font(size: 11, weight: .semibold).monospacedDigit())
                                        .foregroundStyle(.white)
                                        .padding(.horizontal, 4)
                                        .frame(minWidth: 17, minHeight: 17)
                                        .background(Color.red, in: Capsule())
                                        .offset(x: 10, y: -9)
                                }
                            }
                    }
                    .accessibilityLabel("Changes, \(changed.count) \(changed.count == 1 ? "file" : "files")")
                }
                ToolbarItem(placement: .topBarTrailing) {
                    Menu {
                        Button {
                            renameText = session.displayTitle
                            renaming = true
                        } label: { Label("Rename", systemImage: "pencil") }
                        Button {
                            UIPasteboard.general.string = session.id.uuidString.lowercased()
                            Haptics.success()
                        } label: { Label("Copy Session ID", systemImage: "doc.on.doc") }
                        Button {
                            router.sheet = .settings(projectID: project.id)
                        } label: { Label("Settings", systemImage: "gearshape") }
                        Section {
                            Button(role: .destructive) { deleting = true } label: { Label("Delete", systemImage: "trash") }
                        }
                    } label: {
                        Image(systemName: "ellipsis")
                    }
                    .accessibilityLabel("Session actions")
                }
            }
            .alert("Rename session", isPresented: $renaming) {
                TextField("Session name", text: $renameText)
                Button("Cancel", role: .cancel) {}
                Button("Rename") { Task { await app.rename(session.id, to: renameText) } }
            }
            .confirmationDialog("Delete “\(session.displayTitle)”?", isPresented: $deleting, titleVisibility: .visible) {
                Button("Delete", role: .destructive) {
                    dismiss()
                    Task { await app.deleteSession(session.id) }
                }
            } message: {
                Text("This can’t be undone.")
            }
            .sheet(item: Binding(
                get: { app.pendingPermission?.sessionID == session.id ? app.pendingPermission : nil },
                set: { if $0 == nil { app.pendingPermission = nil } }
            )) { permission in
                PermissionSheet(app: app, permission: permission)
            }
    }
}

/// A session's title on the left of the header, beside the back button:
/// its title over its project and branch.
private struct SessionTitle: ViewModifier {
    let title: String
    let subtitle: String

    func body(content: Content) -> some View {
        if #available(iOS 26.0, *) {
            content.toolbar {
                ToolbarItem(placement: .topBarLeading) { label }
                    .sharedBackgroundVisibility(.hidden)
            }
        } else {
            content.toolbar {
                ToolbarItem(placement: .topBarLeading) { label }
            }
        }
    }

    private var label: some View {
        VStack(alignment: .leading, spacing: 0) {
            Text(title)
                .font(AppTheme.font(size: 17, weight: .bold))
                .foregroundStyle(AppTheme.content)
            Text(subtitle)
                .font(AppTheme.font(size: 13))
                .foregroundStyle(AppTheme.muted)
        }
        .lineLimit(1)
        .frame(maxWidth: 220, alignment: .leading)
    }
}

/// An approval the agent is waiting on, answered from the phone.
private struct PermissionSheet: View {
    @Bindable var app: AppModel
    let permission: PendingPermission

    var body: some View {
        VStack(alignment: .leading, spacing: 16) {
            HStack(spacing: 10) {
                IconView(.lock, size: 20).foregroundStyle(AppTheme.caution)
                Text("Needs your approval")
                    .font(AppTheme.font(size: 13, weight: .semibold))
                    .foregroundStyle(AppTheme.caution)
            }
            Text(permission.title)
                .font(AppTheme.font(size: 17, weight: .semibold))
                .foregroundStyle(AppTheme.content)
            if let detail = permission.detail, !detail.isEmpty {
                ScrollView {
                    Text(detail)
                        .font(AppTheme.code(size: 13))
                        .foregroundStyle(AppTheme.content.opacity(0.85))
                        .frame(maxWidth: .infinity, alignment: .leading)
                        .padding(12)
                }
                .frame(maxHeight: 220)
                .background(AppTheme.content.opacity(0.05), in: RoundedRectangle(cornerRadius: 12, style: .continuous))
            }
            VStack(spacing: 10) {
                ForEach(permission.options) { option in
                    Button {
                        Haptics.selection()
                        Task { await app.respond(to: permission, option: option) }
                    } label: {
                        Text(option.label)
                            .font(AppTheme.font(size: 16, weight: .semibold))
                            .foregroundStyle(option.allow ? AppTheme.inverse : AppTheme.content)
                            .frame(maxWidth: .infinity, minHeight: 50)
                            .background(option.allow ? AppTheme.inverseSurface : AppTheme.fill, in: Capsule())
                    }
                    .buttonStyle(PressedOpacityStyle())
                }
            }
        }
        .padding(20)
        .presentationDetents([.medium, .large])
        .presentationBackground(AppTheme.sheet)
        .interactiveDismissDisabled()
    }
}

/// A session's turns, scrolled to the latest, following a streaming reply
/// while the reader is at the end, with a button back there when not.
struct Transcript: View {
    @Bindable var app: AppModel
    let session: AgentSession

    @State private var position = ScrollPosition(edge: .bottom)
    @State private var atBottom = true

    var body: some View {
        let turns = TranscriptTurns.build(session, modelName: app.modelName(provider: session.provider, modelID: session.model))
        ScrollView {
            LazyVStack(alignment: .leading, spacing: 0) {
                if turns.isEmpty {
                    TranscriptLoading(empty: session.messages.isEmpty && !session.status.isBusy)
                }
                ForEach(turns) { turn in
                    TurnView(turn: turn)
                }
            }
            .padding(.top, 12)
            .padding(.bottom, 16)
        }
        .scrollPosition($position)
        .defaultScrollAnchor(.bottom)
        .scrollDismissesKeyboard(.interactively)
        .onScrollGeometryChange(for: Bool.self) { geometry in
            TranscriptScrollPosition.distanceFromBottom(
                contentHeight: geometry.contentSize.height + geometry.contentInsets.bottom,
                visibleMaxY: geometry.visibleRect.maxY + geometry.contentInsets.bottom
            ) < 40
        } action: { _, isAtBottom in
            atBottom = isAtBottom
        }
        .onChange(of: app.transcriptRevision) {
            if atBottom { position.scrollTo(edge: .bottom) }
        }
        .overlay(alignment: .bottom) {
            if !atBottom, !turns.isEmpty {
                CircleButton(icon: .chevronDown, label: "Scroll to bottom", size: 36) {
                    withAnimation(.easeOut(duration: 0.3)) { position.scrollTo(edge: .bottom) }
                }
                .padding(.bottom, 10)
                .transition(.scale(scale: 0.8).combined(with: .opacity))
            }
        }
        .animation(.easeOut(duration: 0.2), value: atBottom)
    }
}

private struct TranscriptLoading: View {
    let empty: Bool

    var body: some View {
        VStack {
            if empty {
                Text("No messages yet.")
                    .font(AppTheme.font(size: 15))
                    .foregroundStyle(AppTheme.muted)
            } else {
                ProgressView()
            }
        }
        .frame(maxWidth: .infinity)
        .padding(.top, 120)
    }
}
