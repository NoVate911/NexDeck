# Plugin System Specification (Draft — Stage 0 Contract)

The **Plugin Engine** (`crates/plugin_engine`) provides an isolated extension environment allowing community developers to build custom data sources, widgets, event handlers, and controlled actions.

> **Status:** Stage 0 establishes the manifest, permission, and resource-quota contracts. The WebAssembly runtime (`wasmtime`) will be integrated in Stage 6 ([ADR-0004](../architecture/adr/0004-plugin-engine-wasm.md)).

---

## 1. Runtime & Isolation Guarantees

- **Primary Runtime:** WebAssembly (`.wasm`) executed in an isolated sandbox.
- **Memory Limit:** Every plugin declares `max_memory_bytes` capped by host policy.
- **Execution Timeout & CPU Quotas:** `max_execution_ms` and `max_cpu_share_pct` prevent runaway loops from starving the Core Agent; hung modules can be terminated (`Faulted` -> `Terminated`).
- **Lazy Activation:** When no plugins are active, the Core Agent avoids loading the WASM runtime to stay within the 2–5 MB idle RAM budget.

---

## 2. Permission Model

Plugins operate under explicit least-privilege grants declared in `PluginManifest`:

| Permission | Capability |
| --- | --- |
| `MetricsRead` | Read allowed snapshots from `crates/monitoring` |
| `NotificationPublish` | Submit notifications into `crates/notifications` |
| `WidgetRender` | Supply state/data for custom dashboard widgets |
| `AllowedCommandInvoke` | Request execution of pre-approved host actions |

All OS filesystem, arbitrary network, and process spawning capabilities are denied unless explicitly bridged by a future host capability grant.
