import SwiftUI

/// A project's sessions at work, resting on the composer: the finished ones
/// stacked above, and those still working in a now-playing pill, as in the
/// Music app. Both step aside while the keyboard is up.
struct LiveSessions: View {
    @Bindable var app: AppModel
    let project: Project
    let inView: Bool

    @State private var keyboard = KeyboardState.shared

    var body: some View {
        let working = app.sessions(in: project.id).filter(\.status.isBusy)
        let finished = app.finished.filter { $0.projectID == project.id }
        let hidden = keyboard.visible || !inView
        VStack(spacing: 10) {
            FinishedDeck(app: app, entries: finished, hidden: hidden)
            NowWorking(app: app, working: working, hidden: hidden)
        }
        .animation(.spring(response: 0.4, dampingFraction: 0.85), value: working.map(\.id))
        .animation(.spring(response: 0.4, dampingFraction: 0.85), value: finished.map(\.id))
        .animation(.easeInOut(duration: 0.28), value: hidden)
    }
}

/// Every working session in one pill: the front one's agent as artwork, the
/// titles scrolling through, and "2 of 3". The square stops the front one;
/// skip brings the next title in. With more than one, a tap fans them out.
private struct NowWorking: View {
    @Bindable var app: AppModel
    let working: [AgentSession]
    let hidden: Bool

    @Environment(Router.self) private var router
    @State private var active = 0
    @State private var front = 0
    @State private var open = false
    @State private var pulse = 0
    @State private var known: Set<UUID>?

    var body: some View {
        let index = min(active, max(0, working.count - 1))
        VStack(spacing: 8) {
            if open, working.count > 1, !hidden {
                ForEach(Array(working.enumerated()), id: \.element.id) { place, session in
                    MiniPill(session: session, model: modelName(session)) {
                        openSession(session)
                    } onStop: {
                        stop(session)
                    }
                    .transition(.move(edge: .bottom).combined(with: .scale(scale: 0.8)).combined(with: .opacity))
                    .animation(.spring(response: 0.35, dampingFraction: 0.8).delay(Double(working.count - 1 - place) * 0.045), value: open)
                }
                .padding(.horizontal, 8)
            }
            if !hidden, working.indices.contains(index) {
                let session = working[index]
                Button {
                    if working.count > 1 {
                        Haptics.selection()
                        withAnimation(.spring(response: 0.35, dampingFraction: 0.8)) { open.toggle() }
                    } else {
                        openSession(session)
                    }
                } label: {
                    HStack(spacing: 4) {
                        Artwork(session: session, pulse: pulse)
                            .padding(.trailing, 8)
                        VStack(alignment: .leading, spacing: 2) {
                            Ticker(
                                titles: working.map(\.displayTitle),
                                font: AppTheme.font(size: 16, weight: .semibold),
                                front: front
                            ) { active = $0 }
                            .foregroundStyle(AppTheme.content)
                            subtitle(session, index: index)
                        }
                        .frame(maxWidth: .infinity, alignment: .leading)
                        StopButton { stop(session) }
                        if working.count > 1 {
                            Button {
                                Haptics.selection()
                                front = (index + 1) % working.count
                            } label: {
                                SkipShape()
                                    .fill(AppTheme.muted)
                                    .frame(width: 26, height: 16)
                                    .frame(width: 40, height: 40)
                            }
                            .buttonStyle(PressedOpacityStyle())
                            .accessibilityLabel("Next working session")
                        }
                    }
                    .padding(.leading, 10)
                    .padding(.trailing, 8)
                    .padding(.vertical, 10)
                    .contentShape(Capsule())
                }
                .buttonStyle(PressedScaleStyle())
                .glass(in: Capsule(), interactive: true)
                .padding(.horizontal, 20)
                .transition(.scale(scale: 0.9).combined(with: .opacity))
                .accessibilityLabel(working.count > 1
                    ? "\(working.count) working sessions. \(open ? "Hide" : "Show") them"
                    : "\(session.displayTitle), \(statusLabel(session)). Open")
            }
        }
        .onChange(of: working.map(\.id)) { _, ids in
            let current = Set(ids)
            defer { known = current }
            guard let before = known, let arrived = ids.firstIndex(where: { !before.contains($0) }) else { return }
            // One that starts working once the pill is up comes to the front.
            Haptics.success()
            pulse += 1
            front = arrived
        }
        .onAppear { known = Set(working.map(\.id)) }
        .onChange(of: working.count) { _, count in if count < 2 { open = false } }
    }

    private func subtitle(_ session: AgentSession, index: Int) -> some View {
        let waiting = session.status == .waiting
        let detail = waiting ? statusLabel(session)
            : [modelName(session), session.branch].compactMap { $0 }.joined(separator: " · ").nilIfEmpty ?? "Working"
        return HStack(spacing: 6) {
            if !waiting {
                TerminalSpinner()
                    .font(AppTheme.font(size: 13))
                    .foregroundStyle(AppTheme.accent)
                    .frame(width: 12)
            }
            Text(detail)
                .font(AppTheme.font(size: 13))
                .foregroundStyle(waiting ? AppTheme.caution : AppTheme.muted)
                .lineLimit(1)
                .id(detail)
                .transition(.opacity)
            if working.count > 1 {
                HStack(spacing: 0) {
                    Text("\(index + 1)")
                        .id(index)
                        .transition(.push(from: .bottom))
                    Text(" of \(working.count)")
                }
                .font(AppTheme.font(size: 12).monospacedDigit())
                .foregroundStyle(AppTheme.faint)
                .animation(.easeOut(duration: 0.24), value: index)
                .accessibilityLabel("\(index + 1) of \(working.count)")
            }
        }
    }

    private func modelName(_ session: AgentSession) -> String? {
        app.modelName(provider: session.provider, modelID: session.model)
    }

    private func openSession(_ session: AgentSession) {
        open = false
        router.open(session.id)
    }

    private func stop(_ session: AgentSession) {
        Haptics.impact(.medium)
        Task {
            do { try await app.stop(session.id) } catch { app.errorMessage = error.localizedDescription }
        }
    }
}

private func statusLabel(_ session: AgentSession) -> String {
    session.status == .waiting ? "Needs your approval" : "Working"
}

/// The agent's mark in a circle, ringed in a slow glow while it works; a new
/// one throws a ring out from it.
private struct Artwork: View {
    let session: AgentSession
    let pulse: Int

    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @State private var glow = false
    @State private var burst: CGFloat = 1

    var body: some View {
        let tone = session.status == .waiting ? AppTheme.caution : AppTheme.accent
        ZStack {
            Circle().fill(AppTheme.fill)
            Circle()
                .stroke(tone, lineWidth: 1.5)
                .scaleEffect(1 + burst * 0.7)
                .opacity(1 - burst)
            ProviderLogo(provider: session.provider, size: 22)
                .foregroundStyle(AppTheme.content)
                .id(session.provider)
                .transition(.asymmetric(insertion: .scale(scale: 0.4).combined(with: .opacity), removal: .opacity))
            Circle()
                .fill(tone.opacity(0.1))
                .overlay(Circle().stroke(tone, lineWidth: 1.5))
                .opacity(glow ? 1 : 0.2)
        }
        .frame(width: 40, height: 40)
        .onAppear {
            guard !reduceMotion else { return }
            withAnimation(.easeInOut(duration: session.status == .waiting ? 0.52 : 1.1).repeatForever(autoreverses: true)) {
                glow = true
            }
        }
        .onChange(of: pulse) {
            guard !reduceMotion else { return }
            burst = 0
            withAnimation(.easeOut(duration: 0.9)) { burst = 1 }
        }
    }
}

private struct MiniPill: View {
    let session: AgentSession
    let model: String?
    let onOpen: () -> Void
    let onStop: () -> Void

    var body: some View {
        let waiting = session.status == .waiting
        let detail = waiting ? statusLabel(session)
            : [model, session.branch].compactMap { $0 }.joined(separator: " · ").nilIfEmpty ?? "Working"
        Button(action: onOpen) {
            HStack(spacing: 10) {
                ZStack(alignment: .bottomTrailing) {
                    ProviderLogo(provider: session.provider, size: 16)
                        .foregroundStyle(AppTheme.content)
                        .frame(width: 32, height: 32)
                        .background(AppTheme.fill, in: Circle())
                    Circle()
                        .fill(waiting ? AppTheme.caution : AppTheme.accent)
                        .frame(width: 10, height: 10)
                        .overlay(Circle().stroke(AppTheme.background, lineWidth: 2))
                        .offset(x: 1, y: 1)
                }
                VStack(alignment: .leading, spacing: 2) {
                    Text(session.displayTitle)
                        .font(AppTheme.font(size: 14, weight: .semibold))
                        .foregroundStyle(AppTheme.content)
                    Text(detail)
                        .font(AppTheme.font(size: 12))
                        .foregroundStyle(waiting ? AppTheme.caution : AppTheme.muted)
                }
                .lineLimit(1)
                .frame(maxWidth: .infinity, alignment: .leading)
                StopButton(action: onStop)
            }
            .padding(.leading, 10)
            .padding(.trailing, 6)
            .frame(height: 52)
            .contentShape(Capsule())
        }
        .buttonStyle(PressedScaleStyle(scale: 1.03))
        .glass(in: Capsule(), interactive: true)
        .accessibilityLabel("\(session.displayTitle), \(statusLabel(session)). Open")
    }
}

/// The Music app's stop: a plain rounded square.
private struct StopButton: View {
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            RoundedRectangle(cornerRadius: 3.5, style: .continuous)
                .fill(AppTheme.content)
                .frame(width: 15, height: 15)
                .frame(width: 40, height: 40)
                .contentShape(Circle())
        }
        .buttonStyle(PressedOpacityStyle())
        .accessibilityLabel("Stop")
    }
}

/// The Music app's skip: two triangles nose to tail.
private struct SkipShape: Shape {
    func path(in rect: CGRect) -> Path {
        var path = Path()
        let half = rect.width / 2
        for offset in [0, half] {
            path.move(to: CGPoint(x: rect.minX + offset + 1, y: rect.minY + 1))
            path.addLine(to: CGPoint(x: rect.minX + offset + half - 1, y: rect.midY))
            path.addLine(to: CGPoint(x: rect.minX + offset + 1, y: rect.maxY - 1))
            path.closeSubpath()
        }
        return path
    }
}

/// Sessions that finished since the phone saw them working: a deck, newest
/// on top with the next two peeking out. One taps open; more spread out
/// first. A long press puts one away.
private struct FinishedDeck: View {
    @Bindable var app: AppModel
    let entries: [FinishedSession]
    let hidden: Bool

    @Environment(Router.self) private var router
    @State private var open = false

    private static let card: CGFloat = 48
    private static let peek: CGFloat = 7
    private static let gap: CGFloat = 8

    var body: some View {
        let shown = entries.compactMap { entry in app.session(withID: entry.sessionID).map { (entry, $0) } }
        if !hidden, !shown.isEmpty {
            VStack(spacing: 0) {
                if open {
                    HStack(spacing: 4) {
                        Text("Finished")
                            .font(AppTheme.font(size: 13, weight: .semibold))
                            .foregroundStyle(AppTheme.muted)
                        Spacer()
                        smallButton(label: "Clear finished sessions") {
                            Haptics.impact(.light)
                            app.finished.removeAll { entry in shown.contains { $0.0.id == entry.id } }
                        } content: {
                            Text("Clear").font(AppTheme.font(size: 12, weight: .semibold))
                        }
                        smallButton(label: "Stack finished sessions") {
                            withAnimation(.spring(response: 0.4, dampingFraction: 0.85)) { open = false }
                        } content: {
                            IconView(.chevronDown, size: 14)
                        }
                    }
                    .padding(.leading, 4)
                    .frame(height: 22)
                    .padding(.bottom, Self.gap)
                    .transition(.opacity.combined(with: .offset(y: 10)))
                }
                ZStack(alignment: .bottom) {
                    ForEach(Array(shown.enumerated().reversed()), id: \.element.0.id) { index, item in
                        card(entry: item.0, session: item.1, index: index, count: shown.count)
                    }
                }
                .frame(height: open
                    ? CGFloat(shown.count) * (Self.card + Self.gap) - Self.gap
                    : Self.card + Self.peek * CGFloat(min(shown.count - 1, 2)), alignment: .bottom)
            }
            .padding(.horizontal, 28)
            .transition(.move(edge: .bottom).combined(with: .opacity))
            .onChange(of: shown.count) { _, count in if count < 2 { open = false } }
        }
    }

    private func card(entry: FinishedSession, session: AgentSession, index: Int, count: Int) -> some View {
        let behind = CGFloat(min(index, 2))
        let faceVisible = open || index == 0
        return Button {
            if !open, count > 1 {
                Haptics.selection()
                withAnimation(.spring(response: 0.4, dampingFraction: 0.85)) { open = true }
            } else {
                open = false
                app.finished.removeAll { $0.id == entry.id }
                router.open(session.id)
            }
        } label: {
            HStack(spacing: 10) {
                ZStack(alignment: .bottomTrailing) {
                    ProviderLogo(provider: session.provider, size: 16)
                        .foregroundStyle(AppTheme.content)
                        .frame(width: 32, height: 32)
                        .background(AppTheme.fill, in: Circle())
                    IconView(.check, size: 9)
                        .foregroundStyle(AppTheme.inverse)
                        .frame(width: 14, height: 14)
                        .background(AppTheme.additions, in: Circle())
                        .offset(x: 2, y: 2)
                }
                Text(session.displayTitle)
                    .font(AppTheme.font(size: 14, weight: .semibold))
                    .foregroundStyle(AppTheme.content)
                    .lineLimit(1)
                    .frame(maxWidth: .infinity, alignment: .leading)
                if !open, count > 1 {
                    Text("+\(count - 1)")
                        .font(AppTheme.font(size: 12, weight: .semibold).monospacedDigit())
                        .foregroundStyle(AppTheme.muted)
                        .padding(.horizontal, 8)
                        .frame(height: 22)
                        .background(AppTheme.fill, in: Capsule())
                } else {
                    Text(relativeTime(entry.at))
                        .font(AppTheme.font(size: 12).monospacedDigit())
                        .foregroundStyle(AppTheme.faint)
                        .padding(.trailing, 6)
                }
            }
            .opacity(faceVisible ? 1 : 0)
            .padding(.horizontal, 8)
            .frame(height: Self.card)
            .contentShape(Capsule())
        }
        .buttonStyle(PressedScaleStyle())
        .glass(in: Capsule(), interactive: true)
        .disabled(!open && index > 0)
        .scaleEffect(open ? 1 : 1 - 0.05 * behind)
        .offset(y: open ? -CGFloat(index) * (Self.card + Self.gap) : -Self.peek * behind)
        .zIndex(Double(count - index))
        .simultaneousGesture(LongPressGesture().onEnded { _ in
            Haptics.impact(.medium)
            app.finished.removeAll { $0.id == entry.id }
        })
        .accessibilityLabel(!open && count > 1 ? "\(count) finished sessions. Show them" : "\(session.displayTitle), finished. Open")
        .transition(.asymmetric(
            insertion: .offset(y: 56).combined(with: .scale(scale: 0.7)).combined(with: .opacity),
            removal: .scale.combined(with: .opacity)
        ))
    }

    private func smallButton(label: String, action: @escaping () -> Void, @ViewBuilder content: () -> some View) -> some View {
        Button(action: action) {
            content()
                .foregroundStyle(AppTheme.muted)
                .padding(.horizontal, 8)
                .frame(minWidth: 22, minHeight: 22)
                .background(AppTheme.fill, in: Capsule())
        }
        .buttonStyle(PressedOpacityStyle())
        .accessibilityLabel(label)
    }
}

private extension String {
    var nilIfEmpty: String? { isEmpty ? nil : self }
}
