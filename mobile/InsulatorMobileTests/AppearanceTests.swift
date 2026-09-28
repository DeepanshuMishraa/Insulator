import Testing
@testable import InsulatorMobile

@Suite("Appearance settings")
@MainActor
struct AppearanceTests {
    @Test("bundled fonts are registered")
    func bundledFontsAreRegistered() {
        for font in AppFontChoice.allCases where font != .satoshi {
            #expect(font.isAvailable, "\(font.name) was not registered from the app bundle")
        }
    }

    @Test("theme names are unique")
    func themeNamesAreUnique() {
        let names = AppThemeChoice.allCases.map(\.name)
        #expect(Set(names).count == names.count)
    }
}
