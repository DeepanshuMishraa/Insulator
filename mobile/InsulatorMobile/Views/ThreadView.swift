import SwiftUI
import PhotosUI
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
    @State private var selectedPhotos: [PhotosPickerItem] = []
    @State private var attachedImages: [UIImage] = []
    @State private var showsCamera = false
    @State private var cameraImage: UIImage? = nil
    @State private var showsRename = false
    @State private var renameText = ""
    @State private var confirmsDelete = false
    @State private var scrollTask: Task<Void, Never>?

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

                    ZStack(alignment: .bottom) {
                        ScrollViewReader { proxy in
                            ScrollView {
                                LazyVStack(alignment: .leading, spacing: 16) {
                                    ForEach(timeline(for: session)) { entry in
                                        TimelineEntryView(entry: entry)
                                            .id(entry.id)
                                    }
                                    if session.status.isBusy {
                                        WorkingIndicator(session: session)
                                    }
                                    Color.clear
                                        .frame(height: 1)
                                        .id("thread-bottom")
                                }
                                .padding(.horizontal, 16)
                                .padding(.top, 16)
                                .padding(.bottom, 160)
                            }
                            .onChange(of: app.transcriptRevision) { _, _ in
                                scheduleScroll(using: proxy)
                            }
                            .task {
                                await Task.yield()
                                proxy.scrollTo("thread-bottom", anchor: .bottom)
                            }
                            .onDisappear {
                                scrollTask?.cancel()
                                scrollTask = nil
                            }
                        }

                        // Background tap barrier when popup is active
                        if activePopup != nil {
                            Color.black.opacity(0.001)
                                .ignoresSafeArea()
                                .onTapGesture {
                                    withAnimation(.spring(response: 0.28, dampingFraction: 0.8)) {
                                        activePopup = nil
                                    }
                                }
                        }

                        // Bottom Area: Popups + Project Selector + Composer Card
                        VStack(alignment: .leading, spacing: 0) {
                            // Popups anchored directly above composer
                            if let popup = activePopup {
                                Group {
                                    switch popup {
                                    case .plus:
                                        PlusPopupView(
                                            session: session,
                                            onTakePhoto: {
                                                activePopup = nil
                                                showsCamera = true
                                            },
                                            onToggleFast: {
                                                activePopup = nil
                                                Task { await app.toggleFastMode(session.id) }
                                            },
                                            onTogglePlan: {
                                                activePopup = nil
                                                Task { await app.togglePlanMode(session.id) }
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
                                onSend: send,
                                onSteer: steer,
                                onCancel: { Task { await app.cancel() } }
                            )
                        }
                        .padding(.bottom, 6)
                    }
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
    }

    private func scheduleScroll(using proxy: ScrollViewProxy) {
        guard scrollTask == nil else { return }
        scrollTask = Task { @MainActor in
            do {
                try await Task.sleep(for: .milliseconds(100))
            } catch {
                scrollTask = nil
                return
            }
            proxy.scrollTo("thread-bottom", anchor: .bottom)
            scrollTask = nil
        }
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

    private func send() {
        let text = input
        input = ""
        Task { await app.send(text) }
    }

    private func steer() {
        let text = input
        input = ""
        Task { await app.steer(text) }
    }

    private func timeline(for session: AgentSession) -> [TimelineEntry] {
        var result: [TimelineEntry] = []
        for block in session.transcriptBlocks where block.afterMessage == 0 {
            result.append(.block(block))
        }
        for (index, message) in session.messages.enumerated() {
            result.append(.message(message, footerTime: assistantFooterTime(at: index, in: session)))
            for block in session.transcriptBlocks where block.afterMessage == index + 1 {
                result.append(.block(block))
            }
        }
        return result
    }

    private func assistantFooterTime(at index: Int, in session: AgentSession) -> UInt64? {
        let message = session.messages[index]
        guard message.role == .assistant,
              !message.streaming,
              !message.content.isEmpty,
              !session.messages[(index + 1)...].contains(where: {
                  $0.role == .assistant && $0.turnID == message.turnID
              }) else { return nil }
        return message.turnID
            .flatMap { turnID in session.turns.first(where: { $0.id == turnID })?.completedAt }
            ?? (session.status.isBusy ? nil : session.lastReplyAt)
            ?? message.createdAt
    }
}

// MARK: - Composer Card

private struct ComposerCard: View {
    @Binding var text: String
    @Binding var attachedImages: [UIImage]
    let session: AgentSession
    @Bindable var app: AppModel
    @Binding var activePopup: ActiveInputPopup?
    let onSend: () -> Void
    let onSteer: () -> Void
    let onCancel: () -> Void

    @FocusState private var focused: Bool

    private var hasText: Bool {
        !text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
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
                    HStack(spacing: 8) {
                        ForEach(attachedImages.indices, id: \.self) { index in
                            ZStack(alignment: .topTrailing) {
                                Image(uiImage: attachedImages[index])
                                    .resizable()
                                    .aspectRatio(contentMode: .fill)
                                    .frame(width: 54, height: 54)
                                    .clipShape(RoundedRectangle(cornerRadius: 10))

                                Button {
                                    attachedImages.remove(at: index)
                                } label: {
                                    Image(systemName: "xmark.circle.fill")
                                        .font(.system(size: 16))
                                        .foregroundStyle(.white, Color.black.opacity(0.7))
                                }
                                .offset(x: 4, y: -4)
                            }
                        }
                    }
                    .padding(.horizontal, 4)
                }
            }

            // Input Text Field
            TextField("Ask Insulator", text: $text, axis: .vertical)
                .lineLimit(1...6)
                .focused($focused)
                .foregroundStyle(.white)
                .tint(.white)
                .padding(.horizontal, 4)
                .padding(.top, 2)

            // Bottom Controls Bar
            HStack(spacing: 12) {
                // 1. Plus Button (Attachments & Modes)
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
                .accessibilityLabel("Add attachments and modes")

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

                // 4. Microphone Button
                Button {
                    // Dictate placeholder
                } label: {
                    Image(systemName: "mic")
                        .font(.system(size: 18))
                        .foregroundStyle(Color.white.opacity(0.8))
                        .frame(width: 32, height: 32)
                }
                .buttonStyle(.plain)
                .accessibilityLabel("Voice input")

                // 5. Send / Stop Button
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

                    if !text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
                        Button {
                            focused = false
                            activePopup = nil
                            onSteer()
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
                        onSend()
                    } label: {
                        Image(systemName: "arrow.up")
                            .font(.system(size: 15, weight: .bold))
                            .frame(width: 36, height: 36)
                            .background(hasText ? Color.white : AppTheme.raised, in: Circle())
                            .foregroundStyle(hasText ? Color.black : AppTheme.secondary)
                    }
                    .disabled(!hasText)
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
    let session: AgentSession
    let onTakePhoto: () -> Void
    let onToggleFast: () -> Void
    let onTogglePlan: () -> Void

    @State private var selectedPhotoItems: [PhotosPickerItem] = []

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            VStack(alignment: .leading, spacing: 16) {
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

                // Divider
                Rectangle()
                    .fill(Color.white.opacity(0.12))
                    .frame(height: 1)
                    .padding(.vertical, 2)

                // 3. Fast Mode
                Button(action: onToggleFast) {
                    HStack(spacing: 14) {
                        Image(systemName: "bolt")
                            .font(.system(size: 16))
                            .foregroundStyle(.white)
                            .frame(width: 20)
                        Text("Fast Mode")
                            .font(.system(size: 15, design: .monospaced))
                            .foregroundStyle(.white)
                        Spacer()
                        if session.serviceTier == "fast" {
                            Image(systemName: "checkmark")
                                .font(.system(size: 13, weight: .bold))
                                .foregroundStyle(.white)
                        }
                    }
                }
                .buttonStyle(.plain)

                // 4. Plan mode
                Button(action: onTogglePlan) {
                    HStack(spacing: 14) {
                        Image(systemName: "slider.horizontal.3")
                            .font(.system(size: 16))
                            .foregroundStyle(.white)
                            .frame(width: 20)
                        Text("Plan mode")
                            .font(.system(size: 15, design: .monospaced))
                            .foregroundStyle(.white)
                        Spacer()
                        if session.agentPreset == "plan" {
                            Image(systemName: "checkmark")
                                .font(.system(size: 13, weight: .bold))
                                .foregroundStyle(.white)
                        }
                    }
                }
                .buttonStyle(.plain)
            }
            .padding(.horizontal, 18)
            .padding(.vertical, 16)
            .frame(width: 230)
            .background(Color(red: 0.17, green: 0.17, blue: 0.19), in: RoundedRectangle(cornerRadius: 22))
            .overlay(RoundedRectangle(cornerRadius: 22).stroke(Color.white.opacity(0.12), lineWidth: 1))
            .shadow(color: Color.black.opacity(0.5), radius: 20, y: 8)

            // Pointer arrow pointing down to plus button
            TooltipArrow()
                .fill(Color(red: 0.17, green: 0.17, blue: 0.19))
                .frame(width: 18, height: 10)
                .offset(x: 16, y: -1)
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

private enum TimelineEntry: Identifiable {
    case message(Message, footerTime: UInt64?)
    case block(TranscriptBlock)

    var id: String {
        switch self {
        case .message(let message, _): "message-\(message.id.uuidString)"
        case .block(let block): "block-\(block.id)"
        }
    }
}

private struct TimelineEntryView: View {
    let entry: TimelineEntry

    var body: some View {
        switch entry {
        case .message(let message, let footerTime):
            MessageRow(message: message, footerTime: footerTime)
        case .block(let block):
            TranscriptBlockView(block: block)
        }
    }
}

private struct MessageRow: View {
    let message: Message
    let footerTime: UInt64?

    var body: some View {
        if message.role == .user {
            HStack {
                Spacer(minLength: 44)
                Text(message.visibleContent)
                    .font(.body)
                    .textSelection(.enabled)
                    .padding(.horizontal, 15)
                    .padding(.vertical, 11)
                    .background(AppTheme.raised, in: RoundedRectangle(cornerRadius: 18))
            }
        } else {
            VStack(alignment: .leading, spacing: 4) {
                Text(message.visibleContent)
                    .font(.body)
                    .lineSpacing(4)
                    .textSelection(.enabled)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .foregroundStyle(message.role == .system ? AppTheme.secondary : .primary)

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

private struct WorkingIndicator: View {
    let session: AgentSession
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    var body: some View {
        TimelineView(.periodic(from: .now, by: reduceMotion ? 1 : 0.14)) { context in
            HStack(spacing: 9) {
                DotMatrixLoader(date: context.date, animated: !reduceMotion)
                Text(label(at: context.date))
                    .font(.subheadline)
                    .foregroundStyle(AppTheme.secondary)
                    .monospacedDigit()
            }
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

// MARK: - Activity & Transcript Views

extension ActivityItem {
    var toolBadgeName: String {
        let k = kind.lowercased()
        if k == "command" || k == "bash" {
            return "Bash"
        } else if k == "file_read" || k == "fileread" || k == "read" {
            return "Read"
        } else if k == "file_change" || k == "filechange" || k == "edit" {
            return "Edit"
        } else if k == "file_search" || k == "filesearch" || k == "search" {
            return "Search"
        } else if k == "file_list" || k == "filelist" || k == "list" {
            return "List"
        } else if k == "reasoning" || k == "think" {
            return "Think"
        } else if k == "plan" {
            return "Plan"
        } else if !title.isEmpty && title != "Tool" && title != "Activity" {
            return title
        } else {
            return "Tool"
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
            HStack(spacing: 4) {
                Image(systemName: copied ? "checkmark" : "doc.on.doc")
                    .font(.system(size: 11, weight: .medium))
                if copied {
                    Text("Copied")
                        .font(.system(size: 10, weight: .medium))
                }
            }
            .foregroundStyle(copied ? Color(red: 0.25, green: 0.85, blue: 0.45) : Color.white.opacity(0.6))
            .padding(.horizontal, 6)
            .padding(.vertical, 3)
            .background(Color.white.opacity(0.08), in: RoundedRectangle(cornerRadius: 6))
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
            .font(.system(size: 11.5, design: .monospaced))
            .foregroundStyle(textColor)
            .frame(maxWidth: .infinity, alignment: .leading)
            .padding(.horizontal, 8)
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
            ForEach(Array(diff.components(separatedBy: .newlines).prefix(50).enumerated()), id: \.offset) { _, line in
                DiffLineRow(line: line)
            }
        }
        .padding(.vertical, 4)
        .background(Color.black.opacity(0.4), in: RoundedRectangle(cornerRadius: 8))
        .overlay(RoundedRectangle(cornerRadius: 8).stroke(Color.white.opacity(0.06), lineWidth: 1))
    }
}

private struct ActivityParamPill: View {
    let param: String
    let failed: Bool

    var body: some View {
        Text(param)
            .font(.system(size: 11.5, design: .monospaced))
            .foregroundStyle(failed ? Color(red: 0.95, green: 0.5, blue: 0.5) : Color.white.opacity(0.75))
            .lineLimit(1)
            .truncationMode(.middle)
            .padding(.horizontal, 7)
            .padding(.vertical, 2.5)
            .background(
                failed ? Color.red.opacity(0.15) : Color.white.opacity(0.06),
                in: RoundedRectangle(cornerRadius: 6)
            )
            .overlay(
                RoundedRectangle(cornerRadius: 6)
                    .stroke(failed ? Color.red.opacity(0.25) : Color.white.opacity(0.04), lineWidth: 0.5)
            )
    }
}

private struct ActivityFileStatsView: View {
    let additions: UInt64
    let deletions: UInt64

    var body: some View {
        HStack(spacing: 4) {
            if additions > 0 {
                Text("+\(additions)")
                    .font(.system(size: 11.5, weight: .medium, design: .monospaced))
                    .foregroundStyle(Color(red: 0.25, green: 0.85, blue: 0.45))
            }
            if deletions > 0 {
                Text("-\(deletions)")
                    .font(.system(size: 11.5, weight: .medium, design: .monospaced))
                    .foregroundStyle(Color(red: 0.95, green: 0.35, blue: 0.35))
            }
        }
    }
}

private struct ReasoningBlockView: View {
    let reasoning: ReasoningBlock
    @State private var expanded = false

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
        VStack(spacing: 0) {
            Button {
                UIImpactFeedbackGenerator(style: .light).impactOccurred()
                withAnimation(.spring(response: 0.26, dampingFraction: 0.8)) {
                    expanded.toggle()
                }
            } label: {
                HStack(spacing: 8) {
                    Image(systemName: "sparkles")
                        .font(.system(size: 13, weight: .medium))
                        .foregroundStyle(AppTheme.accent)

                    if let dur = durationString {
                        Text("Thought for \(dur)")
                            .font(.system(size: 13, weight: .medium))
                            .foregroundStyle(Color.white.opacity(0.9))
                    } else {
                        HStack(spacing: 6) {
                            Text("Thinking")
                                .font(.system(size: 13, weight: .medium))
                                .foregroundStyle(Color.white.opacity(0.9))
                            Circle()
                                .fill(AppTheme.accent)
                                .frame(width: 5, height: 5)
                        }
                    }

                    Spacer()

                    Image(systemName: "chevron.right")
                        .font(.system(size: 11, weight: .semibold))
                        .foregroundStyle(Color.white.opacity(0.4))
                        .rotationEffect(.degrees(expanded ? 90 : 0))
                }
                .padding(.horizontal, 12)
                .padding(.vertical, 10)
                .contentShape(Rectangle())
            }
            .buttonStyle(.plain)

            if expanded {
                VStack(alignment: .leading, spacing: 8) {
                    Rectangle()
                        .fill(Color.white.opacity(0.07))
                        .frame(height: 1)

                    HStack {
                        Spacer()
                        ActivityCopyButton(text: reasoning.content)
                    }
                    .padding(.horizontal, 10)
                    .padding(.top, 4)

                    ScrollView {
                        Text(reasoning.content)
                            .font(.system(size: 13))
                            .lineSpacing(3)
                            .foregroundStyle(Color.white.opacity(0.7))
                            .textSelection(.enabled)
                            .frame(maxWidth: .infinity, alignment: .leading)
                            .padding(.horizontal, 10)
                            .padding(.bottom, 8)
                    }
                    .frame(maxHeight: 280)
                }
                .transition(.opacity)
            }
        }
        .background(Color(red: 0.12, green: 0.12, blue: 0.14).opacity(0.85), in: RoundedRectangle(cornerRadius: 12))
        .overlay(RoundedRectangle(cornerRadius: 12).stroke(Color.white.opacity(0.08), lineWidth: 1))
    }
}

private struct ActivityRow: View {
    let activity: ActivityItem
    @State private var expanded = false

    private var hasDetail: Bool {
        activity.hasDetailContent
    }

    var body: some View {
        VStack(spacing: 0) {
            // Main Header Row (Compact height ~36)
            Button {
                guard hasDetail else { return }
                UIImpactFeedbackGenerator(style: .light).impactOccurred()
                withAnimation(.spring(response: 0.25, dampingFraction: 0.8)) {
                    expanded.toggle()
                }
            } label: {
                HStack(spacing: 7) {
                    // Tool Icon
                    Image(systemName: activity.failed ? "xmark.circle.fill" : activity.iconName)
                        .font(.system(size: 12.5, weight: .medium))
                        .foregroundStyle(activity.failed ? Color(red: 0.95, green: 0.35, blue: 0.35) : Color.white.opacity(0.65))
                        .frame(width: 16)

                    // Tool Action Name
                    Text(activity.failed ? "Error" : activity.toolBadgeName)
                        .font(.system(size: 13, weight: .semibold))
                        .foregroundStyle(activity.failed ? Color(red: 0.95, green: 0.35, blue: 0.35) : .white)

                    // Target / Parameter Pill
                    if let param = activity.displayParam {
                        ActivityParamPill(param: param, failed: activity.failed)
                    }

                    Spacer(minLength: 4)

                    // File Change Stats (+X / -Y)
                    if let stats = activity.diffStats {
                        ActivityFileStatsView(additions: stats.additions, deletions: stats.deletions)
                    }

                    // Trailing Status Indicator
                    if !activity.complete && !activity.failed {
                        Circle()
                            .fill(AppTheme.accent)
                            .frame(width: 6, height: 6)
                    } else if hasDetail {
                        Image(systemName: "chevron.right")
                            .font(.system(size: 10.5, weight: .semibold))
                            .foregroundStyle(Color.white.opacity(0.35))
                            .rotationEffect(.degrees(expanded ? 90 : 0))
                    } else {
                        Image(systemName: "checkmark")
                            .font(.system(size: 11, weight: .medium))
                            .foregroundStyle(Color.white.opacity(0.3))
                    }
                }
                .padding(.horizontal, 10)
                .padding(.vertical, 8)
                .contentShape(Rectangle())
            }
            .buttonStyle(.plain)

            // Expanded Detail View
            if expanded && hasDetail {
                VStack(alignment: .leading, spacing: 10) {
                    Rectangle()
                        .fill(Color.white.opacity(0.07))
                        .frame(height: 1)

                    // Error Banner
                    if activity.failed {
                        let errorMsg = activity.output ?? activity.detail ?? activity.title
                        HStack(alignment: .top, spacing: 8) {
                            Image(systemName: "exclamationmark.triangle.fill")
                                .font(.system(size: 12))
                                .foregroundStyle(Color(red: 0.95, green: 0.35, blue: 0.35))
                            Text(errorMsg)
                                .font(.system(size: 11.5, design: .monospaced))
                                .foregroundStyle(Color(red: 0.95, green: 0.7, blue: 0.7))
                                .textSelection(.enabled)
                                .frame(maxWidth: .infinity, alignment: .leading)
                        }
                        .padding(8)
                        .background(Color.red.opacity(0.12), in: RoundedRectangle(cornerRadius: 7))
                        .overlay(RoundedRectangle(cornerRadius: 7).stroke(Color.red.opacity(0.25), lineWidth: 0.5))
                    }

                    // Command (for Bash/command)
                    let isCommand = activity.kind.lowercased() == "command" || activity.kind.lowercased() == "bash"
                    if isCommand, let cmd = activity.arguments ?? activity.displayTarget, !cmd.isEmpty {
                        VStack(alignment: .leading, spacing: 4) {
                            HStack {
                                Text("COMMAND")
                                    .font(.system(size: 9.5, weight: .bold, design: .monospaced))
                                    .foregroundStyle(Color.white.opacity(0.45))
                                Spacer()
                                ActivityCopyButton(text: cmd)
                            }

                            ScrollView(.horizontal, showsIndicators: false) {
                                HStack(spacing: 4) {
                                    Text("$")
                                        .font(.system(size: 12, weight: .bold, design: .monospaced))
                                        .foregroundStyle(Color.white.opacity(0.4))
                                    Text(cmd)
                                        .font(.system(size: 12, design: .monospaced))
                                        .foregroundStyle(Color.white.opacity(0.9))
                                        .textSelection(.enabled)
                                }
                                .padding(.horizontal, 8)
                                .padding(.vertical, 6)
                            }
                            .background(Color.black.opacity(0.4), in: RoundedRectangle(cornerRadius: 7))
                            .overlay(RoundedRectangle(cornerRadius: 7).stroke(Color.white.opacity(0.06), lineWidth: 0.5))
                        }
                    }

                    // Diff (for File Changes)
                    if let diff = activity.fileChanges?.first?.diff, !diff.isEmpty {
                        VStack(alignment: .leading, spacing: 4) {
                            HStack {
                                Text("DIFF")
                                    .font(.system(size: 9.5, weight: .bold, design: .monospaced))
                                    .foregroundStyle(Color.white.opacity(0.45))
                                Spacer()
                                ActivityCopyButton(text: diff)
                            }

                            ScrollView([.horizontal, .vertical], showsIndicators: true) {
                                ActivityDiffView(diff: diff)
                            }
                            .frame(maxHeight: 240)
                        }
                    }

                    // Arguments (for other tools)
                    if !isCommand, let args = activity.arguments, !args.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
                        VStack(alignment: .leading, spacing: 4) {
                            HStack {
                                Text("ARGUMENTS")
                                    .font(.system(size: 9.5, weight: .bold, design: .monospaced))
                                    .foregroundStyle(Color.white.opacity(0.45))
                                Spacer()
                                ActivityCopyButton(text: args)
                            }

                            ScrollView([.horizontal, .vertical], showsIndicators: true) {
                                Text(args)
                                    .font(.system(size: 11.5, design: .monospaced))
                                    .foregroundStyle(Color.white.opacity(0.75))
                                    .textSelection(.enabled)
                                    .padding(8)
                                    .frame(maxWidth: .infinity, alignment: .leading)
                            }
                            .frame(maxHeight: 180)
                            .background(Color.black.opacity(0.4), in: RoundedRectangle(cornerRadius: 7))
                            .overlay(RoundedRectangle(cornerRadius: 7).stroke(Color.white.opacity(0.06), lineWidth: 0.5))
                        }
                    }

                    // Output
                    if let output = activity.output, !output.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
                        VStack(alignment: .leading, spacing: 4) {
                            HStack {
                                Text("OUTPUT")
                                    .font(.system(size: 9.5, weight: .bold, design: .monospaced))
                                    .foregroundStyle(Color.white.opacity(0.45))
                                Spacer()
                                ActivityCopyButton(text: output)
                            }

                            ScrollView([.horizontal, .vertical], showsIndicators: true) {
                                Text(output)
                                    .font(.system(size: 11.5, design: .monospaced))
                                    .foregroundStyle(Color.white.opacity(0.8))
                                    .textSelection(.enabled)
                                    .padding(8)
                                    .frame(maxWidth: .infinity, alignment: .leading)
                            }
                            .frame(maxHeight: 220)
                            .background(Color.black.opacity(0.4), in: RoundedRectangle(cornerRadius: 7))
                            .overlay(RoundedRectangle(cornerRadius: 7).stroke(Color.white.opacity(0.06), lineWidth: 0.5))
                        }
                    } else if !isCommand, activity.arguments == nil, let d = activity.detail,
                              !d.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty,
                              d.lowercased() != "completed" && d.lowercased() != "failed" {
                        VStack(alignment: .leading, spacing: 4) {
                            HStack {
                                Text("DETAIL")
                                    .font(.system(size: 9.5, weight: .bold, design: .monospaced))
                                    .foregroundStyle(Color.white.opacity(0.45))
                                Spacer()
                                ActivityCopyButton(text: d)
                            }

                            ScrollView([.horizontal, .vertical], showsIndicators: true) {
                                Text(d)
                                    .font(.system(size: 11.5, design: .monospaced))
                                    .foregroundStyle(Color.white.opacity(0.8))
                                    .textSelection(.enabled)
                                    .padding(8)
                                    .frame(maxWidth: .infinity, alignment: .leading)
                            }
                            .frame(maxHeight: 180)
                            .background(Color.black.opacity(0.4), in: RoundedRectangle(cornerRadius: 7))
                            .overlay(RoundedRectangle(cornerRadius: 7).stroke(Color.white.opacity(0.06), lineWidth: 0.5))
                        }
                    }
                }
                .padding(.horizontal, 10)
                .padding(.bottom, 10)
                .transition(.opacity)
            }
        }
        .background(
            Color(red: 0.12, green: 0.12, blue: 0.14).opacity(0.85),
            in: RoundedRectangle(cornerRadius: 11)
        )
        .overlay(
            RoundedRectangle(cornerRadius: 11)
                .stroke(activity.failed ? Color.red.opacity(0.3) : Color.white.opacity(0.08), lineWidth: 1)
        )
    }
}

private struct TranscriptBlockView: View {
    let block: TranscriptBlock

    var body: some View {
        switch block.content {
        case .reasoning(let reasoning):
            ReasoningBlockView(reasoning: reasoning)
        case .activities(let activities):
            VStack(spacing: 6) {
                ForEach(activities) { activity in
                    ActivityRow(activity: activity)
                }
            }
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
