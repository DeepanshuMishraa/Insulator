import SwiftUI

/// Remembered Mac, currently unreachable: monitor icon, host, Reconnect —
/// not the first-time entry form.
struct ReconnectView: View {
    @Bindable var app: AppModel
    @State private var showsSettings = false
    @State private var showsManualPair = false
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
                            .font(.subheadline.monospaced())
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

                Image(systemName: "display")
                    .font(.system(size: 34, weight: .light))
                    .foregroundStyle(AppTheme.secondary)
                    .frame(width: 104, height: 104)
                    .background(AppTheme.raised.opacity(0.55), in: Circle())
                    .accessibilityHidden(true)

                Text(host)
                    .font(.title3.bold())
                    .foregroundStyle(.white)
                    .lineLimit(1)
                    .padding(.top, 22)

                Text("Trusted device")
                    .font(.subheadline.monospaced())
                    .foregroundStyle(AppTheme.secondary)
                    .padding(.top, 6)

                Button {
                    Task { await app.reconnect() }
                } label: {
                    HStack {
                        if isConnecting { ProgressView().tint(.black) }
                        Text(isConnecting ? "Connecting…" : "Reconnect")
                            .fontWeight(.semibold)
                    }
                    .frame(maxWidth: .infinity, minHeight: 56)
                    .foregroundStyle(.black)
                    .background(.white, in: RoundedRectangle(cornerRadius: 28))
                }
                .disabled(isConnecting)
                .padding(.horizontal, 28)
                .padding(.top, 30)

                Button {
                    showsManualPair = true
                } label: {
                    Label("Pair with Code", systemImage: "keyboard")
                        .fontWeight(.medium)
                        .frame(maxWidth: .infinity, minHeight: 52)
                        .foregroundStyle(.white)
                        .background(AppTheme.raised, in: RoundedRectangle(cornerRadius: 26))
                        .overlay(RoundedRectangle(cornerRadius: 26).stroke(AppTheme.border))
                }
                .padding(.horizontal, 28)
                .padding(.top, 12)

                Button {
                    confirmsForget = true
                } label: {
                    Text("Forget Mac")
                        .font(.subheadline.monospaced())
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
                    Button {
                        showsManualPair = true
                    } label: {
                        Label("Pair with Code", systemImage: "keyboard")
                    }
                    Button(role: .destructive) {
                        confirmsForget = true
                    } label: {
                        Label("Forget Mac", systemImage: "trash")
                    }
                }
            }
            .sheet(isPresented: $showsSettings) {
                SettingsView(app: app)
            }
            .sheet(isPresented: $showsManualPair) {
                ConnectionView(app: app)
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
        .tint(.white)
    }
}
