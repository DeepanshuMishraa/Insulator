import SwiftUI

private enum Layout {
    static let gutter: CGFloat = 20
    static let workText: CGFloat = 16
    static let workLine: CGFloat = 24
    static let foldMark: CGFloat = 16
    static let phaseIcon: CGFloat = 16
    /// Where a rail's branch starts, so its curve lands on the centre of a
    /// row's first line.
    static let branchTop: CGFloat = 4 + workLine / 2 - 8
    static let railX: CGFloat = gutter + foldMark / 2
    static let contentX: CGFloat = gutter + foldMark + 6
}

/// One turn, laid out as the desktop's transcript: the prompt in a bubble on
/// the right, the work folded behind one status line, the answer as
/// full-width Markdown, and when it landed underneath.
struct TurnView: View {
    let turn: TranscriptTurn

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            ForEach(Array(turn.items.enumerated()), id: \.element.id) { index, item in
                switch item {
                case .user(let message):
                    UserBubble(message: message)
                case .header(let header):
                    FoldLine(header: header)
                case .work(_, let phases):
                    VStack(alignment: .leading, spacing: 2) {
                        ForEach(Array(phases.enumerated()), id: \.element.id) { phaseIndex, phase in
                            PhaseView(phase: phase, active: phaseIndex == phases.count - 1)
                        }
                    }
                    .padding(.vertical, 2)
                    .padding(.horizontal, Layout.gutter)
                case .reply(let message):
                    MarkdownView(text: message.content, streaming: message.streaming)
                        .padding(.horizontal, Layout.gutter)
                        .padding(.top, followsWork(index) ? 4 : 12)
                        .padding(.bottom, 4)
                case .notice(_, let text, let error):
                    Notice(text: text, error: error)
                }
            }
            if turn.completedAt != nil || turn.copyText != nil {
                TurnFooter(turn: turn)
            }
        }
    }

    private func followsWork(_ index: Int) -> Bool {
        guard index > 0 else { return false }
        switch turn.items[index - 1] {
        case .user, .reply: return false
        default: return true
        }
    }
}

private struct UserBubble: View {
    let message: Message

    @State private var expanded = false
    @State private var singleLine = true

    var body: some View {
        let text = message.visibleContent
        VStack(alignment: .trailing, spacing: 4) {
            VStack(alignment: .leading, spacing: 4) {
                if !message.attachments.isEmpty {
                    Text(message.attachments.count == 1 ? "1 attachment" : "\(message.attachments.count) attachments")
                        .font(AppTheme.font(size: 13))
                        .foregroundStyle(AppTheme.muted)
                }
                if !text.isEmpty {
                    Text(text)
                        .font(AppTheme.font(size: 16))
                        .lineSpacing(7)
                        .foregroundStyle(AppTheme.content)
                        .lineLimit(expanded ? nil : 4)
                        .textSelection(.enabled)
                        .onGeometryChange(for: Bool.self) { $0.size.height < 30 } action: { singleLine = $0 }
                }
                if text.count > 220 || text.filter({ $0 == "\n" }).count >= 4 {
                    Button(expanded ? "Show less" : "Show more") { expanded.toggle() }
                        .font(AppTheme.font(size: 13))
                        .foregroundStyle(AppTheme.content.opacity(0.6))
                }
            }
            .padding(.horizontal, 14)
            .padding(.vertical, 9)
            .background(
                AppTheme.bubble,
                in: RoundedRectangle(cornerRadius: singleLine && message.attachments.isEmpty ? 21 : 14, style: .continuous)
            )
            Text(Date(timeIntervalSince1970: TimeInterval(message.createdAt)), format: .dateTime.hour().minute())
                .font(AppTheme.font(size: 12))
                .foregroundStyle(AppTheme.content.opacity(0.4))
                .padding(.horizontal, 12)
        }
        .frame(maxWidth: .infinity, alignment: .trailing)
        .padding(.leading, 56)
        .padding(.trailing, Layout.gutter)
        .padding(.top, 6)
        .padding(.bottom, 16)
    }
}

/// The line a turn's work folds behind: the provider's mark and the model's
/// clock, ticking while it works and how long it took once done. Tapped open,
/// the work hangs off it on a rail.
private struct FoldLine: View {
    let header: TurnHeader

    @State private var open = false

    var body: some View {
        let expandable = !header.entries.isEmpty
        let shown = open && expandable
        VStack(alignment: .leading, spacing: 0) {
            Button {
                Haptics.selection()
                withAnimation(.easeOut(duration: 0.22)) { open.toggle() }
            } label: {
                HStack(spacing: 6) {
                    Group {
                        if shown {
                            IconView(.chevronDown, size: Layout.foldMark)
                        } else {
                            ProviderLogo(provider: header.provider, size: Layout.foldMark)
                        }
                    }
                    .foregroundStyle(AppTheme.content.opacity(0.45))
                    if header.live {
                        TimelineView(.periodic(from: .now, by: 1)) { context in
                            ShimmerText(text: liveLabel(at: context.date), font: AppTheme.font(size: Layout.workText), color: AppTheme.content.opacity(0.5), duration: 1)
                        }
                    } else {
                        Text(TranscriptTurns.workingLabel(seconds: header.durationSeconds, model: header.model, done: true))
                            .font(AppTheme.font(size: Layout.workText))
                            .foregroundStyle(AppTheme.content.opacity(0.5))
                            .lineLimit(1)
                    }
                }
                .frame(minHeight: Layout.workLine)
                .padding(.horizontal, Layout.gutter)
                .padding(.vertical, 4)
                .contentShape(Rectangle())
                .overlay(alignment: .topLeading) {
                    if shown {
                        Rectangle()
                            .fill(AppTheme.rail)
                            .frame(width: 1)
                            .padding(.top, 4 + (Layout.workLine + Layout.foldMark) / 2)
                            .offset(x: Layout.railX)
                    }
                }
            }
            .buttonStyle(.plain)
            .disabled(!expandable)
            .accessibilityLabel(expandable ? (shown ? "Hide the work" : "Show the work") : "")
            Collapsible(open: shown) {
                VStack(alignment: .leading, spacing: 0) {
                    ForEach(Array(header.entries.enumerated()), id: \.element.id) { index, entry in
                        RailRow(last: index == header.entries.count - 1, railX: Layout.railX) {
                            switch entry {
                            case .prose(_, let text):
                                MarkdownView(text: text, muted: true)
                                    .padding(.vertical, 4)
                            case .phase(let phase):
                                PhaseView(phase: phase, active: false)
                            }
                        }
                        .padding(.leading, Layout.contentX)
                        .padding(.trailing, Layout.gutter)
                        .padding(.bottom, 4)
                    }
                }
                .padding(.bottom, 4)
            }
        }
    }

    private func liveLabel(at date: Date) -> String {
        if let waiting = header.waiting { return waiting }
        let elapsed = header.startedAt.map { UInt64(max(0, date.timeIntervalSince1970 - TimeInterval($0))) }
        return TranscriptTurns.workingLabel(seconds: elapsed, model: header.model, done: false)
    }
}

/// A row hanging off a rail: the spine down its left and a quarter-circle
/// branch into its first line, as the desktop's fold.
private struct RailRow<Content: View>: View {
    let last: Bool
    let railX: CGFloat
    @ViewBuilder let content: () -> Content

    var body: some View {
        content()
            .frame(maxWidth: .infinity, alignment: .leading)
            .background(alignment: .topLeading) {
                GeometryReader { proxy in
                    let x = railX - Layout.contentX
                    Path { path in
                        path.move(to: CGPoint(x: x + 0.5, y: 0))
                        path.addLine(to: CGPoint(x: x + 0.5, y: last ? Layout.branchTop : proxy.size.height))
                        path.move(to: CGPoint(x: x + 0.5, y: Layout.branchTop))
                        path.addQuadCurve(
                            to: CGPoint(x: x + 8.5, y: Layout.branchTop + 8),
                            control: CGPoint(x: x + 0.5, y: Layout.branchTop + 8)
                        )
                    }
                    .stroke(AppTheme.rail, lineWidth: 1)
                }
            }
    }
}

private extension WorkKind {
    var icon: Icon? {
        switch self {
        case .edit: .edit
        case .research: .search
        case .run: .terminal
        case .agent: .robot
        case .other: .wrench
        case .note: .minus
        case .think: nil
        }
    }
}

/// A run of work under the icon for what it did most. Live, the phase in
/// progress stays open on its latest steps; tapping any phase lists them.
private struct PhaseView: View {
    let phase: WorkPhase
    let active: Bool

    @State private var override: Bool?
    @State private var preferences = Preferences.shared

    var body: some View {
        let expandable = !phase.steps.isEmpty
        let open = expandable && (override ?? (active && !preferences.collapseToolCalls))
        let steps = active && override == nil ? Array(phase.steps.suffix(4)) : phase.steps
        VStack(alignment: .leading, spacing: 0) {
            Button {
                Haptics.selection()
                withAnimation(.easeOut(duration: 0.22)) { override = !open }
            } label: {
                HStack(spacing: 6) {
                    if let icon = phase.kind.icon {
                        IconView(icon, size: Layout.phaseIcon)
                            .foregroundStyle(AppTheme.content.opacity(0.45))
                    }
                    if active {
                        ShimmerText(text: phase.title, font: AppTheme.font(size: Layout.workText), color: AppTheme.content.opacity(0.5))
                    } else {
                        Text(phase.title)
                            .font(AppTheme.font(size: Layout.workText))
                            .foregroundStyle(AppTheme.content.opacity(0.5))
                            .lineLimit(1)
                    }
                }
                .frame(minHeight: Layout.workLine)
                .padding(.vertical, 4)
                .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .disabled(!expandable)
            Collapsible(open: open) {
                VStack(alignment: .leading, spacing: 0) {
                    ForEach(Array(steps.enumerated()), id: \.element.id) { index, step in
                        Text(step.label)
                            .font(AppTheme.font(size: Layout.workText))
                            .lineSpacing(Layout.workLine - Layout.workText * 1.2)
                            .foregroundStyle(step.failed ? AppTheme.deletions.opacity(0.8) : AppTheme.content.opacity(0.45))
                            .lineLimit(2)
                            .padding(.vertical, 4)
                            .padding(.leading, Layout.phaseIcon + 6)
                            .frame(maxWidth: .infinity, alignment: .leading)
                            .background(alignment: .topLeading) {
                                GeometryReader { proxy in
                                    let x = Layout.phaseIcon / 2 - 0.5
                                    Path { path in
                                        path.move(to: CGPoint(x: x, y: 0))
                                        path.addLine(to: CGPoint(x: x, y: index == steps.count - 1 ? Layout.branchTop : proxy.size.height))
                                        path.move(to: CGPoint(x: x, y: Layout.branchTop))
                                        path.addQuadCurve(
                                            to: CGPoint(x: x + 8, y: Layout.branchTop + 8),
                                            control: CGPoint(x: x, y: Layout.branchTop + 8)
                                        )
                                    }
                                    .stroke(AppTheme.rail, lineWidth: 1)
                                }
                            }
                    }
                }
                .padding(.bottom, 4)
            }
        }
    }
}

private struct Notice: View {
    let text: String
    let error: Bool

    var body: some View {
        if error {
            HStack(spacing: 12) {
                IconView(.chat, size: 18).foregroundStyle(AppTheme.muted)
                Text(text)
                    .font(AppTheme.font(size: 14, weight: .medium))
                    .foregroundStyle(AppTheme.muted)
                    .frame(maxWidth: .infinity, alignment: .leading)
            }
            .padding(14)
            .frame(minHeight: 64)
            .background(AppTheme.content.opacity(0.04), in: RoundedRectangle(cornerRadius: 16, style: .continuous))
            .overlay(RoundedRectangle(cornerRadius: 16, style: .continuous).stroke(AppTheme.stroke, lineWidth: 0.5))
            .padding(.horizontal, Layout.gutter)
            .padding(.vertical, 8)
        } else {
            Text(text)
                .font(AppTheme.font(size: 14))
                .foregroundStyle(AppTheme.content.opacity(0.5))
                .padding(.horizontal, Layout.gutter)
                .padding(.vertical, 8)
        }
    }
}

/// What a finished turn leaves under the answer: copy the reply, and when it
/// landed.
private struct TurnFooter: View {
    let turn: TranscriptTurn

    @State private var copied = false

    var body: some View {
        HStack(spacing: 10) {
            if let copyText = turn.copyText {
                Button {
                    UIPasteboard.general.string = copyText
                    Haptics.impact(.light)
                    copied = true
                    Task {
                        try? await Task.sleep(for: .seconds(2))
                        copied = false
                    }
                } label: {
                    IconView(copied ? .check : .copy, size: 16)
                        .foregroundStyle(AppTheme.content.opacity(0.4))
                        .padding(4)
                }
                .buttonStyle(PressedHighlightStyle(radius: 6))
                .accessibilityLabel(copied ? "Copied" : "Copy response")
            }
            if let completedAt = turn.completedAt {
                if turn.copyText != nil {
                    Circle().fill(AppTheme.content.opacity(0.25)).frame(width: 3, height: 3)
                }
                Text(Date(timeIntervalSince1970: TimeInterval(completedAt)), format: .dateTime.hour().minute())
                    .font(AppTheme.font(size: 14))
                    .foregroundStyle(AppTheme.content.opacity(0.35))
            }
        }
        .padding(.leading, Layout.gutter - 4)
        .padding(.trailing, Layout.gutter)
        .padding(.top, 4)
        .padding(.bottom, 12)
    }
}
