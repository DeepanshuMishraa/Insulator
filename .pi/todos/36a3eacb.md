{
  "id": "36a3eacb",
  "title": "Replace mobile prototype with a direct Insulator daemon client",
  "tags": [
    "mobile",
    "ios"
  ],
  "status": "closed",
  "created_at": "2026-09-27T08:34:46.191Z"
}

Replaced the mock mobile prototype with a native Insulator iOS client. It connects directly to protocol v8 daemon endpoints over Tailscale, stores credentials in Keychain, loads/hydrates daemon tasks, streams replies, supports prompt/steer/cancel/approval, creates tasks, and discovers every enabled desktop provider/model plus reasoning, speed-tier, context, and access options. Removed terminal, subscriptions, pets, hosted relay, widgets, and Codex-only app assumptions. Added protocol/connection tests and regenerated the Xcode project. Verification: iOS simulator build/test succeeded with 4 Swift Testing tests.
