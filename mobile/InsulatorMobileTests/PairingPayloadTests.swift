import Foundation
import Testing
@testable import InsulatorMobile

struct ConnectionTests {
    @Test func trimsTheDaemonToken() throws {
        let payload = try PairingPayload(
            url: #require(URL(string: "wss://insulator.tail.test")),
            token: "  secret  "
        )

        #expect(payload.token == "secret")
    }

    @Test func rejectsHTTPAddresses() {
        #expect(throws: ConnectionError.self) {
            try PairingPayload(url: #require(URL(string: "https://example.com")), token: "secret")
        }
    }

    @Test func pairingPayloadAppendsV1WhenMissing() throws {
        let bare = try PairingPayload(
            url: #require(URL(string: "ws://100.64.0.1:34123")),
            token: "secret"
        )
        #expect(bare.url.path == "/v1")

        let withSlash = try PairingPayload(
            url: #require(URL(string: "ws://100.64.0.1:34123/")),
            token: "secret"
        )
        #expect(withSlash.url.path == "/v1")

        let kept = try PairingPayload(
            url: #require(URL(string: "ws://100.64.0.1:34123/v1")),
            token: "secret"
        )
        #expect(kept.url.path == "/v1")
    }

    @Test func savedPayloadWithoutV1MigratesOnDecode() throws {
        let json = #"{"url":"ws://100.64.0.1:34123","token":"secret"}"#
        let payload = try JSONDecoder().decode(
            PairingPayload.self,
            from: #require(json.data(using: .utf8))
        )
        #expect(payload.url.path == "/v1")
    }

    @Test func decodesDesktopProviderCatalog() throws {
        let json = #"{"provider":"pi","installed":true,"path":"/usr/local/bin/pi","models":[{"id":"fast","name":"Fast","is_default":true,"reasoning_efforts":[{"id":"high","label":"High"}],"default_reasoning_effort":"high","service_tiers":[],"context_windows":[]}],"agent_presets":[]}"#

        let probe = try JSONDecoder().decode(ProviderProbe.self, from: #require(json.data(using: .utf8)))

        #expect(probe.provider == .pi)
        #expect(probe.models.first?.reasoningEfforts.first?.label == "High")
    }

    @Test func streamedReasoningKeepsStableTranscriptIdentity() {
        let turnID = UUID()
        let initial = TranscriptBlock(
            afterMessage: 1,
            turnID: turnID,
            content: .reasoning(ReasoningBlock(content: "Reading", startedAt: 10, finishedAt: 0))
        )
        let updated = TranscriptBlock(
            afterMessage: 1,
            turnID: turnID,
            content: .reasoning(ReasoningBlock(content: "Reading files", startedAt: 10, finishedAt: 0))
        )

        #expect(initial.id == updated.id)
    }

    @Test func requestEnvelopeMatchesTheDaemonContract() {
        let requestID = UUID(uuid: (1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1))
        let envelope = daemonRequestEnvelope(
            requestID: requestID,
            sessionID: UUID(uuid: (0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0)),
            runtimeID: UUID(uuid: (0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0)),
            command: .object(["type": .string("loadTaskState")])
        )

        #expect(envelope["type"]?.string == "request")
        #expect(envelope["request"] == nil)
        #expect(envelope["requestId"]?.string == requestID.uuidString.lowercased())
        #expect(envelope["command"]?["type"]?.string == "loadTaskState")
    }
}
