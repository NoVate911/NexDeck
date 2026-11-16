# ADR-0001: Rust Core Agent and Modular Cargo Workspace

- **Status:** Accepted
- **Date:** 2026-11-17
- **Deciders:** `@NoVate911`

## Context

NexDeck requires a background Windows 11 service (`Core Agent`) targeting 2–5 MB idle RAM, low CPU usage, memory safety, strong concurrency, and zero heavy runtime dependencies (such as JVM, .NET, or Node.js).

## Decision

1. Implement the `Core Agent` and shared domain subsystems in **Rust** (Edition 2024).
2. Structure the codebase as a modular Cargo workspace (`crates/*`, `apps/desktop`, `tests/integration`) separating core orchestration, transport, hardware monitoring, notifications, remote approval, Universal API, and plugin execution.
3. Enforce strict compiler/clippy lints, `cargo-deny` license/advisory checks, and `#![forbid(unsafe_code)]` in pure contract crates.

## Consequences

- **Positive:** Predictable native memory usage, single compact binary distribution, strict compile-time safety, and clean crate-level boundaries.
- **Negative:** Windows low-level hardware sensor APIs require careful safe FFI wrappers (`windows-sys` in Stage 1+) rather than managed runtime libraries.
