import SwiftUI
import UIKit

// MARK: - Segmented switch

/// A glass segmented control whose lighter thumb slides behind the selected
/// segment. Unselected segments show only their icon; the selected one reveals
/// its label. Everything follows `progress`, the selected index as a
/// continuous value, so a swipe between pages moves the thumb with the finger.
struct SegmentedSwitch: View {
    struct Segment {
        let label: String
        let icon: Icon
    }

    let segments: [Segment]
    let progress: CGFloat
    let onSelect: (Int) -> Void

    private static let height: CGFloat = 44
    private static let inset: CGFloat = 4
    private static let padCollapsed: CGFloat = 12
    private static let padExpanded: CGFloat = 16
    private static let gap: CGFloat = 8
    private static let iconSize: CGFloat = 18

    @State private var labelWidths: [Int: CGFloat] = [:]

    var body: some View {
        let widths = segments.indices.map(width(of:))
        let clamped = min(CGFloat(segments.count - 1), max(0, progress))
        let from = Int(clamped.rounded(.down))
        let to = min(segments.count - 1, from + 1)
        let t = clamped - CGFloat(from)
        let thumbX = widths.prefix(from).reduce(0, +) + widths[from] * t
        let thumbWidth = widths[from] * (1 - t) + widths[to] * t

        ZStack(alignment: .leading) {
            Capsule(style: .continuous)
                .fill(AppTheme.thumb)
                .frame(width: thumbWidth, height: Self.height - Self.inset * 2)
                .offset(x: thumbX)
            HStack(spacing: 0) {
                ForEach(segments.indices, id: \.self) { index in
                    segment(index, widths: widths)
                }
            }
        }
        .padding(Self.inset)
        .frame(height: Self.height)
        .glass(in: Capsule())
        .background(measurer)
    }

    private func activeness(_ index: Int) -> CGFloat {
        max(0, 1 - abs(progress - CGFloat(index)))
    }

    private func width(of index: Int) -> CGFloat {
        let a = activeness(index)
        let pad = Self.padCollapsed + (Self.padExpanded - Self.padCollapsed) * a
        return pad * 2 + Self.iconSize + ((labelWidths[index] ?? 0) + Self.gap) * a
    }

    private func segment(_ index: Int, widths: [CGFloat]) -> some View {
        let a = activeness(index)
        let segment = segments[index]
        return Button {
            guard Int(progress.rounded()) != index else { return }
            Haptics.selection()
            onSelect(index)
        } label: {
            HStack(spacing: 0) {
                IconView(segment.icon, size: Self.iconSize)
                    .foregroundStyle(AppTheme.content)
                    .opacity(0.5 + 0.5 * a)
                Text(segment.label)
                    .font(.system(size: 15, weight: .semibold))
                    .foregroundStyle(AppTheme.content)
                    .lineLimit(1)
                    .fixedSize()
                    .padding(.leading, Self.gap)
                    .opacity(max(0, min(1, (a - 0.4) / 0.6)))
                    .frame(width: ((labelWidths[index] ?? 0) + Self.gap) * a, alignment: .leading)
                    .clipped()
            }
            .frame(width: widths[index], height: Self.height - Self.inset * 2)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .accessibilityLabel(segment.label)
        .accessibilityAddTraits(Int(progress.rounded()) == index ? .isSelected : [])
    }

    /// Lays each label out once, unseen, to learn its natural width.
    private var measurer: some View {
        ZStack {
            ForEach(segments.indices, id: \.self) { index in
                Text(segments[index].label)
                    .font(.system(size: 15, weight: .semibold))
                    .fixedSize()
                    .onGeometryChange(for: CGFloat.self) { $0.size.width } action: { width in
                        labelWidths[index] = min(120, ceil(width))
                    }
            }
        }
        .hidden()
        .accessibilityHidden(true)
    }
}

// MARK: - Pager

/// A horizontal pan that turns a screen's pages, only toward a page there is,
/// so a swipe off the edge goes to whatever is under the finger, such as a
/// row's own swipe actions.
struct PagerPan: UIGestureRecognizerRepresentable {
    let page: Int
    let pageCount: Int
    let onChanged: (CGFloat) -> Void
    let onEnded: (CGFloat, CGFloat) -> Void

    func makeCoordinator(converter: CoordinateSpaceConverter) -> Coordinator {
        Coordinator()
    }

    func makeUIGestureRecognizer(context: Context) -> UIPanGestureRecognizer {
        let recognizer = UIPanGestureRecognizer()
        recognizer.delegate = context.coordinator
        return recognizer
    }

    func updateUIGestureRecognizer(_ recognizer: UIPanGestureRecognizer, context: Context) {
        context.coordinator.page = page
        context.coordinator.pageCount = pageCount
    }

    func handleUIGestureRecognizerAction(_ recognizer: UIPanGestureRecognizer, context: Context) {
        let view = recognizer.view
        let translation = recognizer.translation(in: view).x
        switch recognizer.state {
        case .changed:
            onChanged(translation)
        case .ended, .cancelled, .failed:
            onEnded(translation, recognizer.velocity(in: view).x)
        default:
            break
        }
    }

    final class Coordinator: NSObject, UIGestureRecognizerDelegate {
        var page = 0
        var pageCount = 1

        func gestureRecognizerShouldBegin(_ recognizer: UIGestureRecognizer) -> Bool {
            guard let pan = recognizer as? UIPanGestureRecognizer else { return false }
            let velocity = pan.velocity(in: pan.view)
            guard abs(velocity.x) > abs(velocity.y) else { return false }
            let next = page + (velocity.x < 0 ? 1 : -1)
            return next >= 0 && next < pageCount
        }
    }
}

// MARK: - Ticker

/// Titles on one line scrolling left without end, as the Music app's
/// now-playing title, edges fading into the surface. A single title that
/// fits stays still. `onActive` says which one is in front.
struct Ticker: View {
    let titles: [String]
    let font: Font
    /// Advanced by the skip button: brings that title to the front.
    let front: Int
    var onActive: (Int) -> Void = { _ in }

    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @State private var widths: [CGFloat] = []
    @State private var boxWidth: CGFloat = 0
    @State private var origin = Date()
    @State private var skipOffset: CGFloat = 0

    private static let speed: CGFloat = 30
    private static let gap: CGFloat = 56
    private static let fade: CGFloat = 16

    var body: some View {
        GeometryReader { proxy in
            content(boxWidth: proxy.size.width)
                .onAppear { boxWidth = proxy.size.width }
                .onChange(of: proxy.size.width) { _, width in boxWidth = width }
        }
        .frame(height: 22)
        .mask(
            LinearGradient(
                stops: [
                    .init(color: .clear, location: 0),
                    .init(color: .black, location: 0.06),
                    .init(color: .black, location: 0.94),
                    .init(color: .clear, location: 1),
                ],
                startPoint: .leading,
                endPoint: .trailing
            )
        )
        .background(measurer)
        .onChange(of: front) { _, index in skip(to: index) }
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(titles.indices.contains(front) ? titles[front] : "")
    }

    private var total: CGFloat {
        widths.reduce(0) { $0 + $1 + Self.gap }
    }

    private var moving: Bool {
        !reduceMotion && widths.count == titles.count && boxWidth > 0
            && (titles.count > 1 || (widths.first ?? 0) > boxWidth)
    }

    @ViewBuilder
    private func content(boxWidth: CGFloat) -> some View {
        if moving {
            TimelineView(.animation) { context in
                let travel = (CGFloat(context.date.timeIntervalSince(origin)) * Self.speed + skipOffset)
                    .truncatingRemainder(dividingBy: max(total, 1))
                HStack(spacing: Self.gap) {
                    ForEach(0..<(titles.count * 2), id: \.self) { index in
                        Text(titles[index % titles.count]).font(font).lineLimit(1).fixedSize()
                    }
                }
                .offset(x: Self.fade - travel)
                .onChange(of: frontIndex(travel)) { _, index in onActive(index) }
            }
        } else {
            Text(titles.indices.contains(front) ? titles[front] : titles.first ?? "")
                .font(font)
                .lineLimit(1)
                .padding(.leading, Self.fade)
                .frame(maxWidth: .infinity, alignment: .leading)
        }
    }

    private func frontIndex(_ travel: CGFloat) -> Int {
        var start: CGFloat = 0
        for (index, width) in widths.enumerated() {
            if travel < start + width + Self.gap - Self.fade { return index }
            start += width + Self.gap
        }
        return 0
    }

    private func skip(to index: Int) {
        guard moving, widths.indices.contains(index) else { return }
        let target = widths.prefix(index).reduce(0) { $0 + $1 + Self.gap }
        let now = CGFloat(Date().timeIntervalSince(origin)) * Self.speed
        let current = (now + skipOffset).truncatingRemainder(dividingBy: max(total, 1))
        var delta = target - current
        if delta < 0 { delta += total }
        withAnimation(.easeInOut(duration: 0.52)) { skipOffset += delta }
    }

    private var measurer: some View {
        ZStack {
            ForEach(titles.indices, id: \.self) { index in
                Text(titles[index]).font(font).fixedSize()
                    .onGeometryChange(for: CGFloat.self) { $0.size.width } action: { width in
                        var next = widths
                        if next.count != titles.count { next = Array(repeating: 0, count: titles.count) }
                        next[index] = ceil(width)
                        widths = next
                    }
            }
        }
        .hidden()
    }
}

// MARK: - Keyboard

/// Whether the keyboard is up, for what steps aside while it is.
@MainActor
@Observable
final class KeyboardState {
    static let shared = KeyboardState()
    private(set) var visible = false

    private init() {
        let center = NotificationCenter.default
        center.addObserver(forName: UIResponder.keyboardWillShowNotification, object: nil, queue: .main) { _ in
            MainActor.assumeIsolated { KeyboardState.shared.visible = true }
        }
        center.addObserver(forName: UIResponder.keyboardWillHideNotification, object: nil, queue: .main) { _ in
            MainActor.assumeIsolated { KeyboardState.shared.visible = false }
        }
    }
}

/// A pulsing dot, for a connection on its way back.
struct PulsingDot: View {
    @State private var on = false

    var body: some View {
        Circle()
            .fill(AppTheme.caution)
            .frame(width: 8, height: 8)
            .opacity(on ? 1 : 0.3)
            .onAppear {
                withAnimation(.easeInOut(duration: 0.8).repeatForever(autoreverses: true)) { on = true }
            }
            .accessibilityHidden(true)
    }
}
