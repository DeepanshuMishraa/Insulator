import Foundation
import Testing
@testable import InsulatorMobile

@Suite("Pairing links")
struct PairingLinkTests {
    @Test("reads the desktop's pairing link")
    func readsLink() throws {
        let payload = try PairingPayload(link: "insulator://pair?url=ws%3A%2F%2F100.64.0.1%3A34123%2Fv1&token=a+b%26c")
        #expect(payload.url.absoluteString == "ws://100.64.0.1:34123/v1")
        #expect(payload.token == "a b&c")
    }

    @Test("rejects links that aren't Insulator pairing links")
    func rejectsOtherLinks() {
        #expect(throws: ConnectionError.self) { try PairingPayload(link: "https://example.com/pair?url=ws://x&token=t") }
        #expect(throws: ConnectionError.self) { try PairingPayload(link: "insulator://pair?url=ws://100.64.0.1:1") }
    }
}

@Suite("Unified diffs")
struct UnifiedDiffTests {
    @Test("splits a patch by file and numbers its lines")
    func splitsAndNumbers() throws {
        let patch = """
        diff --git a/Sources/App.swift b/Sources/App.swift
        index 1111111..2222222 100644
        --- a/Sources/App.swift
        +++ b/Sources/App.swift
        @@ -10,3 +10,4 @@ struct App {
         let a = 1
        -let b = 2
        +let b = 3
        +let c = 4
         let d = 5
        diff --git a/new.txt b/new.txt
        new file mode 100644
        --- /dev/null
        +++ b/new.txt
        @@ -0,0 +1 @@
        +hello
        """
        let files = UnifiedDiff.files(in: patch)
        let app = try #require(files["Sources/App.swift"])
        #expect(app.map(\.kind) == [.hunk, .context, .delete, .add, .add, .context])
        #expect(app[1].oldNumber == 10 && app[1].newNumber == 10)
        #expect(app[2].oldNumber == 11 && app[2].newNumber == nil)
        #expect(app[3].newNumber == 11)
        #expect(app[5].oldNumber == 12 && app[5].newNumber == 13)
        #expect(files["new.txt"]?.last?.text == "hello")
        #expect(files["new.txt"]?.last?.newNumber == 1)
    }
}

@Suite("Workspace decoding")
struct WorkspaceDecodingTests {
    @Test("a worktree session keeps its workspace through a save")
    func sessionWorkspaceRoundTrips() throws {
        let workspace = SessionWorkspace.worktree(path: "/w/fix", branch: "fix-bug")
        let data = try JSONEncoder().encode(workspace)
        let json = try #require(String(data: data, encoding: .utf8))
        #expect(json.contains(#""kind":"worktree""#))
        #expect(try JSONDecoder().decode(SessionWorkspace.self, from: data) == workspace)
        let local = try JSONDecoder().decode(SessionWorkspace.self, from: Data(#"{"kind":"local"}"#.utf8))
        #expect(local == .local)
    }

    @Test("decodes the daemon's working-tree status")
    func decodesStatus() throws {
        let json = #"""
        {"branch":"main","default_branch":"main","has_remote":true,"upstream":"origin/main","ahead":1,"behind":0,
         "ahead_of_default":0,"files":[{"path":"a/b.swift","status":"modified","staged":true,"unstaged":true,"additions":3,"deletions":1}]}
        """#
        let status = try JSONDecoder().decode(WorkingTreeStatus.self, from: Data(json.utf8))
        #expect(status.onDefaultBranch)
        let file = try #require(status.files.first)
        #expect(file.isPartlyStaged && !file.isFullyStaged)
        #expect(file.name == "b.swift" && file.directory == "a")
        #expect(file.staging(true).isFullyStaged)
    }
}

@Suite("Transcript turns")
struct TranscriptTurnTests {
    private func session(messages: [Message], blocks: [TranscriptBlock], turns: [AgentTurn], status: SessionStatus) -> AgentSession {
        AgentSession(
            id: UUID(), title: "Thread", autoTitle: nil, projectID: UUID(), workspace: nil, provider: .claude,
            model: nil, runtimeMode: .ask, reasoningEffort: nil, serviceTier: nil, contextWindow: nil,
            agentPreset: nil, status: status, createdAt: 0, updatedAt: 0, lastReplyAt: nil, providerCursor: nil,
            messages: messages, transcriptBlocks: blocks, turns: turns
        )
    }

    private func activity(_ kind: String, target: String, complete: Bool = true) -> ActivityItem {
        ActivityItem(id: UUID().uuidString, kind: kind, title: kind, complete: complete, displayTarget: target)
    }

    @Test("folds the work behind one line and keeps the last answer open")
    func foldsWork() throws {
        let turnID = UUID()
        let user = Message(id: UUID(), turnID: turnID, role: .user, content: "fix it", createdAt: 100, streaming: false)
        let note = Message(id: UUID(), turnID: turnID, role: .assistant, content: "Looking around", createdAt: 101, streaming: false)
        let answer = Message(id: UUID(), turnID: turnID, role: .assistant, content: "Done.", createdAt: 130, streaming: false)
        let block = TranscriptBlock(afterMessage: 2, turnID: turnID, content: .activities([
            activity("file_read", target: "src/a.swift"),
            activity("file_change", target: "src/a.swift"),
        ]))
        let record = AgentTurn(id: turnID, turnCount: 1, status: "completed", providerTurnStarted: true, providerResumeAt: nil, startedAt: 100, completedAt: 130, checkpoint: nil)
        let turns = TranscriptTurns.build(
            session(messages: [user, note, answer], blocks: [block], turns: [record], status: .idle),
            modelName: "Claude Opus"
        )
        let turn = try #require(turns.first)
        #expect(turn.items.count == 3)
        guard case .header(let header) = turn.items[1] else {
            Issue.record("expected the fold line second")
            return
        }
        #expect(!header.live)
        #expect(header.durationSeconds == 30)
        #expect(header.entries.count == 2)
        guard case .reply(let reply) = turn.items[2] else {
            Issue.record("expected the answer last")
            return
        }
        #expect(reply.content == "Done.")
        #expect(turn.copyText == "Done.")
        #expect(turn.completedAt == 130)
    }

    @Test("a turn still working leaves its latest run open")
    func liveWorkStaysOpen() throws {
        let turnID = UUID()
        let user = Message(id: UUID(), turnID: turnID, role: .user, content: "go", createdAt: 0, streaming: false)
        let block = TranscriptBlock(afterMessage: 1, turnID: turnID, content: .activities([
            activity("command", target: "swift test", complete: false),
        ]))
        let turns = TranscriptTurns.build(session(messages: [user], blocks: [block], turns: [], status: .working), modelName: nil)
        let items = try #require(turns.first).items
        guard case .header(let header) = items[1], case .work(_, let phases) = items[2] else {
            Issue.record("expected a live fold line and open work")
            return
        }
        #expect(header.live)
        #expect(phases.first?.title == "Running a command")
        #expect(phases.first?.kind == .run)
    }

    @Test("sums a run of work one clause per kind")
    func summarisesWork() {
        let activities = [
            activity("file_read", target: "a.swift"),
            activity("file_read", target: "b.swift"),
            activity("file_change", target: "a.swift"),
            activity("command", target: "make"),
            activity("command", target: "make test"),
        ]
        #expect(TranscriptTurns.workSummary(activities, live: false) == "Read 2 files · Edited a.swift · Ran 2 commands")
        #expect(TranscriptTurns.workingLabel(seconds: 124, model: "Claude Opus", done: true) == "Claude Opus worked for 2m 4s")
        #expect(TranscriptTurns.workingLabel(seconds: nil, model: nil, done: false) == "Working…")
    }
}

@Suite("Markdown")
struct MarkdownTests {
    @Test("parses the blocks agent replies use")
    func parsesBlocks() {
        let blocks = MarkdownParser.parse("""
        # Title

        Some **bold** text.

        - one
        - two

        ```swift
        let x = 1
        ```

        | a | b |
        |---|---|
        | 1 | 2 |
        """)
        #expect(blocks == [
            .heading(level: 1, text: "Title"),
            .paragraph("Some **bold** text."),
            .list(ordered: false, start: 1, items: [[.paragraph("one")], [.paragraph("two")]]),
            .code("let x = 1"),
            .table([["a", "b"], ["1", "2"]]),
        ])
    }

    @Test("names files in code spans and nothing else")
    @MainActor
    func fileNames() {
        #expect(Inline.fileName(in: "src/App.swift:12") == "App.swift")
        #expect(Inline.fileName(in: "Makefile") == "Makefile")
        #expect(Inline.fileName(in: "git status") == nil)
        #expect(Inline.fileName(in: "x = 1") == nil)
    }
}
