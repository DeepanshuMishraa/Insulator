import SwiftUI

/// A project: a new session first, then the project's sessions to its right.
/// Swiping left slides the sessions in and the composer out with the new
/// session, the header switch following the finger. The mascot on the left
/// switches project; the cog on the right opens settings.
struct ProjectScreen: View {
    @Bindable var app: AppModel
    let project: Project

    @Environment(Router.self) private var router
    @State private var page = 0
    @State private var drag: CGFloat = 0
    @State private var width: CGFloat = 1
    @State private var workspace: WorkspaceChoice = .current
    @State private var status: WorkingTreeStatus?
    @State private var statusError: String?
    @State private var worktrees: [WorktreeEntry] = []
    @State private var showsWorkspace = false
    @State private var emptyOpacity: Double = 1

    private static let pages = 2

    /// The page index as a continuous value: 0 on the first page, 1 on the second.
    private var progress: CGFloat {
        min(CGFloat(Self.pages - 1), max(0, CGFloat(page) - drag / width))
    }

    private var cwd: String? {
        switch workspace {
        case .current: project.path
        case .newWorktree: nil
        case .existing(let worktree): worktree.path
        }
    }

    private var branch: String {
        switch workspace {
        case .newWorktree: "new worktree"
        case .existing(let worktree): status?.branch ?? worktree.name
        case .current: status?.branch ?? ""
        }
    }

    var body: some View {
        GeometryReader { proxy in
            ZStack(alignment: .top) {
                HStack(spacing: 0) {
                    newSessionPage
                        .frame(width: proxy.size.width)
                    SessionsPage(
                        app: app,
                        project: project,
                        worktree: { if case .existing(let tree) = workspace { tree } else { nil } }(),
                        active: page == 1
                    )
                    .frame(width: proxy.size.width)
                }
                .offset(x: -progress * proxy.size.width)
                .frame(width: proxy.size.width, alignment: .leading)

                GradualBlur(edge: .top, height: 56)
                branchLine
                    .padding(.top, 2)
            }
            .gesture(PagerPan(page: page, pageCount: Self.pages, onChanged: { translation in
                drag = translation
            }, onEnded: { translation, velocity in
                let decisive = (abs(translation) > 5 && abs(velocity) > 500) || abs(translation) > 60
                let direction = (velocity == 0 ? translation : velocity) < 0 ? 1 : -1
                let next = decisive ? min(Self.pages - 1, max(0, page + direction)) : page
                if next != page { Haptics.selection() }
                withAnimation(.spring(response: 0.35, dampingFraction: 1)) {
                    page = next
                    drag = 0
                }
            }))
            .onAppear { width = max(proxy.size.width, 1) }
            .onChange(of: proxy.size.width) { _, newWidth in width = max(newWidth, 1) }
        }
        .background(AppTheme.background.ignoresSafeArea())
        .navigationBarTitleDisplayMode(.inline)
        .toolbarBackground(.hidden, for: .navigationBar)
        .toolbar {
            ToolbarItem(placement: .topBarLeading) {
                Button { router.sheet = .projects } label: {
                    ProjectMascot(path: project.path, size: 20, active: app.sessions(in: project.id).contains { $0.status.isBusy })
                        .frame(width: 36, height: 36)
                }
                .accessibilityLabel("Switch project, \(project.name)")
            }
            ToolbarItem(placement: .principal) {
                SegmentedSwitch(
                    segments: [.init(label: "New", icon: .compose), .init(label: "Sessions", icon: .list)],
                    progress: progress
                ) { index in
                    withAnimation(.spring(response: 0.35, dampingFraction: 1)) { page = index }
                }
            }
            ToolbarItem(placement: .topBarTrailing) {
                Button { router.sheet = .settings(projectID: project.id) } label: {
                    IconView(.settings, size: 22)
                        .foregroundStyle(AppTheme.content)
                        .frame(width: 36, height: 36)
                }
                .accessibilityLabel("Settings")
            }
        }
        .onChange(of: page) { _, newPage in
            if newPage != 0 { UIApplication.shared.sendAction(#selector(UIResponder.resignFirstResponder), to: nil, from: nil, for: nil) }
        }
        .task(id: "\(cwd ?? "fresh")-\(app.isConnected)") { await loadStatus() }
        .task(id: app.isConnected) {
            guard app.isConnected else { return }
            worktrees = (try? await app.worktrees(of: project)) ?? []
        }
        .onAppear {
            emptyOpacity = 1
            Preferences.shared.currentProjectID = project.id
        }
        .sheet(isPresented: $showsWorkspace) {
            WorkspaceSheet(
                app: app,
                project: project,
                value: $workspace,
                worktrees: worktrees,
                status: status
            )
        }
    }

    // MARK: - Pages

    private var newSessionPage: some View {
        EmptySession(project: project)
            .opacity(emptyOpacity)
            .safeAreaInset(edge: .bottom, spacing: 0) {
                VStack(spacing: 10) {
                    LiveSessions(app: app, project: project, inView: page == 0 && router.path.isEmpty)
                    Composer(
                        app: app,
                        context: .newSession(project: project, branch: branch, workspace: $workspace),
                        onSend: start
                    )
                }
                .padding(.bottom, 8)
            }
    }

    @ViewBuilder
    private var branchLine: some View {
        if !app.isConnected {
            HStack(spacing: 8) {
                PulsingDot()
                Text("Reconnecting to desktop")
                    .font(.system(size: 13))
                    .foregroundStyle(AppTheme.muted)
            }
            .frame(height: 22)
        } else {
            Button { showsWorkspace = true } label: {
                HStack(spacing: 6) {
                    BranchLine(
                        current: workspace == .current,
                        branch: branch,
                        files: status?.files ?? [],
                        loading: status == nil && statusError == nil && cwd != nil
                    )
                    IconView(.chevronDown, size: 12)
                        .foregroundStyle(AppTheme.muted)
                }
                .frame(height: 22)
                .padding(.horizontal, 24)
                .contentShape(Rectangle())
            }
            .buttonStyle(PressedOpacityStyle())
            .accessibilityLabel("Workspace: \(branch)")
        }
    }

    // MARK: - Actions

    private func loadStatus() async {
        guard app.isConnected, let cwd else {
            status = nil
            return
        }
        do {
            status = try await app.inspectWorkingTree(cwd)
            statusError = nil
        } catch {
            statusError = error.localizedDescription
        }
    }

    /// Starts a session, waiting for the empty screen to fade when it will
    /// open, and back for another otherwise.
    private func start(_ text: String, images: [UIImage], files: [PickedFile]) async throws {
        let opens = Preferences.shared.openAfterStarting
        if opens {
            withAnimation(.easeOut(duration: 0.32)) { emptyOpacity = 0 }
        }
        do {
            let session = try await app.startSession(in: project, text: text, images: images, files: files, workspace: workspace)
            if opens {
                router.open(session.id)
            }
            if workspace == .newWorktree { workspace = .current }
        } catch {
            withAnimation(.easeOut(duration: 0.2)) { emptyOpacity = 1 }
            throw error
        }
    }
}

/// The working copy's branch and how much has changed in it.
struct BranchLine: View {
    let current: Bool
    let branch: String
    let files: [FileChange]
    var loading = false

    var body: some View {
        let additions = files.reduce(0) { $0 + $1.additions }
        let deletions = files.reduce(0) { $0 + $1.deletions }
        HStack(spacing: 6) {
            IconView(current ? .folder : .branch, size: 13)
                .foregroundStyle(AppTheme.muted)
            Text(branch.isEmpty ? "No branch" : branch)
                .font(.system(size: 13, weight: .medium))
                .foregroundStyle(AppTheme.content)
                .lineLimit(1)
                .truncationMode(.middle)
            if loading {
                ProgressView().controlSize(.mini)
            } else if !files.isEmpty {
                Text("+\(formatCount(additions))")
                    .foregroundStyle(DiffTheme.addText)
                Text("-\(formatCount(deletions))")
                    .foregroundStyle(DiffTheme.deleteText)
            }
        }
        .font(.system(size: 13, weight: .medium).monospacedDigit())
    }
}

/// A new session's empty screen: a dimmed Insulator mark above the prompt and
/// the project's name under it, over the photo set for it on this phone, or a
/// still frame of the ASCII field where none is.
struct EmptySession: View {
    let project: Project
    @State private var backgrounds = EmptyBackgrounds.shared
    @State private var keyboard = KeyboardState.shared

    static let headline = "What should we work on?"

    var body: some View {
        ZStack {
            if let image = backgrounds.image(for: project.id) {
                Image(uiImage: image)
                    .resizable()
                    .scaledToFill()
                    .opacity(backgrounds.opacity(for: project.id))
                    .ignoresSafeArea()
                    .transition(.opacity)
            } else {
                AsciiField(still: true)
                    .ignoresSafeArea()
            }
            ScrollView {
                VStack(spacing: 18) {
                    InsulatorMark()
                        .fill(AppTheme.mark)
                        .frame(width: 84, height: 84)
                        .opacity(0.3)
                    Text(Self.headline)
                        .font(.system(size: 22, weight: .semibold))
                        .foregroundStyle(AppTheme.content)
                        .multilineTextAlignment(.center)
                    Text(project.name)
                        .font(.system(size: 15, weight: .medium))
                        .foregroundStyle(AppTheme.muted)
                        .lineLimit(1)
                        .padding(.top, -8)
                }
                .padding(.horizontal, 32)
                .padding(.bottom, 110)
                .opacity(keyboard.visible ? 0 : 1)
                .animation(.easeOut(duration: 0.25), value: keyboard.visible)
                .containerRelativeFrame(.vertical)
            }
            .scrollDismissesKeyboard(.interactively)
            .scrollIndicators(.hidden)
        }
        .clipped()
    }
}

/// Where a new session works: the project's checkout, a new worktree, or one
/// it already has, with the way to its changes.
struct WorkspaceSheet: View {
    @Bindable var app: AppModel
    let project: Project
    @Binding var value: WorkspaceChoice
    let worktrees: [WorktreeEntry]
    let status: WorkingTreeStatus?

    @Environment(\.dismiss) private var dismiss
    @Environment(Router.self) private var router

    private var linked: [WorktreeEntry] { worktrees.filter { !$0.isMain } }

    var body: some View {
        NavigationStack {
            List {
                Section("Start the session in") {
                    choice(.folder, "Current checkout", hint: worktrees.first(where: \.isMain)?.name ?? "The project's own folder", selected: value == .current) {
                        pick(.current)
                    }
                    choice(.branch, "New worktree", hint: "A fresh branch, in a folder of its own", selected: value == .newWorktree) {
                        pick(.newWorktree)
                    }
                    if !linked.isEmpty {
                        NavigationLink {
                            List(linked) { tree in
                                choice(.branch, tree.name, hint: tree.path, selected: value == .existing(tree)) {
                                    pick(.existing(tree))
                                }
                            }
                            .scrollContentBackground(.hidden)
                            .navigationTitle("Worktrees")
                            .navigationBarTitleDisplayMode(.inline)
                        } label: {
                            row(.folder, "Existing worktree", hint: {
                                if case .existing(let tree) = value { return tree.name }
                                return "\(linked.count) \(linked.count == 1 ? "worktree" : "worktrees")"
                            }(), selected: false)
                        }
                    }
                }
                Section {
                    Button {
                        let target = value
                        dismiss()
                        guard target != .newWorktree else { return }
                        let cwd: String = if case .existing(let tree) = target { tree.path } else { project.path }
                        router.sheet = .changes(ChangesTarget(cwd: cwd, provider: app.newSessionChoice(for: project.id)?.provider))
                    } label: {
                        HStack(spacing: 14) {
                            IconView(.changes, size: 18).foregroundStyle(AppTheme.content)
                            VStack(alignment: .leading, spacing: 2) {
                                Text("View changes").foregroundStyle(AppTheme.content)
                                let count = status?.files.count ?? 0
                                Text("\(formatCount(count)) \(count == 1 ? "file" : "files") changed")
                                    .font(.footnote)
                                    .foregroundStyle(AppTheme.muted)
                            }
                            Spacer()
                            let files = status?.files ?? []
                            HStack(spacing: 6) {
                                Text("+\(formatCount(files.reduce(0) { $0 + $1.additions }))").foregroundStyle(DiffTheme.addText)
                                Text("-\(formatCount(files.reduce(0) { $0 + $1.deletions }))").foregroundStyle(DiffTheme.deleteText)
                            }
                            .font(.footnote.monospacedDigit())
                        }
                    }
                    .disabled(value == .newWorktree)
                } footer: {
                    if value == .newWorktree {
                        Text("Changes are available after the worktree is created.")
                    }
                }
            }
            .scrollContentBackground(.hidden)
            .navigationTitle("Workspace")
            .navigationBarTitleDisplayMode(.inline)
        }
        .presentationDetents([.medium, .large])
        .presentationBackground(.regularMaterial)
    }

    private func pick(_ next: WorkspaceChoice) {
        Haptics.selection()
        value = next
        dismiss()
    }

    private func choice(_ icon: Icon, _ title: String, hint: String, selected: Bool, action: @escaping () -> Void) -> some View {
        Button(action: action) { row(icon, title, hint: hint, selected: selected) }
    }

    private func row(_ icon: Icon, _ title: String, hint: String, selected: Bool) -> some View {
        HStack(spacing: 14) {
            IconView(icon, size: 18).foregroundStyle(AppTheme.content)
            VStack(alignment: .leading, spacing: 2) {
                Text(title).foregroundStyle(AppTheme.content)
                Text(hint)
                    .font(.footnote)
                    .foregroundStyle(AppTheme.muted)
                    .lineLimit(1)
                    .truncationMode(.middle)
            }
            Spacer()
            if selected {
                IconView(.check, size: 18).foregroundStyle(AppTheme.accent)
            }
        }
    }
}
