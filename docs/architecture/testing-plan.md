# NexDeck Testing Plan

Every feature and bug fix in NexDeck must be backed by automated, reproducible verification aligned with [TZ.md](../../TZ.md) (§17, §24, §35).

---

## 1. Verification Layers

| Layer | Scope | Execution Trigger |
| --- | --- | --- |
| **Formatting & Static Analysis** | `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `RUSTDOCFLAGS="-D warnings" cargo doc` | Every commit / PR (`ci.yml`) |
| **Unit Tests** | Pure domain logic, state machines, serialization/validation, replay & expiry guards inside `crates/*/src` | Every commit / PR (`ci.yml`) |
| **Integration Tests** | Cross-crate contracts and workflows in `tests/integration/` | Every PR (`ci.yml`) |
| **Security & Dependency Audits** | `CodeQL (rust)`, `cargo-deny` (RustSec + Apache-2.0 license compatibility), `Dependency Review`, `gitleaks` | Every PR + weekly schedule (`security.yml`, `dependency-review.yml`) |
| **Release Binary Smoke Tests** | Compilation and CLI startup check (`nexdeck-agent --version`, `nexdeck-desktop --version`) on `windows-latest` and `ubuntu-latest` | Every PR (`ci.yml`) & Release tag (`release.yml`) |
| **Performance & RAM Benchmarks** | Idle working-set RAM (target 2–5 MB), CPU usage, WebSocket serialization throughput on Windows 11 release builds | Stage 1+ benchmark suite |
| **Mobile & UI Tests** | Flutter widget/unit tests (`flutter test`) and integration tests | Stage 2+ mobile CI |

---

## 2. Critical Test Scenarios by Subsystem

- **Remote Approval (`crates/approval`):**
  - Reject duplicate/replayed one-time nonces.
  - Reject decisions after `expires_at_epoch_ms` timeout.
  - Reject actions not declared in the request's `allowed_actions`.
  - Verify strict isolation from Windows UAC elevation flows.
- **Universal API (`crates/api`):**
  - Reject unauthenticated clients, revoked tokens, and insufficient permissions.
  - Validate payload size limits, field constraints, and malformed JSON/input rejection.
- **Notifications (`crates/notifications`):**
  - Enforce bounded queue capacity during Windows sleep/disconnect so memory cannot grow unbounded.
  - Verify priority ordering and filter rules.
- **Plugin Engine (`crates/plugin_engine`):**
  - Enforce permission deny-by-default and reject invalid resource limits.
  - Stage 6: Verify fuel/epoch interruption terminates runaway WASM plugins without crashing `Core Agent`.
- **Transport (`crates/transport`):**
  - Verify state transitions (`Disconnected` -> `Connecting` -> `Connected` -> `Reconnecting`).
  - Stage 1+: Verify reconnect resilience across network drop and Windows sleep/wake cycles.

---

## 3. Local Pre-PR Verification

Before pushing or marking a PR ready for merge, run:

```powershell
./scripts/verify.ps1
```
