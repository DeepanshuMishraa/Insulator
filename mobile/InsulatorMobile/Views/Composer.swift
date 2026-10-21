import PhotosUI
import SwiftUI
import UniformTypeIdentifiers

/// Where the composer sends: a new session in a project, or one already open.
enum ComposerContext {
    case newSession(project: Project, branch: String, workspace: Binding<WorkspaceChoice>)
    case session(AgentSession, project: Project, branch: String, changed: [FileChange])

    var project: Project {
        switch self {
        case .newSession(let project, _, _), .session(_, let project, _, _): project
        }
    }

    var sessionID: UUID? {
        if case .session(let session, _, _, _) = self { return session.id }
        return nil
    }
}

/// The prompt field. At rest it's a glass pill: add, the field and the mic.
/// Focused, or holding a draft, it grows into a card with where it works on
/// top and the model along the bottom. While the Mac can't be reached the
/// field gives way to a line saying so; a draft waits under it.
struct Composer: View {
    @Bindable var app: AppModel
    let context: ComposerContext
    var busy = false
    var onStop: (() async throws -> Void)?
    let onSend: (String, [UIImage], [PickedFile]) async throws -> Void

    @Environment(Router.self) private var router
    @FocusState private var focused: Bool
    @State private var text = ""
    @State private var images: [UIImage] = []
    @State private var files: [PickedFile] = []
    @State private var sending = false
    @State private var dictating = false
    @State private var photoItems: [PhotosPickerItem] = []
    @State private var showsPhotos = false
    @State private var showsCamera = false
    @State private var showsFiles = false
    @State private var failure: String?
    @Namespace private var morph

    static let placeholder = "Plan, ask, build…"
    private static let busyPlaceholder = "Send a follow-up…"
    private static let button: CGFloat = 36
    private static let edge: CGFloat = 7
    private static let maxAttachments = 10

    private var hasDraft: Bool { !text.isEmpty || !images.isEmpty || !files.isEmpty }
    private var expanded: Bool { focused || hasDraft || dictating }
    private var canSend: Bool {
        (!text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty || !images.isEmpty || !files.isEmpty) && !sending
    }
    private var offline: Bool { !app.isConnected }

    var body: some View {
        VStack(spacing: 0) {
            if expanded {
                contextRow
                    .frame(height: 46)
                    .transition(.opacity)
                if !images.isEmpty || !files.isEmpty {
                    attachmentStrip
                        .frame(height: 58)
                        .transition(.opacity)
                }
            }
            HStack(alignment: .bottom, spacing: 8) {
                if !expanded {
                    addButton.matchedGeometryEffect(id: "add", in: morph)
                }
                field
                if !expanded {
                    trailingButton.matchedGeometryEffect(id: "trailing", in: morph)
                }
            }
            .padding(.horizontal, expanded ? 11 : 0)
            if expanded {
                ZStack {
                    HStack(spacing: 8) {
                        addButton.matchedGeometryEffect(id: "add", in: morph)
                        modelChip
                        Spacer(minLength: 0)
                        trailingButton.matchedGeometryEffect(id: "trailing", in: morph)
                    }
                    .opacity(dictating ? 0 : 1)
                    if dictating {
                        DictationBar { words in
                            dictating = false
                            if let words, !words.isEmpty {
                                text = text.isEmpty ? words : "\(text) \(words)"
                            }
                        }
                        .transition(.opacity)
                    }
                }
                .frame(height: Self.button + Self.edge * 2)
            }
        }
        .padding(Self.edge)
        .glass(in: RoundedRectangle(cornerRadius: (Self.button + Self.edge * 2) / 2, style: .continuous))
        .padding(.horizontal, expanded ? 12 : 20)
        .animation(.easeOut(duration: 0.28), value: expanded)
        .animation(.easeOut(duration: 0.18), value: dictating)
        .photosPicker(isPresented: $showsPhotos, selection: $photoItems, maxSelectionCount: Self.maxAttachments, matching: .images)
        .fileImporter(isPresented: $showsFiles, allowedContentTypes: [.item], allowsMultipleSelection: true) { result in
            attachFiles(result)
        }
        .fullScreenCover(isPresented: $showsCamera) {
            CameraPicker { image in
                if let image { images.append(image) }
                showsCamera = false
                focused = true
            }
            .ignoresSafeArea()
        }
        .onChange(of: photoItems) { _, items in
            guard !items.isEmpty else { return }
            Task { await attachPhotos(items) }
        }
        .onChange(of: offline) { _, isOffline in
            if isOffline { focused = false }
        }
        .alert("Message not sent", isPresented: Binding(get: { failure != nil }, set: { if !$0 { failure = nil } })) {
            Button("OK", role: .cancel) { failure = nil }
        } message: {
            Text(failure ?? "")
        }
    }

    // MARK: - Rows

    @ViewBuilder
    private var contextRow: some View {
        HStack(spacing: 6) {
            switch context {
            case .newSession(_, _, let workspace):
                ComposerWorkspace(value: workspace, disabled: offline)
            case .session(let session, _, let branch, let changed):
                BranchLine(current: session.worktreePath == nil, branch: branch, files: changed)
            }
            Spacer(minLength: 0)
        }
        .padding(.horizontal, 13)
        .padding(.top, 4)
    }

    private var field: some View {
        ZStack(alignment: .leading) {
            TextField(busy ? Self.busyPlaceholder : Self.placeholder, text: $text, axis: .vertical)
                .font(AppTheme.font(size: 17))
                .foregroundStyle(AppTheme.content)
                .lineLimit(expanded ? 2...7 : 1...1)
                .focused($focused)
                .disabled(dictating || offline)
                .opacity(offline || (dictating && !expanded) ? 0 : 1)
                .padding(.vertical, expanded ? 8 : 7)
                .frame(minHeight: expanded ? 64 : Self.button, alignment: .topLeading)
                .tint(AppTheme.accent)
            if offline {
                HStack(spacing: 10) {
                    PulsingDot()
                    Text("Reconnecting to desktop")
                        .font(AppTheme.font(size: 15))
                        .foregroundStyle(AppTheme.muted)
                        .lineLimit(1)
                }
                .accessibilityElement(children: .combine)
                .accessibilityLabel("Reconnecting to your desktop. Messages can't be sent until it's back.")
                .transition(.opacity)
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }

    private var addButton: some View {
        Menu {
            Button { showsPhotos = true } label: { Label("Photo Library", systemImage: "photo.on.rectangle") }
            Button { showsCamera = true } label: { Label("Camera", systemImage: "camera") }
            Button { showsFiles = true } label: { Label("Files", systemImage: "folder") }
        } label: {
            IconView(.add, size: 18)
                .foregroundStyle(AppTheme.content)
                .frame(width: Self.button, height: Self.button)
                .background(AppTheme.fill, in: Circle())
        }
        .disabled(offline)
        .opacity(offline ? 0.4 : 1)
        .accessibilityLabel("Add photos or files")
    }

    @ViewBuilder
    private var trailingButton: some View {
        if canSend {
            round(label: "Send", primary: true) { send() } content: {
                IconView(.arrowUp, size: 18).foregroundStyle(AppTheme.inverse)
            }
        } else if busy, let onStop {
            round(label: "Stop", primary: true) {
                Task {
                    do { try await onStop() } catch { failure = error.localizedDescription }
                }
            } content: {
                RoundedRectangle(cornerRadius: 3, style: .continuous)
                    .fill(AppTheme.inverse)
                    .frame(width: 12, height: 12)
            }
        } else {
            round(label: "Dictate", primary: false) {
                focused = false
                dictating = true
            } content: {
                IconView(.mic, size: 18).foregroundStyle(AppTheme.content)
            }
            .disabled(offline)
            .opacity(offline ? 0.4 : 1)
        }
    }

    private func round(
        label: String,
        primary: Bool,
        action: @escaping () -> Void,
        @ViewBuilder content: () -> some View
    ) -> some View {
        Button(action: action) {
            content()
                .frame(width: Self.button, height: Self.button)
                .background(primary ? AppTheme.inverseSurface : AppTheme.fill, in: Circle())
        }
        .buttonStyle(PressedOpacityStyle())
        .accessibilityLabel(label)
    }

    @ViewBuilder
    private var modelChip: some View {
        let choice = context.sessionID.flatMap { id in app.session(withID: id).map(app.choice(of:)) }
            ?? app.newSessionChoice(for: context.project.id)
        let model = choice.flatMap { choice in app.models(for: choice.provider).first { $0.id == choice.modelID } }
        let name = choice.flatMap { app.modelName(provider: $0.provider, modelID: $0.modelID) } ?? "Desktop default"
        let effort = model?.reasoningEfforts.first { $0.id == choice?.reasoningEffort }?.label
        Button {
            router.sheet = .models(projectID: context.project.id, sessionID: context.sessionID)
        } label: {
            HStack(spacing: 6) {
                if let provider = choice?.provider {
                    ProviderLogo(provider: provider, size: 16).foregroundStyle(AppTheme.content)
                }
                (Text(name).foregroundStyle(AppTheme.content).fontWeight(.medium)
                    + Text(effort.map { " \($0)" } ?? "").foregroundStyle(AppTheme.muted))
                    .font(AppTheme.font(size: 15))
                    .lineLimit(1)
                IconView(.chevronDown, size: 14).foregroundStyle(AppTheme.muted)
            }
            .padding(.horizontal, 12)
            .frame(height: Self.button)
            .background(AppTheme.fill, in: Capsule())
        }
        .buttonStyle(PressedOpacityStyle())
        .accessibilityLabel("Model: \(name)\(effort.map { ", \($0) effort" } ?? "")")
        .accessibilityHint("Choose the model, effort and permissions")
    }

    private var attachmentStrip: some View {
        ScrollView(.horizontal) {
            HStack(spacing: 8) {
                ForEach(images.indices, id: \.self) { index in
                    Image(uiImage: images[index])
                        .resizable()
                        .scaledToFill()
                        .frame(width: 48, height: 48)
                        .clipShape(RoundedRectangle(cornerRadius: 10, style: .continuous))
                        .overlay(alignment: .topTrailing) {
                            removeButton("Remove image") { images.remove(at: index) }
                        }
                }
                ForEach(files) { file in
                    HStack(spacing: 6) {
                        IconView(.fileText, size: 18).foregroundStyle(AppTheme.muted)
                        Text(file.name)
                            .font(AppTheme.font(size: 13))
                            .foregroundStyle(AppTheme.content)
                            .lineLimit(1)
                    }
                    .padding(.leading, 10)
                    .padding(.trailing, 14)
                    .frame(maxWidth: 180, minHeight: 48)
                    .background(AppTheme.fill, in: RoundedRectangle(cornerRadius: 10, style: .continuous))
                    .overlay(alignment: .topTrailing) {
                        removeButton("Remove \(file.name)") { files.removeAll { $0.id == file.id } }
                    }
                }
            }
            .padding(.horizontal, 9)
            .padding(.vertical, 5)
        }
        .scrollIndicators(.hidden)
    }

    private func removeButton(_ label: String, action: @escaping () -> Void) -> some View {
        Button(action: action) {
            IconView(.close, size: 10)
                .foregroundStyle(AppTheme.content)
                .frame(width: 18, height: 18)
                .background(AppTheme.surfaceRaised, in: Circle())
        }
        .buttonStyle(.plain)
        .offset(x: 5, y: -5)
        .accessibilityLabel(label)
    }

    // MARK: - Actions

    /// Clears and folds back to the pill at once; the draft comes back if the
    /// Mac never takes it.
    private func send() {
        guard canSend, !offline else { return }
        let message = text.trimmingCharacters(in: .whitespacesAndNewlines)
        let sentImages = images
        let sentFiles = files
        sending = true
        text = ""
        images = []
        files = []
        focused = false
        Haptics.impact(.light)
        Task {
            defer { sending = false }
            do {
                try await onSend(message, sentImages, sentFiles)
            } catch {
                if text.isEmpty { text = message }
                if images.isEmpty { images = sentImages }
                if files.isEmpty { files = sentFiles }
                failure = error.localizedDescription
            }
        }
    }

    private func attachPhotos(_ items: [PhotosPickerItem]) async {
        for item in items.prefix(Self.maxAttachments - images.count - files.count) {
            if let data = try? await item.loadTransferable(type: Data.self), let image = UIImage(data: data) {
                images.append(image)
            }
        }
        photoItems = []
        focused = true
    }

    private func attachFiles(_ result: Result<[URL], Error>) {
        guard case .success(let urls) = result else { return }
        for url in urls.prefix(Self.maxAttachments - images.count - files.count) {
            let scoped = url.startAccessingSecurityScopedResource()
            defer { if scoped { url.stopAccessingSecurityScopedResource() } }
            guard let data = try? Data(contentsOf: url) else { continue }
            if let type = UTType(filenameExtension: url.pathExtension), type.conforms(to: .image), let image = UIImage(data: data) {
                images.append(image)
            } else {
                files.append(PickedFile(name: url.lastPathComponent, data: data))
            }
        }
        focused = true
    }
}

/// A new draft's working copy: the project's checkout or a fresh worktree,
/// made when the first message is sent.
struct ComposerWorkspace: View {
    @Binding var value: WorkspaceChoice
    var disabled = false

    var body: some View {
        let current = value == .current
        let content = HStack(spacing: 6) {
            IconView(current ? .folder : .branch, size: 14).foregroundStyle(AppTheme.muted)
            Text(value.label)
                .font(AppTheme.font(size: 13, weight: .medium))
                .foregroundStyle(AppTheme.content)
                .lineLimit(1)
                .truncationMode(.middle)
        }
        if case .existing = value {
            content
        } else {
            Button {
                Haptics.selection()
                value = current ? .newWorktree : .current
            } label: {
                HStack(spacing: 6) {
                    content
                    IconView(.chevronUpDown, size: 14).foregroundStyle(AppTheme.muted)
                }
                .padding(.horizontal, 10)
                .frame(height: 30)
                .background(AppTheme.fill, in: Capsule())
            }
            .buttonStyle(PressedOpacityStyle())
            .disabled(disabled)
            .padding(.leading, -6)
            .accessibilityHint(current ? "Start this session in a new worktree" : "Start this session in the current checkout")
        }
    }
}

private struct CameraPicker: UIViewControllerRepresentable {
    let onPick: (UIImage?) -> Void

    func makeUIViewController(context: Context) -> UIImagePickerController {
        let picker = UIImagePickerController()
        picker.sourceType = UIImagePickerController.isSourceTypeAvailable(.camera) ? .camera : .photoLibrary
        picker.delegate = context.coordinator
        return picker
    }

    func updateUIViewController(_ controller: UIImagePickerController, context: Context) {}

    func makeCoordinator() -> Coordinator { Coordinator(onPick: onPick) }

    final class Coordinator: NSObject, UIImagePickerControllerDelegate, UINavigationControllerDelegate {
        let onPick: (UIImage?) -> Void

        init(onPick: @escaping (UIImage?) -> Void) { self.onPick = onPick }

        func imagePickerController(
            _ picker: UIImagePickerController,
            didFinishPickingMediaWithInfo info: [UIImagePickerController.InfoKey: Any]
        ) {
            onPick(info[.originalImage] as? UIImage)
        }

        func imagePickerControllerDidCancel(_ picker: UIImagePickerController) {
            onPick(nil)
        }
    }
}
