@preconcurrency import AVFoundation
import SwiftUI

/// The first screen on a fresh install: a field of drifting ASCII waves that
/// thins out before halfway down, the mark at the head, and at the foot the
/// pitch above the ways to pair. Mascots rain down over it all, landing on
/// the words and buttons, and tumble as the phone tilts.
struct ConnectScreen: View {
    @Bindable var app: AppModel
    @State private var scanning = false
    @State private var pasting = false
    @State private var cameraError: String?
    @State private var frames: [String: CGRect] = [:]
    @State private var appeared = false

    private var working: Bool { app.connectionState == .connecting }

    var body: some View {
        ZStack {
            AppTheme.background.ignoresSafeArea()
            AsciiField()
                .ignoresSafeArea()
                .opacity(appeared ? 1 : 0)
            MascotRain(colliders: colliders)
                .ignoresSafeArea()
            VStack(spacing: 0) {
                VStack(spacing: 8) {
                    InsulatorMark()
                        .fill(AppTheme.content)
                        .frame(width: 44, height: 44)
                    Text("Insulator")
                        .font(.system(size: 15, weight: .semibold))
                        .foregroundStyle(AppTheme.content.opacity(0.8))
                }
                .padding(.top, 8)
                .opacity(appeared ? 1 : 0)

                VStack(alignment: .leading, spacing: 14) {
                    Text("Keep coding,\nwherever you are.")
                        .font(.system(size: 38, weight: .bold))
                        .lineSpacing(6)
                        .foregroundStyle(AppTheme.content)
                        .measured("title", into: $frames)
                    Text(app.rememberedHost.map { "Reconnect to \($0) to pick up your sessions where you left them." }
                        ?? "Pair this phone with Insulator on your Mac to pick up your sessions on the go.")
                        .font(.system(size: 16))
                        .lineSpacing(7)
                        .foregroundStyle(AppTheme.content.opacity(0.78))
                        .frame(maxWidth: 320, alignment: .leading)
                        .measured("subtitle", into: $frames)
                }
                .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .bottomLeading)
                .padding(.bottom, 40)
                .offset(y: appeared ? 0 : 20)
                .opacity(appeared ? 1 : 0)

                VStack(spacing: 12) {
                    if working {
                        ProgressView().tint(AppTheme.content).padding(.bottom, 4)
                    }
                    if let error = cameraError ?? connectionError {
                        Text(error)
                            .font(.system(size: 14))
                            .foregroundStyle(AppTheme.deletions)
                            .frame(maxWidth: .infinity, alignment: .leading)
                            .padding(.bottom, 4)
                    }
                    if app.rememberedHost != nil {
                        primaryButton("Reconnect") { Task { await app.reconnect() } }
                        secondaryButton("Pair another Mac") { scan() }
                    } else {
                        primaryButton("Connect to desktop") { scan() }
                        secondaryButton("Paste a pairing link") { pasting = true }
                    }
                }
                .offset(y: appeared ? 0 : 20)
                .opacity(appeared ? 1 : 0)
            }
            .padding(.horizontal, 28)
            .padding(.bottom, 12)
        }
        .onAppear {
            withAnimation(.easeOut(duration: 0.7).delay(0.15)) { appeared = true }
        }
        .fullScreenCover(isPresented: $scanning) {
            ScannerScreen { code in
                scanning = false
                Task { await app.pair(link: code) }
            } onCancel: {
                scanning = false
            }
        }
        .sheet(isPresented: $pasting) {
            PairingLinkSheet(app: app)
                .presentationDetents([.medium, .large])
        }
    }

    private var connectionError: String? {
        if case .failed(let message) = app.connectionState { return message }
        return nil
    }

    private var colliders: [RainCollider] {
        frames.map { name, rect in
            RainCollider(rect: rect, radius: name.hasPrefix("button") ? rect.height / 2 : 2)
        }
    }

    private func scan() {
        cameraError = nil
        switch AVCaptureDevice.authorizationStatus(for: .video) {
        case .authorized:
            scanning = true
        case .notDetermined:
            AVCaptureDevice.requestAccess(for: .video) { granted in
                Task { @MainActor in
                    if granted { scanning = true } else { denied() }
                }
            }
        default:
            denied()
        }
    }

    private func denied() {
        cameraError = "Allow camera access in your phone’s settings, or paste a pairing link."
        pasting = true
    }

    private func primaryButton(_ title: String, action: @escaping () -> Void) -> some View {
        Button(action: action) {
            Text(title)
                .font(.system(size: 16, weight: .semibold))
                .foregroundStyle(AppTheme.inverse)
                .frame(maxWidth: .infinity, minHeight: 54)
                .contentShape(Capsule())
        }
        .buttonStyle(PressedScaleStyle(scale: 1.03))
        .glass(in: Capsule(), tint: AppTheme.inverseSurface, interactive: true)
        .disabled(working)
        .measured("button-primary", into: $frames)
    }

    private func secondaryButton(_ title: String, action: @escaping () -> Void) -> some View {
        Button(action: action) {
            Text(title)
                .font(.system(size: 16, weight: .semibold))
                .foregroundStyle(AppTheme.content)
                .frame(maxWidth: .infinity, minHeight: 54)
                .contentShape(Capsule())
        }
        .buttonStyle(PressedScaleStyle(scale: 1.03))
        .glass(in: Capsule(), interactive: true)
        .disabled(working)
        .measured("button-secondary", into: $frames)
    }
}

private extension View {
    /// Records this view's frame in the connect screen, for the mascots to land on.
    func measured(_ name: String, into frames: Binding<[String: CGRect]>) -> some View {
        onGeometryChange(for: CGRect.self) { $0.frame(in: .global) } action: { rect in
            frames.wrappedValue[name] = rect
        }
    }
}

/// A pairing link pasted in, or the Mac's address and token typed by hand.
struct PairingLinkSheet: View {
    @Bindable var app: AppModel
    @Environment(\.dismiss) private var dismiss
    @State private var link = ""
    @State private var manual = false
    @State private var address = ""
    @State private var token = ""
    @FocusState private var focused: Bool

    var body: some View {
        NavigationStack {
            VStack(alignment: .leading, spacing: 14) {
                if manual {
                    Text("Enter the Tailscale address and token shown in Insulator Desktop under Settings → Daemon.")
                        .font(.system(size: 15))
                        .foregroundStyle(AppTheme.muted)
                    field("100.x.x.x:port", text: $address, secure: false)
                    field("Daemon token", text: $token, secure: true)
                } else {
                    Text("Copy the pairing link from Insulator Desktop under Settings → Daemon, with the token revealed, and paste it here.")
                        .font(.system(size: 15))
                        .foregroundStyle(AppTheme.muted)
                    field("insulator://pair?…", text: $link, secure: false)
                    Button("Paste") {
                        link = UIPasteboard.general.string ?? link
                    }
                    .font(.system(size: 15, weight: .medium))
                    .tint(AppTheme.accent)
                }
                Spacer()
                Button {
                    Task {
                        if manual {
                            await app.connect(address: address, token: token)
                        } else {
                            await app.pair(link: link)
                        }
                        if app.isConnected { dismiss() }
                    }
                } label: {
                    Text(app.connectionState == .connecting ? "Connecting…" : "Connect")
                        .font(.system(size: 16, weight: .semibold))
                        .foregroundStyle(AppTheme.inverse)
                        .frame(maxWidth: .infinity, minHeight: 54)
                        .background(AppTheme.inverseSurface, in: Capsule())
                }
                .disabled(manual ? address.isEmpty || token.isEmpty : link.isEmpty)
                Button(manual ? "Use a pairing link instead" : "Enter address and token instead") {
                    withAnimation(.snappy) { manual.toggle() }
                }
                .font(.system(size: 15))
                .tint(AppTheme.accent)
                .frame(maxWidth: .infinity)
            }
            .padding(20)
            .background(AppTheme.sheet.ignoresSafeArea())
            .navigationTitle(manual ? "Connect manually" : "Pairing link")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .topBarLeading) {
                    Button { dismiss() } label: { IconView(.close, size: 18) }
                        .accessibilityLabel("Close")
                }
            }
            .onAppear { focused = true }
        }
    }

    private func field(_ placeholder: String, text: Binding<String>, secure: Bool) -> some View {
        Group {
            if secure {
                SecureField(placeholder, text: text)
            } else {
                TextField(placeholder, text: text)
                    .keyboardType(.URL)
            }
        }
        .textInputAutocapitalization(.never)
        .autocorrectionDisabled()
        .focused($focused)
        .font(.system(size: 16))
        .padding(14)
        .background(AppTheme.content.opacity(0.07), in: RoundedRectangle(cornerRadius: 14, style: .continuous))
    }
}

/// The camera, reading the desktop's pairing QR code.
struct ScannerScreen: View {
    let onScan: (String) -> Void
    let onCancel: () -> Void

    var body: some View {
        VStack(spacing: 0) {
            QRScanner(onScan: onScan)
                .ignoresSafeArea(edges: .top)
            VStack(spacing: 12) {
                Text("Scan the code in Insulator Desktop under Settings → Daemon")
                    .font(.system(size: 16))
                    .foregroundStyle(AppTheme.muted)
                    .multilineTextAlignment(.center)
                Button("Cancel", action: onCancel)
                    .font(.system(size: 16))
                    .tint(AppTheme.accent)
                    .padding(14)
            }
            .padding(24)
        }
        .background(AppTheme.background.ignoresSafeArea())
    }
}

private struct QRScanner: UIViewControllerRepresentable {
    let onScan: (String) -> Void

    func makeUIViewController(context: Context) -> ScannerController {
        let controller = ScannerController()
        controller.onScan = onScan
        return controller
    }

    func updateUIViewController(_ controller: ScannerController, context: Context) {
        controller.onScan = onScan
    }

    final class ScannerController: UIViewController, AVCaptureMetadataOutputObjectsDelegate {
        var onScan: ((String) -> Void)?
        private let session = AVCaptureSession()
        private var preview: AVCaptureVideoPreviewLayer?
        private var scanned = false

        override func viewDidLoad() {
            super.viewDidLoad()
            view.backgroundColor = .black
            guard let device = AVCaptureDevice.default(for: .video),
                  let input = try? AVCaptureDeviceInput(device: device),
                  session.canAddInput(input) else { return }
            session.addInput(input)
            let output = AVCaptureMetadataOutput()
            guard session.canAddOutput(output) else { return }
            session.addOutput(output)
            output.setMetadataObjectsDelegate(self, queue: .main)
            output.metadataObjectTypes = [.qr]
            let preview = AVCaptureVideoPreviewLayer(session: session)
            preview.videoGravity = .resizeAspectFill
            view.layer.addSublayer(preview)
            self.preview = preview
        }

        override func viewDidLayoutSubviews() {
            super.viewDidLayoutSubviews()
            preview?.frame = view.bounds
        }

        override func viewWillAppear(_ animated: Bool) {
            super.viewWillAppear(animated)
            let session = session
            DispatchQueue.global(qos: .userInitiated).async { session.startRunning() }
        }

        override func viewWillDisappear(_ animated: Bool) {
            super.viewWillDisappear(animated)
            session.stopRunning()
        }

        nonisolated func metadataOutput(
            _ output: AVCaptureMetadataOutput,
            didOutput objects: [AVMetadataObject],
            from connection: AVCaptureConnection
        ) {
            let code = (objects.first as? AVMetadataMachineReadableCodeObject)?.stringValue
            MainActor.assumeIsolated {
                guard !scanned, let code else { return }
                scanned = true
                Haptics.success()
                onScan?(code)
            }
        }
    }
}
