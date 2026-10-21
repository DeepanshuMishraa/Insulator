import SwiftUI

/// A working copy's uncommitted changes, after the desktop's changes panel:
/// each file with its status, staged ones gathered above the rest. Tapping a
/// file opens its diff; its circle stages or unstages it. A glass button at
/// the foot stages everything, or with something staged opens the commit sheet.
struct ChangesScreen: View {
    @Bindable var app: AppModel
    let target: ChangesTarget

    @Environment(\.dismiss) private var dismiss
    @State private var status: WorkingTreeStatus?
    @State private var error: String?
    @State private var diffs: [String: [DiffLine]] = [:]
    @State private var wanted: [String: Bool] = [:]
    @State private var collapsed: Set<String> = []
    @State private var stagingAll = false
    @State private var committing = false
    @State private var failure: (title: String, message: String)?

    private var files: [FileChange] {
        (status?.files ?? []).map { file in wanted[file.path].map(file.staging) ?? file }
    }

    var body: some View {
        let staged = files.filter(\.isFullyStaged)
        let unstaged = files.filter { !$0.isFullyStaged }
        NavigationStack {
            ScrollView {
                VStack(spacing: 12) {
                    if files.isEmpty {
                        section("Changes", count: 0, loading: status == nil && error == nil, key: nil, action: nil) { EmptyView() }
                        if let error {
                            Text(error)
                                .font(AppTheme.font(size: 15))
                                .foregroundStyle(AppTheme.muted)
                                .padding(.horizontal, 16)
                                .frame(maxWidth: .infinity, alignment: .leading)
                        }
                    }
                    if !staged.isEmpty {
                        section("Staged Changes", count: staged.count, loading: false, key: "staged",
                                action: (.minus, "Unstage All", { Task { await stageAll(false, staged) } })) {
                            rows(staged)
                        }
                    }
                    if !unstaged.isEmpty {
                        section("Changes", count: unstaged.count, loading: false, key: "changes",
                                action: (.add, "Stage All", { Task { await stageAll(true, unstaged) } })) {
                            rows(unstaged)
                        }
                    }
                }
                .padding(.horizontal, 10)
                .padding(.top, 4)
                .padding(.bottom, 90)
            }
            .scrollIndicators(.hidden)
            .background(AppTheme.background.ignoresSafeArea())
            .refreshable { await reload() }
            .safeAreaInset(edge: .bottom, spacing: 0) { bar(staged: staged) }
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .topBarLeading) {
                    Button { dismiss() } label: { IconView(.close, size: 18) }
                        .accessibilityLabel("Close")
                }
                ToolbarItem(placement: .principal) {
                    VStack(spacing: 1) {
                        Text("Changes").font(AppTheme.font(size: 16, weight: .semibold)).foregroundStyle(AppTheme.content)
                        Text(status?.branch ?? (target.cwd as NSString).lastPathComponent)
                            .font(AppTheme.font(size: 12))
                            .foregroundStyle(AppTheme.muted)
                            .lineLimit(1)
                    }
                }
            }
            .navigationDestination(for: FileChange.self) { file in
                DiffScreen(file: file, lines: diffs[file.path])
            }
            .sheet(isPresented: $committing) {
                CommitSheet(
                    app: app,
                    target: target,
                    status: status,
                    staged: staged,
                    changed: files.count
                ) { await reload() }
            }
            .alert(failure?.title ?? "", isPresented: Binding(get: { failure != nil }, set: { if !$0 { failure = nil } })) {
                Button("OK", role: .cancel) { failure = nil }
            } message: {
                Text(failure?.message ?? "")
            }
        }
        .task { await reload() }
    }

    // MARK: - Sections and rows

    private func section(
        _ title: String,
        count: Int,
        loading: Bool,
        key: String?,
        action: (icon: Icon, label: String, run: () -> Void)?,
        @ViewBuilder content: () -> some View
    ) -> some View {
        let open = key.map { !collapsed.contains($0) } ?? false
        return VStack(spacing: 0) {
            Button {
                guard let key else { return }
                withAnimation(.easeOut(duration: 0.18)) {
                    if collapsed.contains(key) { collapsed.remove(key) } else { collapsed.insert(key) }
                }
            } label: {
                HStack(spacing: 4) {
                    HStack(spacing: 8) {
                        Text(title.uppercased())
                            .font(AppTheme.font(size: 13, weight: .semibold))
                            .tracking(0.6)
                            .foregroundStyle(AppTheme.muted)
                            .lineLimit(1)
                        if loading {
                            ProgressView().controlSize(.small)
                        } else {
                            Text(formatCount(count))
                                .font(AppTheme.font(size: 12, weight: .semibold).monospacedDigit())
                                .foregroundStyle(count == 0 ? AppTheme.muted : .white)
                                .padding(.horizontal, 7)
                                .frame(minWidth: 24, minHeight: 20)
                                .background(count == 0 ? AppTheme.surfaceRaised : AppTheme.accent, in: Capsule())
                        }
                    }
                    .frame(maxWidth: .infinity, alignment: .leading)
                    if let action {
                        Button(action: action.run) {
                            IconView(action.icon, size: 18)
                                .foregroundStyle(AppTheme.muted)
                                .frame(width: 36, height: 36)
                        }
                        .buttonStyle(PressedOpacityStyle(opacity: 0.5))
                        .accessibilityLabel(action.label)
                    }
                    IconView(open ? .chevronDown : .chevronRight, size: 16)
                        .foregroundStyle(key == nil ? AppTheme.faint : AppTheme.muted)
                        .frame(width: 22, height: 22)
                }
                .padding(.horizontal, 16)
                .frame(height: 48)
                .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .disabled(key == nil)
            if open {
                Rectangle().fill(AppTheme.stroke).frame(height: 0.5)
                content()
                    .transition(.opacity)
            }
        }
        .background(AppTheme.content.opacity(0.05), in: RoundedRectangle(cornerRadius: 26, style: .continuous))
        .clipShape(RoundedRectangle(cornerRadius: 26, style: .continuous))
    }

    private func rows(_ section: [FileChange]) -> some View {
        ForEach(Array(section.enumerated()), id: \.element.id) { index, file in
            if index > 0 {
                Rectangle().fill(AppTheme.stroke).frame(height: 0.5)
                    .padding(.leading, 16 + 22 + 10)
                    .padding(.trailing, 16)
                    .padding(.vertical, 2)
            }
            FileChangeRow(file: file, openable: !(diffs[file.path]?.isEmpty ?? true)) {
                Task { await stage([file.path], to: !file.isFullyStaged) }
            }
        }
    }

    @ViewBuilder
    private func bar(staged: [FileChange]) -> some View {
        if let action = barAction(staged: staged.count) {
            Button {
                switch action.kind {
                case .stage: Task { await stageAll(true, files.filter { !$0.isFullyStaged }) }
                default: committing = true
                }
            } label: {
                HStack(spacing: 8) {
                    if stagingAll {
                        ProgressView().tint(AppTheme.content)
                    } else {
                        IconView(action.icon, size: 18).foregroundStyle(AppTheme.content)
                    }
                    Text(action.label)
                        .font(AppTheme.font(size: 17, weight: .semibold))
                        .foregroundStyle(AppTheme.content)
                }
                .padding(.trailing, 12)
                .frame(maxWidth: .infinity, minHeight: 50)
                .contentShape(Capsule())
            }
            .buttonStyle(PressedScaleStyle())
            .glass(in: Capsule(), interactive: true)
            .disabled(stagingAll)
            .padding(.horizontal, 20)
            .padding(.bottom, 8)
        }
    }

    private enum BarKind { case stage, commit, branch }

    /// Staging everything while nothing is, then committing what is, else the
    /// branch's next step. None with nothing to do.
    private func barAction(staged: Int) -> (kind: BarKind, icon: Icon, label: String)? {
        if staged > 0 { return (.commit, .check, "Commit \(staged) \(staged == 1 ? "file" : "files")") }
        if !files.isEmpty { return (.stage, .add, "Stage All") }
        guard let status, status.hasRemote else { return nil }
        if status.upstream == nil { return (.branch, .send, "Publish Branch") }
        if status.ahead > 0 || status.behind > 0 { return (.branch, .send, "Sync") }
        if status.aheadOfDefault > 0, !status.onDefaultBranch { return (.branch, .branch, "Create PR") }
        return nil
    }

    // MARK: - Actions

    private func reload() async {
        do {
            async let readStatus = app.inspectWorkingTree(target.cwd)
            async let readDiffs = app.uncommittedDiffs(target.cwd)
            status = try await readStatus
            diffs = (try? await readDiffs) ?? diffs
            wanted = [:]
            error = nil
        } catch {
            self.error = error.localizedDescription
        }
    }

    /// Shown at once, ahead of the Mac's answer, and put back if it refuses.
    private func stage(_ paths: [String], to staged: Bool) async {
        Haptics.selection()
        for path in paths { wanted[path] = staged }
        do {
            try await app.stage(paths, staged: staged, in: target.cwd)
            await reload()
        } catch {
            for path in paths { wanted[path] = nil }
            failure = (staged ? "Couldn't stage" : "Couldn't unstage", error.localizedDescription)
        }
    }

    private func stageAll(_ staged: Bool, _ section: [FileChange]) async {
        guard !section.isEmpty else { return }
        stagingAll = true
        defer { stagingAll = false }
        for file in section { wanted[file.path] = staged }
        do {
            try await app.stage([], staged: staged, in: target.cwd)
            await reload()
        } catch {
            wanted = [:]
            failure = (staged ? "Couldn't stage" : "Couldn't unstage", error.localizedDescription)
        }
    }
}

private struct FileChangeRow: View {
    let file: FileChange
    let openable: Bool
    let onToggle: () -> Void

    var body: some View {
        let row = HStack(spacing: 10) {
            Button(action: onToggle) {
                ZStack {
                    Circle()
                        .strokeBorder(file.isFullyStaged ? AppTheme.inverseSurface : file.isPartlyStaged ? AppTheme.muted : AppTheme.faint, lineWidth: 1.5)
                        .background(Circle().fill(file.isFullyStaged ? AppTheme.inverseSurface : .clear))
                    if file.isFullyStaged {
                        IconView(.check, size: 13).foregroundStyle(AppTheme.inverse)
                    } else if file.isPartlyStaged {
                        IconView(.minus, size: 13).foregroundStyle(AppTheme.content)
                    }
                }
                .frame(width: 22, height: 22)
                .padding(.vertical, 12)
                .padding(.leading, 14)
                .padding(.trailing, 8)
                .contentShape(Rectangle())
                .padding(.vertical, -12)
                .padding(.leading, -14)
                .padding(.trailing, -8)
            }
            .buttonStyle(.plain)
            .accessibilityLabel(file.isFullyStaged ? "Unstage \(file.name)" : "Stage \(file.name)")
            VStack(alignment: .leading, spacing: 4) {
                HStack(spacing: 8) {
                    IconView(.fileText, size: 14).foregroundStyle(AppTheme.muted)
                    Text(file.name)
                        .font(AppTheme.font(size: 15, weight: .semibold))
                        .foregroundStyle(AppTheme.content)
                        .lineLimit(1)
                        .frame(maxWidth: .infinity, alignment: .leading)
                    if file.additions > 0 {
                        Text("+\(formatCount(file.additions))").foregroundStyle(DiffTheme.addText)
                    }
                    if file.deletions > 0 {
                        Text("-\(formatCount(file.deletions))").foregroundStyle(DiffTheme.deleteText)
                    }
                }
                .font(AppTheme.font(size: 13, weight: .semibold).monospacedDigit())
                HStack(spacing: 8) {
                    IconView(.folder, size: 12).foregroundStyle(AppTheme.faint).frame(width: 14)
                    Text(file.directory)
                        .font(AppTheme.font(size: 12))
                        .foregroundStyle(AppTheme.muted)
                        .lineLimit(1)
                        .truncationMode(.middle)
                        .frame(maxWidth: .infinity, alignment: .leading)
                    Text(file.status.letter)
                        .font(AppTheme.code(size: 12).weight(.semibold))
                        .foregroundStyle(DiffTheme.status(file.status))
                        .frame(width: 16, alignment: .trailing)
                }
            }
        }
        .padding(.horizontal, 16)
        .padding(.vertical, 12)
        .contentShape(Rectangle())

        if openable {
            NavigationLink(value: file) { row }
                .buttonStyle(PressedHighlightStyle(radius: 0))
        } else {
            row
        }
    }
}

/// Committing the staged changes, after the desktop's commit box: a message
/// the Mac can write, Commit, then Publish or Sync and Create PR once the
/// branch has something for them.
private struct CommitSheet: View {
    @Bindable var app: AppModel
    let target: ChangesTarget
    let status: WorkingTreeStatus?
    let staged: [FileChange]
    let changed: Int
    let onChanged: () async -> Void

    @Environment(\.dismiss) private var dismiss
    @Environment(\.openURL) private var openURL
    @State private var message = ""
    @State private var busy: Busy?
    @State private var generation = 0
    @State private var failure: (title: String, message: String)?

    private enum Busy { case generate, commit, sync, pr }

    var body: some View {
        let hasFiles = !staged.isEmpty
        let canCommit = hasFiles && !message.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty && busy == nil
        let remote = status?.hasRemote ?? false
        let canPublish = remote && status?.upstream == nil
        let canSync = remote && status?.upstream != nil && ((status?.ahead ?? 0) > 0 || (status?.behind ?? 0) > 0)
        let canCreatePR = remote && !(status?.onDefaultBranch ?? true) && changed == 0
            && (status?.aheadOfDefault ?? 0) > 0 && status?.behind == 0 && busy == nil

        VStack(alignment: .leading, spacing: 10) {
            HStack(spacing: 10) {
                IconView(.branch, size: 18).foregroundStyle(AppTheme.muted)
                VStack(alignment: .leading, spacing: 2) {
                    (Text(status?.branch ?? "Changes").foregroundStyle(AppTheme.content).fontWeight(.semibold)
                        + Text(counts.map { "  \($0)" } ?? "").foregroundStyle(AppTheme.muted))
                        .font(AppTheme.font(size: 16))
                    Group {
                        if hasFiles {
                            Text("\(formatCount(staged.count)) staged \(staged.count == 1 ? "file" : "files")  ").foregroundStyle(AppTheme.muted)
                                + Text("+\(formatCount(staged.reduce(0) { $0 + $1.additions }))").foregroundStyle(AppTheme.additions)
                                + Text(" -\(formatCount(staged.reduce(0) { $0 + $1.deletions }))").foregroundStyle(AppTheme.deletions)
                        } else {
                            Text(changed > 0 ? "Nothing staged" : "No uncommitted changes").foregroundStyle(AppTheme.muted)
                        }
                    }
                    .font(AppTheme.font(size: 13))
                }
                .lineLimit(1)
            }
            .frame(height: 44)

            ZStack(alignment: .bottomTrailing) {
                TextField(hasFiles ? "Commit message" : "Nothing to commit", text: $message, axis: .vertical)
                    .lineLimit(3...4)
                    .font(AppTheme.font(size: 16))
                    .disabled(!hasFiles || busy != nil)
                    .padding(12)
                    .padding(.trailing, 40)
                    .frame(minHeight: 104, alignment: .topLeading)
                Button(action: generate) {
                    Group {
                        if busy == .generate { ProgressView().controlSize(.small) } else { IconView(.sparkle, size: 17) }
                    }
                    .foregroundStyle(AppTheme.content)
                    .frame(width: 34, height: 34)
                    .background(AppTheme.fill, in: Circle())
                }
                .buttonStyle(PressedOpacityStyle())
                .disabled(!hasFiles || (busy != nil && busy != .generate))
                .opacity(hasFiles ? 1 : 0.4)
                .padding(8)
                .accessibilityLabel(busy == .generate ? "Stop writing" : "Write a message")
            }
            .background(AppTheme.content.opacity(0.06), in: RoundedRectangle(cornerRadius: 18, style: .continuous))
            .opacity(hasFiles ? 1 : 0.5)

            Spacer(minLength: 0)

            actionButton(icon: .check, label: "Commit", primary: true, busy: busy == .commit, enabled: canCommit) {
                run(.commit, failure: "Commit failed") {
                    try await app.commit(message, in: target.cwd)
                    message = ""
                }
            }
            HStack(spacing: 10) {
                if canPublish || canSync {
                    actionButton(icon: .send, label: canPublish ? "Publish Branch" : syncLabel, primary: false, busy: busy == .sync, enabled: busy == nil) {
                        run(.sync, failure: canPublish ? "Publish failed" : "Sync failed") {
                            try await app.push(target.cwd)
                        }
                    }
                }
                if canCreatePR || busy == .pr {
                    actionButton(icon: .branch, label: "Create PR", primary: false, busy: busy == .pr, enabled: canCreatePR) {
                        run(.pr, failure: "Couldn't create the pull request") {
                            if let url = try await app.createPullRequest(in: target.cwd, provider: target.provider) {
                                openURL(url)
                            }
                        }
                    }
                }
            }
        }
        .padding(.horizontal, 20)
        .padding(.top, 26)
        .padding(.bottom, 8)
        .presentationDetents([.height(status == nil ? 200 : 360), .large])
        .presentationDragIndicator(.visible)
        .presentationBackground(.regularMaterial)
        .alert(failure?.title ?? "", isPresented: Binding(get: { failure != nil }, set: { if !$0 { failure = nil } })) {
            Button("OK", role: .cancel) { failure = nil }
        } message: {
            Text(failure?.message ?? "")
        }
    }

    private var counts: String? {
        guard let status, status.ahead > 0 || status.behind > 0 else { return nil }
        return [status.ahead > 0 ? "↑\(status.ahead)" : nil, status.behind > 0 ? "↓\(status.behind)" : nil]
            .compactMap { $0 }.joined(separator: " ")
    }

    private var syncLabel: String {
        guard let status else { return "Sync" }
        if status.ahead > 0, status.behind == 0 { return "Push \(status.ahead)" }
        return "Sync"
    }

    private func generate() {
        if busy == .generate {
            generation += 1
            busy = nil
            return
        }
        generation += 1
        let mine = generation
        busy = .generate
        Task {
            do {
                let written = try await app.generateCommitMessage(in: target.cwd, provider: target.provider)
                if generation == mine { message = written }
            } catch {
                if generation == mine { failure = ("Couldn't write a message", error.localizedDescription) }
            }
            if generation == mine { busy = nil }
        }
    }

    private func run(_ kind: Busy, failure title: String, _ work: @escaping () async throws -> Void) {
        busy = kind
        Task {
            do {
                try await work()
                Haptics.success()
            } catch {
                failure = (title, error.localizedDescription)
            }
            busy = nil
            await onChanged()
        }
    }

    private func actionButton(icon: Icon, label: String, primary: Bool, busy: Bool, enabled: Bool, action: @escaping () -> Void) -> some View {
        Button(action: action) {
            HStack(spacing: 8) {
                if busy {
                    ProgressView().tint(primary ? AppTheme.inverse : AppTheme.content)
                } else {
                    IconView(icon, size: 18)
                }
                Text(label).font(AppTheme.font(size: 16, weight: .semibold))
            }
            .foregroundStyle(primary ? AppTheme.inverse : AppTheme.content)
            .padding(.trailing, 12)
            .frame(maxWidth: .infinity, minHeight: 50)
            .background(primary ? AppTheme.inverseSurface : AppTheme.fill, in: Capsule())
        }
        .buttonStyle(PressedOpacityStyle())
        .disabled(!enabled)
        .opacity(enabled || busy ? 1 : 0.4)
    }
}

/// One file's unified diff, styled after the desktop's diff view.
struct DiffScreen: View {
    let file: FileChange
    let lines: [DiffLine]?

    @AppStorage("code.wrap") private var wrap = false

    var body: some View {
        Group {
            if let lines {
                CodeView(rows: lines.map(CodeRow.init(diff:)), wrap: wrap)
            } else {
                Text("This diff is no longer available. Open it again from Changes.")
                    .font(AppTheme.font(size: 15))
                    .foregroundStyle(AppTheme.muted)
                    .padding(20)
                    .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
            }
        }
        .background(AppTheme.sheet.ignoresSafeArea())
        .navigationBarTitleDisplayMode(.inline)
        .toolbar {
            ToolbarItem(placement: .principal) {
                VStack(spacing: 1) {
                    Text(file.name).font(AppTheme.font(size: 16, weight: .semibold)).foregroundStyle(AppTheme.content)
                    (Text("\(file.status.letter) ").foregroundStyle(DiffTheme.status(file.status))
                        + Text("\(file.directory)  ").foregroundStyle(AppTheme.muted)
                        + Text(file.additions > 0 ? "+\(formatCount(file.additions)) " : "").foregroundStyle(DiffTheme.addText)
                        + Text(file.deletions > 0 ? "-\(formatCount(file.deletions))" : "").foregroundStyle(DiffTheme.deleteText))
                        .font(AppTheme.font(size: 12))
                        .lineLimit(1)
                }
            }
            ToolbarItem(placement: .topBarTrailing) {
                Button { wrap.toggle() } label: {
                    Image(systemName: "text.word.spacing")
                        .foregroundStyle(wrap ? AppTheme.accent : AppTheme.content)
                }
                .accessibilityLabel(wrap ? "Don't wrap lines" : "Wrap lines")
            }
        }
    }
}
