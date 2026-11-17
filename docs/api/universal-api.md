# Universal API (`crates/api`, `crates/core`, `sdk/`)

**Universal API** — локальный программный интерфейс **NexDeck**, позволяющий сторонним приложениям, локальным скриптам и автоматизациям взаимодействовать с `nexdeck-agent` без прямого доступа к мобильному протоколу сопряжения:

- отправлять системные и прикладные уведомления на телефон (`send_notification`);
- запрашивать удалённое подтверждение действий с телефона (`request_approval`);
- читать актуальный срез системной телеметрии ПК (`read_metrics`);
- вызывать мультимедийные и системные команды управления (`execute_control`).

Архитектурное обоснование выбора авторизованного HTTP + WebSocket поверх существующего `axum`-сервера зафиксировано в [ADR-0007](../architecture/adr/0007-universal-api-http-ws-axum.md).

---

## 1. Модель безопасности и права доступа

1. **Deny-by-default и аутентификация Bearer-токеном:**
   - Каждый запрос к `/api/v1/*` или `/api/v1/ws` обязан передавать заголовок `Authorization: Bearer <client_token>`.
   - В файле `clients.json` хранится только **SHA-256 хеш** токена (`token_sha256_hex`); сам секрет выдаётся однократно в момент регистрации.
   - Сравнение хешей выполняется за константное время (`constant_time_eq`).
2. **Гранулярные разрешения (`ApiPermission`):**
   - `read_metrics` — доступ к `GET /api/v1/metrics` и получению снимков `telemetry` в `/api/v1/ws`;
   - `send_notification` — доступ к `GET /api/v1/notifications`, `POST /api/v1/notifications` и событиям `notification` в `/api/v1/ws`;
   - `request_approval` — доступ к `GET|POST /api/v1/approvals*` и событиям `approval_requested` / `approval_updated` в `/api/v1/ws`;
   - `execute_control` — доступ к `POST /api/v1/control` и событиям `control_ack` в `/api/v1/ws`.
3. **Защита от спуфинга и граница Windows UAC:**
   - При создании запроса подтверждения (`POST /api/v1/approvals`) поле `application` обязано совпадать с зарегистрированным `application_name` клиента (`403 Forbidden` при попытке подмены имени).
   - Удалённое подтверждение (`crates/approval`) **никогда не заменяет и не обходит штатный диалог повышения привилегий Windows UAC**.

---

## 2. Регистрация и управление клиентами

### Через CLI (`nexdeck-agent`)

```powershell
# Регистрация нового клиента со всеми базовыми правами
nexdeck-agent --register-client --id org.example.app --name ExampleApp --permissions read_metrics,send_notification,request_approval

# Просмотр списка зарегистрированных клиентов
nexdeck-agent --list-clients

# Отзыв токена клиента
nexdeck-agent --revoke-client org.example.app
```

При вызове `--register-client` агент выводит JSON со свежевыпущенным `client_token` и сохраняет его SHA-256 хеш в `%APPDATA%\NexDeck\clients.json`.

### Через Desktop Manager (`nexdeck-desktop`)

Откройте вкладку **«Устройства»** -> секция **«Universal API клиенты (clients.json)»**:
1. Укажите `client_id` (например, `org.example.app`), имя приложения (`ExampleApp`) и отметьте нужные scopes.
2. Нажмите **«Выпустить токен клиента»** и скопируйте одноразовый `client_token`.
3. В таблице ниже доступны кнопки **«Отозвать» / «Разблокировать»** и **«Удалить»**.

---

## 3. Справочник HTTP и WebSocket маршрутов

| Метод и маршрут | Требуемое право | Описание |
| --- | --- | --- |
| `GET /api/v1/status` | Любой активный токен | Возвращает `status`, `protocol_version`, `client_id`, `application_name`, список `permissions` |
| `GET /api/v1/metrics` | `read_metrics` | Возвращает последний снимок `SystemSnapshot` (CPU, RAM, GPU, диски, сеть, батарея) |
| `GET /api/v1/notifications` | `send_notification` | Возвращает очередь уведомлений (`NotificationEvent[]`) |
| `POST /api/v1/notifications` | `send_notification` | Публикует новое уведомление (`title`, `body`, `priority`) от имени `ThirdPartyApp(client.application_name)` |
| `GET /api/v1/approvals` | `request_approval` | Возвращает список запросов подтверждения (`ApprovalRequest[]`) |
| `POST /api/v1/approvals` | `request_approval` | Регистрирует одноразовый запрос подтверждения (`CreateApprovalRequest`) |
| `GET /api/v1/approvals/{request_id}` | `request_approval` | Возвращает текущий статус запроса (`Pending`, `Resolved`, `Expired`, `Revoked`) |
| `POST /api/v1/approvals/{request_id}/resolve` | `request_approval` | Фиксирует решение по запросу (`action`) |
| `POST /api/v1/approvals/{request_id}/revoke` | `request_approval` | Отзывает ожидающий запрос подтверждения |
| `POST /api/v1/control` | `execute_control` | Выполняет команду управления (`ControlAction`) и возвращает `ControlAckPayload` |
| `GET /api/v1/ws` | Любой активный токен | WebSocket-поток событий реального времени, отфильтрованный по правам клиента |

---

## 4. Клиентские SDK и примеры

В репозитории поставляются два легковесных SDK без внешних зависимостей:

### Python SDK (`sdk/python/nexdeck_sdk.py`)

```python
from nexdeck_sdk import NexDeckClient

client = NexDeckClient("https://127.0.0.1:8765", "<client_token>", verify_tls=False)
print(client.status())

client.send_notification(
    title="ExampleApp",
    body="Сборка проекта успешно завершена",
)

created = client.request_approval(
    request_id="req-example-1",
    application="ExampleApp",
    title="Применить конфигурацию ExampleApp",
    description="Подтвердите применение синтетического пресета",
    actions=["approve", "deny"],
)
decision = client.wait_for_approval("req-example-1", timeout_sec=60.0)
print(decision["request"]["status"])
```

Готовый пример: [`examples/api_approval_example.py`](../../examples/api_approval_example.py), эталонный JSON-пакет: [`examples/api_approval_request.json`](../../examples/api_approval_request.json).

### JavaScript / TypeScript SDK (`sdk/js/nexdeck_sdk.mjs`, `sdk/js/nexdeck_sdk.d.ts`)

```javascript
import { NexDeckClient } from "./sdk/js/nexdeck_sdk.mjs";

const client = new NexDeckClient("https://127.0.0.1:8765", "<client_token>", {
  rejectUnauthorized: false,
});

const status = await client.status();
await client.sendNotification({
  title: "ExampleApp",
  body: "Фоновая задача завершена",
});
```

Готовый пример: [`examples/api_notification_example.mjs`](../../examples/api_notification_example.mjs).
