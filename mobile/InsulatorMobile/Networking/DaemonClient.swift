import Foundation

enum DaemonNotice: Sendable {
    case event(sessionID: UUID, runtimeID: UUID, kind: String, payload: JSONValue)
    case taskStateChanged
    case disconnected(String)
}

@MainActor
final class DaemonClient {
    static let protocolVersion = 8

    var onNotice: ((DaemonNotice) -> Void)?

    private var socket: URLSessionWebSocketTask?
    private var receiveTask: Task<Void, Never>?
    private var pending: [UUID: CheckedContinuation<JSONValue, Error>] = [:]
    private let clientID = UUID()

    func connect(to payload: PairingPayload) async throws -> String {
        disconnect(notify: false)

        var request = URLRequest(url: payload.url)
        // The daemon treats handshakes with an Origin header as browsers and
        // enforces the allowlist; this header identifies a native client.
        request.setValue("native", forHTTPHeaderField: "x-insulator-client")
        let socket = URLSession.shared.webSocketTask(with: request)
        self.socket = socket
        socket.resume()

        let hello: JSONValue = .object([
            "type": .string("hello"),
            "protocolVersion": .number(Double(Self.protocolVersion)),
            "token": .string(payload.token),
            "clientId": .uuid(clientID),
            "resumeFrom": .array([]),
        ])
        try await send(hello)

        let response = try await receiveValue()
        guard let type = response["type"]?.string else { throw ConnectionError.malformedResponse }
        switch type {
        case "hello":
            let version = response["protocolVersion"]?.int ?? 0
            guard version == Self.protocolVersion else { throw ConnectionError.protocolMismatch(version) }
            let daemonVersion = response["daemonVersion"]?.string ?? "unknown"
            receiveTask = Task { [weak self] in await self?.receiveLoop() }
            return daemonVersion
        case "rejected":
            throw ConnectionError.rejected(response["message"]?.string ?? "The Mac rejected this connection.")
        default:
            throw ConnectionError.malformedResponse
        }
    }

    func request(
        sessionID: UUID = .zero,
        runtimeID: UUID = .zero,
        command: JSONValue
    ) async throws -> JSONValue {
        guard socket != nil else { throw ConnectionError.disconnected }
        let requestID = UUID()
        let envelope = daemonRequestEnvelope(
            requestID: requestID,
            sessionID: sessionID,
            runtimeID: runtimeID,
            command: command
        )

        return try await withCheckedThrowingContinuation { continuation in
            pending[requestID] = continuation
            Task { @MainActor [weak self] in
                do {
                    try await self?.send(envelope)
                } catch {
                    guard let continuation = self?.pending.removeValue(forKey: requestID) else { return }
                    continuation.resume(throwing: error)
                }
            }
        }
    }

    func disconnect() {
        disconnect(notify: false)
    }

    private func disconnect(notify: Bool) {
        receiveTask?.cancel()
        receiveTask = nil
        socket?.cancel(with: .goingAway, reason: nil)
        socket = nil
        let continuations = pending.values
        pending.removeAll()
        continuations.forEach { $0.resume(throwing: ConnectionError.disconnected) }
        if notify {
            onNotice?(.disconnected(ConnectionError.disconnected.localizedDescription))
        }
    }

    private func send(_ value: JSONValue) async throws {
        guard let socket else { throw ConnectionError.disconnected }
        let data = try JSONEncoder().encode(value)
        guard let text = String(data: data, encoding: .utf8) else { throw ConnectionError.malformedResponse }
        try await socket.send(.string(text))
    }

    private func receiveValue() async throws -> JSONValue {
        guard let socket else { throw ConnectionError.disconnected }
        switch try await socket.receive() {
        case .string(let text):
            guard let data = text.data(using: .utf8) else { throw ConnectionError.malformedResponse }
            return try JSONDecoder().decode(JSONValue.self, from: data)
        case .data(let data):
            return try JSONDecoder().decode(JSONValue.self, from: data)
        @unknown default:
            throw ConnectionError.malformedResponse
        }
    }

    private func receiveLoop() async {
        do {
            while !Task.isCancelled {
                handle(try await receiveValue())
            }
        } catch is CancellationError {
            return
        } catch {
            guard !Task.isCancelled else { return }
            disconnect(notify: true)
        }
    }

    private func handle(_ message: JSONValue) {
        switch message["type"]?.string {
        case "response":
            handleResponse(message)
        case "event":
            guard let sessionID = message["sessionId"]?.string.flatMap(UUID.init(uuidString:)),
                  let runtimeID = message["runtimeId"]?.string.flatMap(UUID.init(uuidString:)),
                  let event = message["event"],
                  let kind = event["kind"]?.string else {
                return
            }
            onNotice?(.event(
                sessionID: sessionID,
                runtimeID: runtimeID,
                kind: kind,
                payload: event["payload"] ?? .null
            ))
        case "taskStateChanged":
            onNotice?(.taskStateChanged)
        case "shuttingDown":
            disconnect(notify: true)
        default:
            break
        }
    }

    private func handleResponse(_ message: JSONValue) {
        guard let requestID = message["requestId"]?.string.flatMap(UUID.init(uuidString:)),
              let continuation = pending.removeValue(forKey: requestID),
              let outcome = message["outcome"],
              let status = outcome["status"]?.string else {
            return
        }
        if status == "ok", let payload = outcome["payload"] {
            continuation.resume(returning: payload)
        } else {
            let errorMessage = outcome["error"]?["message"]?.string ?? "The Mac could not complete that request."
            continuation.resume(throwing: ConnectionError.daemon(errorMessage))
        }
    }
}

func daemonRequestEnvelope(
    requestID: UUID,
    sessionID: UUID,
    runtimeID: UUID,
    command: JSONValue
) -> JSONValue {
    .object([
        "type": .string("request"),
        "requestId": .uuid(requestID),
        "sessionId": .uuid(sessionID),
        "runtimeId": .uuid(runtimeID),
        "command": command,
    ])
}

private extension UUID {
    static let zero = UUID(uuid: (0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0))
}
