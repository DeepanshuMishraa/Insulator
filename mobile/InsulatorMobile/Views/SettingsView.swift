import SwiftUI

struct SettingsView: View {
    @Bindable var app: AppModel
    @Environment(\.dismiss) private var dismiss
    @Environment(AppearanceSettings.self) private var appearance
    @State private var confirmsForget = false

    var body: some View {
        @Bindable var appearance = appearance

        NavigationStack {
            List {
                Section {
                    Picker("Theme", selection: $appearance.theme) {
                        ForEach(AppThemeChoice.allCases) { theme in
                            Text(theme.name).tag(theme)
                        }
                    }
                    .listRowBackground(AppTheme.surface)

                    Picker("Font", selection: $appearance.font) {
                        ForEach(AppFontChoice.allCases) { font in
                            Text(font.isAvailable ? font.name : "\(font.name) — Not installed")
                                .tag(font)
                                .disabled(!font.isAvailable)
                        }
                    }
                    .listRowBackground(AppTheme.surface)
                } header: {
                    Text("Appearance")
                } footer: {
                    VStack(alignment: .leading, spacing: 6) {
                        Text("Themes follow the iPhone’s Light or Dark appearance automatically.")
                        if !AppFontChoice.satoshi.isAvailable {
                            Text("Satoshi requires a device-installed copy. Its license does not permit bundling the font in this source repository.")
                        }
                    }
                }

                Section("Connection") {
                    LabeledContent("Status", value: status)
                        .listRowBackground(AppTheme.surface)
                    if let address = app.connectedAddress {
                        LabeledContent("Mac") {
                            Text(address)
                                .font(AppTheme.font(.caption, monospaced: true))
                                .foregroundStyle(AppTheme.secondary)
                                .lineLimit(1)
                        }
                        .listRowBackground(AppTheme.surface)
                    }
                    LabeledContent("Protocol", value: "8")
                        .listRowBackground(AppTheme.surface)
                }

                Section("Available agents") {
                    ForEach(ProviderKind.allCases) { provider in
                    HStack(spacing: 12) {
                        Image(provider.iconName)
                            .resizable()
                            .aspectRatio(contentMode: .fit)
                            .frame(width: 18, height: 18)
                            .foregroundStyle(AppTheme.primary)
                            .frame(width: 24)
                            Text(provider.name)
                            Spacer()
                            if let probe = app.probes[provider] {
                                Text(probe.installed ? modelCount(probe) : "Not installed")
                                    .font(AppTheme.font(.caption))
                                    .foregroundStyle(probe.installed ? AppTheme.secondary : AppTheme.danger)
                            } else {
                                ProgressView().controlSize(.small)
                            }
                        }
                        .listRowBackground(AppTheme.surface)
                    }
                }

                Section {
                    Button("Disconnect") {
                        app.disconnect()
                        dismiss()
                    }
                    .listRowBackground(AppTheme.surface)
                    Button("Forget this Mac", role: .destructive) {
                        confirmsForget = true
                    }
                    .listRowBackground(AppTheme.surface)
                } footer: {
                    Text("The daemon token stays in this iPhone's Keychain until you forget the Mac.")
                }
            }
            .scrollContentBackground(.hidden)
            .listRowSeparatorTint(AppTheme.border)
            .background(AppTheme.background)
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
