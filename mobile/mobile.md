# Insulator Mobile

Insulator Mobile is a native iPhone and iPad client for the daemon already used by Insulator Desktop. The Mac keeps running providers, projects, Git, and task state. The phone is another client of that same daemon.

## Connect

1. Put the Mac and iPhone on the same Tailnet.
2. In Insulator Desktop, open **Settings → Daemon** and enable daemon exposure.
3. Reveal the token. A **Pair a phone** QR code appears under the credentials.
4. In the iOS app, tap **Connect to desktop** and scan it, or copy the link on the Mac and use **Paste a pairing link**.

The pairing link is `insulator://pair?url=…&token=…`; opening it on the phone pairs too. The paste sheet also takes the Mac's Tailscale address and token by hand.

Use `wss://` when Tailscale Serve or another trusted TLS proxy terminates the connection. A raw Tailnet connection can use `ws://` because traffic still stays inside Tailscale.

The app stores the daemon token in the iOS Keychain with device-only protection. Disconnecting the desktop removes it.

## Included

- A project screen with a new session and the project's sessions side by side, switched by the header or a swipe
- Working sessions in a now-playing pill above the composer, and finished ones stacked over it
- Live transcripts with each turn's work folded behind one line, its phases and steps a tap away
- Prompt, steer, stop, and approval controls, with photo and file attachments and on-device dictation
- Provider, model, effort, fast mode, context window and access choices, with plan usage
- Changes: stage, unstage, commit, generate a message, publish, sync and create pull requests
- New sessions in the project's checkout, a new worktree, or an existing one
- Files a transcript names open in a code viewer; diffs open per file
- Themes, a background photo per project, and phone-side defaults for new sessions

## Deliberately omitted

The mobile app has no subscriptions, pets, hosted relay, Codex-only account flow, SSH terminal, widget, or menu-bar companion. Provider credentials and agent processes stay on the Mac.
