# ADR-0007: Транспорт и аутентификация Universal API поверх `axum` (`/api/v1/*`)

- **Статус:** Принято
- **Дата:** 2026-11-17

## Контекст

На Этапе 4 требуется предоставить сторонним приложениям, пользовательским скриптам и `nexdeck-desktop` безопасный локальный интерфейс **Universal API** (отправка уведомлений, создание и опрос запросов на подтверждение `ApprovalRequest`, чтение разрешённых системных метрик, вызов команд управления из белого списка и подписка на события по WebSocket) при сохранении целевого бюджета памяти `nexdeck-agent` (**2–5 МБ RAM** в простое) и без дублирования серверного стека.

## Решение

1. Разместить маршруты Universal API (`/api/v1/status`, `/api/v1/metrics`, `/api/v1/notifications`, `/api/v1/approvals`, `/api/v1/approvals/{request_id}`, `/api/v1/control`, `/api/v1/ws`) на существующем асинхронном сервере **`axum`** ([ADR-0005](0005-network-stack-tokio-axum-wss.md)) без поднятия второго HTTP-сервера или платформозависимых Named Pipes в MVP.
2. Использовать регистрацию внешних приложений (`ClientRegistry` / `clients.json`) с выдачей одноразового секретного токена (`Authorization: Bearer <token>`), хранением на диске только **SHA-256 хэша** токена (`ClientRecord::token_hash_sha256`) и проверкой в константное время.
3. Применять строгую модель минимальных привилегий (`deny-by-default`) через гранулярные разрешения `ApiPermission` (`ReadMetrics`, `SendNotification`, `RequestApproval`, `InteractPlugins`, `ExecuteAllowedAction`) и мгновенный отзыв доступа (`revoked = true`).

## Последствия

- **Плюсы:** нулевой прирост сторонних крейтов в `Cargo.lock`, единый TLS/WSS-контур безопасности, кроссплатформенная совместимость для клиентских SDK (`Python`, `JavaScript/TypeScript`, `Rust`) и интеграционных тестов.
- **Минусы:** локальные клиенты на ПК при подключении к HTTPS/WSS используют самоподписанный сертификат хоста (`host-cert.pem` / `host-fingerprint.sha256`).
