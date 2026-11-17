# Журнал проекта NexDeck

> **Для AI-агента:** читай этот файл сразу после [`AGENTS.md`](AGENTS.md). В каждом PR **обязательно** обновляй этот журнал. Когда объём журнала превысит ~150 строк или завершится этап разработки, переноси детали в [`docs/journal/archive/`](docs/journal/archive/README.md), оставляя здесь только актуальную выжимку.

---

## Текущее состояние

- **Актуальный этап:** Этап 1 (Core Agent) **завершён** — следующий шаг **Этап 2 (Mobile Client: инициализация Flutter-клиента в `apps/mobile`)**.
- **Версия workspace:** `0.1.0` (Rust Edition 2024, `rust-version = "1.88"`).
- **Последний выполненный шаг:** реализован runtime `Core Agent` (`crates/core`: `StoragePaths`, `autostart` через `HKCU\...\Run`, `process_metrics`, асинхронный цикл `spawn_agent_runtime` с WSS-телеметрией и CLI `--daemon`/`--pair`/`--measure-ram`), а также сериализация `SystemSnapshot` в `crates/monitoring`.
- **Фактические показатели `nexdeck-agent.exe` (release, LTO + strip):** размер `1 738 752` байт (`~1.66 MiB`); `--daemon` на старте — `private=1.18 MiB`, `working_set=6.53 MiB` (разница обусловлена отображением разделяемых системных DLL Windows/PDH).
- **Согласованные решения:**
  - Сеть и сервер: `tokio` (`current_thread`) + `axum` + `rustls` WSS с пиннингом SHA-256 отпечатка и PIN/QR-pairing ([ADR-0005](docs/architecture/adr/0005-network-stack-tokio-axum-wss.md)).
  - Мониторинг Windows 11: Win32/PDH поверх `windows-sys` (`crates/monitoring`, [ADR-0006](docs/architecture/adr/0006-monitoring-win32-pdh.md)).
  - Desktop UI: чистый Rust `egui` / `eframe` ([ADR-0002](docs/architecture/adr/0002-desktop-ui-pure-rust.md)); Mobile Client: Flutter ([ADR-0003](docs/architecture/adr/0003-mobile-client-flutter.md)); Плагины: WebAssembly ([ADR-0004](docs/architecture/adr/0004-plugin-engine-wasm.md)).

---

## Что делать следующим (Этап 2 — Mobile Client)

1. **Проверить окружение Flutter/Dart** и согласовать структуру проекта в `apps/mobile` (минимальные зависимости для WebSocket/TLS pinning, сканера QR-кода и графиков телеметрии).
2. **PR 2.1 (`apps/mobile`):** создать каркас Flutter-приложения (Android / iOS), экран ввода/сканирования QR/PIN и сопряжение с `POST /pair` + пиннинг `fingerprint_sha256`.
3. **PR 2.2 (`apps/mobile`):** подключение к `/ws`, автопереподключение, декодирование `Envelope` (`MessageKind::Telemetry`) и отображение метрик CPU/RAM/дисков/сети/батареи.

---

## Известные ошибки и ограничения (не наступать снова)

1. **`cargo-deny` падает с `error[wildcard]` на локальных крейтах:**  
   В `Cargo.toml` внутри `[workspace.dependencies]` обязательно указывать `version = "0.1.0"` вместе с `path = "..."`, а в `deny.toml` (`[bans]`) держать `allow-wildcard-paths = true`.
2. **CodeQL Rust (`Hard-coded cryptographic value`) реагирует на имя `nonce`:**  
   Для прикладного идентификатора запроса использовать `request_id` и динамическую генерацию в тестах; криптографические токены/ключи/nonce генерируются только через CSPRNG (не зашивать литералы в тестах в переменных `token`/`key`/`nonce`).
3. **Лицензии `rustls` / `ring` и `RUSTSEC-2026-0009` (`time`):**  
   `ring 0.17.14` использует стандартную лицензию `Apache-2.0 AND ISC`. Для получения `time >= 0.3.47` (`RUSTSEC-2026-0009`) задан `rust-version = "1.88"`. При добавлении зависимостей проверять `deny.toml` на дубли версий (`errno`, `libc`, `windows-sys`).
4. **Ограничения песочницы агента на Windows (`10013` и `code 5`):**  
   Внутри sandbox-ограничений Windows outbound TCP `connect()` в `nexdeck-transport` возвращает `Os { code: 10013 }`, а обращение к `HKCU\Software\Microsoft\Windows\CurrentVersion\Run` — `code 5` (`ACCESS_DENIED`). Локально проверять только несетевые тесты в sandbox, а сетевые тесты и CI прогонять на раннерах без sandbox-блокировок.
5. **CodeQL `rust/access-invalid-pointer` и предупреждения неиспользуемого кода на Linux:**  
   Не разыменовывать голый `*mut T` после составного условия — оборачивать указатели Win32 в `NonNull` + RAII `Drop`. Общие вспомогательные функции для Windows FFI (например, конвертеры UTF-16 в `autostart.rs`) помечать `#[cfg(any(windows, test))]`, чтобы release-сборка Linux CI не ловила `dead_code`.
6. **Данные в коде и документации:**  
   Только синтетические примеры (`ExampleApp`, `org.example.*`, `localhost`, стандартные порты). Файл `TZ.md` не добавлять в Git.

---

## Архив журнала

- [Этап 0 — Инициализация репозитория и базовой инфраструктуры](docs/journal/archive/stage-0.md)
- [Этап 1 — Core Agent (мониторинг, WSS-сервер, pairing, runtime и автозапуск)](docs/journal/archive/stage-1.md)
