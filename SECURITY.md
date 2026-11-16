# Security Policy

## Supported Versions

NexDeck is currently in pre-1.0 development (`0.1.x` / Stage 0). Security fixes
apply to the `main` branch and the latest tagged release.

| Version | Supported |
| --- | --- |
| `main` (`0.1.x`) | Yes |
| `< 0.1.0` | No |

## Reporting a Vulnerability

**Please do NOT report security vulnerabilities, exploit payloads, tokens, or
credentials in public GitHub Issues or Pull Requests.**

Instead, report vulnerabilities privately through:

1. **GitHub Private Vulnerability Reporting:**  
   https://github.com/NoVate911/NexDeck/security/advisories/new
2. **Direct Maintainer Contact:** `novate911@yandex.ru`

Please include:

- Affected component (`core`, `transport`, `api`, `approval`, `plugin_engine`,
  `apps/desktop`, `apps/mobile`, etc.)
- Steps to reproduce or proof-of-concept
- Potential impact (e.g., unauthorized action execution, replay bypass,
  sandbox escape, privilege escalation)
- Any suggested mitigation

### Response Targets

- Acknowledgement: within 72 hours
- Initial triage and severity assessment: within 7 days
- Coordinated fix and advisory publication once patched

## Security Boundaries & Non-Goals

- **Windows UAC:** NexDeck's Remote Approval System authorizes application-level
  and configured NexDeck requests from a paired mobile device. It does **not**
  replace, bypass, or automate Windows User Account Control (UAC).
- **Universal API & Approval:** Third-party applications must authenticate,
  use least-privilege scopes, and bind every approval request to a single-use
  `request_id` and finite expiration timeout.
- **Plugin Isolation:** Extensions must run in an isolated sandbox with
  explicit capability grants and strict resource quotas.
- **Dependency Hygiene:** All third-party crates must pass `cargo-deny` license
  and RustSec advisory checks before merging.
