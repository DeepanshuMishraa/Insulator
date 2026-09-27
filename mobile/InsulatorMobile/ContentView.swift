import SwiftUI

struct ContentView: View {
    @State private var app = AppModel()

    var body: some View {
        Group {
            if app.isConnected {
                MainView(app: app)
            } else if app.rememberedHost != nil {
                ReconnectView(app: app)
            } else {
                ConnectionView(app: app)
            }
        }
        .background(AppTheme.background.ignoresSafeArea())
        .task { await app.connectSaved() }
        .alert("Insulator", isPresented: errorPresented) {
            Button("OK", role: .cancel) { app.errorMessage = nil }
        } message: {
            Text(app.errorMessage ?? "Unknown error")
        }
    }

    private var errorPresented: Binding<Bool> {
        Binding(
            get: { app.errorMessage != nil },
            set: { if !$0 { app.errorMessage = nil } }
        )
    }
}

#Preview {
    ContentView()
}
