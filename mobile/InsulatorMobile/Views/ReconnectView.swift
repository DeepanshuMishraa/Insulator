import SwiftUI

/// Remembered Mac, currently unreachable: monitor icon, host, Reconnect —
/// not the first-time entry form.
struct ReconnectView: View {
    @Bindable var app: AppModel
    @State private var showsSettings = false
    @State private var confirmsForget = false

    private var isConnecting: Bool { app.connectionState == .connecting }
    private var host: String { app.displayHost ?? app.rememberedHost ?? "Mac" }

    var body: some View {
        NavigationStack {
            VStack(spacing: 0) {
                HStack {
                    HStack(spacing: 7) {
                        Circle()
                            .fill(Color.secondary)
                            .frame(width: 7, height: 7)
                            .accessibilityHidden(true)
                        Text("Offline")
                            .font(AppTheme.font(.subheadline, monospaced: true))
                            .foregroundStyle(AppTheme.secondary)
                    }
                    .padding(.horizontal, 14)
                    .frame(minHeight: 32)
                    .background(AppTheme.raised, in: Capsule())
                    .accessibilityElement(children: .combine)
                    .accessibilityLabel("Offline")
                    Spacer()
                }
                .padding(.horizontal, 20)
                .padding(.top, 6)

                Spacer()

                ReiconIcon(.display, size: 34)
                    .foregroundStyle(AppTheme.secondary)
                    .frame(width: 104, height: 104)
                    .background(AppTheme.raised.opacity(0.55), in: Circle())
                    .accessibilityHidden(true)

                Text(host)
                    .font(AppTheme.font(.title3, weight: .bold))
                    .foregroundStyle(AppTheme.primary)
                    .lineLimit(1)
                    .padding(.top, 22)

                Text("Trusted device")
                    .font(AppTheme.font(.subheadline, monospaced: true))
                    .foregroundStyle(AppTheme.secondary)
                    .padding(.top, 6)

                Button {
                    Task { await app.reconnect() }
                } label: {
                    HStack {
                        if isConnecting { ProgressView().tint(AppTheme.background) }
                        Text(isConnecting ? "Connecting…" : "Reconnect")
                            .fontWeight(.semibold)
                    }
                    .frame(maxWidth: .infinity, minHeight: 56)
                    .foregroundStyle(AppTheme.background)
                    .background(AppTheme.primary, in: RoundedRectangle(cornerRadius: 28))
                }
                .disabled(isConnecting)
                .padding(.horizontal, 28)
                .padding(.top, 30)


                Button {
                    confirmsForget = true
                } label: {
                    Text("Forget Mac")
                        .font(AppTheme.font(.subheadline, monospaced: true))
                        .foregroundStyle(AppTheme.secondary)
                }
                .padding(.top, 26)

                Spacer()
            }
            .frame(maxWidth: .infinity, maxHeight: .infinity)
            .background(AppTheme.background)
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                HomeToolbar(
                    host: app.displayHost,
                    isConnected: false,
                    onGear: { showsSettings = true }
                ) {
                    Button(role: .destructive) {
                        confirmsForget = true
                    } label: {
                        Label("Forget Mac", image: Reicon.trash.rawValue)
                    }
                }
            }
            .sheet(isPresented: $showsSettings) {
                SettingsView(app: app)
            }
            .confirmationDialog("Forget this Mac?", isPresented: $confirmsForget, titleVisibility: .visible) {
                Button("Forget Mac", role: .destructive) {
                    app.disconnect(forget: true)
                }
                Button("Cancel", role: .cancel) {}
            } message: {
                Text("You will need its Tailscale address and daemon token to connect again. Running tasks will continue on the Mac.")
            }
        }
        .tint(AppTheme.primary)
    }
}
