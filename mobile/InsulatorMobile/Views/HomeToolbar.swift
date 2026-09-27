import SwiftUI

/// Remodex-style home header: round gear button, centered app title with
/// connection status, round … menu. Shared by the task list and the
/// reconnect screen so both tops look identical.
struct HomeToolbar<RightMenu: View>: ToolbarContent {
    let host: String?
    let isConnected: Bool
    let onGear: () -> Void
    @ViewBuilder let rightMenu: () -> RightMenu

    var body: some ToolbarContent {
        ToolbarItem(placement: .topBarLeading) {
            Button(action: onGear) {
                Image(systemName: "gearshape")
                    .font(.system(size: 18, weight: .medium))
                    .foregroundStyle(.white)
                    .frame(width: 40, height: 40)
                    .background(AppTheme.raised, in: Circle())
            }
            .buttonStyle(.plain)
            .accessibilityLabel("Settings")
        }
        ToolbarItem(placement: .principal) {
            VStack(spacing: 3) {
                Text("Insulator")
                    .font(.title3.bold())
                    .foregroundStyle(.white)
                HStack(spacing: 5) {
                    Circle()
                        .fill(isConnected ? AppTheme.accent : Color.secondary)
                        .frame(width: 6, height: 6)
                        .accessibilityHidden(true)
                    Image(systemName: "laptopcomputer")
                        .font(.caption2)
                        .foregroundStyle(AppTheme.secondary)
                        .accessibilityHidden(true)
                    Text(host ?? "No Mac paired")
                        .font(.caption)
                        .foregroundStyle(AppTheme.secondary)
                        .lineLimit(1)
                }
                .accessibilityElement(children: .combine)
                .accessibilityLabel(isConnected ? "Connected to \(host ?? "Mac")" : "Offline")
            }
        }
        ToolbarItem(placement: .topBarTrailing) {
            Menu {
                rightMenu()
            } label: {
                Image(systemName: "ellipsis")
                    .font(.system(size: 17, weight: .medium))
                    .foregroundStyle(.white)
                    .frame(width: 40, height: 40)
                    .background(AppTheme.raised, in: Circle())
            }
            .buttonStyle(.plain)
            .accessibilityLabel("More actions")
        }
    }
}
