# История изменений (Changelog)

Все заметные изменения проекта фиксируются в этом файле. Формат основан на [Keep a Changelog](https://keepachangelog.com/ru/1.1.0/) и [Semantic Versioning](https://semver.org/lang/ru/).

## [Unreleased]

### Исправлено
- Надёжное определение каталога данных на Android (`context.filesDir` через `MethodChannel` в `MainActivity.kt` и общий [`app_data_dir.dart`](apps/mobile/lib/src/storage/app_data_dir.dart)) для хранения `session.json` и `panels.json` без добавления сторонних плагинов, а также автовыбор установленной версии NDK в `apps/mobile/android/app/build.gradle.kts`.

### Добавлено
- Серверная часть Universal API (`/api/v1/*` и `/api/v1/ws`) в `crates/api` и `crates/core` поверх существующего `axum`-сервера ([ADR-0007](docs/architecture/adr/0007-universal-api-http-ws-axum.md)), реестр сторонних клиентов `ClientRegistry` с SHA-256 хешами токенов в `clients.json`, разграничение прав (`ApiPermission`), CLI-флаги `--register-client` / `--list-clients` / `--revoke-client` в `nexdeck-agent` и управление клиентами в `apps/desktop`.
- Динамический Widget Engine и редактор панелей в `apps/mobile` (`DeckStore` / `FileDeckStore` для `panels.json`, `DeckController` с троттлингом `refreshIntervalMs`, `DeckPanelView`, `DeckWidgetCard` для 9 типов виджетов и модальные редакторы стилей/привязок/JSON во вкладке «Панели» `DashboardScreen`).
- Отдельный GUI-менеджер `nexdeck-desktop` на чистом Rust (`apps/desktop`, `eframe 0.33` / `glow`): визуальный редактор сетки панелей и виджетов (`panels.json`) с контролем коллизий и экспортом/импортом JSON, редактор настроек `config.json`, управление автозапуском `HKCU\...\Run`, запуск/остановка локального `nexdeck-agent` (включая режим `--pair` с извлечением PIN/QR), отзыв устройств (`devices.json`) и управление TLS-отпечатком.
- Кросс-платформенные модели конструктора панелей и виджетов (`DeckConfig`, `PanelConfig`, `WidgetSpec`, `WidgetKind`, `WidgetStyle`, валидация коллизий сетки, импорт/экспорт JSON) в `crates/api` и `apps/mobile`, сохранение `panels.json` в `crates/core` и пример `examples/panel_default.json`.
- Визуализация телеметрии в реальном времени (`TelemetryLineChart` на `CustomPainter` для CPU, RAM и сети TX/RX, раскрываемая детализация по ядрам CPU, GPU VRAM и трафик интерфейсов) и панель управления мультимедиа/громкостью в `apps/mobile`.
- Строгий белый список команд удалённого управления `ControlAction` / `ControlCommandRequest` / `ControlCommandResponse` в `crates/transport`, маршрутизация `MessageKind::ControlCommand` и нативный Windows-исполнитель `WindowsMediaExecutor` (`SendInput`: `VK_VOLUME_*`, `VK_MEDIA_*`) в `crates/core`.
- Инициализирован Flutter-клиент `nexdeck_mobile` (`apps/mobile`): модели протокола и телеметрии (`Envelope`, `PairingQrPayload`, `SystemSnapshot`), HTTP/WSS-транспорт с SHA-256 TLS-пиннингом сертификата (`SecurityContext(withTrustedRoots: false)` + constant-time сравнение), хранилище сессии, контроллер автопереподключения, экраны сопряжения/мониторинга и проверки Flutter (`Mobile Client (Flutter)`) в GitHub Actions CI.
- Полноценный runtime `Core Agent` (`nexdeck-agent`) в `crates/core`: асинхронный цикл `tokio` (`current_thread`), рассылка телеметрии по WSS, постоянное хранилище конфигурации/устройств/TLS (`StoragePaths`), автозапуск Windows через `HKCU\...\Run`, замер памяти процесса (`process_metrics`) и CLI (`--daemon`, `--pair`, `--install-autostart`, `--uninstall-autostart`, `--autostart-status`, `--measure-ram`, `--check-config`).
- JSON-сериализация снимков `SystemSnapshot` и метрик (`serde`) в `crates/monitoring`.
- TLS/WSS-сервер (`tokio` + `axum` + `axum-server` + `rustls` + `rcgen`) и подсистема сопряжения (`DeviceRegistry`, PIN/QR, SHA-256 хэши `device_token`, эндпоинты `/health`, `/pair`, `/ws`) в `crates/transport`.
- Нативный сборщик системной телеметрии Windows 11 `WindowsCollector` в `crates/monitoring` поверх `windows-sys` (Win32/PDH: CPU, по-ядерная нагрузка, частота, RAM, диски, сеть `tx/rx`, батарея, uptime) и `StaticCollector` для тестов.
- Согласованы и зафиксированы архитектурные решения Этапа 1: `egui / eframe` ([ADR-0002](docs/architecture/adr/0002-desktop-ui-pure-rust.md)), `tokio + axum + rustls` WSS + PIN/QR pairing ([ADR-0005](docs/architecture/adr/0005-network-stack-tokio-axum-wss.md)), нативный Win32/PDH сборщик `windows-sys` ([ADR-0006](docs/architecture/adr/0006-monitoring-win32-pdh.md)).
- Локализация документации, `README.md`, шаблонов Issue/PR и описания репозитория на русский язык (с англоязычным зеркалом `docs/README.en.md`).
- Инструкции для AI-агентов [`AGENTS.md`](AGENTS.md), актуальный журнал проекта [`JOURNAL.md`](JOURNAL.md) и архив [`docs/journal/archive/`](docs/journal/archive/README.md).
- Векторный логотип [`assets/logo.svg`](assets/logo.svg).

### Изменено
- Исключён файл `TZ.md` из Git-индекса (добавлен в `.gitignore`), все примеры приведены к синтетическим значениям.

### Этап 0 (завершено в PR #2)
- Утверждена лицензия Apache License 2.0 (`LICENSE`).
- Создан Cargo Workspace (`0.1.0`) с контрактами крейтов (`core`, `transport`, `monitoring`, `plugin_engine`, `api`, `notifications`, `approval`), заготовкой `apps/desktop` и интеграционными тестами `tests/integration`.
- Настроены GitHub Actions (`ci.yml`, `security.yml`, `dependency-review.yml`, `release.yml`), Dependabot, `CODEOWNERS` и Ruleset защиты `main`.
