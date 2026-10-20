import PhotosUI
import SwiftUI
import UIKit

// MARK: - ASCII field

/// Interfering waves drawn in characters, drifting slowly: densest at the top
/// and thinning out before halfway down, coloured in bands that sweep across
/// it. Still, on one frame, when motion is reduced or `still` asks for it.
struct AsciiField: View {
    var opacity: Double = 0.3
    var still = false

    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    private static let ramp = Array(" .·:-=+*#%@")
    private static let fontSize: CGFloat = 10
    private static let lineHeight: CGFloat = 12
    private static let charWidth: CGFloat = fontSize * 0.602
    /// How far down the field reaches, as a share of the height.
    private static let reach: CGFloat = 0.55

    var body: some View {
        GeometryReader { proxy in
            let cols = Int((proxy.size.width / Self.charWidth).rounded(.up))
            let rows = Int((proxy.size.height * Self.reach / Self.lineHeight).rounded(.up))
            if still || reduceMotion {
                field(cols: cols, rows: rows, time: 0)
            } else {
                TimelineView(.periodic(from: .now, by: 0.1)) { context in
                    field(cols: cols, rows: rows, time: context.date.timeIntervalSinceReferenceDate.truncatingRemainder(dividingBy: 10_000))
                }
            }
        }
        .opacity(opacity)
        .clipped()
        .allowsHitTesting(false)
        .accessibilityHidden(true)
    }

    private func field(cols: Int, rows: Int, time: Double) -> some View {
        Text(Self.frame(cols: cols, rows: rows, time: time))
            .font(.custom("Menlo", size: Self.fontSize))
            .lineSpacing(Self.lineHeight - Self.fontSize * 1.17)
            .fixedSize()
            .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
    }

    /// One frame as coloured runs, so the text is a few hundred spans rather
    /// than one per character.
    static func frame(cols: Int, rows: Int, time t: Double) -> AttributedString {
        var output = AttributedString()
        var run = ""
        var runHue = 0
        func flush() {
            guard !run.isEmpty else { return }
            var piece = AttributedString(run)
            piece.foregroundColor = BrandHues.field[runHue]
            output += piece
            run = ""
        }
        // Ripples spread from just above the mark, at the top centre.
        let cx = Double(cols) / 2
        let cy = -Double(rows) * 0.1
        for y in 0..<max(rows, 0) {
            let fade = pow(1 - Double(y) / Double(rows), 1.6)
            if y > 0 { run += "\n" }
            for x in 0..<max(cols, 0) {
                let dx = Double(x) - cx
                let dy = (Double(y) - cy) * 2
                let wave = sin(Double(x) * 0.11 + t * 0.6)
                    + sin(Double(y) * 0.23 - t * 0.45)
                    + sin(Double(x + y * 2) * 0.07 + t * 0.3)
                    + sin((dx * dx + dy * dy).squareRoot() * 0.16 - t * 0.9)
                    + sin(Double(x) * 0.42 - Double(y) * 0.6 + t * 1.3) * 0.6
                let level = pow((wave + 4.6) / 9.2, 1.4) * fade
                let char = ramp[min(ramp.count - 1, Int(level * Double(ramp.count)))]
                var sweep = (Double(x) * 0.012 + Double(y) * 0.025 + wave * 0.05 - t * 0.04)
                    .truncatingRemainder(dividingBy: 1)
                if sweep < 0 { sweep += 1 }
                let hue = min(BrandHues.field.count - 1, Int(sweep * Double(BrandHues.field.count)))
                // Blanks take any colour, so they join the run before them.
                if char != " ", hue != runHue {
                    flush()
                    runHue = hue
                }
                run.append(char)
            }
        }
        flush()
        return output
    }
}

// MARK: - Empty-session photo

/// A project's empty-session background on this phone: a photo, dithered and
/// faded toward the foot as the desktop's "Haze" background, and how
/// strongly it shows.
@MainActor
@Observable
final class EmptyBackgrounds {
    static let shared = EmptyBackgrounds()
    static let opacityRange: ClosedRange<Double> = 0.05...0.65

    private(set) var images: [UUID: UIImage] = [:]
    private(set) var busy: Set<UUID> = []
    private var opacities: [String: Double]

    private static let opacityKey = "emptyBackground.opacity"

    private init() {
        opacities = UserDefaults.standard.dictionary(forKey: Self.opacityKey) as? [String: Double] ?? [:]
    }

    func image(for projectID: UUID) -> UIImage? {
        if let image = images[projectID] { return image }
        guard let data = try? Data(contentsOf: Self.file(projectID)), let image = UIImage(data: data) else { return nil }
        images[projectID] = image
        return image
    }

    func opacity(for projectID: UUID) -> Double {
        opacities[projectID.uuidString] ?? 0.45
    }

    func setOpacity(_ opacity: Double, for projectID: UUID) {
        opacities[projectID.uuidString] = opacity.clamped(to: Self.opacityRange)
        UserDefaults.standard.set(opacities, forKey: Self.opacityKey)
    }

    func set(_ picked: UIImage, for projectID: UUID, screen: CGSize) async {
        busy.insert(projectID)
        defer { busy.remove(projectID) }
        let box = SendableImage(picked)
        let rendered = await Task.detached(priority: .userInitiated) {
            Dither.render(box.image, aspect: screen.width / max(screen.height, 1))
        }.value
        guard let rendered else { return }
        try? FileManager.default.createDirectory(at: Self.directory, withIntermediateDirectories: true)
        try? rendered.pngData()?.write(to: Self.file(projectID), options: .atomic)
        images[projectID] = rendered
    }

    func remove(for projectID: UUID) {
        try? FileManager.default.removeItem(at: Self.file(projectID))
        images[projectID] = nil
    }

    private static var directory: URL {
        URL.applicationSupportDirectory.appending(path: "backgrounds", directoryHint: .isDirectory)
    }

    private static func file(_ projectID: UUID) -> URL {
        directory.appending(path: "\(projectID.uuidString).png")
    }
}

private struct SendableImage: @unchecked Sendable {
    let image: UIImage
    init(_ image: UIImage) { self.image = image }
}

/// The desktop's dither: the photo rebuilt from a dithered palette in 2×2
/// blocks, then faded out toward its foot with the fade baked into its alpha.
enum Dither {
    private static let maxHeight: CGFloat = 1600
    private static let bayer: [[Int]] = [[0, 8, 2, 10], [12, 4, 14, 6], [3, 11, 1, 9], [15, 7, 13, 5]]
    private static let mask = (values: [1, 1, 0.94, 0.72, 0.42, 0.16, 0.04, 0], locations: [0, 0.19, 0.3, 0.42, 0.54, 0.66, 0.78, 0.9])
    private static let overlay = (values: [0, 0.26, 0.55, 0.78], locations: [0.16, 0.38, 0.55, 0.75])

    static func render(_ image: UIImage, aspect: CGFloat) -> UIImage? {
        // Cropped to the screen's shape, as the empty session covers it.
        let source = image.size
        let cropWidth = min(source.width, source.height * aspect)
        let cropHeight = min(source.height, source.width / aspect)
        let scale = min(1, maxHeight / cropHeight)
        let width = Int((cropWidth * scale).rounded())
        let height = Int((cropHeight * scale).rounded())
        guard width > 0, height > 0 else { return nil }
        var pixels = [UInt8](repeating: 0, count: width * height * 4)
        let space = CGColorSpaceCreateDeviceRGB()
        guard let context = CGContext(
            data: &pixels, width: width, height: height, bitsPerComponent: 8, bytesPerRow: width * 4,
            space: space, bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue
        ) else { return nil }
        UIGraphicsPushContext(context)
        context.translateBy(x: 0, y: CGFloat(height))
        context.scaleBy(x: 1, y: -1)
        image.draw(in: CGRect(
            x: -(source.width - cropWidth) / 2 * scale,
            y: -(source.height - cropHeight) / 2 * scale,
            width: source.width * scale,
            height: source.height * scale
        ))
        UIGraphicsPopContext()

        var output = pixels
        for y in stride(from: 0, to: height, by: 2) {
            for x in stride(from: 0, to: width, by: 2) {
                let base = (min(y + 1, height - 1) * width + min(x + 1, width - 1)) * 4
                let r = Double(pixels[base]), g = Double(pixels[base + 1]), b = Double(pixels[base + 2])
                let peak = max(r, g, b)
                let threshold = Double(bayer[(y / 2) % 4][(x / 2) % 4])
                let gain = peak / 255 > (threshold + 0.5) / 16 ? 255 / max(peak, 1) : 0.08
                for dy in 0..<min(2, height - y) {
                    for dx in 0..<min(2, width - x) {
                        let index = ((y + dy) * width + x + dx) * 4
                        output[index] = UInt8(min(255, (r * gain).rounded()))
                        output[index + 1] = UInt8(min(255, (g * gain).rounded()))
                        output[index + 2] = UInt8(min(255, (b * gain).rounded()))
                    }
                }
            }
        }
        for y in 0..<height {
            let t = (Double(y) + 0.5) / Double(height)
            let alpha = (1 - gradient(overlay, t)) * gradient(mask, t)
            for x in 0..<width {
                let index = (y * width + x) * 4
                // Premultiplied, so the colour fades with the alpha.
                output[index] = UInt8(Double(output[index]) * alpha)
                output[index + 1] = UInt8(Double(output[index + 1]) * alpha)
                output[index + 2] = UInt8(Double(output[index + 2]) * alpha)
                output[index + 3] = UInt8((255 * alpha).rounded())
            }
        }
        guard let provider = CGDataProvider(data: Data(output) as CFData),
              let cgImage = CGImage(
                width: width, height: height, bitsPerComponent: 8, bitsPerPixel: 32, bytesPerRow: width * 4,
                space: space, bitmapInfo: CGBitmapInfo(rawValue: CGImageAlphaInfo.premultipliedLast.rawValue),
                provider: provider, decode: nil, shouldInterpolate: false, intent: .defaultIntent
              ) else { return nil }
        return UIImage(cgImage: cgImage)
    }

    private static func gradient(_ stops: (values: [Double], locations: [Double]), _ t: Double) -> Double {
        if t <= stops.locations[0] { return stops.values[0] }
        for index in 1..<stops.locations.count where t <= stops.locations[index] {
            let span = (t - stops.locations[index - 1]) / (stops.locations[index] - stops.locations[index - 1])
            return stops.values[index - 1] + (stops.values[index] - stops.values[index - 1]) * span
        }
        return stops.values[stops.values.count - 1]
    }
}

private extension Double {
    func clamped(to range: ClosedRange<Double>) -> Double {
        Swift.min(range.upperBound, Swift.max(range.lowerBound, self))
    }
}
