import SwiftUI

enum Reicon: String {
    case arrowLeft = "ReiconArrowLeft"
    case edit = "ReiconEdit"
    case trash = "ReiconTrash"
    case more = "ReiconMore"
    case arrowDown = "ReiconArrowDown"
    case check = "ReiconCheck"
    case folder = "ReiconFolder"
    case sort = "ReiconSort"
    case closeCircle = "ReiconCloseCircle"
    case plus = "ReiconPlus"
    case stop = "ReiconStop"
    case steer = "ReiconSteer"
    case arrowUp = "ReiconArrowUp"
    case bolt = "ReiconBolt"
    case hand = "ReiconHand"
    case code = "ReiconCode"
    case shieldAlert = "ReiconShieldAlert"
    case camera = "ReiconCamera"
    case gallery = "ReiconGallery"
    case copy = "ReiconCopy"
    case fileText = "ReiconFileText"
    case close = "ReiconClose"
    case thinking = "ReiconThinking"
    case chevronRight = "ReiconChevronRight"
    case error = "ReiconError"
    case settings = "ReiconSettings"
    case laptop = "ReiconLaptop"
    case refresh = "ReiconRefresh"
    case search = "ReiconSearch"
    case compose = "ReiconCompose"
    case display = "ReiconDisplay"
    case command = "ReiconCommand"
    case list = "ReiconList"
    case tools = "ReiconTools"
    case sparkles = "ReiconSparkles"
}

struct ReiconIcon: View {
    let icon: Reicon
    let size: CGFloat

    init(_ icon: Reicon, size: CGFloat = 16) {
        self.icon = icon
        self.size = size
    }

    var body: some View {
        Image(icon.rawValue)
            .resizable()
            .renderingMode(.template)
            .scaledToFit()
            .frame(width: size, height: size)
            .accessibilityHidden(true)
    }
}
