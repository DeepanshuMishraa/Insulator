import SwiftUI

struct ConnectionView: View {
    @Bindable var app: AppModel
    @State private var address = ""
    @State private var token = ""

    var body: some View {
        NavigationStack {
            VStack(alignment: .leading, spacing: 24) {
                Spacer()

                VStack(alignment: .center, spacing: 12) {
                    Image("AppLogo")
                        .resizable()
                        .aspectRatio(contentMode: .fit)
                        .frame(width: 88, height: 88)
                        .clipShape(RoundedRectangle(cornerRadius: 22, style: .continuous))
                        .accessibilityHidden(true)

                    Text("Connect to Insulator")
                        .font(.largeTitle.bold())

                    Text("Enter the Tailscale IP and token shown in Insulator Desktop under Settings → Daemon. Both devices just need to be on the same Tailnet — they can be on different networks.")
                        .font(.body)
                        .multilineTextAlignment(.center)
                        .foregroundStyle(AppTheme.secondary)
                        .fixedSize(horizontal: false, vertical: true)
                }
                .frame(maxWidth: .infinity)

                VStack(spacing: 12) {
                    TextField("100.x.x.x:port", text: $address)
                        .textInputAutocapitalization(.never)
                        .autocorrectionDisabled()
                        .keyboardType(.URL)
                        .textContentType(.URL)
                        .padding(14)
                        .background(AppTheme.raised, in: RoundedRectangle(cornerRadius: 14))
                        .accessibilityLabel("Daemon address")

                    SecureField("Daemon token", text: $token)
                        .textInputAutocapitalization(.never)
                        .autocorrectionDisabled()
                        .textContentType(.password)
                        .padding(14)
                        .background(AppTheme.raised, in: RoundedRectangle(cornerRadius: 14))
                }

                Button {
                    Task { await app.connect(address: address, token: token) }
                } label: {
                    HStack {
                        if app.connectionState == .connecting {
                            ProgressView().tint(.black)
                        }
                        Text(app.connectionState == .connecting ? "Connecting…" : "Connect")
                            .fontWeight(.semibold)
                    }
                    .frame(maxWidth: .infinity, minHeight: 50)
                    .foregroundStyle(.black)
                    .background(.white, in: RoundedRectangle(cornerRadius: 15))
                }
                .disabled(address.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty || token.isEmpty || app.connectionState == .connecting)

                Text("Use the Mac's Tailscale IP from Desktop Settings → Daemon (for example 100.x.x.x:34123). The token stays in this iPhone's Keychain.")
                    .font(.footnote)
                    .foregroundStyle(AppTheme.secondary)

                Spacer()
            }
            .padding(24)
            .background(AppTheme.background.ignoresSafeArea())
        }
    }
}
