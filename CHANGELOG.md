# История изменений (Changelog)

Все заметные изменения проекта фиксируются в этом файле. Формат основан на [Keep a Changelog](https://keepachangelog.com/ru/1.1.0/) и [Semantic Versioning](https://semver.org/lang/ru/).

## [Unreleased]

### Добавлено
- Локализация документации, `README.md`, шаблонов Issue/PR и описания репозитория на русский язык (с англоязычным зеркалом `docs/README.en.md`).
- Инструкции для AI-агентов [`AGENTS.md`](AGENTS.md), актуальный журнал проекта [`JOURNAL.md`](JOURNAL.md) и архив [`docs/journal/archive/`](docs/journal/archive/README.md).
- Векторный логотип [`assets/logo.svg`](assets/logo.svg).

### Изменено
- Исключён файл `TZ.md` из Git-индекса (добавлен в `.gitignore`), все примеры приведены к синтетическим значениям.

### Этап 0 (завершено в PR #2)
- Утверждена лицензия Apache License 2.0 (`LICENSE`).
- Создан Cargo Workspace (`0.1.0`) с контрактами крейтов (`core`, `transport`, `monitoring`, `plugin_engine`, `api`, `notifications`, `approval`), заготовкой `apps/desktop` и интеграционными тестами `tests/integration`.
- Настроены GitHub Actions (`ci.yml`, `security.yml`, `dependency-review.yml`, `release.yml`), Dependabot, `CODEOWNERS` и Ruleset защиты `main`.
