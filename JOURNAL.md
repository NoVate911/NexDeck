# Журнал проекта NexDeck

> **Для AI-агента:** читай этот файл сразу после [`AGENTS.md`](AGENTS.md). В каждом PR **обязательно** обновляй этот журнал. Когда объём журнала превысит ~150 строк или завершится этап разработки, переноси детали в [`docs/journal/archive/`](docs/journal/archive/README.md), оставляя здесь только актуальную выжимку.

---

## Текущее состояние

- **Актуальный этап:** Этап 3 (Widget Engine и Desktop Manager) — **выполнен PR 3.2 (Desktop Manager `apps/desktop` на чистом Rust `egui / eframe`)**.
- **Версия workspace:** `0.1.0` (Rust Edition 2024, `rust-version = "1.88"`; Flutter `3.50.0` / Dart `3.14.0` в `apps/mobile`).
- **Последний выполненный шаг:** реализован чистый Rust GUI-конфигуратор `nexdeck-desktop` (`apps/desktop/src/main.rs`, `state.rs`, `ui.rs` на `eframe 0.33.3` / `glow`): конструктор сетки панелей и виджетов (`panels.json`) с контролем коллизий и импортом/экспортом JSON, редактор `config.json`, автозапуск `nexdeck-agent` в `HKCU\...\Run`, запуск/остановка локального процесса агента (обычный и `--pair` с показом PIN/QR), отзыв устройств (`devices.json`), просмотр/перевыпуск TLS SHA-256 отпечатка и обновление `deny.toml` под зависимости `eframe`.
- **Фактические показатели `nexdeck-agent.exe` (release, LTO + strip):** размер `1 743 872` байт (`~1.66 MiB`); `--measure-ram` — `private=1.23 MiB`, `working_set=6.44 MiB`.
- **Согласованные решения:**
  - Сеть и сервер: `tokio` (`current_thread`) + `axum` + `rustls` WSS с пиннингом SHA-256 отпечатка и PIN/QR-pairing ([ADR-0005](docs/architecture/adr/0005-network-stack-tokio-axum-wss.md)).
  - Мониторинг Windows 11: Win32/PDH поверх `windows-sys` (`crates/monitoring`, [ADR-0006](docs/architecture/adr/0006-monitoring-win32-pdh.md)).
  - UI/клиенты/плагины: `egui / eframe` ([ADR-0002](docs/architecture/adr/0002-desktop-ui-pure-rust.md)), Flutter ([ADR-0003](docs/architecture/adr/0003-mobile-client-flutter.md)), WebAssembly ([ADR-0004](docs/architecture/adr/0004-plugin-engine-wasm.md)).

---

## Что делать следующим (Этап 3 — Widget Engine и Desktop Manager)

1. **PR 3.3 (`apps/mobile` Widget Engine):** динамический рендерер настраиваемых панелей и виджетов (текст, числа, кнопки действий, переключатели, графики, индикаторы, иконки) на базе `DeckConfig` / `PanelConfig` с локальным сохранением и импортом/экспортом JSON.
2. **Этап 4 (Уведомления и команды подтверждения):** интеграция `crates/notifications` (пересылка toast-уведомлений в фоне и буферизация при сне) и `crates/approval` (одноразовые запросы подтверждения с проверкой границ безопасности и без обхода UAC).

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
   `egui::Color32` использует premultiplied alpha и искажает RGB при конвертации полупрозрачных `#AARRGGBB`: для редактора цвета использовать `color_edit_button_srgba_unmultiplied(&mut [u8; 4])`. В `eframe 0.33` устаревший `Slider::clamp_to_range(true)` заменён на `Slider::clamping(SliderClamping::Always)`, а в `Stroke::new` требуется явный `f32` (`1.0_f32`). При обновлении GUI-стека проверять `deny.toml`: `epaint_default_fonts` требует `OFL-1.1` + `Ubuntu-font-1.0`, `clipboard-win`/`error-code` — `BSL-1.0`, а `winit`/`smithay-client-toolkit` на Linux имеют легальные транзитивные дубли (`calloop`, `rustix`, `thiserror 1.x`).

---

## Архив журнала

- [Этап 0 — Инициализация репозитория и базовой инфраструктуры](docs/journal/archive/stage-0.md)
- [Этап 1 — Core Agent (мониторинг, WSS-сервер, pairing, runtime и автозапуск)](docs/journal/archive/stage-1.md)
- [Этап 2 — Mobile Client (Flutter-клиент, TLS-пиннинг, графики телеметрии и мультимедиа-управление)](docs/journal/archive/stage-2.md)
