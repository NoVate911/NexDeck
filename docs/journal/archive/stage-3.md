# Этап 3 — Widget Engine и Desktop Manager (архив)

Период: общие модели конструктора панелей, отдельный Rust Desktop Manager (`apps/desktop`) и динамический Widget Engine во Flutter-клиенте (`apps/mobile`) ([PR #23](https://github.com/NoVate911/NexDeck/pull/23), [PR #25](https://github.com/NoVate911/NexDeck/pull/25), [Issue #26](https://github.com/NoVate911/NexDeck/issues/26)).

## Выполненные задачи

1. **Общие модели панелей/виджетов и хранилище (`crates/api`, `crates/core`, `apps/mobile`, [PR #23](https://github.com/NoVate911/NexDeck/pull/23)):**
   - Введена единая схема `panels.json` (`DeckConfig`, `PanelConfig`, `WidgetSpec`, `WidgetKind`, `WidgetStyle`, `TelemetryBinding`, `WidgetAction`, `WidgetIcon`) в `crates/api/src/panels.rs` и синхронное Dart-зеркало в `apps/mobile/lib/src/models/panel_models.dart`.
   - Реализована строгая проверка коллизий сетки (`2..=8` колонок), интервала обновления (`100..=60000` мс), hex-цветов и безопасных идентификаторов; добавлены `StoragePaths::load_or_init_panels` / `save_panels` и синтетический пример `examples/panel_default.json`.
2. **Desktop Manager на чистом Rust (`apps/desktop`, [PR #25](https://github.com/NoVate911/NexDeck/pull/25)):**
   - Реализовано отдельное GUI-приложение `nexdeck-desktop` поверх `eframe 0.33.3` / `egui` (`glow`) с headless CLI (`--version`, `--help`, `--config-dir`), интерактивным предпросмотром сетки панелей, поиском свободной ячейки (`find_free_cell`), редактором `config.json`, автозапуском `nexdeck-agent` в `HKCU\...\Run` без обхода UAC, запуском дочернего агента (включая `--pair --ttl` с разбором PIN/QR), отзывом устройств (`devices.json`) и перевыпуском TLS SHA-256 отпечатка.
   - В `deny.toml` согласованы лицензии шрифта/буфера (`OFL-1.1`, `Ubuntu-font-1.0`, `BSL-1.0`), транзитивные дубли (`winit`/`png`) и информационный advisory `RUSTSEC-2026-0192` (`ttf-parser`).
3. **Динамический Widget Engine в мобильном клиенте (`apps/mobile`, [Issue #26](https://github.com/NoVate911/NexDeck/issues/26)):**
   - Реализовано локальное хранилище `DeckStore` (`FileDeckStore` в `NexDeckMobile/panels.json`, `InMemoryDeckStore`) и `DeckController` с троттлингом по `PanelConfig.refreshIntervalMs`, историей метрик, поиском свободных ячеек, проверкой коллизий, импортом/экспортом JSON и диспетчеризацией `ControlAction` / `Heartbeat`.
   - Добавлена вкладка «Панели» в `DashboardScreen` (`DeckPanelView`, `DeckWidgetCard`, `widget_editor_sheet.dart`) с рендерингом всех 9 типов виджетов (`Text`, `MetricNumber`, `Button`, `Toggle`, `Chart`, `Gauge`, `Icon`, `Image`, `Custom`) и встроенным конструктором/редактором стилей и привязок.

## Проверки этапа

- Rust: `./scripts/verify.ps1` (`cargo fmt`, `clippy -- -D warnings`, workspace unit/integration/doc-тесты, `rustdoc`, `--release`), релизный `nexdeck-desktop.exe` (`3 614 720` байт) и `nexdeck-agent.exe` (`1 743 360` байт).
- Flutter: `dart format`, `flutter analyze --fatal-infos` (0 замечаний) и `flutter test` (`21` пройденный тест, включая `FileDeckStore`, `DeckController` и `DeckPanelView`).
