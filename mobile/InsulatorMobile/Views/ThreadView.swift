import SwiftUI
import PhotosUI
import Textual
import UIKit

enum ActiveInputPopup: Identifiable {
    case plus
    case permissions
    case effort

    var id: Int {
        switch self {
        case .plus: 0
        case .permissions: 1
        case .effort: 2
        }
    }
}

struct ThreadView: View {
    @Bindable var app: AppModel
    let sessionID: UUID

    @Environment(\.dismiss) private var dismiss
    @State private var input = ""
    @State private var activePopup: ActiveInputPopup? = nil
    @State private var attachedImages: [UIImage] = []
    @State private var showsCamera = false
    @State private var cameraImage: UIImage? = nil
    @State private var showsRename = false
    @State private var renameText = ""
    @State private var confirmsDelete = false
    @State private var viewerTarget: ImageViewerTarget?
    /// Whether the transcript is being held at the live edge. Only while this is
    /// true does new content keep the bottom where it is.
    @State private var isPinnedToBottom = true
    /// A finger is on the scroll view, so where the bottom sits is the user's
    /// call until they return to the edge.
    @State private var isUserScrolling = false
    /// Set while a jump to the live edge is animating, so the sticky pin does
    /// not fight the animation frame by frame.
    @State private var jumpInFlight = false

    private static let bottomAnchor = "thread-bottom"
    /// Close enough to the edge to count as still reading the newest line.
    private static let pinThreshold: CGFloat = 56

    private var session: AgentSession? {
        if app.selectedSession?.id == sessionID { return app.selectedSession }
        return app.sessions.first { $0.id == sessionID }
    }

    var body: some View {
        Group {
            if let session {
                VStack(spacing: 0) {
                    // Custom Navigation Bar (No system bar chrome = zero double buttons)
                    HStack(alignment: .center) {
                        Button {
                            dismiss()
                        } label: {
                            Image(systemName: "chevron.left")
                                .font(.system(size: 16, weight: .semibold))
                                .foregroundStyle(.white)
                                .frame(width: 44, height: 44)
                                .background(AppTheme.raised, in: Circle())
                        }
                        .buttonStyle(.plain)
                        .accessibilityLabel("Back")

                        Spacer()

                        VStack(spacing: 2) {
                            Text(session.displayTitle)
                                .font(.system(size: 17, weight: .semibold))
                                .foregroundStyle(.white)
                                .lineLimit(1)
                            Text(app.displayHost ?? "Mac")
                                .font(.caption2)
                                .foregroundStyle(AppTheme.secondary)
                                .lineLimit(1)
                        }

                        Spacer()

                        Menu {
                            Button {
                                renameText = session.displayTitle
                                showsRename = true
                            } label: {
                                Label("Rename thread", systemImage: "pencil")
                            }
                            Button(role: .destructive) {
                                confirmsDelete = true
                            } label: {
                                Label("Delete thread", systemImage: "trash")
                            }
                        } label: {
                            Image(systemName: "ellipsis")
                                .font(.system(size: 17, weight: .medium))
                                .foregroundStyle(.white)
                                .frame(width: 44, height: 44)
                                .background(AppTheme.raised, in: Circle())
                        }
                        .buttonStyle(.plain)
                        .accessibilityLabel("Thread options")
                    }
                    .padding(.horizontal, 16)
                    .padding(.top, 4)
                    .padding(.bottom, 4)

                    transcript(for: session)
                }
                .toolbar(.hidden, for: .navigationBar)
                .sheet(item: permissionBinding) { permission in
                    PermissionSheet(app: app, permission: permission)
                        .presentationDetents([.medium])
                }
                .sheet(isPresented: $showsCamera) {
                    CameraPicker(image: $cameraImage)
                }
                .onChange(of: cameraImage) { _, newImage in
                    if let newImage {
                        attachedImages.append(newImage)
                        cameraImage = nil
                    }
                }
                .alert("Rename Thread", isPresented: $showsRename) {
                    TextField("Thread title", text: $renameText)
                    Button("Save") {
                        Task { await app.renameSession(session.id, title: renameText) }
                    }
                    Button("Cancel", role: .cancel) {}
                }
                .confirmationDialog("Delete this thread?", isPresented: $confirmsDelete, titleVisibility: .visible) {
                    Button("Delete thread", role: .destructive) {
                        Task {
                            await app.deleteSession(session.id)
                            dismiss()
                        }
                    }
                    Button("Cancel", role: .cancel) {}
                } message: {
                    Text("This thread will be deleted from your Mac.")
                }
            } else {
                ProgressView("Loading thread…")
            }
        }
        .background(AppTheme.background)
        .fullScreenCover(item: $viewerTarget) { target in
            AttachmentViewer(image: target.image, name: target.name)
        }
    }

    // MARK: - Transcript

    private func transcript(for session: AgentSession) -> some View {
        ScrollViewReader { proxy in
            ScrollView(showsIndicators: false) {
                VStack(spacing: 0) {
                    LazyVStack(alignment: .leading, spacing: 20) {
                        ForEach(TranscriptTimeline(session).entries) { entry in
                            TimelineEntryView(
                                entry: entry,
                                images: app.attachmentImages,
                                onOpenImage: { image, name in
                                    viewerTarget = ImageViewerTarget(image: image, name: name)
                                }
                            )
                            // Rows that did not change must not re-evaluate their
                            // bodies: during a stream the parent re-renders many
                            // times a second, and a Markdown row re-parses on it.
                            .equatable()
                            .id(entry.id)
                        }
                        if session.status.isBusy {
                            WorkingIndicator(session: session)
                        }
                    }
                    .scrollTargetLayout()
                    .padding(.horizontal, 16)
                    .padding(.top, 16)
                    .padding(.bottom, 12)

                    Color.clear
                        .frame(height: 1)
                        .id(Self.bottomAnchor)
                }
            }
            .scrollIndicators(.hidden)
            .overlay {
                // Tap barrier while a popup is open. Sits above the transcript and
                // below the jump control, which stays reachable.
                if activePopup != nil {
                    Color.black.opacity(0.001)
                        .onTapGesture {
                            withAnimation(.spring(response: 0.28, dampingFraction: 0.8)) {
                                activePopup = nil
                            }
                        }
                }
            }
            .overlay(alignment: .bottomTrailing) {
                jumpToLiveEdge(using: proxy, isStreaming: session.status.isBusy)
            }
            .onScrollGeometryChange(for: CGFloat.self, of: Self.distanceFromBottom) { _, distance in
                guard !jumpInFlight else {
                    // Our own jump is in flight; the pin resumes once it lands.
                    if distance <= 1 { jumpInFlight = false }
                    return
                }
                if isUserScrolling {
                    isPinnedToBottom = distance <= Self.pinThreshold
                    return
                }
                // Content grew while we were following. Hold the edge with an
                // unanimated scroll: an eased scroll restarted on every token is
                // what made streaming feel like a fight with the scroll view.
                if isPinnedToBottom, distance > 1 {
                    proxy.scrollTo(Self.bottomAnchor, anchor: .bottom)
                }
            }
            .onScrollPhaseChange { _, phase in
                switch phase {
                case .tracking, .interacting, .decelerating:
                    jumpInFlight = false
                    isUserScrolling = true
                case .idle:
                    isUserScrolling = false
                    jumpInFlight = false
                case .animating:
                    break
                @unknown default:
                    break
                }
            }
            .safeAreaInset(edge: .bottom, spacing: 0) {
                composerArea(for: session, using: proxy)
            }
            .task {
                // Backstop for the first layout: the geometry handler pins from
                // here on, but a long thread can open before it has a size.
                await Task.yield()
                guard !isUserScrolling else { return }
                proxy.scrollTo(Self.bottomAnchor, anchor: .bottom)
            }
        }
    }

    private static func distanceFromBottom(_ geometry: ScrollGeometry) -> CGFloat {
        max(0, geometry.contentSize.height + geometry.contentOffset.y - geometry.containerSize.height)
    }

    private func jumpToLiveEdge(using proxy: ScrollViewProxy, isStreaming: Bool) -> some View {
        Button {
            UIImpactFeedbackGenerator(style: .soft).impactOccurred()
            followLiveEdge(using: proxy)
        } label: {
            ZStack {
                Circle()
                    .fill(.ultraThinMaterial)
                Circle()
                    .strokeBorder(Color.white.opacity(0.14), lineWidth: 1)
                Image(systemName: "arrow.down")
                    .font(.system(size: 16, weight: .semibold))
                    .foregroundStyle(.white)
            }
            .frame(width: 46, height: 46)
            .overlay(alignment: .bottomTrailing) {
                // Paired with the arrow rather than replacing it: streaming
                // progress is not conveyed by the button alone.
                if isStreaming {
                    Circle()
                        .fill(AppTheme.accent)
                        .frame(width: 10, height: 10)
                        .overlay(Circle().strokeBorder(AppTheme.background, lineWidth: 2))
                        .offset(x: 1, y: 1)
                }
            }
            .shadow(color: .black.opacity(0.5), radius: 14, y: 5)
            .contentShape(Circle())
        }
        .buttonStyle(BouncyButtonStyle(scale: 0.9))
        .accessibilityLabel("Jump to the latest line")
        .padding(.trailing, 16)
        .padding(.bottom, 12)
        .opacity(isPinnedToBottom ? 0 : 1)
        .scaleEffect(isPinnedToBottom ? 0.8 : 1)
        .animation(.spring(response: 0.3, dampingFraction: 0.78), value: isPinnedToBottom)
        .allowsHitTesting(!isPinnedToBottom)
        .accessibilityHidden(isPinnedToBottom)
    }

    /// Re-arms following and animates to the newest line.
    private func followLiveEdge(using proxy: ScrollViewProxy) {
        isPinnedToBottom = true
        // A finger may still be lifting: the jump owns the position until the
        // scroll view reports itself idle again.
        isUserScrolling = false
        jumpInFlight = true
        withAnimation(.easeOut(duration: 0.32)) {
            proxy.scrollTo(Self.bottomAnchor, anchor: .bottom)
        }
    }

    // MARK: - Composer

    private func composerArea(for session: AgentSession, using proxy: ScrollViewProxy) -> some View {
        VStack(alignment: .leading, spacing: 0) {
            // Popups anchored directly above composer
            if let popup = activePopup {
                Group {
                    switch popup {
                    case .plus:
                        PlusPopupView(
                            onTakePhoto: {
                                activePopup = nil
                                showsCamera = true
                            },
                            onAddImages: { images in
                                activePopup = nil
                                attachedImages.append(contentsOf: images)
                            }
                        )
                        .padding(.leading, 14)

                    case .permissions:
                        PermissionPopupView(
                            currentMode: session.runtimeMode,
                            onSelect: { mode in
                                activePopup = nil
                                Task { await app.updateSessionMode(session.id, mode: mode) }
                            }
                        )
                        .padding(.leading, 20)

                    case .effort:
                        EffortPopupView(
                            session: session,
                            app: app,
                            onSelectEffort: { option in
                                Task { await app.updateSessionReasoning(session.id, reasoningEffort: option.id) }
                            }
                        )
                        .padding(.horizontal, 12)
                    }
                }
                .padding(.bottom, 6)
                .transition(.asymmetric(
                    insertion: .scale(scale: 0.94, anchor: popup == .effort ? .bottom : .bottomLeading).combined(with: .opacity),
                    removal: .opacity
                ))
            }

            // Project selector above composer
            HStack {
                Menu {
                    ForEach(app.projects) { project in
                        Button {
                            Task { await app.updateSessionProject(session.id, projectID: project.id) }
                        } label: {
                            HStack {
                                Text(project.name)
                                if project.id == session.projectID {
                                    Image(systemName: "checkmark")
                                }
                            }
                        }
                    }
                } label: {
                    HStack(spacing: 6) {
                        Image(systemName: "folder")
                            .font(.subheadline)
                        Text(app.project(for: session)?.name ?? "Quick Chat")
                            .font(.subheadline.weight(.medium))
                        Image(systemName: "chevron.up.chevron.down")
                            .font(.system(size: 11, weight: .bold))
                    }
                    .foregroundStyle(Color.white.opacity(0.85))
                }
                Spacer()
            }
            .padding(.horizontal, 16)
            .padding(.bottom, 8)

            // Main Composer Card
            ComposerCard(
                text: $input,
                attachedImages: $attachedImages,
                session: session,
                app: app,
                activePopup: $activePopup,
                onSend: { text, images in send(text, images: images, using: proxy) },
                onSteer: { text, images in steer(text, images: images) },
                onCancel: { Task { await app.cancel() } }
            )
        }
        .padding(.bottom, 6)
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

    private func send(_ text: String, images: [UIImage], using proxy: ScrollViewProxy) {
        input = ""
        attachedImages.removeAll()
        // The user just acted at the bottom; follow their own turn even if they
        // had scrolled up to read something.
        isPinnedToBottom = true
        followLiveEdge(using: proxy)
        Task { await app.send(text, images: images) }
    }

    private func steer(_ text: String, images: [UIImage]) {
        input = ""
        attachedImages.removeAll()
        Task { await app.steer(text, images: images) }
    }
}

/// An attachment opened full screen.
private struct ImageViewerTarget: Identifiable {
    let id = UUID()
    let image: UIImage
    let name: String
}


// MARK: - Composer Card

private struct ComposerCard: View {
    @Binding var text: String
    @Binding var attachedImages: [UIImage]
    let session: AgentSession
    @Bindable var app: AppModel
    @Binding var activePopup: ActiveInputPopup?
    let onSend: (String, [UIImage]) -> Void
    let onSteer: (String, [UIImage]) -> Void
    let onCancel: () -> Void

    @FocusState private var focused: Bool

    private var canSend: Bool {
        !text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty || !attachedImages.isEmpty
    }

    private var availableModels: [ProviderModel] {
        app.models(for: session.provider)
    }

    private var currentModel: ProviderModel? {
        availableModels.first { $0.id == session.model }
    }

    private var modelDisplayName: String {
        if let currentModel { return currentModel.name }
        if let model = session.model { return model }
        return session.provider.name
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            // Attached images preview
            if !attachedImages.isEmpty {
                ScrollView(.horizontal, showsIndicators: false) {
                    HStack(spacing: 10) {
                        ForEach(attachedImages.indices, id: \.self) { index in
                            ZStack(alignment: .topTrailing) {
                                Image(uiImage: attachedImages[index])
                                    .resizable()
                                    .aspectRatio(contentMode: .fill)
                                    .frame(width: 58, height: 58)
                                    .clipShape(RoundedRectangle(cornerRadius: 12))
                                    .overlay(
                                        RoundedRectangle(cornerRadius: 12)
                                            .stroke(Color.white.opacity(0.15), lineWidth: 1)
                                    )

                                Button {
                                    withAnimation(.spring(response: 0.25, dampingFraction: 0.8)) {
                                        _ = attachedImages.remove(at: index)
                                    }
                                } label: {
                                    Image(systemName: "xmark.circle.fill")
                                        .font(.system(size: 18))
                                        .foregroundStyle(Color.white, Color(red: 0.15, green: 0.15, blue: 0.17))
                                        .background(Circle().fill(Color.black.opacity(0.4)))
                                }
                                .offset(x: 5, y: -5)
                            }
                            .padding(.top, 4)
                            .padding(.trailing, 4)
                        }
                    }
                    .padding(.horizontal, 4)
                }
            }

            // Input Text Field
            TextField("Ask Insulator", text: $text, axis: .vertical)
                .lineLimit(2...7)
                .frame(minHeight: 46)
                .focused($focused)
                .foregroundStyle(.white)
                .tint(.white)
                .padding(.horizontal, 6)
                .padding(.vertical, 4)

            // Bottom Controls Bar
            HStack(spacing: 12) {
                // 1. Plus Button (Attachments)
                Button {
                    withAnimation(.spring(response: 0.28, dampingFraction: 0.8)) {
                        activePopup = (activePopup == .plus ? nil : .plus)
                    }
                } label: {
                    Image(systemName: "plus")
                        .font(.system(size: 19, weight: .regular))
                        .foregroundStyle(activePopup == .plus ? .white : Color.white.opacity(0.8))
                        .frame(width: 32, height: 32)
                }
                .buttonStyle(.plain)
                .accessibilityLabel("Add attachments")

                // 2. Hand Button (Permissions)
                Button {
                    withAnimation(.spring(response: 0.28, dampingFraction: 0.8)) {
                        activePopup = (activePopup == .permissions ? nil : .permissions)
                    }
                } label: {
                    Image(systemName: permissionIconName)
                        .font(.system(size: 18, weight: .regular))
                        .foregroundStyle(activePopup == .permissions ? .white : Color.white.opacity(0.8))
                        .frame(width: 32, height: 32)
                }
                .buttonStyle(.plain)
                .accessibilityLabel("Permission mode")

                Spacer()

                // 3. Model & Reasoning Selector (Opens Effort Popover)
                Button {
                    withAnimation(.spring(response: 0.28, dampingFraction: 0.8)) {
                        activePopup = (activePopup == .effort ? nil : .effort)
                    }
                } label: {
                    HStack(spacing: 6) {
                        Circle()
                            .strokeBorder(Color.white.opacity(0.35), lineWidth: 1.5)
                            .frame(width: 14, height: 14)

                        Text(modelDisplayName)
                            .font(.system(size: 14, weight: .medium))
                            .foregroundStyle(.white)
                            .lineLimit(1)

                        if let reasoning = session.reasoningEffort, !reasoning.isEmpty {
                            Text(reasoning.capitalized)
                                .font(.system(size: 14))
                                .foregroundStyle(AppTheme.secondary)
                        }
                    }
                    .frame(height: 32)
                }
                .buttonStyle(.plain)
                .accessibilityLabel("Reasoning effort and model selector")

                // 4. Send / Stop Button
                if session.status.isBusy {
                    Button {
                        focused = false
                        activePopup = nil
                        onCancel()
                    } label: {
                        Image(systemName: "stop.fill")
                            .font(.system(size: 13, weight: .bold))
                            .frame(width: 36, height: 36)
                            .background(AppTheme.raised, in: Circle())
                            .foregroundStyle(.white)
                    }
                    .accessibilityLabel("Stop agent")

                    if canSend {
                        Button {
                            focused = false
                            activePopup = nil
                            onSteer(text, attachedImages)
                        } label: {
                            Image(systemName: "arrow.turn.up.right")
                                .font(.system(size: 13, weight: .bold))
                                .frame(width: 36, height: 36)
                                .background(.white, in: Circle())
                                .foregroundStyle(.black)
                        }
                        .accessibilityLabel("Steer agent")
                    }
                } else {
                    Button {
                        focused = false
                        activePopup = nil
                        onSend(text, attachedImages)
                    } label: {
                        Image(systemName: "arrow.up")
                            .font(.system(size: 15, weight: .bold))
                            .frame(width: 36, height: 36)
                            .background(canSend ? Color.white : AppTheme.raised, in: Circle())
                            .foregroundStyle(canSend ? Color.black : AppTheme.secondary)
                    }
                    .disabled(!canSend)
                    .accessibilityLabel("Send message")
                }
            }
        }
        .padding(.horizontal, 14)
        .padding(.vertical, 12)
        .background(Color(red: 0.12, green: 0.12, blue: 0.14), in: RoundedRectangle(cornerRadius: 24))
        .overlay(RoundedRectangle(cornerRadius: 24).stroke(AppTheme.border))
        .padding(.horizontal, 12)
    }

    private var permissionIconName: String {
        switch session.runtimeMode {
        case .ask: return "hand.raised"
        case .autoAcceptEdits, .auto: return "chevron.left.forwardslash.chevron.right"
        case .fullAccess: return "exclamationmark.octagon"
        }
    }
}

// MARK: - Effort & Model Selector Popover (Matches file-96612d6b26bdce2542e9f6b5290d25b8.heic)

struct EffortOption: Identifiable, Equatable {
    let id: String
    let label: String
}

private struct EffortPopupView: View {
    let session: AgentSession
    @Bindable var app: AppModel
    let onSelectEffort: (EffortOption) -> Void

    @State private var showingModelList = false
    @State private var selectedProviderTab: ProviderKind

    init(session: AgentSession, app: AppModel, onSelectEffort: @escaping (EffortOption) -> Void) {
        self.session = session
        self.app = app
        self.onSelectEffort = onSelectEffort
        _selectedProviderTab = State(initialValue: session.provider)
    }

    private var effortOptions: [EffortOption] {
        let models = app.models(for: session.provider)
        if let current = models.first(where: { $0.id == session.model }), !current.reasoningEfforts.isEmpty {
            return current.reasoningEfforts.map { EffortOption(id: $0.id, label: $0.label) }
        }
        return [
            EffortOption(id: "low", label: "Low"),
            EffortOption(id: "medium", label: "Medium"),
            EffortOption(id: "high", label: "High"),
            EffortOption(id: "max", label: "Max")
        ]
    }

    private var modelShortName: String {
        let models = app.models(for: session.provider)
        if let current = models.first(where: { $0.id == session.model }) {
            return current.name
        }
        return session.model ?? session.provider.name
    }

    private var currentEffortLabel: String {
        let raw = session.reasoningEffort ?? "medium"
        if let match = effortOptions.first(where: { $0.id.lowercased() == raw.lowercased() }) {
            return match.label
        }
        return raw.capitalized
    }

    var body: some View {
        VStack(spacing: 0) {
            if showingModelList {
                // Models List View with Provider Tabs
                VStack(alignment: .leading, spacing: 14) {
                    // Header
                    HStack {
                        Button {
                            withAnimation(.easeInOut(duration: 0.2)) {
                                showingModelList = false
                            }
                        } label: {
                            HStack(spacing: 5) {
                                Image(systemName: "chevron.left")
                                    .font(.system(size: 14, weight: .bold))
                                Text("Effort")
                                    .font(.system(size: 15))
                            }
                            .foregroundStyle(.white)
                        }
                        .buttonStyle(.plain)

                        Spacer()

                        Text("Select Model")
                            .font(.system(size: 16, weight: .semibold))
                            .foregroundStyle(.white)

                        Spacer()

                        Color.clear.frame(width: 50, height: 20)
                    }

                    // Provider Tabs Carousel (Icon + Name)
                    ScrollView(.horizontal, showsIndicators: false) {
                        HStack(spacing: 8) {
                            ForEach(app.allAvailableProviders) { provider in
                                Button {
                                    UISelectionFeedbackGenerator().selectionChanged()
                                    withAnimation(.easeInOut(duration: 0.18)) {
                                        selectedProviderTab = provider
                                    }
                                } label: {
                                    HStack(spacing: 6) {
                                        Image(provider.iconName)
                                            .resizable()
                                            .aspectRatio(contentMode: .fit)
                                            .frame(width: 16, height: 16)
                                        Text(provider.name)
                                            .font(.system(size: 13, weight: .medium))
                                    }
                                    .padding(.horizontal, 14)
                                    .padding(.vertical, 7)
                                    .background(
                                        selectedProviderTab == provider ? Color.white : AppTheme.raised,
                                        in: Capsule()
                                    )
                                    .foregroundStyle(selectedProviderTab == provider ? Color.black : Color.white.opacity(0.85))
                                }
                                .buttonStyle(.plain)
                            }
                        }
                        .padding(.horizontal, 2)
                    }

                    // Models for the Selected Provider
                    let providerModels = app.models(for: selectedProviderTab)
                    ScrollView {
                        LazyVStack(spacing: 8) {
                            ForEach(providerModels) { model in
                                let isSelected = (selectedProviderTab == session.provider && (model.id == session.model || (session.model == nil && model.isDefault)))
                                Button {
                                    UIImpactFeedbackGenerator(style: .medium).impactOccurred()
                                    Task {
                                        if selectedProviderTab != session.provider {
                                            await app.updateSessionProvider(session.id, provider: selectedProviderTab)
                                        }
                                        await app.updateSessionModel(session.id, model: model)
                                    }
                                    withAnimation(.easeInOut(duration: 0.2)) {
                                        showingModelList = false
                                    }
                                } label: {
                                    HStack {
                                        VStack(alignment: .leading, spacing: 3) {
                                            HStack(spacing: 6) {
                                                Text(model.name)
                                                    .font(.system(size: 15, weight: .medium))
                                                    .foregroundStyle(.white)
                                                if model.isDefault {
                                                    Text("DEFAULT")
                                                        .font(.system(size: 10, weight: .bold))
                                                        .padding(.horizontal, 6)
                                                        .padding(.vertical, 2)
                                                        .background(Color.white.opacity(0.12), in: Capsule())
                                                        .foregroundStyle(AppTheme.secondary)
                                                }
                                            }

                                            HStack(spacing: 6) {
                                                if let sub = model.subProvider {
                                                    Text(sub)
                                                }
                                                if !model.reasoningEfforts.isEmpty {
                                                    Text("· Reasoning")
                                                }
                                            }
                                            .font(.caption)
                                            .foregroundStyle(AppTheme.secondary)
                                        }

                                        Spacer()

                                        if isSelected {
                                            Image(systemName: "checkmark")
                                                .font(.system(size: 14, weight: .bold))
                                                .foregroundStyle(AppTheme.accent)
                                        }
                                    }
                                    .padding(.horizontal, 14)
                                    .padding(.vertical, 10)
                                    .background(
                                        isSelected ? Color.white.opacity(0.10) : Color.white.opacity(0.04),
                                        in: RoundedRectangle(cornerRadius: 12)
                                    )
                                    .overlay(
                                        RoundedRectangle(cornerRadius: 12)
                                            .stroke(isSelected ? AppTheme.accent.opacity(0.3) : Color.clear, lineWidth: 1)
                                    )
                                }
                                .buttonStyle(.plain)
                            }
                        }
                    }
                    .frame(maxHeight: 220)
                }
                .padding(.horizontal, 16)
                .padding(.vertical, 16)
            } else {
                // Effort Slider View (Matches file-96612d6b26bdce2542e9f6b5290d25b8.heic)
                VStack(spacing: 16) {
                    // Header row: ⚡ 5.5 Medium >
                    Button {
                        selectedProviderTab = session.provider
                        withAnimation(.easeInOut(duration: 0.2)) {
                            showingModelList = true
                        }
                    } label: {
                        HStack(spacing: 8) {
                            Image(systemName: "bolt")
                                .font(.system(size: 16, weight: .regular))
                                .foregroundStyle(.white)

                            Text(modelShortName)
                                .font(.system(size: 16, weight: .bold, design: .monospaced))
                                .foregroundStyle(.white)

                            Text(currentEffortLabel)
                                .font(.system(size: 16, weight: .regular, design: .monospaced))
                                .foregroundStyle(Color.white.opacity(0.6))

                            Image(systemName: "chevron.right")
                                .font(.system(size: 13, weight: .semibold))
                                .foregroundStyle(Color.white.opacity(0.5))
                        }
                        .frame(maxWidth: .infinity)
                    }
                    .buttonStyle(.plain)

                    // Discrete Effort Slider with Haptic Feedback
                    EffortSliderView(
                        options: effortOptions,
                        selectedEffort: session.reasoningEffort,
                        onSelect: onSelectEffort
                    )
                }
                .padding(.horizontal, 16)
                .padding(.top, 18)
                .padding(.bottom, 18)
            }
        }
        .frame(maxWidth: .infinity)
        .background(Color(red: 0.16, green: 0.16, blue: 0.18), in: RoundedRectangle(cornerRadius: 24))
        .overlay(RoundedRectangle(cornerRadius: 24).stroke(Color.white.opacity(0.12), lineWidth: 1))
        .shadow(color: Color.black.opacity(0.5), radius: 24, y: 8)
    }
}

private struct EffortSliderView: View {
    let options: [EffortOption]
    let selectedEffort: String?
    let onSelect: (EffortOption) -> Void

    @State private var dragLocationX: CGFloat? = nil

    private var selectedIndex: Int {
        guard let selectedEffort else { return 1 }
        if let idx = options.firstIndex(where: { $0.id.lowercased() == selectedEffort.lowercased() }) {
            return idx
        }
        return 1
    }

    var body: some View {
        GeometryReader { geo in
            let totalWidth = geo.size.width
            let totalSteps = max(options.count, 2)
            let inset: CGFloat = 6
            let trackHeight: CGFloat = 56
            let thumbHeight: CGFloat = trackHeight - 2 * inset // 44
            let thumbRadius = thumbHeight / 2 // 22
            let usableWidth = totalWidth - 2 * inset - thumbHeight
            let stepSpacing = usableWidth / CGFloat(totalSteps - 1)

            let activeIndex = dragIndex(totalWidth: totalWidth, stepSpacing: stepSpacing, inset: inset, thumbRadius: thumbRadius) ?? selectedIndex

            let activeCenterX = inset + thumbRadius + CGFloat(activeIndex) * stepSpacing
            let pillWidth = activeCenterX + thumbRadius - inset

            ZStack(alignment: .leading) {
                // 1. Dark capsule track
                Capsule()
                    .fill(Color(red: 0.10, green: 0.10, blue: 0.12))
                    .frame(height: trackHeight)

                // 2. White filled pill extending from left to current step
                Capsule()
                    .fill(Color.white)
                    .frame(width: max(pillWidth, thumbHeight), height: thumbHeight)
                    .offset(x: inset)

                // 3. Step Dots & Knob
                ForEach(0..<totalSteps, id: \.self) { i in
                    let cx = inset + thumbRadius + CGFloat(i) * stepSpacing
                    if i == activeIndex {
                        // Black circular knob at current active position
                        Circle()
                            .fill(Color.black)
                            .frame(width: 36, height: 36)
                            .position(x: cx, y: trackHeight / 2)
                    } else if i < activeIndex {
                        // Subtle dark dot inside white pill
                        Circle()
                            .fill(Color.black.opacity(0.25))
                            .frame(width: 6, height: 6)
                            .position(x: cx, y: trackHeight / 2)
                    } else {
                        // Subtle light dot on dark track
                        Circle()
                            .fill(Color.white.opacity(0.35))
                            .frame(width: 6, height: 6)
                            .position(x: cx, y: trackHeight / 2)
                    }
                }
            }
            .contentShape(Rectangle())
            .gesture(
                DragGesture(minimumDistance: 0)
                    .onChanged { value in
                        dragLocationX = value.location.x
                        let step = calculateStep(x: value.location.x, totalWidth: totalWidth, stepSpacing: stepSpacing, inset: inset, thumbRadius: thumbRadius, totalSteps: totalSteps)
                        if step != selectedIndex {
                            triggerHaptic()
                            onSelect(options[step])
                        }
                    }
                    .onEnded { value in
                        let step = calculateStep(x: value.location.x, totalWidth: totalWidth, stepSpacing: stepSpacing, inset: inset, thumbRadius: thumbRadius, totalSteps: totalSteps)
                        dragLocationX = nil
                        if step != selectedIndex {
                            triggerHaptic()
                            onSelect(options[step])
                        }
                    }
            )
        }
        .frame(height: 56)
    }

    private func dragIndex(totalWidth: CGFloat, stepSpacing: CGFloat, inset: CGFloat, thumbRadius: CGFloat) -> Int? {
        guard let dragLocationX else { return nil }
        return calculateStep(x: dragLocationX, totalWidth: totalWidth, stepSpacing: stepSpacing, inset: inset, thumbRadius: thumbRadius, totalSteps: options.count)
    }

    private func calculateStep(x: CGFloat, totalWidth: CGFloat, stepSpacing: CGFloat, inset: CGFloat, thumbRadius: CGFloat, totalSteps: Int) -> Int {
        let relativeX = x - inset - thumbRadius
        let rawStep = Int(round(relativeX / stepSpacing))
        return min(max(0, rawStep), totalSteps - 1)
    }

    private func triggerHaptic() {
        UIImpactFeedbackGenerator(style: .medium).impactOccurred()
    }
}

// MARK: - Other Popups (Permission & Plus)

private struct TooltipArrow: Shape {
    func path(in rect: CGRect) -> Path {
        var path = Path()
        path.move(to: CGPoint(x: rect.minX, y: rect.minY))
        path.addLine(to: CGPoint(x: rect.maxX, y: rect.minY))
        path.addLine(to: CGPoint(x: rect.midX, y: rect.maxY))
        path.closeSubpath()
        return path
    }
}

private struct PermissionPopupView: View {
    let currentMode: RuntimeMode
    let onSelect: (RuntimeMode) -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            VStack(alignment: .leading, spacing: 18) {
                // Option 1: Ask for approval
                Button {
                    onSelect(.ask)
                } label: {
                    HStack(alignment: .top, spacing: 14) {
                        Image(systemName: "hand.raised")
                            .font(.system(size: 17))
                            .foregroundStyle(.white)
                            .frame(width: 22, alignment: .center)

                        VStack(alignment: .leading, spacing: 3) {
                            Text("Ask for approval")
                                .font(.system(size: 15, weight: .regular, design: .monospaced))
                                .foregroundStyle(.white)
                            Text("Always ask to edit external files and use the internet")
                                .font(.system(size: 13))
                                .foregroundStyle(Color.white.opacity(0.55))
                                .fixedSize(horizontal: false, vertical: true)
                        }

                        Spacer(minLength: 8)

                        if currentMode == .ask {
                            Image(systemName: "checkmark")
                                .font(.system(size: 15, weight: .semibold))
                                .foregroundStyle(.white)
                        }
                    }
                }
                .buttonStyle(.plain)

                // Option 2: Approve for me
                Button {
                    onSelect(.auto)
                } label: {
                    HStack(alignment: .top, spacing: 14) {
                        Image(systemName: "chevron.left.forwardslash.chevron.right")
                            .font(.system(size: 15))
                            .foregroundStyle(.white)
                            .frame(width: 22, alignment: .center)

                        VStack(alignment: .leading, spacing: 3) {
                            Text("Approve for me")
                                .font(.system(size: 15, weight: .regular, design: .monospaced))
                                .foregroundStyle(.white)
                            Text("Only ask for actions detected as potentially unsafe")
                                .font(.system(size: 13))
                                .foregroundStyle(Color.white.opacity(0.55))
                                .fixedSize(horizontal: false, vertical: true)
                        }

                        Spacer(minLength: 8)

                        if currentMode == .auto || currentMode == .autoAcceptEdits {
                            Image(systemName: "checkmark")
                                .font(.system(size: 15, weight: .semibold))
                                .foregroundStyle(.white)
                        }
                    }
                }
                .buttonStyle(.plain)

                // Option 3: Full access
                Button {
                    onSelect(.fullAccess)
                } label: {
                    HStack(alignment: .top, spacing: 14) {
                        Image(systemName: "exclamationmark.octagon")
                            .font(.system(size: 17))
                            .foregroundStyle(.white)
                            .frame(width: 22, alignment: .center)

                        VStack(alignment: .leading, spacing: 3) {
                            Text("Full access")
                                .font(.system(size: 15, weight: .regular, design: .monospaced))
                                .foregroundStyle(.white)
                            Text("Full computer access (elevated risk)")
                                .font(.system(size: 13))
                                .foregroundStyle(Color.white.opacity(0.55))
                                .fixedSize(horizontal: false, vertical: true)
                        }

                        Spacer(minLength: 8)

                        if currentMode == .fullAccess {
                            Image(systemName: "checkmark")
                                .font(.system(size: 15, weight: .semibold))
                                .foregroundStyle(.white)
                        }
                    }
                }
                .buttonStyle(.plain)
            }
            .padding(.horizontal, 18)
            .padding(.vertical, 18)
            .frame(maxWidth: 320)
            .background(Color(red: 0.17, green: 0.17, blue: 0.19), in: RoundedRectangle(cornerRadius: 22))
            .overlay(RoundedRectangle(cornerRadius: 22).stroke(Color.white.opacity(0.12), lineWidth: 1))
            .shadow(color: Color.black.opacity(0.5), radius: 20, y: 8)

            // Pointer arrow pointing down to hand button
            TooltipArrow()
                .fill(Color(red: 0.17, green: 0.17, blue: 0.19))
                .frame(width: 18, height: 10)
                .offset(x: 48, y: -1)
        }
    }
}

private struct PlusPopupView: View {
    let onTakePhoto: () -> Void
    let onAddImages: ([UIImage]) -> Void

    @State private var selectedPhotoItems: [PhotosPickerItem] = []

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            VStack(alignment: .leading, spacing: 14) {
                // 1. Take a photo
                Button(action: onTakePhoto) {
                    HStack(spacing: 14) {
                        Image(systemName: "camera")
                            .font(.system(size: 16))
                            .foregroundStyle(.white)
                            .frame(width: 20)
                        Text("Take a photo")
                            .font(.system(size: 15, design: .monospaced))
                            .foregroundStyle(.white)
                    }
                }
                .buttonStyle(.plain)

                // 2. Photo library
                PhotosPicker(selection: $selectedPhotoItems, matching: .images) {
                    HStack(spacing: 14) {
                        Image(systemName: "photo.on.rectangle")
                            .font(.system(size: 16))
                            .foregroundStyle(.white)
                            .frame(width: 20)
                        Text("Photo library")
                            .font(.system(size: 15, design: .monospaced))
                            .foregroundStyle(.white)
                    }
                }
                .buttonStyle(.plain)
            }
            .padding(.horizontal, 18)
            .padding(.vertical, 16)
            .frame(width: 200)
            .background(Color(red: 0.17, green: 0.17, blue: 0.19), in: RoundedRectangle(cornerRadius: 20))
            .overlay(RoundedRectangle(cornerRadius: 20).stroke(Color.white.opacity(0.12), lineWidth: 1))
            .shadow(color: Color.black.opacity(0.5), radius: 20, y: 8)

            // Pointer arrow pointing down to plus button
            TooltipArrow()
                .fill(Color(red: 0.17, green: 0.17, blue: 0.19))
                .frame(width: 18, height: 10)
                .offset(x: 16, y: -1)
        }
        .onChange(of: selectedPhotoItems) { _, items in
            guard !items.isEmpty else { return }
            Task {
                var loaded: [UIImage] = []
                for item in items {
                    if let data = try? await item.loadTransferable(type: Data.self),
                       let image = UIImage(data: data) {
                        loaded.append(image)
                    }
                }
                await MainActor.run {
                    if !loaded.isEmpty {
                        onAddImages(loaded)
                    }
                    selectedPhotoItems = []
                }
            }
        }
    }
}

// MARK: - Camera Picker Representable

private struct CameraPicker: UIViewControllerRepresentable {
    @Binding var image: UIImage?
    @Environment(\.dismiss) private var dismiss

    func makeUIViewController(context: Context) -> UIImagePickerController {
        let picker = UIImagePickerController()
        if UIImagePickerController.isSourceTypeAvailable(.camera) {
            picker.sourceType = .camera
        } else {
            picker.sourceType = .photoLibrary
        }
        picker.delegate = context.coordinator
        return picker
    }

    func updateUIViewController(_ uiViewController: UIImagePickerController, context: Context) {}

    func makeCoordinator() -> Coordinator {
        Coordinator(self)
    }

    class Coordinator: NSObject, UIImagePickerControllerDelegate, UINavigationControllerDelegate {
        let parent: CameraPicker
        init(_ parent: CameraPicker) { self.parent = parent }

        func imagePickerController(_ picker: UIImagePickerController, didFinishPickingMediaWithInfo info: [UIImagePickerController.InfoKey: Any]) {
            if let image = info[.originalImage] as? UIImage {
                parent.image = image
            }
            parent.dismiss()
        }

        func imagePickerControllerDidCancel(_ picker: UIImagePickerController) {
            parent.dismiss()
        }
    }
}

// MARK: - Timeline & Message Views

private struct TimelineEntryView: View, Equatable {
    let entry: TranscriptEntry
    let images: AttachmentImageStore
    let onOpenImage: (UIImage, String) -> Void

    /// Only the row's own content can change what it draws. The store and the
    /// callback are shared by every row, and the image store invalidates the
    /// thumbnails that need it on its own.
    nonisolated static func == (lhs: TimelineEntryView, rhs: TimelineEntryView) -> Bool {
        lhs.entry == rhs.entry
    }

    var body: some View {
        switch entry {
        case .message(let message, let footerTime):
            MessageRow(
                message: message,
                footerTime: footerTime,
                images: images,
                onOpenImage: onOpenImage
            )
        case .block(let block):
            TranscriptBlockView(block: block)
        }
    }
}

private struct MessageRow: View, Equatable {
    let message: Message
    let footerTime: UInt64?
    let images: AttachmentImageStore
    let onOpenImage: (UIImage, String) -> Void

    nonisolated static func == (lhs: MessageRow, rhs: MessageRow) -> Bool {
        lhs.message == rhs.message && lhs.footerTime == rhs.footerTime
    }

    var body: some View {
        if message.role == .user {
            HStack(alignment: .bottom, spacing: 0) {
                Spacer(minLength: 44)
                VStack(alignment: .trailing, spacing: 8) {
                    if !message.attachments.isEmpty {
                        AttachmentStrip(
                            attachments: message.attachments,
                            images: images,
                            onOpen: { image, name in onOpenImage(image, name) }
                        )
                    }
                    if !message.visibleContent.isEmpty {
                        // Typed text is shown verbatim: rendering a half-typed
                        // prompt as Markdown would rewrite what the user wrote.
                        Text(message.visibleContent)
                            .font(.body)
                            .foregroundStyle(.white)
                            .multilineTextAlignment(.trailing)
                            .textSelection(.enabled)
                            .padding(.horizontal, 15)
                            .padding(.vertical, 11)
                            .background(AppTheme.raised, in: RoundedRectangle(cornerRadius: 18))
                    }
                }
            }
        } else {
            VStack(alignment: .leading, spacing: 4) {
                if message.visibleContent.isEmpty {
                    Color.clear.frame(height: 1)
                } else {
                    MessageText(markdown: message.visibleContent)
                        .opacity(message.streaming ? 0.94 : 1)
                }

                if let footerTime, message.role == .assistant {
                    AssistantMessageFooter(message: message, completedAt: footerTime)
                }
            }
        }
    }
}

private struct AssistantMessageFooter: View {
    let message: Message
    let completedAt: UInt64
    @State private var copied = false

    private var tokensPerSecond: UInt64 {
        let estimatedTokens = UInt64((message.content.count + 3) / 4)
        return estimatedTokens / completedAt.saturatingSubtracting(message.createdAt).clampedToAtLeastOne
    }

    var body: some View {
        HStack(spacing: 2) {
            Button {
                UIPasteboard.general.string = message.visibleContent
                copied = true
                Task { @MainActor in
                    try? await Task.sleep(for: .seconds(2))
                    copied = false
                }
            } label: {
                Image(systemName: copied ? "checkmark" : "doc.on.doc")
                    .font(.system(size: 13, weight: .medium))
                    .frame(width: 44, height: 44)
            }
            .buttonStyle(.plain)
            .accessibilityLabel(copied ? "Copied" : "Copy response")

            Text("\(formattedTime) • \(tokensPerSecond) tok/s")
                .font(.caption)
                .foregroundStyle(AppTheme.secondary)
                .monospacedDigit()
        }
        .foregroundStyle(AppTheme.secondary)
    }

    private var formattedTime: String {
        Date(timeIntervalSince1970: TimeInterval(completedAt))
            .formatted(date: .omitted, time: .shortened)
    }
}

private extension UInt64 {
    func saturatingSubtracting(_ other: UInt64) -> UInt64 {
        self >= other ? self - other : 0
    }

    var clampedToAtLeastOne: UInt64 { Swift.max(self, 1) }
}

struct BouncyButtonStyle: ButtonStyle {
    var scale: CGFloat = 0.97

    func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .scaleEffect(configuration.isPressed ? scale : 1.0)
            .opacity(configuration.isPressed ? 0.8 : 1.0)
            .animation(.spring(response: 0.22, dampingFraction: 0.75), value: configuration.isPressed)
    }
}

private struct WorkingIndicator: View {
    let session: AgentSession
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    var body: some View {
        TimelineView(.periodic(from: .now, by: reduceMotion ? 1 : 0.14)) { context in
            HStack(spacing: 8) {
                DotMatrixLoader(date: context.date, animated: !reduceMotion)
                Text(label(at: context.date))
                    .font(.subheadline)
                    .foregroundStyle(AppTheme.secondary)
            }
            .padding(.vertical, 3)
            .accessibilityElement(children: .combine)
        }
    }

    private func label(at date: Date) -> String {
        if session.status == .waiting {
            return "Waiting for you"
        }
        let startedAt = session.messages.last(where: { $0.role == .user })?.createdAt
            ?? session.lastReplyAt
            ?? UInt64(date.timeIntervalSince1970)
        let elapsed = UInt64(max(0, date.timeIntervalSince1970 - Double(startedAt)))
        return "Working for \(formatElapsed(elapsed))"
    }

    private func formatElapsed(_ seconds: UInt64) -> String {
        switch seconds {
        case 0..<60: return "\(seconds)s"
        case 60..<3_600:
            let minutes = seconds / 60
            let remainder = seconds % 60
            return remainder == 0 ? "\(minutes)m" : "\(minutes)m \(remainder)s"
        default:
            let hours = seconds / 3_600
            let minutes = seconds % 3_600 / 60
            return minutes == 0 ? "\(hours)h" : "\(hours)h \(minutes)m"
        }
    }
}

private struct DotMatrixLoader: View {
    let date: Date
    let animated: Bool

    private let columns = Array(repeating: GridItem(.fixed(3), spacing: 2), count: 3)

    var body: some View {
        let phase = animated ? Int(date.timeIntervalSinceReferenceDate / 0.14) % 9 : 4
        LazyVGrid(columns: columns, spacing: 2) {
            ForEach(0..<9, id: \.self) { index in
                Circle()
                    .fill(AppTheme.secondary)
                    .frame(width: 3, height: 3)
                    .opacity(opacity(for: index, phase: phase))
            }
        }
        .frame(width: 13, height: 13)
        .accessibilityHidden(true)
    }

    private func opacity(for index: Int, phase: Int) -> Double {
        let distance = (index - phase + 9) % 9
        return distance == 0 ? 1 : distance <= 2 ? 0.55 : 0.2
    }
}


/// Renders an agent reply as a Markdown document.
///
/// Textual parses the markup and lays it out with SwiftUI's own text engine, so
/// headings, lists, tables, code blocks and links keep native text selection,
/// Dynamic Type, and the platform's tap-to-open behaviour instead of arriving
/// as one flat `Text` with the syntax still visible.
struct MessageText: View {
    let markdown: String

    var body: some View {
        StructuredText(markdown: markdown)
            .textual.structuredTextStyle(ChatMarkdownStyle())
            .textual.highlighterTheme(.default)
            .textual.textSelection(.enabled)
            .font(.body)
            .foregroundStyle(Color.white)
            .frame(maxWidth: .infinity, alignment: .leading)
    }
}

/// Markdown styling tuned for a transcript: tighter than prose defaults, and
/// every surface chosen against the near-black chat background.
private struct ChatMarkdownStyle: StructuredText.Style {
    var inlineStyle: InlineStyle {
        InlineStyle()
            .code(
                .monospaced,
                .fontScale(0.86),
                .backgroundColor(Color.white.opacity(0.09)),
                .foregroundColor(Color(red: 0.82, green: 0.90, blue: 1))
            )
            .strong(.fontWeight(.semibold))
            .emphasis(.italic)
            .link(.foregroundColor(Color(red: 0.45, green: 0.76, blue: 1)))
    }

    var headingStyle: some StructuredText.HeadingStyle {
        ChatHeadingStyle()
    }

    var paragraphStyle: some StructuredText.ParagraphStyle {
        ChatParagraphStyle()
    }

    var blockQuoteStyle: some StructuredText.BlockQuoteStyle {
        ChatBlockQuoteStyle()
    }

    var codeBlockStyle: some StructuredText.CodeBlockStyle {
        ChatCodeBlockStyle()
    }

    var listItemStyle: some StructuredText.ListItemStyle {
        .default(markerSpacing: .fontScaled(0.45))
    }

    var unorderedListMarker: StructuredText.SymbolListMarker { .disc }

    var orderedListMarker: StructuredText.DecimalListMarker { .decimal }

    var tableStyle: some StructuredText.TableStyle { .default }

    var tableCellStyle: some StructuredText.TableCellStyle { .default }

    var thematicBreakStyle: some StructuredText.ThematicBreakStyle { .divider }
}

/// Prose sizes rather than the document defaults: an H1 in a chat bubble should
/// read as emphasis, not as a second title.
private struct ChatHeadingStyle: StructuredText.HeadingStyle {
    private static let fontScales: [CGFloat] = [1.6, 1.4, 1.22, 1.1, 1, 1]

    func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .textual.fontScale(Self.fontScales[min(configuration.headingLevel, 6) - 1])
            .textual.blockSpacing(.fontScaled(top: 0.9, bottom: 0.4))
            .fontWeight(.semibold)
            .foregroundStyle(Color.white)
    }
}

private struct ChatParagraphStyle: StructuredText.ParagraphStyle {
    func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .textual.lineSpacing(.fontScaled(0.2))
            .textual.blockSpacing(.fontScaled(top: 0.5))
    }
}

private struct ChatBlockQuoteStyle: StructuredText.BlockQuoteStyle {
    func makeBody(configuration: Configuration) -> some View {
        HStack(alignment: .top, spacing: 10) {
            Rectangle()
                .fill(Color.white.opacity(0.22))
                .frame(width: 3)
            configuration.label
                .frame(maxWidth: .infinity, alignment: .leading)
                .textual.lineSpacing(.fontScaled(0.2))
        }
        .textual.padding(.fontScaled(0.6))
        .foregroundStyle(AppTheme.secondary)
    }
}

/// Code gets a labelled header with a copy button, the way a terminal does,
/// instead of a bare grey slab with no way to lift the text out.
private struct ChatCodeBlockStyle: StructuredText.CodeBlockStyle {
    func makeBody(configuration: Configuration) -> some View {
        VStack(alignment: .leading, spacing: 0) {
            if configuration.languageHint != nil {
                HStack(spacing: 8) {
                    Text(configuration.languageHint ?? "")
                        .font(.system(size: 10.5, weight: .semibold, design: .monospaced))
                        .foregroundStyle(AppTheme.secondary)
                    Spacer(minLength: 8)
                    CodeCopyButton(codeBlock: configuration.codeBlock)
                }
                .padding(.horizontal, 12)
                .padding(.vertical, 7)
                .background(Color.white.opacity(0.04))
                .overlay(alignment: .bottom) {
                    Rectangle()
                        .fill(Color.white.opacity(0.07))
                        .frame(height: 1)
                }
            }

            Overflow {
                configuration.label
                    .textual.lineSpacing(.fontScaled(0.32))
                    .textual.fontScale(0.86)
                    .fixedSize(horizontal: false, vertical: true)
                    .monospaced()
                    .padding(.vertical, 10)
                    .padding(.horizontal, 12)
            }
        }
        .background(Color(red: 0.09, green: 0.09, blue: 0.11))
        .clipShape(RoundedRectangle(cornerRadius: 12, style: .continuous))
        .overlay(
            RoundedRectangle(cornerRadius: 12, style: .continuous)
                .stroke(Color.white.opacity(0.09), lineWidth: 1)
        )
        .textual.blockSpacing(.fontScaled(top: 0.7, bottom: 0))
    }
}

private struct CodeCopyButton: View {
    let codeBlock: StructuredText.CodeBlockProxy
    @State private var copied = false

    var body: some View {
        Button {
            codeBlock.copyToPasteboard()
            UIImpactFeedbackGenerator(style: .light).impactOccurred()
            withAnimation(.easeOut(duration: 0.15)) {
                copied = true
            }
            Task { @MainActor in
                try? await Task.sleep(for: .seconds(2))
                withAnimation(.easeOut(duration: 0.15)) {
                    copied = false
                }
            }
        } label: {
            HStack(spacing: 4) {
                Image(systemName: copied ? "checkmark" : "doc.on.doc")
                    .font(.system(size: 10, weight: .medium))
                if copied {
                    Text("Copied")
                        .font(.system(size: 10, weight: .medium))
                }
            }
            .foregroundStyle(copied ? AppTheme.accent : AppTheme.secondary)
            .padding(.horizontal, 8)
            .padding(.vertical, 4)
            .background(Color.white.opacity(0.06), in: Capsule())
        }
        .buttonStyle(.plain)
        .accessibilityLabel(copied ? "Copied" : "Copy code")
    }
}

/// The previews a sent image leaves behind in the transcript. One row per
/// attachment, above the message text, in the order it was sent.
/// The previews a sent image leaves behind in the transcript: one line of
/// thumbnails, wrapped, sitting directly above the message they belong to.
struct AttachmentStrip: View {
    let attachments: [MessageAttachment]
    let images: AttachmentImageStore
    let onOpen: (UIImage, String) -> Void

    var body: some View {
        FlowLayout(spacing: 8, lineSpacing: 8) {
            ForEach(attachments) { attachment in
                AttachmentThumbnail(
                    attachment: attachment,
                    images: images,
                    onOpen: onOpen
                )
            }
        }
        .padding(.vertical, 2)
    }
}

/// Wraps onto as many lines as the thumbnails need and reports the width it
/// actually used, so the strip hugs its pictures instead of stretching across
/// the transcript and parking them on the far side.
private struct FlowLayout: Layout {
    var spacing: CGFloat = 8
    var lineSpacing: CGFloat = 8

    func sizeThatFits(proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) -> CGSize {
        let limit = proposal.width ?? .infinity
        var rowWidth: CGFloat = 0
        var rowHeight: CGFloat = 0
        var height: CGFloat = 0
        var width: CGFloat = 0
        for subview in subviews {
            let size = subview.sizeThatFits(.unspecified)
            let needed = rowWidth == 0 ? size.width : rowWidth + spacing + size.width
            if needed > limit, rowWidth > 0 {
                width = max(width, rowWidth)
                height += rowHeight + lineSpacing
                rowWidth = size.width
                rowHeight = size.height
            } else {
                rowWidth = needed
                rowHeight = max(rowHeight, size.height)
            }
        }
        return CGSize(width: max(width, rowWidth), height: height + rowHeight)
    }

    func placeSubviews(in bounds: CGRect, proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) {
        var origin = bounds.minX
        var rowTop = bounds.minY
        var rowHeight: CGFloat = 0
        for subview in subviews {
            let size = subview.sizeThatFits(.unspecified)
            let needed = origin == bounds.minX ? size.width : origin - bounds.minX + spacing + size.width
            if needed > bounds.maxX, origin > bounds.minX {
                rowTop += rowHeight + lineSpacing
                origin = bounds.minX
                rowHeight = 0
            } else if origin > bounds.minX {
                origin += spacing
            }
            subview.place(at: CGPoint(x: origin, y: rowTop), anchor: .topLeading, proposal: ProposedViewSize(size))
            origin += size.width
            rowHeight = max(rowHeight, size.height)
        }
    }
}

private struct AttachmentThumbnail: View {
    let attachment: MessageAttachment
    let images: AttachmentImageStore
    let onOpen: (UIImage, String) -> Void

    private let maxSide: CGFloat = 132
    private let minSide: CGFloat = 84

    /// Image bytes only exist on the Mac, so the preview is fetched by blob
    /// reference and this cell is the only thing that waits on it.
    private var reference: String? {
        attachment.isImage ? attachment.blobReference : nil
    }

    private var image: UIImage? {
        reference.flatMap { images.image(for: $0) }
    }

    /// Fits the picture inside a bounded box, keeping a floor on the height so a
    /// wide screenshot stays legible instead of collapsing to a sliver.
    private var fitted: CGSize {
        guard let image, image.size.width > 0, image.size.height > 0 else {
            return CGSize(width: minSide, height: minSide)
        }
        let factor = min(maxSide / image.size.width, maxSide / image.size.height)
        return CGSize(
            width: min(maxSide, max(minSide * 0.75, image.size.width * factor)),
            height: min(maxSide, max(minSide, image.size.height * factor))
        )
    }

    var body: some View {
        Button {
            guard let image else {
                // The bytes have not arrived yet. Fetch them, then open.
                guard let reference else { return }
                images.load(reference)
                Task { @MainActor in
                    for _ in 0..<40 {
                        try? await Task.sleep(for: .milliseconds(50))
                        if let loaded = images.image(for: reference) {
                            onOpen(loaded, attachment.name)
                            return
                        }
                    }
                }
                return
            }
            onOpen(image, attachment.name)
        } label: {
            content
        }
        .buttonStyle(BouncyButtonStyle(scale: 0.94))
        .onAppear {
            if let reference { images.load(reference) }
        }
        .accessibilityLabel(attachment.name)
        .accessibilityHint("Double tap to expand")
    }

    @ViewBuilder
    private var content: some View {
        if let image {
            Image(uiImage: image)
                .resizable()
                .scaledToFill()
                .frame(width: fitted.width, height: fitted.height)
                .clipShape(RoundedRectangle(cornerRadius: 14, style: .continuous))
                .overlay(
                    RoundedRectangle(cornerRadius: 14, style: .continuous)
                        .stroke(Color.white.opacity(0.14), lineWidth: 1)
                )
        } else {
            placeholder
        }
    }

    private var placeholder: some View {
        ZStack {
            RoundedRectangle(cornerRadius: 14, style: .continuous)
                .fill(AppTheme.raised)
            VStack(spacing: 5) {
                Image(systemName: reference == nil ? "doc.text" : "photo")
                    .font(.system(size: 18, weight: .regular))
                    .foregroundStyle(AppTheme.secondary)
                Text(attachment.name)
                    .font(.system(size: 9.5))
                    .foregroundStyle(AppTheme.secondary)
                    .lineLimit(2)
                    .multilineTextAlignment(.center)
                    .padding(.horizontal, 6)
            }
        }
        .frame(width: minSide, height: minSide)
        .overlay(
            RoundedRectangle(cornerRadius: 14, style: .continuous)
                .stroke(Color.white.opacity(0.10), lineWidth: 1)
        )
    }
}

/// A single attachment opened full screen: pinch or double tap to zoom, drag to
/// pan, and drag down to dismiss.
struct AttachmentViewer: View {
    let image: UIImage
    let name: String

    @Environment(\.dismiss) private var dismiss

    var body: some View {
        ZStack(alignment: .topTrailing) {
            Color.black
                .ignoresSafeArea()

            ZoomableAttachmentImage(image: image, onDismiss: { dismiss() })
                .accessibilityAddTraits(.isImage)
                .accessibilityLabel(name)

            Button {
                dismiss()
            } label: {
                Image(systemName: "xmark")
                    .font(.system(size: 15, weight: .bold))
                    .foregroundStyle(.white)
                    .frame(width: 44, height: 44)
                    .background(.ultraThinMaterial, in: Circle())
                    .overlay(Circle().strokeBorder(Color.white.opacity(0.16), lineWidth: 1))
            }
            .buttonStyle(BouncyButtonStyle(scale: 0.92))
            .padding(.trailing, 16)
            .padding(.top, 8)
            .accessibilityLabel("Close")
        }
        .accessibilityAddTraits(.isImage)
        .accessibilityLabel(name)
    }
}

/// The image itself, with the gesture state it needs. Split out of the viewer so
/// the gestures can be built where the geometry they read is in scope.
private struct ZoomableAttachmentImage: View {
    let image: UIImage
    let onDismiss: () -> Void

    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    @State private var scale: CGFloat = 1
    @State private var settledScale: CGFloat = 1
    @State private var offset: CGSize = .zero
    @State private var settledOffset: CGSize = .zero
    @State private var dismissOffset: CGFloat = 0

    private let maximumScale: CGFloat = 6
    private let doubleTapScale: CGFloat = 2.5

    var body: some View {
        GeometryReader { proxy in
            let size = proxy.size
            Image(uiImage: image)
                .resizable()
                .aspectRatio(contentMode: .fit)
                .scaleEffect(scale)
                .offset(x: offset.width, y: offset.height + dismissOffset)
                .frame(width: size.width, height: size.height)
                .contentShape(Rectangle())
                .gesture(
                    DragGesture()
                        .onChanged { value in
                            if settledScale > 1.01 {
                                offset = CGSize(
                                    width: settledOffset.width + value.translation.width,
                                    height: settledOffset.height + value.translation.height
                                )
                            } else {
                                dismissOffset = value.translation.height
                            }
                        }
                        .onEnded { value in
                            if settledScale > 1.01 {
                                settlePanned(in: size)
                                return
                            }
                            let travels = abs(value.translation.height) > 120
                                || abs(value.predictedEndTranslation.height) > 260
                            if travels {
                                finish()
                            } else {
                                withAnimation(reduceMotion ? nil : .spring(response: 0.3, dampingFraction: 0.85)) {
                                    dismissOffset = 0
                                }
                            }
                        }
                )
                .simultaneousGesture(
                    MagnifyGesture()
                        .onChanged { value in
                            scale = min(max(settledScale * value.magnification, 1), maximumScale)
                        }
                        .onEnded { _ in
                            if scale <= 1.02 {
                                settleZoomedOut()
                            } else {
                                settledScale = scale
                            }
                        }
                )
                .simultaneousGesture(
                    SpatialTapGesture(count: 2).onEnded { value in
                        toggleZoom(at: value.location, in: size)
                    }
                )
        }
        .offset(y: dismissOffset)
        .opacity(1 - min(abs(dismissOffset) / 320, 0.6))
    }

    /// Keeps a zoomed image inside its frame, so panning can never strand it off
    /// screen where no gesture is left to bring it back.
    private func settlePanned(in size: CGSize) {
        let imageSize = fittedSize(in: size)
        let maxX = max(0, (imageSize.width * settledScale - size.width) / 2)
        let maxY = max(0, (imageSize.height * settledScale - size.height) / 2)
        let clamped = CGSize(
            width: min(max(offset.width, -maxX), maxX),
            height: min(max(offset.height, -maxY), maxY)
        )
        withAnimation(reduceMotion ? nil : .spring(response: 0.28, dampingFraction: 0.9)) {
            offset = clamped
            settledOffset = clamped
        }
    }

    /// Zooms around the tapped point so the detail under the finger is the
    /// detail that gets bigger.
    private func toggleZoom(at location: CGPoint, in size: CGSize) {
        UIImpactFeedbackGenerator(style: .light).impactOccurred()
        if settledScale > 1.01 {
            settleZoomedOut()
            return
        }
        let center = CGPoint(x: size.width / 2, y: size.height / 2)
        let anchor = CGPoint(
            x: (location.x - center.x - settledOffset.width) / settledScale + center.x,
            y: (location.y - center.y - settledOffset.height) / settledScale + center.y
        )
        let zoomed = CGSize(
            width: (anchor.x - center.x) * (1 - doubleTapScale),
            height: (anchor.y - center.y) * (1 - doubleTapScale)
        )
        withAnimation(reduceMotion ? nil : .spring(response: 0.32, dampingFraction: 0.85)) {
            scale = doubleTapScale
            settledScale = doubleTapScale
            offset = zoomed
            settledOffset = zoomed
        }
    }

    private func settleZoomedOut() {
        withAnimation(reduceMotion ? nil : .spring(response: 0.3, dampingFraction: 0.9)) {
            scale = 1
            settledScale = 1
            offset = .zero
            settledOffset = .zero
        }
    }

    private func fittedSize(in size: CGSize) -> CGSize {
        let factor = min(size.width / image.size.width, size.height / image.size.height)
        return CGSize(width: image.size.width * factor, height: image.size.height * factor)
    }

    private func finish() {
        withAnimation(reduceMotion ? nil : .easeOut(duration: 0.22)) {
            dismissOffset = 600
        }
        Task { @MainActor in
            try? await Task.sleep(for: .milliseconds(180))
            onDismiss()
        }
    }
}

// MARK: - Activity & Transcript Views

extension ActivityItem {
    var actionPastVerb: String {
        let k = kind.lowercased()
        if k == "command" || k == "bash" {
            return "Ran"
        } else if k == "file_read" || k == "fileread" || k == "read" {
            return "Read"
        } else if k == "file_change" || k == "filechange" || k == "edit" {
            return "Edited"
        } else if k == "file_search" || k == "filesearch" || k == "search" {
            return "Searched"
        } else if k == "file_list" || k == "filelist" || k == "list" {
            return "Listed"
        } else if k == "reasoning" || k == "think" {
            return "Thought"
        } else if k == "plan" {
            return "Updated plan"
        } else if !title.isEmpty && title != "Tool" && title != "Activity" {
            return title
        } else {
            return "Tool"
        }
    }

    var actionActiveVerb: String {
        let k = kind.lowercased()
        if k == "command" || k == "bash" {
            return "Running"
        } else if k == "file_read" || k == "fileread" || k == "read" {
            return "Reading"
        } else if k == "file_change" || k == "filechange" || k == "edit" {
            return "Editing"
        } else if k == "file_search" || k == "filesearch" || k == "search" {
            return "Searching"
        } else if k == "file_list" || k == "filelist" || k == "list" {
            return "Listing"
        } else if k == "reasoning" || k == "think" {
            return "Thinking"
        } else if k == "plan" {
            return "Planning"
        } else {
            return "Running"
        }
    }

    var iconName: String {
        let k = kind.lowercased()
        if k == "command" || k == "bash" {
            return "terminal"
        } else if k == "file_read" || k == "fileread" || k == "read" {
            return "doc.text"
        } else if k == "file_change" || k == "filechange" || k == "edit" {
            return "pencil"
        } else if k == "file_search" || k == "filesearch" || k == "search" {
            return "magnifyingglass"
        } else if k == "file_list" || k == "filelist" || k == "list" {
            return "folder"
        } else if k == "reasoning" || k == "think" {
            return "sparkles"
        } else if k == "plan" {
            return "list.bullet"
        } else {
            return "wrench.and.screwdriver"
        }
    }

    var displayParam: String? {
        if let target = displayTarget, !target.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
            return target
        }
        if let desc = displayDescription, !desc.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
            return desc
        }
        if let firstChange = fileChanges?.first, !firstChange.path.isEmpty {
            return firstChange.path
        }
        if let args = arguments, !args.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
            let first = args.components(separatedBy: .newlines).first(where: { !$0.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty }) ?? args
            return first.trimmingCharacters(in: .whitespacesAndNewlines)
        }
        if let d = detail, !d.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty,
           d.lowercased() != "completed" && d.lowercased() != "failed" {
            let first = d.components(separatedBy: .newlines).first(where: { !$0.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty }) ?? d
            return first.trimmingCharacters(in: .whitespacesAndNewlines)
        }
        return nil
    }

    var diffStats: (additions: UInt64, deletions: UInt64)? {
        guard let changes = fileChanges, !changes.isEmpty else { return nil }
        var adds: UInt64 = 0
        var dels: UInt64 = 0
        var hasStats = false
        for change in changes {
            if let a = change.additions {
                adds &+= a
                hasStats = true
            }
            if let d = change.deletions {
                dels &+= d
                hasStats = true
            }
        }
        return hasStats ? (adds, dels) : nil
    }

    var hasDetailContent: Bool {
        if failed { return true }
        if let output, !output.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty { return true }
        if let arguments, !arguments.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty { return true }
        if let diff = fileChanges?.first?.diff, !diff.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty { return true }
        if let reasoning, !reasoning.content.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty { return true }
        if let detail, !detail.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty,
           detail.lowercased() != "completed" && detail.lowercased() != "failed" {
            return true
        }
        return false
    }
}

private struct ActivityCopyButton: View {
    let text: String
    @State private var copied = false

    var body: some View {
        Button {
            UIPasteboard.general.string = text
            UIImpactFeedbackGenerator(style: .light).impactOccurred()
            withAnimation(.easeInOut(duration: 0.15)) {
                copied = true
            }
            Task { @MainActor in
                try? await Task.sleep(for: .seconds(2))
                withAnimation(.easeInOut(duration: 0.15)) {
                    copied = false
                }
            }
        } label: {
            HStack(spacing: 3) {
                Image(systemName: copied ? "checkmark" : "doc.on.doc")
                    .font(.system(size: 10, weight: .medium))
                if copied {
                    Text("Copied")
                        .font(.system(size: 9.5, weight: .medium))
                }
            }
            .foregroundStyle(copied ? Color(red: 0.35, green: 0.85, blue: 0.45) : Color.white.opacity(0.45))
            .padding(.horizontal, 6)
            .padding(.vertical, 3)
            .background(Color.white.opacity(0.06), in: RoundedRectangle(cornerRadius: 5))
        }
        .buttonStyle(.plain)
    }
}

private struct DiffLineRow: View {
    let line: String

    private var isAdd: Bool { line.hasPrefix("+") && !line.hasPrefix("+++") }
    private var isDel: Bool { line.hasPrefix("-") && !line.hasPrefix("---") }
    private var isMeta: Bool { line.hasPrefix("@@") || line.hasPrefix("diff") || line.hasPrefix("index") }

    var body: some View {
        Text(line)
            .font(.system(size: 11, design: .monospaced))
            .foregroundStyle(textColor)
            .frame(maxWidth: .infinity, alignment: .leading)
            .padding(.horizontal, 6)
            .padding(.vertical, 1)
            .background(bgColor)
    }

    private var textColor: Color {
        if isAdd { return Color(red: 0.35, green: 0.9, blue: 0.5) }
        if isDel { return Color(red: 0.95, green: 0.4, blue: 0.4) }
        if isMeta { return Color.cyan.opacity(0.8) }
        return Color.white.opacity(0.65)
    }

    private var bgColor: Color {
        if isAdd { return Color.green.opacity(0.12) }
        if isDel { return Color.red.opacity(0.12) }
        return Color.clear
    }
}

private struct ActivityDiffView: View {
    let diff: String

    var body: some View {
        VStack(alignment: .leading, spacing: 1) {
            ForEach(Array(diff.components(separatedBy: .newlines).prefix(60).enumerated()), id: \.offset) { _, line in
                DiffLineRow(line: line)
            }
        }
        .padding(.vertical, 3)
        .background(Color.black.opacity(0.35), in: RoundedRectangle(cornerRadius: 6))
        .overlay(RoundedRectangle(cornerRadius: 6).stroke(Color.white.opacity(0.05), lineWidth: 0.5))
    }
}

private struct ReasoningBlockView: View {
    let reasoning: ReasoningBlock
    @State private var expanded = false

    private var isThinking: Bool {
        reasoning.finishedAt == 0
    }

    private var durationString: String? {
        guard reasoning.finishedAt > reasoning.startedAt else { return nil }
        let ms = reasoning.finishedAt - reasoning.startedAt
        let secs = max(1, (ms + 500) / 1000)
        if secs < 60 {
            return "\(secs)s"
        } else {
            return "\(secs / 60)m \(secs % 60)s"
        }
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            Button {
                UIImpactFeedbackGenerator(style: .light).impactOccurred()
                withAnimation(.spring(response: 0.28, dampingFraction: 0.82)) {
                    expanded.toggle()
                }
            } label: {
                HStack(spacing: 7) {
                    Image(systemName: "brain")
                        .font(.system(size: 12, weight: .medium))
                        .foregroundStyle(isThinking ? AppTheme.accent : Color.white.opacity(0.5))

                    if isThinking {
                        Text("Thinking…")
                            .font(.system(size: 13, weight: .medium))
                            .foregroundStyle(Color.white.opacity(0.75))
                    } else if let dur = durationString {
                        Text("Thought for \(dur)")
                            .font(.system(size: 13, weight: .regular))
                            .foregroundStyle(Color.white.opacity(0.55))
                    } else {
                        Text("Thought")
                            .font(.system(size: 13, weight: .regular))
                            .foregroundStyle(Color.white.opacity(0.55))
                    }

                    Image(systemName: "chevron.right")
                        .font(.system(size: 9.5, weight: .semibold))
                        .foregroundStyle(Color.white.opacity(0.25))
                        .rotationEffect(.degrees(expanded ? 90 : 0))
                        .padding(.leading, 1)

                    Spacer()
                }
                .contentShape(Rectangle())
            }
            .buttonStyle(BouncyButtonStyle(scale: 0.99))

            if expanded {
                HStack(alignment: .top, spacing: 10) {
                    RoundedRectangle(cornerRadius: 1)
                        .fill(Color.white.opacity(0.12))
                        .frame(width: 2)
                        .padding(.vertical, 2)

                    VStack(alignment: .leading, spacing: 8) {
                        HStack {
                            Text("THINKING PROCESS")
                                .font(.system(size: 9.5, weight: .semibold, design: .monospaced))
                                .foregroundStyle(Color.white.opacity(0.35))
                            Spacer()
                            ActivityCopyButton(text: reasoning.content)
                        }

                        ScrollView(showsIndicators: false) {
                            Text(reasoning.content)
                                .font(.system(size: 13, weight: .regular))
                                .lineSpacing(3.5)
                                .foregroundStyle(Color.white.opacity(0.65))
                                .textSelection(.enabled)
                                .frame(maxWidth: .infinity, alignment: .leading)
                        }
                        .scrollIndicators(.hidden)
                        .frame(maxHeight: 280)
                    }
                }
                .padding(.leading, 4)
                .padding(.top, 2)
                .transition(.opacity.combined(with: .move(edge: .top)))
            }
        }
    }
}

private struct ActivityRow: View {
    let activity: ActivityItem
    @State private var expanded = false

    private var hasDetail: Bool {
        activity.hasDetailContent
    }

    private var isRunning: Bool {
        !activity.complete && !activity.failed
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            // Minimal Inline Trigger Row
            Button {
                guard hasDetail else { return }
                UIImpactFeedbackGenerator(style: .light).impactOccurred()
                withAnimation(.spring(response: 0.28, dampingFraction: 0.82)) {
                    expanded.toggle()
                }
            } label: {
                HStack(spacing: 7) {
                    // Leading Icon
                    Image(systemName: activity.failed ? "xmark.circle" : activity.iconName)
                        .font(.system(size: 11.5, weight: .medium))
                        .foregroundStyle(
                            activity.failed ? Color(red: 0.95, green: 0.4, blue: 0.4) :
                            isRunning ? AppTheme.accent :
                            Color.white.opacity(0.4)
                        )
                        .frame(width: 14)

                    // Text Content
                    if activity.failed {
                        let errorMsg = activity.output ?? activity.detail ?? activity.title
                        let firstLine = errorMsg.components(separatedBy: .newlines).first(where: { !$0.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty }) ?? errorMsg
                        Text("Failed: \(firstLine)")
                            .font(.system(size: 13, weight: .regular))
                            .foregroundStyle(Color(red: 0.95, green: 0.4, blue: 0.4))
                            .lineLimit(1)
                            .truncationMode(.tail)
                    } else if isRunning {
                        let target = activity.displayParam ?? ""
                        Text(target.isEmpty ? "\(activity.actionActiveVerb)…" : "\(activity.actionActiveVerb) \(target)…")
                            .font(.system(size: 13, weight: .medium))
                            .foregroundStyle(Color.white.opacity(0.8))
                            .lineLimit(1)
                            .truncationMode(.middle)
                    } else {
                        // Completed minimal inline summary
                        HStack(spacing: 5) {
                            Text(activity.actionPastVerb)
                                .font(.system(size: 13, weight: .regular))
                                .foregroundStyle(Color.white.opacity(0.5))

                            if let param = activity.displayParam {
                                Text(param)
                                    .font(.system(size: 12.5, design: .monospaced))
                                    .foregroundStyle(Color.white.opacity(0.85))
                                    .lineLimit(1)
                                    .truncationMode(.middle)
                            }

                            if let stats = activity.diffStats {
                                HStack(spacing: 3) {
                                    if stats.additions > 0 {
                                        Text("+\(stats.additions)")
                                            .font(.system(size: 11.5, weight: .medium, design: .monospaced))
                                            .foregroundStyle(Color(red: 0.35, green: 0.85, blue: 0.45))
                                    }
                                    if stats.deletions > 0 {
                                        Text("-\(stats.deletions)")
                                            .font(.system(size: 11.5, weight: .medium, design: .monospaced))
                                            .foregroundStyle(Color(red: 0.95, green: 0.4, blue: 0.4))
                                    }
                                }
                            }
                        }
                    }

                    if hasDetail {
                        Image(systemName: "chevron.right")
                            .font(.system(size: 9.5, weight: .semibold))
                            .foregroundStyle(Color.white.opacity(0.25))
                            .rotationEffect(.degrees(expanded ? 90 : 0))
                            .padding(.leading, 1)
                    }

                    Spacer()
                }
                .contentShape(Rectangle())
            }
            .buttonStyle(BouncyButtonStyle(scale: 0.99))

            // Expanded Dropdown Detail
            if expanded && hasDetail {
                HStack(alignment: .top, spacing: 10) {
                    RoundedRectangle(cornerRadius: 1)
                        .fill(Color.white.opacity(0.12))
                        .frame(width: 2)
                        .padding(.vertical, 2)

                    VStack(alignment: .leading, spacing: 8) {
                        // Error details if failed
                        if activity.failed {
                            let errorMsg = activity.output ?? activity.detail ?? activity.title
                            VStack(alignment: .leading, spacing: 4) {
                                Text("ERROR")
                                    .font(.system(size: 9.5, weight: .bold, design: .monospaced))
                                    .foregroundStyle(Color(red: 0.95, green: 0.4, blue: 0.4))

                                Text(errorMsg)
                                    .font(.system(size: 11.5, design: .monospaced))
                                    .foregroundStyle(Color(red: 0.95, green: 0.65, blue: 0.65))
                                    .textSelection(.enabled)
                                    .padding(8)
                                    .frame(maxWidth: .infinity, alignment: .leading)
                                    .background(Color.red.opacity(0.08), in: RoundedRectangle(cornerRadius: 6))
                                    .overlay(RoundedRectangle(cornerRadius: 6).stroke(Color.red.opacity(0.18), lineWidth: 0.5))
                            }
                        }

                        // Command details
                        let isCommand = activity.kind.lowercased() == "command" || activity.kind.lowercased() == "bash"
                        if isCommand, let cmd = activity.arguments ?? activity.displayTarget, !cmd.isEmpty {
                            VStack(alignment: .leading, spacing: 4) {
                                HStack {
                                    Text("COMMAND")
                                        .font(.system(size: 9.5, weight: .semibold, design: .monospaced))
                                        .foregroundStyle(Color.white.opacity(0.35))
                                    Spacer()
                                    ActivityCopyButton(text: cmd)
                                }

                                ScrollView(.horizontal, showsIndicators: false) {
                                    HStack(spacing: 5) {
                                        Text("$")
                                            .font(.system(size: 11.5, weight: .bold, design: .monospaced))
                                            .foregroundStyle(Color.white.opacity(0.35))
                                        Text(cmd)
                                            .font(.system(size: 11.5, design: .monospaced))
                                            .foregroundStyle(Color.white.opacity(0.85))
                                            .textSelection(.enabled)
                                    }
                                    .padding(.horizontal, 8)
                                    .padding(.vertical, 6)
                                }
                                .scrollIndicators(.hidden)
                                .background(Color.white.opacity(0.03), in: RoundedRectangle(cornerRadius: 6))
                                .overlay(RoundedRectangle(cornerRadius: 6).stroke(Color.white.opacity(0.06), lineWidth: 0.5))
                            }
                        }

                        // Diff (for edits)
                        if let diff = activity.fileChanges?.first?.diff, !diff.isEmpty {
                            VStack(alignment: .leading, spacing: 4) {
                                HStack {
                                    Text("CHANGES")
                                        .font(.system(size: 9.5, weight: .semibold, design: .monospaced))
                                        .foregroundStyle(Color.white.opacity(0.35))
                                    Spacer()
                                    ActivityCopyButton(text: diff)
                                }

                                ScrollView([.horizontal, .vertical], showsIndicators: false) {
                                    ActivityDiffView(diff: diff)
                                }
                                .scrollIndicators(.hidden)
                                .frame(maxHeight: 240)
                            }
                        }

                        // Arguments (for other tools)
                        if !isCommand, let args = activity.arguments, !args.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
                            VStack(alignment: .leading, spacing: 4) {
                                HStack {
                                    Text("ARGUMENTS")
                                        .font(.system(size: 9.5, weight: .semibold, design: .monospaced))
                                        .foregroundStyle(Color.white.opacity(0.35))
                                    Spacer()
                                    ActivityCopyButton(text: args)
                                }

                                ScrollView([.horizontal, .vertical], showsIndicators: false) {
                                    Text(args)
                                        .font(.system(size: 11.5, design: .monospaced))
                                        .foregroundStyle(Color.white.opacity(0.7))
                                        .textSelection(.enabled)
                                        .padding(8)
                                        .frame(maxWidth: .infinity, alignment: .leading)
                                }
                                .scrollIndicators(.hidden)
                                .frame(maxHeight: 180)
                                .background(Color.white.opacity(0.03), in: RoundedRectangle(cornerRadius: 6))
                                .overlay(RoundedRectangle(cornerRadius: 6).stroke(Color.white.opacity(0.06), lineWidth: 0.5))
                            }
                        }

                        // Output
                        if let output = activity.output, !output.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
                            VStack(alignment: .leading, spacing: 4) {
                                HStack {
                                    Text("OUTPUT")
                                        .font(.system(size: 9.5, weight: .semibold, design: .monospaced))
                                        .foregroundStyle(Color.white.opacity(0.35))
                                    Spacer()
                                    ActivityCopyButton(text: output)
                                }

                                ScrollView([.horizontal, .vertical], showsIndicators: false) {
                                    Text(output)
                                        .font(.system(size: 11.5, design: .monospaced))
                                        .foregroundStyle(Color.white.opacity(0.75))
                                        .textSelection(.enabled)
                                        .padding(8)
                                        .frame(maxWidth: .infinity, alignment: .leading)
                                }
                                .scrollIndicators(.hidden)
                                .frame(maxHeight: 220)
                                .background(Color.white.opacity(0.03), in: RoundedRectangle(cornerRadius: 6))
                                .overlay(RoundedRectangle(cornerRadius: 6).stroke(Color.white.opacity(0.06), lineWidth: 0.5))
                            }
                        } else if !isCommand, activity.arguments == nil, let d = activity.detail,
                                  !d.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty,
                                  d.lowercased() != "completed" && d.lowercased() != "failed" {
                            VStack(alignment: .leading, spacing: 4) {
                                HStack {
                                    Text("DETAIL")
                                        .font(.system(size: 9.5, weight: .semibold, design: .monospaced))
                                        .foregroundStyle(Color.white.opacity(0.35))
                                    Spacer()
                                    ActivityCopyButton(text: d)
                                }

                                ScrollView([.horizontal, .vertical], showsIndicators: false) {
                                    Text(d)
                                        .font(.system(size: 11.5, design: .monospaced))
                                        .foregroundStyle(Color.white.opacity(0.75))
                                        .textSelection(.enabled)
                                        .padding(8)
                                        .frame(maxWidth: .infinity, alignment: .leading)
                                }
                                .scrollIndicators(.hidden)
                                .frame(maxHeight: 180)
                                .background(Color.white.opacity(0.03), in: RoundedRectangle(cornerRadius: 6))
                                .overlay(RoundedRectangle(cornerRadius: 6).stroke(Color.white.opacity(0.06), lineWidth: 0.5))
                            }
                        }
                    }
                }
                .padding(.leading, 4)
                .padding(.top, 2)
                .transition(.opacity.combined(with: .move(edge: .top)))
            }
        }
    }
}

private struct TranscriptBlockView: View {
    let block: TranscriptBlock

    var body: some View {
        switch block.content {
        case .reasoning(let reasoning):
            ReasoningBlockView(reasoning: reasoning)
                .padding(.vertical, 4)
        case .activities(let activities):
            VStack(alignment: .leading, spacing: 9) {
                ForEach(activities) { activity in
                    ActivityRow(activity: activity)
                }
            }
            .padding(.vertical, 4)
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
