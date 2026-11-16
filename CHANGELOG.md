# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- Stage 0 repository structure aligned with `TZ.md` v0.3.0 (`crates/`, `apps/`, `sdk/`, `examples/`, `tests/`, `scripts/`, `docs/`).
- Approved Apache License 2.0 (`LICENSE`), governance docs (`CONTRIBUTING.md`, `SECURITY.md`, `CODE_OF_CONDUCT.md`), and bilingual README (`README.md`, `docs/README.ru.md`).
- Cargo workspace (`0.1.0`) with contract crates (`core`, `transport`, `monitoring`, `plugin_engine`, `api`, `notifications`, `approval`), `apps/desktop` placeholder crate, and `tests/integration` smoke suite.
- Architecture documentation, requirements matrix, contradictions/constraints analysis, roadmap, testing plan, API/plugin drafts, threat model, and ADRs (`docs/architecture/adr/`).
- GitHub Actions CI, CodeQL (`rust`), `cargo-deny`, Dependency Review, Secret Scan, Release workflow, Dependabot, CODEOWNERS, Issue templates, and PR template.
