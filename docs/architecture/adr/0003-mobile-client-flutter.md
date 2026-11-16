# ADR-0003: Flutter for Cross-Platform Mobile Client

- **Status:** Accepted
- **Date:** 2026-11-17
- **Deciders:** `@NoVate911`

## Context

`TZ.md` requires a unified mobile client supporting both **Android** and **iOS** for customizable Stream Deck-style panels, live charts/gauges, notifications, remote controls, and approval dialogs.

## Decision

Use **Flutter** (Dart) in `apps/mobile` as the single codebase for Android and iOS clients, communicating with `Core Agent` over the `nexdeck-transport` protocol (Wi-Fi WebSocket in MVP, USB/Bluetooth in Stage 7).

## Consequences

- **Positive:** Single codebase for Android and iOS, 60/120fps custom canvas/grid rendering for interactive deck widgets, strong WebSocket/crypto ecosystem.
- **Negative:** Requires installing Flutter SDK (plus Android SDK / macOS Xcode for iOS builds) in Stage 2; mobile CI cannot build iOS artifacts on Windows/Linux runners.
