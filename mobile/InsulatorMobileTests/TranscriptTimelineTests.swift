import Foundation
import Testing
@testable import InsulatorMobile

@Suite("Transcript timeline")
struct TranscriptTimelineTests {
    private func session(
        messages: [Message],
        blocks: [TranscriptBlock] = [],
        turns: [AgentTurn] = [],
        status: SessionStatus = .idle,
        lastReplyAt: UInt64? = nil
    ) -> AgentSession {
        AgentSession(
            id: UUID(),
            title: "Thread",
            autoTitle: nil,
            projectID: UUID(),
            provider: .pi,
            model: nil,
            runtimeMode: .ask,
            reasoningEffort: nil,
            serviceTier: nil,
            contextWindow: nil,
            agentPreset: nil,
            status: status,
            createdAt: 0,
            updatedAt: 0,
            lastReplyAt: lastReplyAt,
            providerCursor: nil,
            messages: messages,
            transcriptBlocks: blocks,
            turns: turns
        )
    }

    private func message(
        _ role: MessageRole,
        _ content: String,
        turnID: UUID? = nil,
        streaming: Bool = false,
        createdAt: UInt64 = 0
    ) -> Message {
        Message(
            id: UUID(),
            turnID: turnID,
            role: role,
            content: content,
            displayContent: nil,
            createdAt: createdAt,
            streaming: streaming
        )
    }

    @Test("keeps messages and blocks in the order the Mac anchored them")
    func ordersEntries() {
        let turnID = UUID()
        let reasoning = TranscriptBlock(
            afterMessage: 1,
            turnID: turnID,
            content: .reasoning(ReasoningBlock(content: "thinking", startedAt: 1, finishedAt: 0))
        )
        let activities = TranscriptBlock(
            afterMessage: 2,
            turnID: turnID,
            content: .activities([ActivityItem(id: "a", kind: "command", title: "Ran")])
        )
        let transcript = TranscriptTimeline(session(
            messages: [message(.user, "do it", turnID: turnID), message(.assistant, "done", turnID: turnID)],
            blocks: [activities, reasoning]
        ))

        let shape = transcript.entries.map { entry in
            switch entry {
            case .message(let message, _): "message(\(message.role))"
            case .block(let block):
                switch block.content {
                case .reasoning: "block(reasoning)"
                case .activities: "block(activities)"
                }
            }
        }

        #expect(shape == [
            "message(user)",
            "block(reasoning)",
            "message(assistant)",
            "block(activities)",
        ])
    }

    @Test("only the last assistant row of a turn carries a footer")
    func footerOnLastAssistantRowPerTurn() {
        let firstTurn = UUID()
        let secondTurn = UUID()
        let transcript = TranscriptTimeline(session(
            messages: [
                message(.user, "one", turnID: firstTurn),
                message(.assistant, "partial", turnID: firstTurn, streaming: true),
                message(.assistant, "first answer", turnID: firstTurn, createdAt: 10),
                message(.assistant, "second answer", turnID: firstTurn, createdAt: 20),
                message(.user, "two", turnID: secondTurn),
                message(.assistant, "second turn answer", turnID: secondTurn, createdAt: 30),
            ],
            turns: [
                AgentTurn(
                    id: firstTurn,
                    turnCount: 1,
                    status: "completed",
                    providerTurnStarted: true,
                    providerResumeAt: nil,
                    startedAt: 0,
                    completedAt: 25,
                    checkpoint: nil
                ),
            ],
            lastReplyAt: 40
        ))

        let footers = transcript.entries.compactMap { entry -> UInt64?? in
            guard case .message(let message, let footerTime) = entry else { return nil }
            return .some(footerTime)
        }

        #expect(footers.count == 4)
        #expect(footers[0] == .some(nil))   // user row
        #expect(footers[1] == .some(nil))   // streaming assistant row
        #expect(footers[2] == .some(nil))   // superseded by the next assistant row
        #expect(footers[3] == .some(25))    // completed turn wins over lastReplyAt
    }

    @Test("a finished reply with no recorded turn falls back to the reply time")
    func footerFallsBackToLastReply() {
        let transcript = TranscriptTimeline(session(
            messages: [message(.assistant, "answer", createdAt: 7)],
            lastReplyAt: 99
        ))

        guard case .message(_, let footerTime) = transcript.entries[0] else {
            Issue.record("expected a message entry")
            return
        }
        #expect(footerTime == 99)
    }

    @Test("a streaming turn reports no footer time at all")
    func footerOmittedWhileBusy() {
        let transcript = TranscriptTimeline(session(
            messages: [message(.assistant, "answer", streaming: true, createdAt: 7)],
            status: .working,
            lastReplyAt: 99
        ))

        guard case .message(_, let footerTime) = transcript.entries[0] else {
            Issue.record("expected a message entry")
            return
        }
        #expect(footerTime == nil)
    }
}

@Suite("Message decoding")
struct MessageDecodingTests {
    @Test("plain messages without presentation fields still decode")
    func decodesPlainMessage() throws {
        let json = Data(
            """
            {"id":"11111111-1111-1111-1111-111111111111","role":"user","content":"hi","created_at":5,"streaming":false}
            """.utf8
        )

        let message = try JSONDecoder().decode(Message.self, from: json)

        #expect(message.attachments.isEmpty)
        #expect(message.displayContent == nil)
        #expect(message.visibleContent == "hi")
    }

    @Test("attachments decode into previewable metadata")
    func decodesAttachments() throws {
        let json = Data(
            """
            {
              "id": "22222222-2222-2222-2222-222222222222",
              "role": "user",
              "content": "look @/blobs/a.jpg",
              "display_content": "look",
              "attachments": [{
                "path": "/blobs/a.jpg",
                "mention": "/blobs/a.jpg",
                "name": "image.jpg",
                "is_dir": false,
                "is_image": true,
                "blob_reference": "insulator-blob:a.jpg"
              }],
              "created_at": 5,
              "streaming": false
            }
            """.utf8
        )

        let message = try JSONDecoder().decode(Message.self, from: json)
        let attachment = try #require(message.attachments.first)

        #expect(message.visibleContent == "look")
        #expect(attachment.isImage)
        #expect(attachment.blobReference == "insulator-blob:a.jpg")
        #expect(attachment.id == "insulator-blob:a.jpg")
    }
}
