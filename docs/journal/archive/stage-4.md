# Архив журнала: Этап 4 — Universal API (`crates/api`, `crates/core`, `apps/desktop`, `sdk/`)

## Сводка этапа

- **Период / коммиты:** PR #31 (Issue #30 — серверная часть `/api/v1/*`, `clients.json`, `ADR-0007`, CLI и Desktop Manager) и PR #33 (Issue #32 — клиентские SDK `sdk/python`, `sdk/js`, примеры в `examples/` и документация `docs/api/universal-api.md`).
- **Цель этапа:** предоставить сторонним локальным приложениям, скриптам и автоматизациям безопасный программный интерфейс для отправки уведомлений, запроса удалённого подтверждения (`Remote Approval`), чтения системных метрик и вызова команд управления по принципу минимальных привилегий (`deny-by-default`).

---

## Выполненные задачи

1. **Архитектурное решение (`ADR-0007`):**
   - В [`docs/architecture/adr/0007-universal-api-http-ws-axum.md`](../../architecture/adr/0007-universal-api-http-ws-axum.md) зафиксировано размещение маршрутов `/api/v1/*` и `/api/v1/ws` на существующем `axum`-сервере с Bearer-аутентификацией без добавления новых зависимостей и отдельных IPC-серверов.
2. **Реестр клиентов и протокол (`crates/api`, `crates/approval`, `crates/notifications`):**
   - Реализован `ClientRegistry` (`crates/api/src/clients.rs`) с хранением SHA-256 хешей секретов в `clients.json`, constant-time проверкой Bearer-токена, отзывом и обновлением прав (`ApiPermission`).
   - Добавлена `serde`-сериализация для `ApprovalRequest`, `ApprovalStatus`, `RiskLevel`, `NotificationEvent`, `NotificationSource`, `NotificationPriority` и DTO протокола `/api/v1/*` (`crates/api/src/protocol.rs`).
   - В `submit_approval_request` добавлена проверка совпадения `request.application` с `client.application_name` для защиты от спуфинга имени приложения.
3. **Серверные маршруты и управление (`crates/core`, `apps/desktop`):**
   - В `crates/core/src/api_routes.rs` реализованы маршруты `GET /api/v1/status`, `GET /api/v1/metrics`, `GET|POST /api/v1/notifications`, `GET|POST /api/v1/approvals`, `GET /api/v1/approvals/{request_id}`, `POST /api/v1/approvals/{request_id}/resolve`, `POST /api/v1/approvals/{request_id}/revoke`, `POST /api/v1/control` и `GET /api/v1/ws` (с фильтрацией бродкаста по правам клиента).
   - В `nexdeck-agent` добавлены флаги `--register-client`, `--list-clients`, `--revoke-client`; в `nexdeck-desktop` (вкладка «Устройства») добавлено графическое управление клиентами `clients.json`.
4. **Клиентские SDK, синтетические примеры и документация (`sdk/`, `examples/`, `docs/api/`):**
   - Реализован Python SDK (`sdk/python/nexdeck_sdk.py` + `sdk/python/test_nexdeck_sdk.py`) на стандартной библиотеке Python 3.
   - Реализован JavaScript/TypeScript SDK (`sdk/js/nexdeck_sdk.mjs` + `sdk/js/nexdeck_sdk.d.ts` + `sdk/js/nexdeck_sdk.test.mjs`) без внешних `npm`-зависимостей.
   - Добавлены синтетические примеры (`examples/api_approval_request.json`, `examples/api_approval_example.py`, `examples/api_notification_example.mjs`) и документация [`docs/api/universal-api.md`](../../api/universal-api.md).
