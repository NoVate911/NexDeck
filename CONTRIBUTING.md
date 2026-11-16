# Contributing to NexDeck

Thank you for your interest in contributing to **NexDeck**! This document
defines the development rules, Git workflow, commit convention, and quality
gates for the repository.

## Development Workflow

1. **Find or create an Issue:** Every change should correspond to a GitHub Issue.
2. **Branch from `main`:** Direct pushes to `main` are prohibited. Create a
   topic branch matching one of the standard prefixes:
   - `feature/<short-name>` — new functionality
   - `fix/<short-name>` — bug fixes
   - `refactor/<short-name>` — internal code improvements
   - `docs/<short-name>` — documentation updates
   - `test/<short-name>` — test additions or harnesses
   - `ci/<short-name>` — CI/CD, GitHub settings, and automation
3. **Keep changes focused:** One logical change per Pull Request. Do not bundle
   unrelated modifications.
4. **Verify locally before opening a PR:**
   - Windows (PowerShell): `./scripts/verify.ps1`
   - Linux / macOS / Git Bash: `./scripts/verify.sh`
5. **Open a Pull Request:** Fill out all sections of the PR template and link
   the related Issue (`Closes #<id>`).
6. **Wait for all CI & security checks:** All required status checks must pass
   and all conflicts must be resolved before **Squash and Merge**.

## Conventional Commits

All commits and PR titles must follow
[Conventional Commits](https://www.conventionalcommits.org/):

```text
<type>(<scope>): <short imperative summary>
```

Common types: `feat`, `fix`, `refactor`, `docs`, `test`, `ci`, `chore`, `perf`,
`security`.

Examples:

```text
feat(core): add system monitoring
fix(api): resolve connection timeout
refactor(transport): simplify websocket layer
docs(readme): update installation guide
test(core): add memory monitoring tests
ci(github): add rust workflow
chore(deps): update dependencies
```

## Code Quality & Architecture Rules

- Follow official Rust idioms, `rustfmt`, and strict `clippy` lints
  (`-D warnings`).
- Keep Core Agent dependencies minimal and memory footprint aligned with the
  2–5 MB idle target (see [TZ.md](TZ.md) and
  [docs/architecture/overview.md](docs/architecture/overview.md)).
- Document all public APIs (`#![warn(missing_docs)]` / `RUSTDOCFLAGS="-D warnings"`).
- Write unit tests for invariants and integration tests for cross-crate flows.
- Never commit tokens, private keys, certificates, or unverified binaries.
- Never place placeholder/stub implementations that pretend unimplemented
  behavior works.

## Licensing Policy

NexDeck is licensed under the **Apache License 2.0** (see [LICENSE](LICENSE)).

- Do not add dependencies with incompatible licenses (e.g., GPL/AGPL) without
  maintainer approval; `deny.toml` enforces allowed SPDX licenses in CI.
- Independent plugins may choose their own compatible licensing terms.
- Never copy external code without verifying license compatibility and
  attribution requirements.

## Versioning

NexDeck follows **Semantic Versioning (`MAJOR.MINOR.PATCH`)**. Before `1.0.0`,
APIs may evolve between `0.x` releases with documented breaking-change notices.
