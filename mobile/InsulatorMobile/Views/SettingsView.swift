import SwiftUI

struct SettingsView: View {
    @Bindable var app: AppModel
    @Environment(\.dismiss) private var dismiss
    @State private var confirmsForget = false

    var body: some View {
        NavigationStack {
            List {
                Section("Connection") {
                    LabeledContent("Status", value: status)
                    if let address = app.connectedAddress {
                        LabeledContent("Mac") {
                            Text(address)
                                .font(.caption.monospaced())
                                .foregroundStyle(AppTheme.secondary)
                                .lineLimit(1)
                        }
                    }
                    LabeledContent("Protocol", value: "8")
                }

                Section("Available agents") {
                    ForEach(ProviderKind.allCases) { provider in
                    HStack(spacing: 12) {
                        Image(provider.iconName)
                            .resizable()
                            .aspectRatio(contentMode: .fit)
                            .frame(width: 18, height: 18)
                            .foregroundStyle(.white)
                            .frame(width: 24)
                            Text(provider.name)
                            Spacer()
                            if let probe = app.probes[provider] {
                                Text(probe.installed ? modelCount(probe) : "Not installed")
                                    .font(.caption)
                                    .foregroundStyle(probe.installed ? AppTheme.secondary : .red)
                            } else {
                                ProgressView().controlSize(.small)
                            }
                        }
                    }
                }

                Section {
                    Button("Disconnect") {
                        app.disconnect()
                        dismiss()
                    }
                    Button("Forget this Mac", role: .destructive) {
                        confirmsForget = true
                    }
                } footer: {
                    Text("The daemon token stays in this iPhone's Keychain until you forget the Mac.")
                }
            }
            .navigationTitle("Settings")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .confirmationAction) {
                    Button("Done") { dismiss() }
                }
            }
            .confirmationDialog("Forget this Mac?", isPresented: $confirmsForget, titleVisibility: .visible) {
                Button("Forget Mac", role: .destructive) {
                    app.disconnect(forget: true)
                    dismiss()
                }
                Button("Cancel", role: .cancel) {}
            } message: {
                Text("You will need its Tailscale address and daemon token to connect again. Running tasks will continue on the Mac.")
            }
        }
    }

    private var status: String {
        switch app.connectionState {
        case .connected(let version): "Connected · \(version)"
        case .connecting: "Connecting"
        case .disconnected: "Disconnected"
        case .failed: "Connection failed"
        }
    }

    private func modelCount(_ probe: ProviderProbe) -> String {
        probe.models.isEmpty ? "Installed" : "\(probe.models.count) models"
    }
}
