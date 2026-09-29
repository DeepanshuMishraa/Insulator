import SwiftUI

@main
struct InsulatorMobileApp: App {
    @State private var appearance = AppearanceSettings.shared

    var body: some Scene {
        let _ = appearance.theme
        let _ = appearance.font

        WindowGroup {
            ContentView()
                .environment(appearance)
                .font(AppTheme.font(.body))
                .foregroundStyle(AppTheme.primary)
                .tint(AppTheme.accent)
        }
    }
}
