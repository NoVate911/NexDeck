# Архив журнала: Этап 5 — Notifications & Remote Approval (`crates/notifications`, `crates/approval`, `crates/core`, `apps/mobile`, `apps/desktop`)

## Сводка этапа

- **Период / коммиты:** PR #39 (Issue #38 — серверная и доменная часть: `crates/approval`, `crates/notifications`, `crates/transport`, `crates/core`, интеграционный тест `stage5_notifications_and_approval.rs`) и PR 5.2 (Issue #40 — клиентский UI в `apps/mobile` и управление правилами/буфером в `apps/desktop`).
- **Цель этапа:** обеспечить сквозную доставку системных уведомлений, аппаратных тревог ПК и одноразовых запросов удалённого подтверждения действий (`Remote Approval`) с буфером сна в `notifications.json`, синхронизацией действий между телефоном и агентом и строгим сохранением границы безопасности Windows UAC.

---

## Выполненные задачи

1. **Протокол и доменные механизмы (`crates/approval`, `crates/notifications`, `crates/transport`):**
   - В `crates/approval` добавлены `ApprovalDecisionMessage`, восстановление `ApprovalRegistry::from_records`, `pending_active(now_epoch_ms)` и очистка истёкших заявок `expire_stale(now_epoch_ms)`.
   - В `crates/notifications` добавлены `NotificationSyncCommand` (`Dismiss`, `ClearAll`, `UpdateFilter`), нормализация и фильтрация (`NotificationFilter::normalize`), удаление/очистка элементов очереди `NotificationQueue`.
   - В `crates/transport/src/server.rs` реализованы `BacklogRequestTx`, счётчик подключённых клиентов `connected_clients()` и мгновенная отправка бэклога при открытии `/ws`.
2. **Core Agent Runtime и мониторинг тревог (`crates/core`):**
   - Реализован `HardwareAlertMonitor` с `AlertThresholds` (`crates/core/src/alerts.rs`), генерирующий события при превышении порогов CPU/RAM/температур/низкого заряда с защитой от спама (`cooldown_secs`) и сменой питания от сети/батареи.
   - В `StoragePaths` добавлено атомарное сохранение и загрузка буфера сна `notifications.json`, а в `AgentConfig` — `#[serde(default)]` поля `notification_filter` и `alert_thresholds`.
   - В `runtime.rs` подключены горячая перезагрузка политики из `config.json`, асинхронный разбор `ApprovalDecisionMessage` (с защитой от replay) и `NotificationSyncCommand`, рассылка автоистечения `expire_stale` и бэклог на `/ws connect`.
3. **Мобильный клиент (`apps/mobile`):**
   - В `protocol_models.dart` и `AgentController` добавлены модели уведомлений и `ApprovalRequest`, дедупликация по `id`/`requestId`, локальный и хостовый фильтры (`updateNotificationFilter`), команды синхронизации (`dismissNotification`, `clearNotifications`) и одноразовое решение `submitApprovalDecision` с локальной проверкой replay.
   - Создан экран [`NotificationsApprovalView`](../../../apps/mobile/lib/src/ui/notifications_approval_view.dart) во вкладке «События» `DashboardScreen` (бейдж количества, срочный баннер поверх других вкладок, явная заметка о границе Windows UAC, интерактивные карточки подтверждения и фильтрация/удаление уведомлений) + тесты `notifications_and_approval_test.dart`.
4. **Desktop Manager (`apps/desktop`):**
   - В `apps/desktop/src/state.rs` и `apps/desktop/src/ui.rs` добавлено управление фильтром уведомлений (`min_priority`, `muted_origins`), порогами тревог ПК в `config.json`, а также просмотр и очистка буфера сна `notifications.json` во вкладке «Подключение и ПК».
