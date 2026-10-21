import SwiftUI

struct ContentView: View {
    @Environment(\.scenePhase) private var scenePhase
    @Environment(AppearanceSettings.self) private var appearance
    @State private var app = AppModel()

    var body: some View {
        Group {
            // Once a workspace has loaded it stays up while the Mac is away,
            // saying so in the composer, rather than dropping to pairing.
            if app.isConnected || !app.projects.isEmpty {
                WorkspaceView(app: app)
            } else {
                ConnectScreen(app: app)
            }
        }
        .background(AppTheme.background.ignoresSafeArea())
        .preferredColorScheme(appearance.scheme.colorScheme)
        .task { await app.connectSaved() }
        .task(id: reconnectKey) { await retryWhileOffline() }
        .onChange(of: scenePhase) { _, phase in
            if phase == .active { Task { await app.recover() } }
        }
        .onOpenURL { url in
            Task { await app.pair(link: url.absoluteString) }
        }
        .alert("Insulator", isPresented: errorPresented) {
            Button("OK", role: .cancel) { app.errorMessage = nil }
        } message: {
            Text(app.errorMessage ?? "Unknown error")
        }
    }

    private var reconnectKey: String {
        "\(app.connectionState)-\(scenePhase == .active)"
    }

    /// The workspace is up but the Mac is not: try again every few seconds
    /// while the app is in front.
    private func retryWhileOffline() async {
        guard scenePhase == .active, !app.projects.isEmpty, case .failed = app.connectionState else { return }
        try? await Task.sleep(for: .seconds(4))
        guard !Task.isCancelled else { return }
        await app.reconnect()
    }

    private var errorPresented: Binding<Bool> {
        Binding(
            get: { app.errorMessage != nil && app.isConnected },
            set: { if !$0 { app.errorMessage = nil } }
        )
    }
}

/// Pushes and sheets over the workspace.
@MainActor
@Observable
final class Router {
    enum Sheet: Identifiable {
        case settings(projectID: UUID?)
        case projects
        case models(projectID: UUID, sessionID: UUID?)
        case changes(ChangesTarget)
        case file(root: String, path: String)

        var id: String {
            switch self {
            case .settings: "settings"
            case .projects: "projects"
            case .models(let project, let session): "models-\(project)-\(session?.uuidString ?? "")"
            case .changes(let target): "changes-\(target.cwd)"
            case .file(let root, let path): "file-\(root)-\(path)"
            }
        }
    }

    var path: [UUID] = []
    var sheet: Sheet?

    func open(_ sessionID: UUID) {
        if path.last != sessionID { path.append(sessionID) }
    }
}

/// The working copy a Changes sheet reads and commits.
struct ChangesTarget: Hashable {
    let cwd: String
    let provider: ProviderKind?
}

struct WorkspaceView: View {
    @Bindable var app: AppModel
    @State private var router = Router()

    var body: some View {
        NavigationStack(path: $router.path) {
            Group {
                if let project = app.currentProject {
                    ProjectScreen(app: app, project: project)
                        .id(project.id)
                } else {
                    EmptyWorkspace(app: app)
                }
            }
            .navigationDestination(for: UUID.self) { sessionID in
                SessionScreen(app: app, sessionID: sessionID)
            }
        }
        .environment(router)
        .tint(AppTheme.content)
        .sheet(item: $router.sheet) { sheet in
            switch sheet {
            case .settings(let projectID):
                SettingsScreen(app: app, projectID: projectID)
            case .projects:
                ProjectPicker(app: app)
            case .models(let projectID, let sessionID):
                ModelPicker(app: app, projectID: projectID, sessionID: sessionID)
            case .changes(let target):
                ChangesScreen(app: app, target: target)
            case .file(let root, let path):
                FileScreen(app: app, root: root, path: path)
            }
        }
        .task { SessionNotifier.requestPermission() }
    }
}

/// No projects on the phone yet.
private struct EmptyWorkspace: View {
    @Bindable var app: AppModel
    @Environment(Router.self) private var router

    var body: some View {
        VStack(alignment: .leading, spacing: 24) {
            Text("Make yourself at home.")
                .font(AppTheme.font(size: 34, weight: .semibold))
                .foregroundStyle(AppTheme.content)
            Text(app.projects.isEmpty
                 ? "Add a project in Insulator on your Mac, and it shows up here."
                 : "Choose the projects you want on your phone.")
                .font(AppTheme.font(size: 16))
                .foregroundStyle(AppTheme.muted)
            Button {
                router.sheet = .settings(projectID: nil)
            } label: {
                Text("Open settings")
                    .font(AppTheme.font(size: 16, weight: .semibold))
                    .foregroundStyle(AppTheme.inverse)
                    .frame(maxWidth: .infinity, minHeight: 54)
                    .background(AppTheme.inverseSurface, in: Capsule())
            }
        }
        .padding(28)
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .background(AppTheme.background.ignoresSafeArea())
    }
}
