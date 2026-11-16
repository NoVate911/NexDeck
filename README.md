# NexDeck

[![CI](https://github.com/NoVate911/NexDeck/actions/workflows/ci.yml/badge.svg)](https://github.com/NoVate911/NexDeck/actions/workflows/ci.yml)
[![Security](https://github.com/NoVate911/NexDeck/actions/workflows/security.yml/badge.svg)](https://github.com/NoVate911/NexDeck/actions/workflows/security.yml)
[![License: Apache-2.0](https://img.shields.io/badge/License-Apache_2.0-blue.svg)](LICENSE)

> Open-source, modular platform for ultra-lightweight Windows 11 system monitoring, customizable Stream Deck-style panels, unified notifications, remote control, and mobile remote action approval.

[Русская версия](docs/README.ru.md) · [Technical Specification (TZ.md)](TZ.md) · [Architecture](docs/architecture/overview.md) · [Roadmap](docs/architecture/roadmap.md)

---

## Project Status

NexDeck is currently completing **Stage 0 (Repository & Architecture Foundation)** aligned with [TZ.md v0.3.0](TZ.md).  
The Rust workspace (`0.1.0`) defines modular crate boundaries, domain contracts, security boundaries, and full CI/CD automation ahead of Stage 1 Core Agent implementation.

## Core Principles

- **Minimal Footprint:** Background Core Agent in **Rust** targeting **2–5 MB idle RAM**, minimal CPU overhead, and zero heavy GUI dependencies in the background service.
- **Modular Architecture:** Strict separation between the headless Core Agent, pure-Rust Desktop Manager, cross-platform Flutter Mobile Client, Transport Layer, and sandboxed WASM Plugin Engine.
- **Transport Agnostic:** Wi-Fi over WebSocket for MVP, designed from day one to support USB and Bluetooth without coupling business logic to transport details.
- **Security by Design:** Explicit device pairing, least-privilege API tokens, single-use nonces with expiration for remote approvals, sandboxed plugins with resource quotas, and clear separation from Windows UAC.

## Planned Capabilities

- **Hardware Monitoring:** CPU (per-core load, clock, temperature), RAM, GPU (load, temp, clocks, VRAM), Disks (capacity, read/write throughput), Network, fans, sensors, processes, uptime, and battery status.
- **Custom Control Deck Panels:** Multi-panel builder with movable/resizable widgets (text, metrics, buttons, toggles, charts, indicators, icons, custom views), themes, refresh rates, and import/export.
- **Universal API:** Local IPC, HTTP, and WebSocket interfaces plus language SDKs so external apps can publish events, read authorized telemetry, and trigger remote approval prompts.
- **Remote Approval System:** Forward scoped approval prompts from third-party applications to your phone with risk levels, single-use IDs, timeouts, and audit history (without replacing Windows UAC).
- **Notification Center:** Centralized feed for Windows events, application alerts, hardware warnings (thermal/disk), priorities, filtering, and sleep-wake buffering.
- **Remote Control:** App launcher, volume and media controls, hotkeys, process actions, authorized commands, and authenticated power controls.
- **Sandboxed Plugin Engine:** Isolated **WebAssembly** extension runtime with granular permissions, CPU/memory limits, and hang termination.

## Technology Stack

| Component | Technology | Status |
| --- | --- | --- |
| **Core Agent & Crates** | Rust (Edition 2024) | Stage 0 contracts & CI active |
| **Desktop Manager** | Pure Rust UI (`egui` / `slint`, see [ADR-0002](docs/architecture/adr/0002-desktop-ui-pure-rust.md)) | Stage 0 skeleton |
| **Mobile Client (Android / iOS)** | Flutter (see [ADR-0003](docs/architecture/adr/0003-mobile-client-flutter.md)) | Planned (Stage 2) |
| **Plugin Runtime** | WebAssembly / Wasmtime (see [ADR-0004](docs/architecture/adr/0004-plugin-engine-wasm.md)) | Stage 0 contract |
| **Primary Transport (MVP)** | Wi-Fi / WebSocket (USB & Bluetooth later) | Stage 0 contract |

## Repository Layout

```text
NexDeck/
├── .github/                 # CI/CD workflows, Dependabot, CODEOWNERS, Issue/PR templates
├── apps/
│   ├── desktop/             # Pure-Rust Desktop Manager GUI crate
│   └── mobile/              # Flutter Android/iOS client workspace plan
├── crates/
│   ├── core/                # Core Agent binary (nexdeck-agent) & orchestration
│   ├── transport/           # Transport abstraction (Wi-Fi WebSocket, USB, Bluetooth)
│   ├── monitoring/          # Hardware telemetry traits & snapshot models
│   ├── plugin_engine/       # Sandboxed WASM plugin contracts & quotas
│   ├── api/                 # Universal API schemas & auth policies
│   ├── notifications/       # Notification center, filtering, sleep queue
│   └── approval/            # Remote Approval state machine & replay protection
├── docs/
│   ├── architecture/        # Requirements, architecture overview, roadmap, test plan, ADRs
│   ├── api/                 # Universal API & Plugin specifications
│   ├── development/         # Developer & release guides
│   └── security/            # Threat model & security boundaries
├── examples/                # Sample payloads and integration examples
├── scripts/                 # Local verification scripts (verify.ps1 / verify.sh)
├── sdk/                     # Future external language SDKs (Stage 4)
├── tests/                   # Workspace integration test suites
├── Cargo.toml               # Workspace configuration
├── deny.toml                # cargo-deny advisory, license, and ban policies
└── TZ.md                    # Full Technical Specification (Russian, v0.3.0)
```

## Quick Start (Stage 0 Verification)

Prerequisites: [Rust toolchain](https://rustup.rs/) (stable).

```powershell
# Clone the repository
git clone https://github.com/NoVate911/NexDeck.git
cd NexDeck

# Run formatting, clippy, unit/integration tests, doc checks, and release build
./scripts/verify.ps1
```

On Linux / macOS / Git Bash:

```bash
./scripts/verify.sh
```

## Documentation

- [Technical Specification (RU)](TZ.md)
- [Requirements & Constraints](docs/architecture/requirements.md)
- [Architecture Overview](docs/architecture/overview.md)
- [Development Roadmap & Open Questions](docs/architecture/roadmap.md)
- [Testing Plan](docs/architecture/testing-plan.md)
- [Universal API Draft](docs/api/universal-api.md)
- [Plugin System Draft](docs/api/plugins.md)
- [Threat Model](docs/security/threat-model.md)
- [Getting Started Guide](docs/development/getting-started.md)

## Contributing & Security

Contributions are welcome! Please read [CONTRIBUTING.md](CONTRIBUTING.md) and our [Code of Conduct](CODE_OF_CONDUCT.md) before opening an Issue or Pull Request.  
To report a security issue privately, follow [SECURITY.md](SECURITY.md).

## License

NexDeck is licensed under the **Apache License 2.0** — see [LICENSE](LICENSE) for full details.
