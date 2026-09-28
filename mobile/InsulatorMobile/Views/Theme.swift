import SwiftUI
import UIKit

@MainActor
@Observable
final class AppearanceSettings {
    static let shared = AppearanceSettings()

    var theme: AppThemeChoice {
        didSet { UserDefaults.standard.set(theme.rawValue, forKey: Self.themeKey) }
    }

    var font: AppFontChoice {
        didSet { UserDefaults.standard.set(font.rawValue, forKey: Self.fontKey) }
    }

    fileprivate static let themeKey = "appearance.theme"
    fileprivate static let fontKey = "appearance.font"

    private init() {
        theme = AppThemeChoice(rawValue: UserDefaults.standard.string(forKey: Self.themeKey) ?? "") ?? .system
        font = AppFontChoice(rawValue: UserDefaults.standard.string(forKey: Self.fontKey) ?? "") ?? .sfPro
    }
}

enum AppThemeChoice: String, CaseIterable, Identifiable {
    case system
    case catppuccin
    case tokyoNight
    case dracula

    var id: Self { self }

    var name: String {
        switch self {
        case .system: "System"
        case .catppuccin: "Catppuccin"
        case .tokyoNight: "Tokyo Night"
        case .dracula: "Dracula"
        }
    }

    fileprivate var palette: ThemePalette {
        switch self {
        case .system: .system
        case .catppuccin: .catppuccin
        case .tokyoNight: .tokyoNight
        case .dracula: .dracula
        }
    }
}

enum AppFontChoice: String, CaseIterable, Identifiable {
    case sfPro
    case geist
    case paperMono
    case jetBrainsMono
    case manrope
    case poppins
    case satoshi

    var id: Self { self }

    var name: String {
        switch self {
        case .sfPro: "SF Pro"
        case .geist: "Geist"
        case .paperMono: "Paper Mono"
        case .jetBrainsMono: "JetBrains Mono"
        case .manrope: "Manrope"
        case .poppins: "Poppins"
        case .satoshi: "Satoshi"
        }
    }

    fileprivate var familyName: String? {
        switch self {
        case .sfPro: nil
        case .geist: "Geist"
        case .paperMono: "Paper Mono"
        case .jetBrainsMono: "JetBrains Mono"
        case .manrope: "Manrope"
        case .poppins: "Poppins"
        case .satoshi: "Satoshi"
        }
    }

    fileprivate var isMonospaced: Bool {
        self == .paperMono || self == .jetBrainsMono
    }

    var isAvailable: Bool {
        guard let familyName else { return true }
        return UIFont(name: familyName, size: 17) != nil
    }
}

@MainActor
enum AppTheme {
    static var background: Color { palette.background }
    static var surface: Color { palette.surface }
    static var raised: Color { palette.raised }
    static var border: Color { palette.border }
    static var primary: Color { palette.primary }
    static var secondary: Color { palette.secondary }
    static var accent: Color { palette.accent }
    static var success: Color { palette.success }
    static var danger: Color { palette.danger }
    static var code: Color { palette.code }
    static var codeText: Color { palette.codeText }
    static var link: Color { palette.link }
    static var shadow: Color { Color.black.opacity(0.32) }

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
        } else if monospaced {
            base = .system(style, design: .monospaced)
        } else {
            base = .system(style)
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
        return .system(size: size, weight: weight, design: monospaced ? .monospaced : .default)
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

private struct ThemePalette {
    let background: Color
    let surface: Color
    let raised: Color
    let border: Color
    let primary: Color
    let secondary: Color
    let accent: Color
    let success: Color
    let danger: Color
    let code: Color
    let codeText: Color
    let link: Color

    init(
        light: ThemeVariant,
        dark: ThemeVariant
    ) {
        background = Color.adaptive(light.background, dark: dark.background)
        surface = Color.adaptive(light.surface, dark: dark.surface)
        raised = Color.adaptive(light.raised, dark: dark.raised)
        border = Color.adaptive(light.border, dark: dark.border)
        primary = Color.adaptive(light.primary, dark: dark.primary)
        secondary = Color.adaptive(light.secondary, dark: dark.secondary)
        accent = Color.adaptive(light.accent, dark: dark.accent)
        success = Color.adaptive(light.success, dark: dark.success)
        danger = Color.adaptive(light.danger, dark: dark.danger)
        code = Color.adaptive(light.code, dark: dark.code)
        codeText = Color.adaptive(light.codeText, dark: dark.codeText)
        link = Color.adaptive(light.link, dark: dark.link)
    }

    static let system = ThemePalette(
        light: ThemeVariant(
            background: "f7f7f8", surface: "ffffff", raised: "e9e9ec", border: "d3d3d8",
            primary: "18181b", secondary: "62626a", accent: "238636", success: "238636",
            danger: "cf222e", code: "ededf0", codeText: "24292f", link: "0969da"
        ),
        dark: ThemeVariant(
            background: "000000", surface: "1a1a1c", raised: "27272a", border: "3c3c40",
            primary: "f2f2f3", secondary: "9b9ba3", accent: "66e095", success: "66e095",
            danger: "f26d78", code: "17171c", codeText: "d1e6ff", link: "73b7ff"
        )
    )

    // Official Catppuccin Latte and Mocha tokens: https://catppuccin.com/palette/
    static let catppuccin = ThemePalette(
        light: ThemeVariant(
            background: "eff1f5", surface: "e6e9ef", raised: "ccd0da", border: "bcc0cc",
            primary: "4c4f69", secondary: "5c5f77", accent: "1e66f5", success: "40a02b",
            danger: "d20f39", code: "dce0e8", codeText: "4c4f69", link: "1e66f5"
        ),
        dark: ThemeVariant(
            background: "1e1e2e", surface: "181825", raised: "313244", border: "45475a",
            primary: "cdd6f4", secondary: "a6adc8", accent: "89b4fa", success: "a6e3a1",
            danger: "f38ba8", code: "11111b", codeText: "cdd6f4", link: "89b4fa"
        )
    )

    // Official Tokyo Night Day and Night tokens: https://github.com/folke/tokyonight.nvim
    static let tokyoNight = ThemePalette(
        light: ThemeVariant(
            background: "e1e2e7", surface: "d0d5e3", raised: "c4c8da", border: "b4b5b9",
            primary: "3760bf", secondary: "40434f", accent: "2e7de9", success: "587539",
            danger: "c64343", code: "d0d5e3", codeText: "3760bf", link: "2e7de9"
        ),
        dark: ThemeVariant(
            background: "1a1b26", surface: "16161e", raised: "292e42", border: "414868",
            primary: "c0caf5", secondary: "a9b1d6", accent: "7aa2f7", success: "9ece6a",
            danger: "f7768e", code: "16161e", codeText: "c0caf5", link: "7dcfff"
        )
    )

    // Official Alucard and Dracula tokens: https://draculatheme.com/spec
    static let dracula = ThemePalette(
        light: ThemeVariant(
            background: "fffbeb", surface: "efeddc", raised: "dedccf", border: "bcBAB3",
            primary: "1f1f1f", secondary: "6c664b", accent: "644ac9", success: "14710a",
            danger: "cb3a2a", code: "ece9df", codeText: "1f1f1f", link: "036a96"
        ),
        dark: ThemeVariant(
            background: "282a36", surface: "21222c", raised: "343746", border: "424450",
            primary: "f8f8f2", secondary: "a7acc4", accent: "bd93f9", success: "50fa7b",
            danger: "ff5555", code: "191a21", codeText: "f8f8f2", link: "8be9fd"
        )
    )
}

private struct ThemeVariant {
    let background: String
    let surface: String
    let raised: String
    let border: String
    let primary: String
    let secondary: String
    let accent: String
    let success: String
    let danger: String
    let code: String
    let codeText: String
    let link: String
}

private extension Color {
    static func adaptive(_ light: String, dark: String) -> Color {
        Color(uiColor: UIColor { traits in
            UIColor(hex: traits.userInterfaceStyle == .dark ? dark : light)
        })
    }
}

private extension UIColor {
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

struct PressableCardStyle: ButtonStyle {
    func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .contentShape(Rectangle())
            .opacity(configuration.isPressed ? 0.66 : 1)
    }
}
