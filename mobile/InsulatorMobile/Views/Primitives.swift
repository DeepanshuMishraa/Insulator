import SwiftUI
import UIKit

enum Radii {
    static let card: CGFloat = 22
    static let row: CGFloat = 12
}

// MARK: - Icons

/// Every icon the app draws: Insulator's own Reicon set where it has one, an
/// SF Symbol where it doesn't.
enum Icon {
    case add, appearance, arrowUp, automation, branch, camera, changes, chat, check
    case chevronDown, chevronRight, chevronUpDown, close, compose, computer, computerOff
    case copy, edit, fileText, filter, folder, gallery, github, image, list, lock, menu
    case metrics, mic, minus, more, refresh, robot, search, send, settings, shield
    case sparkle, stop, terminal, trash, wrap, wrench

    fileprivate var reicon: Reicon? {
        switch self {
        case .add: .plus
        case .arrowUp, .send: .arrowUp
        case .camera: .camera
        case .check: .check
        case .chevronRight: .chevronRight
        case .close: .close
        case .compose: .compose
        case .computer: .laptop
        case .copy: .copy
        case .edit: .edit
        case .fileText: .fileText
        case .folder: .folder
        case .gallery, .image: .gallery
        case .github: .github
        case .list: .list
        case .more: .more
        case .refresh: .refresh
        case .search: .search
        case .settings: .settings
        case .shield: .shieldAlert
        case .sparkle: .sparkles
        case .stop: .stop
        case .terminal: .command
        case .trash: .trash
        case .wrench: .tools
        default: nil
        }
    }

    fileprivate var symbol: String {
        switch self {
        case .appearance: "circle.lefthalf.filled"
        case .automation: "bolt"
        case .branch: "arrow.triangle.branch"
        case .changes: "plus.forwardslash.minus"
        case .chat: "bubble.left"
        case .chevronDown: "chevron.down"
        case .chevronUpDown: "chevron.up.chevron.down"
        case .computerOff: "laptopcomputer.slash"
        case .filter: "line.3.horizontal.decrease"
        case .lock: "lock"
        case .menu: "line.3.horizontal"
        case .metrics: "chart.bar"
        case .mic: "mic"
        case .minus: "minus"
        case .robot: "cpu"
        case .wrap: "text.word.spacing"
        default: "questionmark"
        }
    }
}

struct IconView: View {
    let icon: Icon
    let size: CGFloat

    init(_ icon: Icon, size: CGFloat = 18) {
        self.icon = icon
        self.size = size
    }

    var body: some View {
        Group {
            if let reicon = icon.reicon {
                ReiconIcon(reicon, size: size)
            } else {
                Image(systemName: icon.symbol)
                    .font(.system(size: size * 0.82, weight: .medium))
                    .frame(width: size, height: size)
            }
        }
        .accessibilityHidden(true)
    }
}

struct ProviderLogo: View {
    let provider: ProviderKind
    var size: CGFloat = 18

    var body: some View {
        Image(provider.iconName)
            .resizable()
            .renderingMode(.template)
            .aspectRatio(contentMode: .fit)
            .frame(width: size, height: size)
            .accessibilityHidden(true)
    }
}

// MARK: - Insulator mark

/// The Insulator "I": a bar, three stacked blocks, a bar. Drawn as a shape so
/// it takes any tint, unlike the app icon.
struct InsulatorMark: Shape {
    func path(in rect: CGRect) -> Path {
        let side = min(rect.width, rect.height)
        let origin = CGPoint(x: rect.midX - side / 2, y: rect.midY - side / 2)
        // In a 100-unit square, from the app icon's proportions.
        let pieces: [CGRect] = [
            CGRect(x: 14, y: 4, width: 72, height: 18),
            CGRect(x: 32, y: 28, width: 36, height: 13),
            CGRect(x: 32, y: 44, width: 36, height: 13),
            CGRect(x: 32, y: 60, width: 36, height: 13),
            CGRect(x: 14, y: 78, width: 72, height: 18),
        ]
        var path = Path()
        for piece in pieces {
            let scaled = CGRect(
                x: origin.x + piece.minX / 100 * side,
                y: origin.y + piece.minY / 100 * side,
                width: piece.width / 100 * side,
                height: piece.height / 100 * side
            )
            path.addRoundedRect(in: scaled, cornerSize: CGSize(width: side * 0.03, height: side * 0.03))
        }
        return path
    }
}

// MARK: - Glass

extension View {
    /// Liquid Glass on iOS 26, a thin material with a hairline before it.
    @ViewBuilder
    func glass<S: Shape>(in shape: S, tint: Color? = nil, interactive: Bool = false) -> some View {
        if #available(iOS 26.0, *) {
            self.glassEffect(Glass.regular.tint(tint).interactive(interactive), in: shape)
        } else {
            self
                .background {
                    ZStack {
                        shape.fill(.ultraThinMaterial)
                        if let tint { shape.fill(tint) }
                    }
                }
                .overlay(shape.stroke(AppTheme.stroke, lineWidth: 0.5))
        }
    }
}

/// A round glass icon button, the size of a bar button.
struct CircleButton: View {
    let icon: Icon
    let label: String
    var size: CGFloat = 44
    var on = false
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            IconView(icon, size: 20)
                .foregroundStyle(on ? AppTheme.accent : AppTheme.content)
                .frame(width: size, height: size)
                .contentShape(Circle())
        }
        .buttonStyle(.plain)
        .glass(in: Circle(), interactive: true)
        .accessibilityLabel(label)
    }
}

/// Blur that fades out away from a screen edge, so content scrolling under a
/// floating header or footer softens instead of being clipped.
struct GradualBlur: View {
    enum Edge { case top, bottom }

    let edge: Edge
    let height: CGFloat

    var body: some View {
        let stops: [Gradient.Stop] = [
            .init(color: .black, location: 0),
            .init(color: .black.opacity(0.8), location: 0.5),
            .init(color: .clear, location: 1),
        ]
        let gradient = LinearGradient(
            stops: stops,
            startPoint: edge == .top ? .top : .bottom,
            endPoint: edge == .top ? .bottom : .top
        )
        ZStack {
            Rectangle().fill(.ultraThinMaterial)
            AppTheme.background.opacity(0.55)
        }
        .mask(gradient)
        .frame(height: height)
        .frame(maxHeight: .infinity, alignment: edge == .top ? .top : .bottom)
        .ignoresSafeArea()
        .allowsHitTesting(false)
    }
}

// MARK: - Motion and feedback

enum Haptics {
    @MainActor static func selection() {
        UISelectionFeedbackGenerator().selectionChanged()
    }

    @MainActor static func impact(_ style: UIImpactFeedbackGenerator.FeedbackStyle = .light) {
        UIImpactFeedbackGenerator(style: style).impactOccurred()
    }

    @MainActor static func success() {
        UINotificationFeedbackGenerator().notificationOccurred(.success)
    }
}

struct PressedOpacityStyle: ButtonStyle {
    var opacity: Double = 0.6

    func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .contentShape(Rectangle())
            .opacity(configuration.isPressed ? opacity : 1)
    }
}

struct PressedHighlightStyle: ButtonStyle {
    var radius: CGFloat = Radii.row

    func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .contentShape(Rectangle())
            .background(
                RoundedRectangle(cornerRadius: radius, style: .continuous)
                    .fill(configuration.isPressed ? AppTheme.content.opacity(0.05) : .clear)
            )
    }
}

struct PressedScaleStyle: ButtonStyle {
    var scale: CGFloat = 1.02

    func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .contentShape(Rectangle())
            .scaleEffect(configuration.isPressed ? scale : 1)
            .animation(.spring(response: 0.25, dampingFraction: 0.7), value: configuration.isPressed)
    }
}

/// The desktop's terminal spinner: braille frames at 80ms, one clock for all.
struct TerminalSpinner: View {
    private static let frames = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"]
    var paused = false

    var body: some View {
        TimelineView(.periodic(from: .now, by: 0.08)) { context in
            let index = paused ? 0 : Int(context.date.timeIntervalSinceReferenceDate / 0.08) % Self.frames.count
            Text(Self.frames[index])
                .monospacedDigit()
        }
        .accessibilityHidden(true)
    }
}

/// Live text with a band of light sweeping through it, as the desktop's
/// `.shimmer-text`.
struct ShimmerText: View {
    let text: String
    var font: Font
    var color: Color
    var duration: Double = 1.6

    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    var body: some View {
        let label = Text(text).font(font).lineLimit(1)
        if reduceMotion {
            label.foregroundStyle(color)
        } else {
            TimelineView(.animation) { context in
                let phase = context.date.timeIntervalSinceReferenceDate
                    .truncatingRemainder(dividingBy: duration) / duration
                label
                    .foregroundStyle(color)
                    .overlay {
                        GeometryReader { proxy in
                            let width = proxy.size.width
                            let band = max(40, CGFloat(text.count) * 2 * 2)
                            LinearGradient(
                                colors: [.clear, AppTheme.content.opacity(0.9), .clear],
                                startPoint: .leading,
                                endPoint: .trailing
                            )
                            .frame(width: band)
                            .offset(x: width * (-0.25 + 1.5 * phase) - band / 2)
                        }
                        .mask(label)
                    }
            }
            .accessibilityLabel(text)
        }
    }
}

/// Content that opens downward and folds back up, clipped as it goes.
struct Collapsible<Content: View>: View {
    let open: Bool
    @ViewBuilder let content: () -> Content

    var body: some View {
        VStack(spacing: 0) {
            if open {
                content()
                    .transition(.opacity.combined(with: .move(edge: .top)))
            }
        }
        .clipped()
        .animation(.easeOut(duration: 0.22), value: open)
    }
}

/// A count with thousands separated, as 18,300 rather than 18300.
func formatCount(_ value: Int) -> String {
    value.formatted(.number.grouping(.automatic))
}

/// Compact age for list rows: "now", "5m", "3h", "2d".
func relativeTime(_ seconds: UInt64, now: Date = .now) -> String {
    let elapsed = max(0, now.timeIntervalSince1970 - TimeInterval(seconds))
    if elapsed < 60 { return "now" }
    if elapsed < 3_600 { return "\(Int(elapsed / 60))m" }
    if elapsed < 86_400 { return "\(Int(elapsed / 3_600))h" }
    return "\(Int(elapsed / 86_400))d"
}
