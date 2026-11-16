# ADR-0004: WebAssembly as Primary Plugin Engine Runtime

- **Status:** Accepted
- **Date:** 2026-11-17
- **Deciders:** `@NoVate911`

## Context

`TZ.md` (§3.5, §6) requires an isolated extension system supporting scoped permissions, memory/CPU resource limits, and termination of hung modules while keeping the background Core Agent lightweight.

## Decision

Adopt **WebAssembly (WASM)** (targeting `wasmtime` with fuel/epoch interruption and memory limits in Stage 6) as the primary embedded plugin runtime inside `crates/plugin_engine`. Optional external-process bridges for Python or JS scripts may be layered later over the local Universal API without linking their runtimes into `nexdeck-agent`.

## Consequences

- **Positive:** Strong memory sandboxing, deterministic CPU/time quotas, multi-language authoring (Rust, C, AssemblyScript, etc. compiled to `.wasm`), and zero runtime overhead when plugins are disabled.
- **Negative:** Requires defining a stable host-guest ABI/WIT contract and shipping documentation/tooling for plugin authors in Stage 6.
