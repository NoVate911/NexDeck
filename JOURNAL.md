# Журнал проекта NexDeck

> **Для AI-агента:** читай этот файл сразу после [`AGENTS.md`](AGENTS.md). В каждом PR **обязательно** обновляй этот журнал. Когда объём журнала превысит ~150 строк или завершится этап разработки, переноси детали в [`docs/journal/archive/`](docs/journal/archive/README.md), оставляя здесь только актуальную выжимку.

---

## Текущее состояние

- **Актуальный этап:** Этап 1 (Core Agent) — **согласован стек, старт реализации**.
- **Версия workspace:** `0.1.0` (Rust Edition 2024, `rust-version = 1.85`).
- **Последний выполненный PR:** [PR #9](https://github.com/NoVate911/NexDeck/pull/9) — русская локализация, исключение `TZ.md`, добавление `AGENTS.md`, `JOURNAL.md` с архивом, логотипа `assets/logo.svg` и синтетических примеров.
- **Согласованные решения Этапа 1:**
  - Сеть и сервер: `tokio` (`current_thread`) + `axum` (HTTP + WebSocket) + `rustls` WSS с пиннингом отпечатка самоподписанного сертификата и pairing по PIN/QR с хранением SHA-256 хэша `device_token` ([ADR-0005](docs/architecture/adr/0005-network-stack-tokio-axum-wss.md)).
  - Мониторинг Windows 11: нативные Win32/PDH API через `windows-sys` (изолированный FFI в `crates/monitoring`) + портативный mock/адаптер для тестов и Linux CI ([ADR-0006](docs/architecture/adr/0006-monitoring-win32-pdh.md)).
  - Desktop UI: `egui` / `eframe` с лицензией `MIT OR Apache-2.0` ([ADR-0002](docs/architecture/adr/0002-desktop-ui-pure-rust.md)).

---

## Что делать следующим (Шаги Этапа 1)

1. **PR 1.1 (`crates/monitoring`):** реализовать Windows 11 сборщик метрик поверх `windows-sys` (`CPU`, `RAM`, `Disk`, `Network`, `Battery`, `Uptime`) с RAII-обёртками и тестами.
2. **PR 1.2 (`crates/transport`):** реализовать PIN/QR pairing, хранилище хэшей `device_token`, генерацию TLS-сертификата + `axum` WSS-сервер и протокол сообщений.
3. **PR 1.3 (`crates/core`):** собрать асинхронный цикл `nexdeck-agent` (`tokio` `current_thread`), сохранение конфига, автозапуск в реестре Windows и измерение потребления RAM (цель 2–5 МБ в простое).

---

## Известные ошибки и ограничения (не наступать снова)

1. **`cargo-deny` падает с `error[wildcard]` на локальных крейтах:**  
   В `Cargo.toml` внутри `[workspace.dependencies]` обязательно указывать `version = "0.1.0"` вместе с `path = "..."`, а в `deny.toml` (`[bans]`) держать `allow-wildcard-paths = true`.
2. **CodeQL Rust (`Hard-coded cryptographic value`) реагирует на имя `nonce`:**  
   Для прикладного идентификатора запроса использовать `request_id` и генерацию значений в тестовых хелперах; криптографические токены/ключи/nonce в транспорте генерируются только через CSPRNG (не зашивать литералы даже в тестах, если имя переменной `token`/`key`/`nonce`).
3. **Лицензии `rustls` / `ring` и `cargo-deny`:**  
   При подключении `rustls` + `ring` в `deny.toml` понадобится `OpenSSL` в allow-листе (или `[[licenses.clarify]]` для `ring`) — обязательно проверять `cargo deny check` на PR.
4. **Локальная среда Windows без C++ Build Tools (`link.exe not found`):**  
   Пока на ПК не завершена установка «Разработка классических приложений на C++», локально проверяем `cargo fmt`, `cargo check`, `cargo clippy` и `cargo doc`, а линковка/тесты прогоняются в CI.
5. **Данные в коде и документации:**  
   Только синтетические примеры (`ExampleApp`, `org.example.*`, `localhost`, стандартные порты). Файл `TZ.md` не добавлять в Git.

---

## Архив журнала

- [Этап 0 — Инициализация репозитория и базовой инфраструктуры](docs/journal/archive/stage-0.md)
