# Requirements, Contradictions & Technical Constraints

This document captures the structured functional and non-functional requirements extracted from [TZ.md](../../TZ.md) (`v0.3.0`), together with identified technical contradictions and constraints that govern architecture and stage planning.

---

## 1. Non-Functional Requirements (NFR)

| ID | Requirement | Target / Rule | Source |
| --- | --- | --- | --- |
| `NFR-01` | Idle memory footprint | Target **2–5 MB RAM** for minimal background Core Agent configuration (measured on real release builds) | `TZ.md` §2 |
| `NFR-02` | CPU overhead | Minimal background CPU load; asynchronous event-driven execution | `TZ.md` §2 |
| `NFR-03` | Headless background core | No heavy GUI framework linked into `nexdeck-agent` | `TZ.md` §2, §3.1, §35 |
| `NFR-04` | Delta / minimal transport | Transmit only required telemetry deltas and requested fields | `TZ.md` §2 |
| `NFR-05` | Lean dependency graph | Minimize external crates; produce compact binaries | `TZ.md` §2, §24 |
| `NFR-06` | Primary host platform | Windows 11 (`x86_64-pc-windows-msvc`), auto-start capable, works with monitor off | `TZ.md` §9, §35 |
| `NFR-07` | Mobile platforms | Android and iOS via unified Flutter codebase | `TZ.md` §3.3 |
| `NFR-08` | Security & boundaries | Paired device auth, scoped API access, replay protection, no UAC replacement, sandboxed plugins | `TZ.md` §7.2, §8, §25 |
| `NFR-09` | License compliance | Apache License 2.0 across core project; dependency license enforcement in CI | `TZ.md` §12, §17 |

---

## 2. Functional Requirements (FR)

| ID | Domain | Requirement Summary | Target Stage |
| --- | --- | --- | --- |
| `FR-01` | Core Agent | Background Rust service, config management, auto-start, device pairing & authorization | Stage 1 |
| `FR-02` | Monitoring | CPU (load, per-core, temp, clock), RAM, GPU (load, temp, clock, VRAM), Disk (space, R/W throughput), Network (up/down speed, traffic), sensors, fans, processes, uptime, battery | Stage 1–2 |
| `FR-03` | Transport | Transport-agnostic envelope & state machine supporting Wi-Fi (WebSocket MVP), USB, Bluetooth, and reconnect handling | Stage 1 & 7 |
| `FR-04` | Mobile Client | Cross-platform Flutter app connecting over Wi-Fi, rendering panels, telemetry, notifications, remote controls, and approval prompts | Stage 2–3 |
| `FR-05` | Panel Builder | Multi-panel editor: add/remove/move/resize widgets, styles/colors/fonts, refresh rates, save/import/export configurations | Stage 3 |
| `FR-06` | Widgets | Text, numeric readouts, buttons, toggles, charts, indicators, images, icons, custom views | Stage 3 |
| `FR-07` | Desktop Manager | Separate pure-Rust Windows GUI (`apps/desktop`) to configure agent, devices, security, extensions, and connections | Stage 1–3 |
| `FR-08` | Universal API | Local IPC, HTTP, and WebSocket APIs + SDKs for external apps to send notifications, publish events, read allowed telemetry, and request approvals | Stage 4 |
| `FR-09` | Remote Approval | Approval request routing to mobile with app binding, description, risk levels, one-time `request_id`, expiration timeout, and request history | Stage 5 |
| `FR-10` | Notification Center | Centralized Windows, app, hardware-alert (thermal/disk), and custom events with filters, priorities, history, and sleep buffering | Stage 5 |
| `FR-11` | Remote Control | Launch apps, volume & media playback, hotkeys, process termination, allowed commands, authenticated shutdown/reboot | Stage 2–5 |
| `FR-12` | Plugin Engine | Isolated WebAssembly extensions with scoped permissions, resource quotas (CPU/RAM/time), and termination of hung plugins | Stage 6 |

---

## 3. Identified Contradictions & Engineering Constraints

The following constraints and tensions in `TZ.md` were identified during Stage 0 analysis and drive our architectural boundaries:

1. **2–5 MB Idle RAM Target vs. Plugin Runtimes (`TZ.md` §2 vs. §3.5 / §6)**
   - *Tension:* Embedding JavaScript or Python interpreters inside the background Core Agent would exceed the 2–5 MB idle RAM budget immediately.
   - *Resolution:* Keep the Core Agent lean in pure Rust (`crates/core`), adopt **WebAssembly (WASM)** as the primary plugin runtime ([ADR-0004](adr/0004-plugin-engine-wasm.md)), lazy-load the plugin runtime only when plugins are enabled, and run heavy or optional external runtimes in isolated worker processes.
2. **Low-Level Hardware Sensors vs. Unprivileged Minimal Binary (`TZ.md` §4 vs. §2, §25)**
   - *Tension:* Reading CPU/GPU package temperatures and fan speeds on Windows 11 often requires vendor SDKs (NVML/ADL), WMI/PDH queries, or privileged sensor drivers. WMI polling can also spike memory/CPU.
   - *Resolution:* Abstract hardware telemetry behind `MetricCollector` (`crates/monitoring`) using lightweight native Win32/PDH APIs first, and isolate elevated/vendor sensor collectors behind optional backends so minimal builds retain the 2–5 MB profile.
3. **Sleep-Mode Notification Delivery (`TZ.md` §9)**
   - *Constraint:* When Windows enters S3/Modern Standby sleep, the local Core Agent cannot push real-time WebSocket frames over LAN without external push relay infrastructure (FCM/APNs).
   - *Resolution:* Buffer events in a bounded persistent queue (`crates/notifications`) during sleep/disconnect and flush upon wake/reconnect, matching `TZ.md` §9.
4. **Remote Approval vs. Windows UAC (`TZ.md` §8)**
   - *Constraint:* Mobile approval cannot bypass or replace Windows User Account Control (UAC).
   - *Resolution:* Scope remote approvals strictly to application-level workflows (`Universal API`) and pre-authorized NexDeck actions running within the agent's security context.
5. **Mobile Cross-Platform CI (Android / iOS)**
   - *Constraint:* Building iOS binaries requires macOS runners with Xcode, and the local Stage 0 development workstation does not currently have the Flutter SDK installed.
   - *Resolution:* Stage 0 establishes the Rust workspace and CI; Stage 2 introduces Flutter tooling and dedicated mobile CI jobs (`ubuntu-latest` for Android, `macos-latest` for iOS).
