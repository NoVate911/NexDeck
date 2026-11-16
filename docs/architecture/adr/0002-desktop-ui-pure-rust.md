# ADR-0002: Pure-Rust Desktop Manager UI

- **Status:** Accepted
- **Date:** 2026-11-17
- **Deciders:** `@NoVate911`

## Context

NexDeck needs a separate Windows 11 Desktop Manager GUI (`apps/desktop`) to configure the agent, paired devices, security policies, panels, and plugins without adding WebView/Electron/Chromium overhead or bloating the background `Core Agent`.

## Decision

1. Build `apps/desktop` (`nexdeck-desktop`) as a separate **pure-Rust GUI binary** using an immediate-mode or retained native Rust UI framework (`egui` or `slint` — exact crate choice deferred to Stage 1/3 evaluation).
2. Communicate between `nexdeck-desktop` and `nexdeck-agent` over local IPC / Universal API so closing the Desktop Manager leaves the headless background agent operating within its minimal memory budget.

## Consequences

- **Positive:** Eliminates WebView2/Node bundling, keeps total installer and runtime RAM small, shares Rust data models directly across workspace crates, and preserves Apache-2.0 compatibility.
- **Negative:** UI toolkit choice (`egui` vs. `slint`) requires prototyping custom layout editors in Stage 3 compared to web-based canvas tools.
