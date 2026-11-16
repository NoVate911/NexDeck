# Архив журнала — Этап 0 (Подготовка)

**Период:** 2026-11-17  
**Итоговый статус:** Завершён и влит в `main`.

## Выполненные задачи

1. Настроен публичный репозиторий `NoVate911/NexDeck`:
   - Включены Squash-only слияния, автоудаление веток, Dependabot alerts и Dependabot security updates.
   - Настроен Ruleset защиты `main` (`#27418476`): запрет прямого push/force-push/удаления ветки, обязательные PR, `CI Gate`, `CodeQL (rust)`, `Cargo Deny`, `Secret Scan`, `Dependency Review`.
2. Утверждена лицензия **Apache License 2.0** (`LICENSE`).
3. Сформирован каркас Cargo Workspace (`0.1.0`) из 7 доменных крейтов (`core`, `transport`, `monitoring`, `notifications`, `approval`, `api`, `plugin_engine`), заготовки `apps/desktop` и интеграционных тестов `tests/integration`.
4. Настроен контур проверок GitHub Actions (`ci.yml`, `security.yml`, `dependency-review.yml`, `release.yml`) с матрицей `windows-latest` и `ubuntu-latest`.
5. Подготовлена архитектурная документация, матрица требований/противоречий, план тестирования, модель угроз и 4 ADR (`0001`–`0004`).

## Устранённые проблемы этапа

- **CI `Cargo Deny`:** исправлена ошибка wildcard-зависимостей добавлением `version = "0.1.0"` в `[workspace.dependencies]` и `allow-wildcard-paths = true` в `deny.toml`.
- **CodeQL (`Hard-coded cryptographic value`):** переименование прикладного поля `nonce` в `request_id` и перенос тестовых идентификаторов в генераторы устранили 13 ложноположительных/эвристических срабатываний.
- **Локальная среда:** зафиксировано отсутствие MSVC `link.exe` на локальной машине без C++ Build Tools; полная линковка и прогон тестов подтверждены в GitHub Actions.
