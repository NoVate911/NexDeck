# NexDeck Mobile Client (Flutter)

Cross-platform Android and iOS application planned for **Stage 2** (`TZ.md` §3.3, §29, [ADR-0003](../../docs/architecture/adr/0003-mobile-client-flutter.md)).

## Planned Modules

- **Connection & Pairing:** Wi-Fi WebSocket client with TLS pinning, pairing flow, and automatic reconnection (USB & Bluetooth adapters in Stage 7).
- **Live Telemetry:** CPU, RAM, GPU, Disk, Network, and sensor views.
- **Control Deck & Widget Engine:** Multi-page customizable panels with movable/resizable widgets and JSON import/export (Stage 3).
- **Notification Center & Remote Approval:** Actionable notifications and timed `approve` / `deny` approval prompts (Stage 5).

## Note on Tooling

The Flutter SDK will be initialized here in Stage 2 (`flutter create --org dev.nexdeck --platforms=android,ios .`) along with dedicated Android (`ubuntu-latest`) and iOS (`macos-latest`) CI checks.
