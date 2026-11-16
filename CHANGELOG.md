# История изменений (Changelog)

Все заметные изменения проекта фиксируются в этом файле. Формат основан на [Keep a Changelog](https://keepachangelog.com/ru/1.1.0/) и [Semantic Versioning](https://semver.org/lang/ru/).

## [Unreleased]

### Добавлено
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
