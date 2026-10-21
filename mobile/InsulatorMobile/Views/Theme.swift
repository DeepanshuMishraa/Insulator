import SwiftUI
import UIKit

@MainActor
@Observable
final class AppearanceSettings {
    static let shared = AppearanceSettings()

    var theme: AppThemeChoice {
        didSet { UserDefaults.standard.set(theme.rawValue, forKey: Self.themeKey) }
    }

    var scheme: AppSchemeChoice {
        didSet { UserDefaults.standard.set(scheme.rawValue, forKey: Self.schemeKey) }
    }

    var font: AppFontChoice {
        didSet { UserDefaults.standard.set(font.rawValue, forKey: Self.fontKey) }
    }

    fileprivate static let themeKey = "appearance.theme.v2"
    fileprivate static let schemeKey = "appearance.scheme"
    fileprivate static let fontKey = "appearance.font"

    private init() {
        theme = AppThemeChoice(rawValue: UserDefaults.standard.string(forKey: Self.themeKey) ?? "") ?? .classic
        scheme = AppSchemeChoice(rawValue: UserDefaults.standard.string(forKey: Self.schemeKey) ?? "") ?? .system
        font = AppFontChoice(rawValue: UserDefaults.standard.string(forKey: Self.fontKey) ?? "") ?? .sfPro
    }
}

/// Light, dark, or whatever the phone is set to.
enum AppSchemeChoice: String, CaseIterable, Identifiable {
    case system
    case light
    case dark

    var id: Self { self }

    var name: String {
        switch self {
        case .system: "System"
        case .light: "Light"
        case .dark: "Dark"
        }
    }

    var colorScheme: ColorScheme? {
        switch self {
        case .system: nil
        case .light: .light
        case .dark: .dark
        }
    }
}

/// The themes the app draws in, each in both schemes: Classic in neutral
/// greys, the rest washing its surfaces in a hue with an accent to match.
enum AppThemeChoice: String, CaseIterable, Identifiable {
    case classic
    case cobalt
    case jade
    case copper
    case violet
    case blush

    var id: Self { self }

    var name: String {
        switch self {
        case .classic: "Classic"
        case .cobalt: "Cobalt"
        case .jade: "Jade"
        case .copper: "Copper"
        case .violet: "Violet"
        case .blush: "Blush"
        }
    }

    func palette(_ scheme: ColorScheme) -> ThemeVariant {
        switch self {
        case .classic: scheme == .dark ? .classicDark : .classicLight
        case .cobalt: .tinted(hue: 212, scheme: scheme, accent: scheme == .dark ? "5aa9f7" : "1f6fc4")
        case .jade: .tinted(hue: 150, scheme: scheme, accent: scheme == .dark ? "4cc38a" : "13804f")
        case .copper: .tinted(hue: 22, scheme: scheme, accent: scheme == .dark ? "f0884a" : "c2531a")
        case .violet: .tinted(hue: 262, scheme: scheme, accent: scheme == .dark ? "9d87f5" : "6a4fd6")
        case .blush: .tinted(hue: 338, scheme: scheme, accent: scheme == .dark ? "ec6fa3" : "c42a6c")
        }
    }

    fileprivate var palette: ThemePalette {
        ThemePalette(light: palette(.light), dark: palette(.dark))
    }
}

enum AppFontChoice: String, CaseIterable, Identifiable {
    case sfPro
    case newYork
    case geist
    case geistMono
    case paperMono
    case jetBrainsMono
    case inter
    case instrumentSans
    case manrope
    case poppins
    case newsreader
    case sourceSerif
    case lora
    case satoshi

    var id: Self { self }

    var name: String {
        switch self {
        case .sfPro: "SF Pro"
        case .newYork: "New York"
        case .geist: "Geist Sans"
        case .geistMono: "Geist Mono"
        case .paperMono: "Paper Mono"
        case .jetBrainsMono: "JetBrains Mono"
        case .inter: "Inter"
        case .instrumentSans: "Instrument Sans"
        case .manrope: "Manrope"
        case .poppins: "Poppins"
        case .newsreader: "Newsreader"
        case .sourceSerif: "Source Serif"
        case .lora: "Lora"
        case .satoshi: "Satoshi"
        }
    }

    enum Kind: String, CaseIterable {
        case sans = "Sans"
        case serif = "Serif"
        case mono = "Mono"
    }

    var kind: Kind {
        switch self {
        case .sfPro, .geist, .inter, .instrumentSans, .manrope, .poppins, .satoshi: .sans
        case .newYork, .newsreader, .sourceSerif, .lora: .serif
        case .geistMono, .paperMono, .jetBrainsMono: .mono
        }
    }

    /// The bundled family, or nil for a face the system draws itself.
    fileprivate var familyName: String? {
        switch self {
        case .sfPro, .newYork: nil
        case .geist: "Geist"
        case .geistMono: "Geist Mono"
        case .paperMono: "Paper Mono"
        case .jetBrainsMono: "JetBrains Mono"
        case .inter: "Inter"
        case .instrumentSans: "Instrument Sans"
        case .manrope: "Manrope"
        case .poppins: "Poppins"
        case .newsreader: "Newsreader"
        case .sourceSerif: "Source Serif 4"
        case .lora: "Lora"
        case .satoshi: "Satoshi"
        }
    }

    fileprivate var systemDesign: Font.Design {
        self == .newYork ? .serif : .default
    }

    fileprivate var isMonospaced: Bool {
        kind == .mono
    }

    var isAvailable: Bool {
        guard let familyName else { return true }
        return UIFont(name: familyName, size: 17) != nil
    }
}

/// The current theme's colours. Each adapts to light and dark on its own, so
/// a scheme change redraws without the views asking.
@MainActor
enum AppTheme {
    static var background: Color { palette.background }
    static var sheet: Color { palette.sheet }
    static var surface: Color { palette.surface }
    static var surfaceRaised: Color { palette.surfaceRaised }
    static var stroke: Color { palette.stroke }
    static var selection: Color { palette.selection }
    static var content: Color { palette.content }
    static var muted: Color { palette.muted }
    static var faint: Color { palette.faint }
    static var accent: Color { palette.accent }
    static var additions: Color { palette.additions }
    static var deletions: Color { palette.deletions }
    /// Amber, for warnings and full access.
    static var caution: Color { palette.caution }
    /// A file named in a transcript, apart from prose, code and links.
    static var fileRef: Color { palette.fileRef }
    static var inverse: Color { palette.inverse }
    static var inverseSurface: Color { palette.inverseSurface }
    /// Round buttons and pills over glass.
    static var fill: Color { palette.fill }
    /// A segmented switch's thumb, inside the glass.
    static var thumb: Color { palette.thumb }
    /// Laid over the Insulator mark.
    static var mark: Color { palette.mark }
    /// A transcript's fold rails and connectors.
    static var rail: Color { palette.rail }
    /// The user's message bubble.
    static var bubble: Color { palette.bubble }

    static func font(
        _ style: Font.TextStyle,
        weight: Font.Weight? = nil,
        monospaced: Bool = false
    ) -> Font {
        let selected = AppearanceSettings.shared.font
        let family = monospaced && !selected.isMonospaced ? "JetBrains Mono" : selected.familyName
        let base: Font
        if let family, UIFont(name: family, size: 17) != nil {
            base = .custom(family, size: pointSize(for: style), relativeTo: style)
        } else {
            base = .system(style, design: monospaced ? .monospaced : selected.systemDesign)
        }
        return weight.map(base.weight) ?? base
    }

    static func font(
        size: CGFloat,
        weight: Font.Weight = .regular,
        monospaced: Bool = false
    ) -> Font {
        let selected = AppearanceSettings.shared.font
        let family = monospaced && !selected.isMonospaced ? "JetBrains Mono" : selected.familyName
        if let family, UIFont(name: family, size: size) != nil {
            return .custom(family, size: size).weight(weight)
        }
        return .system(size: size, weight: weight, design: monospaced ? .monospaced : selected.systemDesign)
    }

    /// Code, diffs and file contents: Menlo, as the desktop's code views.
    static func code(size: CGFloat) -> Font {
        .custom("Menlo", size: size)
    }

    private static var palette: ThemePalette {
        AppearanceSettings.shared.theme.palette
    }

    private static func pointSize(for style: Font.TextStyle) -> CGFloat {
        switch style {
        case .largeTitle: 34
        case .title: 28
        case .title2: 22
        case .title3: 20
        case .headline: 17
        case .subheadline: 15
        case .callout: 16
        case .caption: 12
        case .caption2: 11
        case .footnote: 13
        default: 17
        }
    }
}

/// Diff and change-status colours, as the desktop's changes panel draws them.
@MainActor
enum DiffTheme {
    static let addRow = Color(red: 16 / 255, green: 185 / 255, blue: 129 / 255).opacity(0.15)
    static let addGutter = Color(red: 16 / 255, green: 185 / 255, blue: 129 / 255).opacity(0.25)
    static let deleteRow = Color(red: 244 / 255, green: 63 / 255, blue: 94 / 255).opacity(0.15)
    static let deleteGutter = Color(red: 244 / 255, green: 63 / 255, blue: 94 / 255).opacity(0.25)
    static let addText = Color.adaptive("047857", dark: "6ee7b7")
    static let deleteText = Color.adaptive("be123c", dark: "fda4af")

    static func status(_ status: FileChangeStatus) -> Color {
        switch status {
        case .modified: .adaptive("b45309", dark: "fbbf24")
        case .added: .adaptive("047857", dark: "6ee7b7")
        case .deleted: .adaptive("be123c", dark: "fda4af")
        case .untracked: .adaptive("0284c7", dark: "38bdf8")
        }
    }
}

/// The ASCII field's sweep, and the colours mascots are painted in.
enum BrandHues {
    static let field = ["4c9ef5", "a78bfa", "f472b6", "fbbf24", "6ee7b7"].map(Color.init(hex:))
}

struct ThemeVariant {
    let background: String
    let sheet: String
    let surface: String
    let surfaceRaised: String
    let stroke: RGBA
    let selection: RGBA
    let content: String
    let muted: String
    let faint: String
    let accent: String
    let additions: String
    let deletions: String
    let caution: String
    let fileRef: String
    let inverse: String
    let inverseSurface: String
    let fill: RGBA
    let thumb: RGBA
    let mark: String
    let rail: String
    let bubble: String

    struct RGBA {
        let hex: String
        let alpha: Double
    }

    static let classicDark = ThemeVariant(
        background: "0f0f0f", sheet: "141414", surface: "1c1c1c", surfaceRaised: "262626",
        stroke: RGBA(hex: "ebebeb", alpha: 0.08), selection: RGBA(hex: "ebebeb", alpha: 0.10),
        content: "ebebeb", muted: "8c8c8c", faint: "5c5c5c", accent: "4c9ef5",
        additions: "6ee7b7", deletions: "f87171", caution: "fbbf24", fileRef: "c4b5fd",
        inverse: "0c0c0c", inverseSurface: "f2f2f2",
        fill: RGBA(hex: "ffffff", alpha: 0.08), thumb: RGBA(hex: "ffffff", alpha: 0.14),
        mark: "ffffff", rail: "2e2e2e", bubble: "252525"
    )

    static let classicLight = ThemeVariant(
        background: "f7f7f7", sheet: "ffffff", surface: "ededed", surfaceRaised: "e3e3e3",
        stroke: RGBA(hex: "2e2e2e", alpha: 0.10), selection: RGBA(hex: "2e2e2e", alpha: 0.06),
        content: "2e2e2e", muted: "737373", faint: "a6a6a6", accent: "1f7ae0",
        additions: "047857", deletions: "be123c", caution: "b45309", fileRef: "6d28d9",
        inverse: "fafafa", inverseSurface: "262626",
        fill: RGBA(hex: "2e2e2e", alpha: 0.09), thumb: RGBA(hex: "2e2e2e", alpha: 0.10),
        mark: "2e2e2e", rail: "dadada", bubble: "e9e9e9"
    )

    /// Classic's surfaces washed in `hue`: dark's barely, so text keeps its
    /// contrast, and light's fainter still.
    static func tinted(hue: Double, scheme: ColorScheme, accent: String) -> ThemeVariant {
        let base = scheme == .dark ? classicDark : classicLight
        if scheme == .dark {
            return base.with(
                background: hsl(hue, 22, 7), sheet: hsl(hue, 20, 9), surface: hsl(hue, 16, 12.5),
                surfaceRaised: hsl(hue, 14, 16.5), rail: hsl(hue, 12, 19), bubble: hsl(hue, 16, 16),
                accent: accent
            )
        }
        return base.with(
            background: hsl(hue, 30, 96.5), sheet: base.sheet, surface: hsl(hue, 22, 92.5),
            surfaceRaised: hsl(hue, 18, 88.5), rail: hsl(hue, 16, 85), bubble: hsl(hue, 22, 91),
            accent: accent
        )
    }

    private func with(
        background: String, sheet: String, surface: String, surfaceRaised: String,
        rail: String, bubble: String, accent: String
    ) -> ThemeVariant {
        ThemeVariant(
            background: background, sheet: sheet, surface: surface, surfaceRaised: surfaceRaised,
            stroke: stroke, selection: selection, content: content, muted: muted, faint: faint,
            accent: accent, additions: additions, deletions: deletions, caution: caution,
            fileRef: fileRef, inverse: inverse, inverseSurface: inverseSurface, fill: fill,
            thumb: thumb, mark: mark, rail: rail, bubble: bubble
        )
    }
}

/// `hsl()` as `rrggbb`.
func hsl(_ hue: Double, _ saturation: Double, _ lightness: Double) -> String {
    let s = saturation / 100
    let l = lightness / 100
    let a = s * min(l, 1 - l)
    func channel(_ n: Double) -> Int {
        let k = (n + hue / 30).truncatingRemainder(dividingBy: 12)
        return Int((255 * (l - a * max(-1, min(k - 3, 9 - k, 1)))).rounded())
    }
    return String(format: "%02x%02x%02x", channel(0), channel(8), channel(4))
}

private struct ThemePalette {
    let background: Color
    let sheet: Color
    let surface: Color
    let surfaceRaised: Color
    let stroke: Color
    let selection: Color
    let content: Color
    let muted: Color
    let faint: Color
    let accent: Color
    let additions: Color
    let deletions: Color
    let caution: Color
    let fileRef: Color
    let inverse: Color
    let inverseSurface: Color
    let fill: Color
    let thumb: Color
    let mark: Color
    let rail: Color
    let bubble: Color

    init(light: ThemeVariant, dark: ThemeVariant) {
        background = .adaptive(light.background, dark: dark.background)
        sheet = .adaptive(light.sheet, dark: dark.sheet)
        surface = .adaptive(light.surface, dark: dark.surface)
        surfaceRaised = .adaptive(light.surfaceRaised, dark: dark.surfaceRaised)
        stroke = .adaptive(light.stroke, dark: dark.stroke)
        selection = .adaptive(light.selection, dark: dark.selection)
        content = .adaptive(light.content, dark: dark.content)
        muted = .adaptive(light.muted, dark: dark.muted)
        faint = .adaptive(light.faint, dark: dark.faint)
        accent = .adaptive(light.accent, dark: dark.accent)
        additions = .adaptive(light.additions, dark: dark.additions)
        deletions = .adaptive(light.deletions, dark: dark.deletions)
        caution = .adaptive(light.caution, dark: dark.caution)
        fileRef = .adaptive(light.fileRef, dark: dark.fileRef)
        inverse = .adaptive(light.inverse, dark: dark.inverse)
        inverseSurface = .adaptive(light.inverseSurface, dark: dark.inverseSurface)
        fill = .adaptive(light.fill, dark: dark.fill)
        thumb = .adaptive(light.thumb, dark: dark.thumb)
        mark = .adaptive(light.mark, dark: dark.mark)
        rail = .adaptive(light.rail, dark: dark.rail)
        bubble = .adaptive(light.bubble, dark: dark.bubble)
    }
}

extension Color {
    init(hex: String) {
        self.init(uiColor: UIColor(hex: hex))
    }

    static func adaptive(_ light: String, dark: String) -> Color {
        Color(uiColor: UIColor { traits in
            UIColor(hex: traits.userInterfaceStyle == .dark ? dark : light)
        })
    }

    fileprivate static func adaptive(_ light: ThemeVariant.RGBA, dark: ThemeVariant.RGBA) -> Color {
        Color(uiColor: UIColor { traits in
            let value = traits.userInterfaceStyle == .dark ? dark : light
            return UIColor(hex: value.hex).withAlphaComponent(value.alpha)
        })
    }
}

extension UIColor {
    convenience init(hex: String) {
        let value = UInt64(hex, radix: 16) ?? 0
        self.init(
            red: CGFloat((value >> 16) & 0xff) / 255,
            green: CGFloat((value >> 8) & 0xff) / 255,
            blue: CGFloat(value & 0xff) / 255,
            alpha: 1
        )
    }
}
