import AVFoundation
import Speech
import SwiftUI

/// The composer's bottom row while dictating: cancel, the live waveform and
/// its time, and done. `onClose` gets the words heard, or nil when cancelled.
struct DictationBar: View {
    let onClose: (String?) -> Void

    @State private var recorder = DictationRecorder()
    @Environment(\.scenePhase) private var scenePhase

    var body: some View {
        HStack(spacing: 10) {
            Button { recorder.cancel(); onClose(nil) } label: {
                IconView(.close, size: 16)
                    .foregroundStyle(AppTheme.content)
                    .frame(width: 36, height: 36)
                    .background(AppTheme.fill, in: Circle())
            }
            .buttonStyle(PressedOpacityStyle())
            .accessibilityLabel("Cancel dictation")

            Waveform(levels: recorder.levels)
                .frame(height: 24)
                .frame(maxWidth: .infinity)

            Text(Duration.seconds(recorder.seconds).formatted(.time(pattern: .minuteSecond)))
                .font(AppTheme.font(size: 13).monospacedDigit())
                .foregroundStyle(AppTheme.muted)

            Button {
                Task { onClose(await recorder.finish()) }
            } label: {
                Group {
                    if recorder.phase == .transcribing || recorder.phase == .preparing {
                        ProgressView().tint(AppTheme.inverse)
                    } else {
                        IconView(.check, size: 18).foregroundStyle(AppTheme.inverse)
                    }
                }
                .frame(width: 36, height: 36)
                .background(AppTheme.inverseSurface, in: Circle())
            }
            .buttonStyle(PressedOpacityStyle())
            .disabled(recorder.phase != .recording)
            .accessibilityLabel("Done dictating")
        }
        .task {
            if let failure = await recorder.start() {
                recorder.failure = failure
            }
        }
        .onDisappear { recorder.cancel() }
        .onChange(of: scenePhase) { _, phase in
            if phase == .background { recorder.cancel(); onClose(nil) }
        }
        .alert("Dictation", isPresented: Binding(get: { recorder.failure != nil }, set: { _ in })) {
            Button("OK") { recorder.failure = nil; onClose(nil) }
        } message: {
            Text(recorder.failure ?? "")
        }
    }
}

private struct Waveform: View {
    let levels: [Double]

    var body: some View {
        Canvas { context, size in
            let pitch: CGFloat = 5
            let count = Int(size.width / pitch)
            let shown = levels.suffix(count)
            for (index, level) in shown.enumerated() {
                let height = max(2, CGFloat(level) * size.height)
                let x = size.width - CGFloat(shown.count - index) * pitch
                context.fill(
                    Path(roundedRect: CGRect(x: x, y: (size.height - height) / 2, width: 2, height: height), cornerRadius: 1),
                    with: .color(AppTheme.content.opacity(0.7))
                )
            }
        }
        .accessibilityHidden(true)
    }
}

/// Records from the microphone and transcribes on the phone, reporting a
/// level per reading for the waveform.
@MainActor
@Observable
final class DictationRecorder {
    enum Phase { case preparing, recording, transcribing, done }

    private(set) var phase: Phase = .preparing
    private(set) var levels: [Double] = Array(repeating: 0, count: 64)
    private(set) var seconds: Int = 0
    var failure: String?

    @ObservationIgnored private let engine = AVAudioEngine()
    @ObservationIgnored private var request: SFSpeechAudioBufferRecognitionRequest?
    @ObservationIgnored private var task: SFSpeechRecognitionTask?
    @ObservationIgnored private var transcript = ""
    @ObservationIgnored private var started = Date()
    @ObservationIgnored private var finishing: CheckedContinuation<String?, Never>?

    /// Asks for the microphone and speech recognition, then starts. Returns
    /// why it couldn't, if it couldn't.
    func start() async -> String? {
        let speech = await withCheckedContinuation { continuation in
            SFSpeechRecognizer.requestAuthorization { continuation.resume(returning: $0) }
        }
        guard speech == .authorized else {
            return "Allow Speech Recognition for Insulator in Settings to dictate."
        }
        guard await AVAudioApplication.requestRecordPermission() else {
            return "Allow microphone access for Insulator in Settings to dictate."
        }
        guard let recognizer = SFSpeechRecognizer(), recognizer.isAvailable else {
            return "Dictation isn't available for your language right now."
        }
        guard phase == .preparing else { return nil }
        do {
            let session = AVAudioSession.sharedInstance()
            try session.setCategory(.record, mode: .measurement, options: .duckOthers)
            try session.setActive(true, options: .notifyOthersOnDeactivation)
            let request = SFSpeechAudioBufferRecognitionRequest()
            request.shouldReportPartialResults = true
            if recognizer.supportsOnDeviceRecognition { request.requiresOnDeviceRecognition = true }
            self.request = request
            let input = engine.inputNode
            let format = input.outputFormat(forBus: 0)
            input.installTap(onBus: 0, bufferSize: 1024, format: format) { [weak self] buffer, _ in
                request.append(buffer)
                let level = Self.level(of: buffer)
                Task { @MainActor in self?.record(level: level) }
            }
            engine.prepare()
            try engine.start()
            task = recognizer.recognitionTask(with: request) { [weak self] result, error in
                let text = result?.bestTranscription.formattedString
                let isFinal = result?.isFinal ?? false
                let failed = error != nil
                Task { @MainActor in self?.update(text: text, final: isFinal || failed) }
            }
            started = .now
            phase = .recording
            Haptics.impact(.light)
            return nil
        } catch {
            cancel()
            return "Couldn't start the microphone. \(error.localizedDescription)"
        }
    }

    /// Stops recording and waits for the words heard.
    func finish() async -> String? {
        guard phase == .recording else { return nil }
        phase = .transcribing
        stopEngine()
        request?.endAudio()
        return await withCheckedContinuation { continuation in
            finishing = continuation
            // A recognizer that never reports final still hands back what it heard.
            Task { @MainActor in
                try? await Task.sleep(for: .seconds(3))
                self.complete()
            }
        }
    }

    func cancel() {
        guard phase != .done else { return }
        phase = .done
        task?.cancel()
        stopEngine()
        finishing?.resume(returning: nil)
        finishing = nil
    }

    private func update(text: String?, final: Bool) {
        if let text { transcript = text }
        if final, phase == .transcribing { complete() }
    }

    private func complete() {
        guard let finishing else { return }
        self.finishing = nil
        phase = .done
        let words = transcript.trimmingCharacters(in: .whitespacesAndNewlines)
        finishing.resume(returning: words.isEmpty ? nil : words)
    }

    private func record(level: Double) {
        guard phase == .recording else { return }
        levels.removeFirst()
        levels.append(level)
        seconds = Int(Date().timeIntervalSince(started))
    }

    private func stopEngine() {
        if engine.isRunning {
            engine.stop()
            engine.inputNode.removeTap(onBus: 0)
        }
        try? AVAudioSession.sharedInstance().setActive(false, options: .notifyOthersOnDeactivation)
    }

    /// A buffer's loudness from 0 to 1, with the room's hum below the floor.
    nonisolated private static func level(of buffer: AVAudioPCMBuffer) -> Double {
        guard let samples = buffer.floatChannelData?[0], buffer.frameLength > 0 else { return 0 }
        var sum: Float = 0
        for index in 0..<Int(buffer.frameLength) { sum += samples[index] * samples[index] }
        let rms = (sum / Float(buffer.frameLength)).squareRoot()
        let decibels = 20 * log10(max(rms, 0.000_01))
        return Double(max(0, min(1, (decibels + 50) / 45)))
    }
}
