# Журнал проекта NexDeck

> **Для AI-агента:** читай этот файл сразу после [`AGENTS.md`](AGENTS.md). В каждом PR **обязательно** обновляй этот журнал. Когда объём журнала превысит ~150 строк или завершится этап разработки, переноси детали в [`docs/journal/archive/`](docs/journal/archive/README.md), оставляя здесь только актуальную выжимку.

---

## Текущее состояние

- **Актуальный этап:** Этап 2 (Mobile Client, `apps/mobile`) — **выполнен PR 2.1 (каркас Flutter, сопряжение с TLS-пиннингом, WSS-клиент и CI)**.
- **Версия workspace:** `0.1.0` (Rust Edition 2024, `rust-version = "1.88"`; Flutter `3.50.0` / Dart `3.14.0` в `apps/mobile`).
- **Последний выполненный шаг:** создан Flutter-проект `nexdeck_mobile` (`org.example.nexdeck_mobile`, платформы `android`, `ios`, `windows`), реализованы Dart-модели контрактов `nexdeck-transport` / `nexdeck-monitoring`, `PinnedHttpPairingClient` и `WssNexDeckTransport` с SHA-256 пиннингом сертификата (`X509Certificate.der`), `AgentController` с экспоненциальным переподключением, UI сопряжения по QR JSON/PIN и базового мониторинга, тесты (`flutter test`: 12 passed) и job `Mobile Client (Flutter)` в `ci.yml`.
- **Фактические показатели `nexdeck-agent.exe` (release, LTO + strip):** размер `1 738 752` байт (`~1.66 MiB`); `--daemon` на старте — `private=1.18 MiB`, `working_set=6.53 MiB`.
- **Согласованные решения:**
  - Сеть и сервер: `tokio` (`current_thread`) + `axum` + `rustls` WSS с пиннингом SHA-256 отпечатка и PIN/QR-pairing ([ADR-0005](docs/architecture/adr/0005-network-stack-tokio-axum-wss.md)).
  - Мониторинг Windows 11: Win32/PDH поверх `windows-sys` (`crates/monitoring`, [ADR-0006](docs/architecture/adr/0006-monitoring-win32-pdh.md)).
  - UI/клиенты/плагины: `egui / eframe` ([ADR-0002](docs/architecture/adr/0002-desktop-ui-pure-rust.md)), Flutter ([ADR-0003](docs/architecture/adr/0003-mobile-client-flutter.md)), WebAssembly ([ADR-0004](docs/architecture/adr/0004-plugin-engine-wasm.md)).

---

## Что делать следующим (Этап 2.2 — Mobile Client)

1. **PR 2.2 (`apps/mobile`):** расширить экран мониторинга живыми графиками в реальном времени (история CPU/RAM/Network TX-RX), детализацией по ядрам CPU, VRAM GPU, мультимедиа/громкости и управлению питанием (через `MessageKind::ControlCommand` + валидацию на стороне `Core Agent`).
2. **Опционально (при появлении Android SDK):** добавить сканер QR-кода камерой и безопасное системное хранилище (Android Keystore / iOS Keychain) поверх интерфейса `SessionStore`.
3. **Этап 3 (`apps/desktop` и конструктор панелей):** после завершения Этапа 2 перейти к `egui / eframe` Desktop Manager и сетке настраиваемых виджетов.

---

## Известные ошибки и ограничения (не наступать снова)

1. **`cargo-deny` падает с `error[wildcard]` на локальных крейтах:**  
   В `Cargo.toml` внутри `[workspace.dependencies]` обязательно указывать `version = "0.1.0"` вместе с `path = "..."`, а в `deny.toml` (`[bans]`) держать `allow-wildcard-paths = true`.
2. **CodeQL Rust (`Hard-coded cryptographic value`) реагирует на имя `nonce`:**  
   Использовать `request_id` и динамическую генерацию в тестах; не зашивать литералы в переменных `token`/`key`/`nonce`.
3. **Лицензии `rustls` / `ring` и `RUSTSEC-2026-0009` (`time`):**  
   `ring 0.17.14` (`Apache-2.0 AND ISC`), `rust-version = "1.88"`. При добавлении зависимостей проверять `deny.toml`.
4. **Ограничения песочницы на Windows (`10013`, `code 5`, `flutter.bat` lockfile):**  
   Внутри sandbox: outbound TCP `connect()` даёт `10013`, чтение/запись `HKCU\...\Run` даёт `code 5`, создание подкаталога `.git/refs/heads/feature/` может быть отклонено (`Access denied`), а вызов `flutter`/`dart` из `%APPDATA%\Flutter SDK` подвисает на lockfile/кэше вне workspace. Команды `flutter` (analyze/test/build) и сетевые тесты требуют `BypassSandbox: true` либо прогона в CI.
5. **Flutter `3.50.0` и `package:material_ui/material_ui.dart`:**  
   Во Flutter `>= 3.47` прямой импорт `package:flutter/material.dart` помечен как `deprecated_member_use`; для Material 3 использовать зависимость `material_ui: ^1.1.0` и импорт `package:material_ui/material_ui.dart`. В Dart `3.14` с `prefer_initializing_formals` использовать `required this._field` для приватных именованных параметров.
6. **CodeQL `rust/access-invalid-pointer` и неиспользуемый код на Linux:**  
   Оборачивать Win32-указатели в `NonNull` + RAII `Drop`; Windows FFI-утилиты, вызываемые в кроссплатформенных тестах, помечать `#[cfg(any(windows, test))]`.
7. **Данные в коде и документации:**  
   Только синтетические примеры (`ExampleApp`, `org.example.*`, `127.0.0.1`, стандартные порты). Файл `TZ.md` не добавлять в Git.
8. **`Dependency Review` для `pubspec.lock` (нижний регистр SPDX):**  
   GitHub Dependency Graph возвращает для `pub` лицензии в нижнем регистре (`apache-2.0`, `bsd-3-clause`, `mit`), из-за чего `spdx-expression-parse` внутри `dependency-review-action` отклоняет их как `unresolved`. При добавлении новых Dart/Flutter-зависимостей сначала проверить их реальную лицензию на совместимость с `Apache-2.0`, затем добавить `pkg:pub/<name>` в `allow-dependencies-licenses` в `.github/workflows/dependency-review.yml` (не отключая `deny-licenses` и `fail-on-severity`).

---

## Архив журнала

- [Этап 0 — Инициализация репозитория и базовой инфраструктуры](docs/journal/archive/stage-0.md)
- [Этап 1 — Core Agent (мониторинг, WSS-сервер, pairing, runtime и автозапуск)](docs/journal/archive/stage-1.md)
