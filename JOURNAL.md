# Журнал проекта NexDeck

> **Для AI-агента:** читай этот файл сразу после [`AGENTS.md`](AGENTS.md). В каждом PR **обязательно** обновляй этот журнал. Когда объём журнала превысит ~150 строк или завершится этап разработки, переноси детали в [`docs/journal/archive/`](docs/journal/archive/README.md), оставляя здесь только актуальную выжимку.

---

## Текущее состояние

- **Актуальный этап:** Этап 5 (Remote Approval и Центр уведомлений) — выполняется **PR 5.1 (ядро Центра уведомлений, буфер сна, аппаратные тревоги и сквозной поток Remote Approval по WSS, Issue #38)**; следующим выполняется **PR 5.2 (клиентский UI центра уведомлений и карточек Remote Approval в `apps/mobile` и `apps/desktop`)**.
- **Версия workspace:** `0.1.0` (Rust Edition 2024, `rust-version = "1.88"`; Flutter `3.50.0` / Dart `3.14.0` в `apps/mobile`).
- **Последний выполненный шаг:** реализованы протокол и бэклог-синхронизация этапа 5 в `crates/approval` (`ApprovalDecisionMessage`, `expire_stale`, `pending_active`), `crates/notifications` (`NotificationSyncCommand`, фильтрация/нормализация, удаление/очистка), `crates/transport` (`backlog_tx`, `connected_clients()`), а также `crates/core` (`HardwareAlertMonitor` в `alerts.rs`, сохранение буфера сна `notifications.json`, обработка входящих решений WSS и автоматическое истечение таймаутов в `runtime.rs`).
- **Фактические показатели `nexdeck-agent.exe` (release, LTO + strip):** размер `1 743 360` байт (`~1.66 MiB`); `--measure-ram` — `private=1.23 MiB`, `working_set=6.44 MiB`. `nexdeck-desktop.exe` — `3 614 720` байт.
- **Согласованные решения:**
  - Сеть и сервер: `tokio` (`current_thread`) + `axum` + `rustls` WSS с пиннингом SHA-256 отпечатка и PIN/QR-pairing ([ADR-0005](docs/architecture/adr/0005-network-stack-tokio-axum-wss.md)), Universal API `/api/v1/*` поверх `axum` ([ADR-0007](docs/architecture/adr/0007-universal-api-http-ws-axum.md)).
  - Мониторинг Windows 11: Win32/PDH поверх `windows-sys` (`crates/monitoring`, [ADR-0006](docs/architecture/adr/0006-monitoring-win32-pdh.md)).
  - UI/клиенты/плагины: `egui / eframe` ([ADR-0002](docs/architecture/adr/0002-desktop-ui-pure-rust.md)), Flutter ([ADR-0003](docs/architecture/adr/0003-mobile-client-flutter.md)), WebAssembly ([ADR-0004](docs/architecture/adr/0004-plugin-engine-wasm.md)).

---

## Что делать следующим (Этап 5 — Remote Approval и Центр уведомлений)

1. **Этап 5 (`crates/notifications`, `crates/approval`, `crates/core`, `apps/mobile`, `apps/desktop`):** реализовать сквозной центр уведомлений с буфером сна при отключённом телефоне, интерактивный поток Remote Approval (`approval_requested` / `approval_updated` по WSS -> карточка решения на экране телефона -> возврат решения без обхода Windows UAC) и управление/просмотр в мобильном и десктопном приложениях.
2. **Этап 6 (`crates/plugin_engine`):** изолированный WASM-движок плагинов (`wasmtime`) с жёсткими квотами памяти/CPU/времени.

---

## Известные ошибки и ограничения (не наступать снова)

1. **`cargo-deny` падает с `error[wildcard]` на локальных крейтах:**  
   В `Cargo.toml` внутри `[workspace.dependencies]` обязательно указывать `version = "0.1.0"` вместе с `path = "..."`, а в `deny.toml` (`[bans]`) держать `allow-wildcard-paths = true`.
2. **CodeQL Rust (`Hard-coded cryptographic value`) реагирует на имя `nonce`:**  
   Использовать `request_id` и динамическую генерацию в тестах; не зашивать литералы в тестах в переменных `token`/`key`/`nonce`.
3. **Лицензии `rustls` / `ring` и `RUSTSEC-2026-0009` (`time`):**  
   `ring 0.17.14` (`Apache-2.0 AND ISC`), `rust-version = "1.88"`. При добавлении зависимостей проверять `deny.toml`.
4. **Ограничения песочницы на Windows (`10013`, `code 5`, `flutter.bat` lockfile):**  
   Внутри sandbox: outbound TCP `connect()` даёт `10013`, чтение/запись `HKCU\...\Run` даёт `code 5`, операции записи в `.git/` (`index.lock`, `FETCH_HEAD`, `refs/heads/feature/`) отклоняются (`Permission denied`), а вызов `flutter`/`dart` из `%APPDATA%\Flutter SDK` подвисает на lockfile/кэше вне workspace. Команды `flutter`, git-записи/сети и сетевые тесты требуют `BypassSandbox: true` либо прогона в CI.
5. **Flutter `3.50.0` и `package:material_ui/material_ui.dart`:**  
   Во Flutter `>= 3.47` прямой импорт `package:flutter/material.dart` помечен как `deprecated_member_use`; для Material 3 использовать `material_ui: ^1.1.0` и `package:material_ui/material_ui.dart`. В Dart `3.14` с `prefer_initializing_formals` использовать `required this._field` для приватных именованных параметров. При тестировании `ListView` с несколькими карточками прокручивать `tester.scrollUntilVisible(..., scrollable: find.byType(Scrollable))`.
6. **CodeQL `rust/access-invalid-pointer` и неиспользуемый код на Linux:**  
   Оборачивать Win32-указатели в `NonNull` + RAII `Drop`; Windows FFI-утилиты, вызываемые в кроссплатформенных тестах, помечать `#[cfg(any(windows, test))]`.
7. **Данные в коде и документации:**  
   Только синтетические примеры (`ExampleApp`, `org.example.*`, `127.0.0.1`, стандартные порты). Файл `TZ.md` не добавлять в Git.
8. **`Dependency Review` для `pubspec.lock` (нижний регистр SPDX):**  
   GitHub Dependency Graph возвращает для `pub` лицензии в нижнем регистре (`apache-2.0`, `bsd-3-clause`, `mit`), из-за чего `spdx-expression-parse` внутри `dependency-review-action` отклоняет их как `unresolved`. При добавлении новых Dart/Flutter-зависимостей сначала проверить их реальную лицензию на совместимость с `Apache-2.0`, затем добавить `pkg:pub/<name>` в `allow-dependencies-licenses` в `.github/workflows/dependency-review.yml` (не отключая `deny-licenses` и `fail-on-severity`).
9. **`egui 0.33` (`eframe`), цвет ARGB и `deny.toml`:**  
   `egui::Color32` использует premultiplied alpha и искажает RGB при конвертации полупрозрачных `#AARRGGBB`: для редактора цвета использовать `color_edit_button_srgba_unmultiplied(&mut [u8; 4])`. В `eframe 0.33` устаревший `Slider::clamp_to_range(true)` заменён на `Slider::clamping(SliderClamping::Always)`, а в `Stroke::new` требуется явный `f32` (`1.0_f32`). При обновлении GUI-стека проверять `deny.toml`: `epaint_default_fonts` требует `OFL-1.1` + `Ubuntu-font-1.0`, `clipboard-win`/`error-code` — `BSL-1.0`, `winit`/`smithay-client-toolkit` на Linux имеют легальные транзитивные дубли (`calloop`, `rustix`, `thiserror 1.x`), а `epaint` -> `ab_glyph 0.2.32` тянет `ttf-parser 0.25.1` (`RUSTSEC-2026-0192` — статус `unmaintained` без апгрейда).
10. **Хранение файлов и NDK на Android (`HOME=/`, Gradle `build.gradle.kts`):**  
    На Android `Platform.environment['HOME']` часто равен `/` (только для чтения). Локальные файлы `session.json` и `panels.json` должны разрешаться через `resolveMobileDataDir` (нативный `filesDir` по `MethodChannel` в `MainActivity.kt`), с fallback только на не-корневой `systemTemp`. Кроме того, в `apps/mobile/android/app/build.gradle.kts` выбор `ndkVersion` автоматически берёт установленную версию из `sdk.dir/ndk` (с импортом `java.io.File`/`java.util.Properties`, так как без `import` идентификатор `java` в `.gradle.kts` конфликтует с `Project.java`) до обращения к `flutter.ndkVersion`, чтобы не ломаться на Android CLI shim при иной версии NDK.

---

## Архив журнала

- [Этап 0 — Инициализация репозитория и базовой инфраструктуры](docs/journal/archive/stage-0.md)
- [Этап 1 — Core Agent (мониторинг, WSS-сервер, pairing, runtime и автозапуск)](docs/journal/archive/stage-1.md)
- [Этап 2 — Mobile Client (Flutter-клиент, TLS-пиннинг, графики телеметрии и мультимедиа-управление)](docs/journal/archive/stage-2.md)
- [Этап 3 — Widget Engine и Desktop Manager (`crates/api`, `apps/desktop`, `apps/mobile`)](docs/journal/archive/stage-3.md)
- [Этап 4 — Universal API (`crates/api`, `crates/core`, `apps/desktop`, `sdk/`, `examples/`)](docs/journal/archive/stage-4.md)
