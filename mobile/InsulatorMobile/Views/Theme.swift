import SwiftUI

enum AppTheme {
    static let background = Color.black
    static let surface = Color(red: 0.10, green: 0.10, blue: 0.11)
    static let raised = Color(red: 0.15, green: 0.15, blue: 0.16)
    static let border = Color.white.opacity(0.11)
    static let secondary = Color.white.opacity(0.56)
    static let accent = Color(red: 0.40, green: 0.88, blue: 0.58)
}

struct PressableCardStyle: ButtonStyle {
    func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .contentShape(Rectangle())
            .opacity(configuration.isPressed ? 0.66 : 1)
    }
}
