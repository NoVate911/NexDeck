# NexDeck Desktop Manager

Separate Windows 11 graphical management shell (`nexdeck-desktop`) built in **pure Rust UI** (`egui` / `slint`, see [ADR-0002](../../docs/architecture/adr/0002-desktop-ui-pure-rust.md)).

## Architectural Boundary

- Runs as an on-demand GUI process separate from the headless `nexdeck-agent` service.
- Communicates with `Core Agent` through `nexdeck-api` over local IPC so closing the UI leaves zero GUI overhead in background RAM.
- Manages paired mobile devices, security/API tokens, plugin permissions, panel presets, and live connection diagnostics.
