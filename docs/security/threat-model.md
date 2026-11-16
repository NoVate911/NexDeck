# NexDeck Threat Model & Security Boundaries

Security is a core design invariant in NexDeck ([TZ.md](../../TZ.md) §7.2, §8, §25).

---

## 1. Trust Boundaries

1. **Host OS (`Windows 11`) <-> `nexdeck-agent`:**
   - Runs as a standard user background process by default.
   - Remote approvals **never** bypass or replace Windows UAC (`TZ.md` §8).
   - Remote control actions are checked against an explicit allowlist; destructive actions (power off, reboot, process termination) require explicit authorization.
2. **`nexdeck-agent` <-> Mobile Client (`apps/mobile`):**
   - Devices must undergo explicit pairing before telemetry, control, or approval channels open.
   - Transport frames are encrypted and authenticated (TLS + paired device identity in Stage 1+ WebSocket transport).
3. **`nexdeck-agent` <-> Third-Party Applications (`Universal API`):**
   - Applications authenticate with unique client IDs and revocable scoped tokens (`ApiTokenPolicy`).
   - Input payloads are strictly bounded and validated before entering `crates/approval` or `crates/notifications`.
4. **`nexdeck-agent` <-> Extensions (`crates/plugin_engine`):**
   - Plugins execute inside a sandboxed WASM runtime with explicit permissions and strict memory/CPU/time quotas.

---

## 2. Key Threats & Mitigations

| Threat | Mitigation | Crate / Control |
| --- | --- | --- |
| Approval replay attack | Single-use unique `request_id` tracked in `ApprovalRegistry`; duplicate decisions rejected | `crates/approval` |
| Stale approval accepted late | Strict `expires_at_epoch_ms` checks before recording any decision | `crates/approval` |
| Unauthorized approval action | Decision action must match request's `allowed_actions` list | `crates/approval` |
| Privilege escalation via Universal API | Least-privilege `ApiPermission` bitset/set, token revocation, no unrestricted OS shell | `crates/api` |
| Malicious or hung plugin | WASM memory caps, execution timeout (`max_execution_ms`), fuel/epoch kill switch | `crates/plugin_engine` |
| Sleep buffer memory exhaustion | Bounded capacity queue (`NotificationQueue`) with deterministic eviction | `crates/notifications` |
| Secret leaks or vulnerable crates | GitHub Push Protection, Gitleaks CI scan, CodeQL (`rust`), `cargo-deny`, Dependabot | `.github/workflows/security.yml` |
