# Development Roadmap & Open Questions

Development is structured into 8 incremental stages aligned with [TZ.md](../../TZ.md) (§27–§35). Each stage is delivered through scoped GitHub Issues and reviewed Pull Requests.

---

## Stage Breakdown

### Stage 0 — Repository & Architecture Foundation (Current)
- Public GitHub repository setup, Apache-2.0 license approval, and governance docs
- Cargo workspace (`crates/*`, `apps/desktop`, `tests/integration`) with contract traits, unit/integration tests, and doc verification
- Requirements matrix, architecture overview, ADRs, threat model, and testing plan
- GitHub Actions CI (`fmt`, `check`, `clippy`, `unit`, `integration`, `doc`, `release build`), security workflows (`CodeQL`, `cargo-deny`, `Dependency Review`, `Secret Scan`), and `main` branch protection ruleset

### Stage 1 — Core Agent (`crates/core`, `crates/transport`, `crates/monitoring`)
- Windows 11 background agent lifecycle and autostart integration
- Baseline Windows hardware monitoring (CPU, RAM, Disk, Network)
- Wi-Fi WebSocket server with TLS/pairing handshake and reconnect recovery
- Idle RAM & CPU measurement harness on real Windows release builds

### Stage 2 — Mobile Client (`apps/mobile`)
- Initialize Flutter project for Android and iOS
- Device pairing and WebSocket client reconnection
- Live telemetry views for CPU, RAM, GPU, Disk, and Network
- Basic remote media/volume control actions

### Stage 3 — Widget Engine & Desktop Manager (`apps/desktop`, `apps/mobile`)
- Panel builder: create, edit, move, resize widgets, customize appearance & refresh rate
- Built-in widget catalog (text, numeric, buttons, toggles, charts, gauges, images, icons)
- JSON panel configuration persistence, import, and export
- Desktop Manager UI (`egui` / `slint`) for device pairing, security, and panel management

### Stage 4 — Universal API (`crates/api`, `sdk/`, `examples/`)
- Local IPC (Named Pipes) + local HTTP + WebSocket server endpoints
- Application registration, token issuance, scoped permissions, revocation
- Official SDK starter(s) and integration examples

### Stage 5 — Notification Center & Remote Approval (`crates/notifications`, `crates/approval`)
- Windows notification capture, system/hardware alerts, filtering, and sleep buffering
- End-to-end Remote Approval flow from third-party app -> mobile prompt -> signed decision callback
- Request history, risk levels, and replay-protection tests

### Stage 6 — Plugin Engine (`crates/plugin_engine`)
- WebAssembly runtime integration (`wasmtime` / Component Model or host ABI)
- Plugin manifest verification, capability enforcement, memory/execution timeouts, and hang termination
- Plugin SDK documentation and example widgets

### Stage 7 — Additional Transports (`crates/transport`)
- USB transport (e.g., ADB reverse / USB accessory channel)
- Bluetooth transport
- Zero-config device discovery (mDNS / QR pairing) and connection resilience

---

## MVP Acceptance Criteria (`TZ.md` §35)

The first MVP milestone requires all of the following end-to-end capabilities:

1. Installs cleanly on Windows 11 and auto-starts the Core Agent independently of the Desktop GUI.
2. Pairs and connects a mobile device over Wi-Fi with secure authentication and automatic reconnection.
3. Streams core hardware telemetry and supports customizable, persistent user panels.
4. Exposes the Universal API to receive third-party `approval_request` payloads, display them on the phone, and return user decisions to the calling application.
5. Passes unit, integration, security, and GitHub Actions CI checks with up-to-date documentation.

---

## Open Questions for Upcoming Stages

These implementation details do not block Stage 0 and should be finalized in dedicated Issues/ADRs at the start of their respective stages:

1. **Desktop UI crate selection (Stage 1/3):** Choose between `egui` (`eframe`) and `slint` for `apps/desktop` based on native Windows look, binary size, accessibility, and Apache-2.0 license compatibility.
2. **Local IPC mechanism on Windows 11 (Stage 4):** Choose Windows Named Pipes vs. authenticated loopback TCP/Unix domain sockets for `Desktop Manager` <-> `Core Agent` and local `Universal API`.
3. **GPU & Thermal Sensor Backend (Stage 1/2):** Decide between direct NVML/D3DKMT/PDH bindings vs. an optional elevated helper service for motherboard/CPU temperature sensors.
4. **Device Discovery & Pairing UX (Stage 1/2):** Finalize QR code payload + mDNS (`_nexdeck._tcp.local`) vs. manual host/PIN pairing.
5. **WASM Plugin ABI (Stage 6):** Choose between WebAssembly Component Model (WIT) vs. minimal flat C-ABI host functions.
