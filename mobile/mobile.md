# Insulator Mobile

Insulator Mobile is a native iPhone and iPad client for the daemon already used by Insulator Desktop. The Mac keeps running providers, projects, Git, and task state. The phone is another client of that same daemon.

## Connect

1. Put the Mac and iPhone on the same Tailnet.
2. In Insulator Desktop, open **Settings → Daemon**.
3. Enable daemon exposure and choose a port.
4. Enter the Mac's Tailscale address and the displayed daemon token in the iOS app.

Use `wss://` when Tailscale Serve or another trusted TLS proxy terminates the connection. A raw Tailnet connection can use `ws://` because traffic still stays inside Tailscale.

The app stores the daemon token in the iOS Keychain with device-only protection. Forgetting the Mac removes it.

## Included

- Projects and daemon-owned tasks
- Live transcripts, reasoning, and tool activity
- Prompt, steer, stop, and approval controls
- Provider and model discovery from the connected Mac
- The same access, reasoning, speed-tier, and context-window choices exposed by the desktop daemon
- Keychain-backed reconnect

## Deliberately omitted

The mobile app has no subscriptions, pets, hosted relay, Codex-only account flow, SSH terminal, widget, or menu-bar companion. Provider credentials and agent processes stay on the Mac.
