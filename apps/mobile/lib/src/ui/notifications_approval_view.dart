import 'package:material_ui/material_ui.dart';

import '../models/protocol_models.dart';
import '../state/agent_controller.dart';
import '../transport/transport_client.dart';
import 'formatters.dart';

/// Filter categories displayed inside the mobile Notification Center.
enum NotificationCategoryFilter {
  /// Show all received events matching the host filter.
  all('Все'),

  /// Show only elevated events (`High` or `Critical`).
  highOnly('Важные'),

  /// Show host hardware and power threshold alerts.
  hardware('Датчики ПК'),

  /// Show events from registered Universal API applications and plugins.
  appsAndPlugins('Приложения');

  const NotificationCategoryFilter(this.label);

  /// Localized chip label.
  final String label;

  /// Returns `true` when [event] satisfies this view filter.
  bool matches(NotificationEvent event) {
    return switch (this) {
      NotificationCategoryFilter.all => true,
      NotificationCategoryFilter.highOnly => event.priority.meetsMinimum(
        NotificationPriority.high,
      ),
      NotificationCategoryFilter.hardware =>
        event.source.kind == NotificationSourceKind.hardwareAlert,
      NotificationCategoryFilter.appsAndPlugins =>
        event.source.kind == NotificationSourceKind.application ||
            event.source.kind == NotificationSourceKind.plugin,
    };
  }
}

/// Interactive Remote Approval prompts and synchronized Notification Center view.
class NotificationsApprovalView extends StatefulWidget {
  /// Creates a [NotificationsApprovalView].
  const NotificationsApprovalView({required this.controller, super.key});

  /// Controller supplying approvals, notifications, and decision dispatch actions.
  final AgentController controller;

  @override
  State<NotificationsApprovalView> createState() =>
      _NotificationsApprovalViewState();
}

class _NotificationsApprovalViewState extends State<NotificationsApprovalView> {
  NotificationCategoryFilter _selectedCategory = NotificationCategoryFilter.all;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final colors = theme.colorScheme;
    final connected =
        widget.controller.connectionState == TransportConnectionState.connected;
    final approvals = widget.controller.approvals;
    final allNotifications = widget.controller.notifications;
    final visibleNotifications = allNotifications
        .where(_selectedCategory.matches)
        .toList(growable: false);
    final hostFilterHighOnly =
        widget.controller.notificationFilter.minPriority ==
        NotificationPriority.high;

    return ListView(
      padding: const EdgeInsets.all(16),
      children: [
        Row(
          children: [
            Icon(Icons.verified_user_outlined, color: colors.primary),
            const SizedBox(width: 8),
            Expanded(
              child: Text(
                'Удалённое подтверждение (${widget.controller.pendingApprovalCount})',
                style: theme.textTheme.titleMedium?.copyWith(
                  fontWeight: FontWeight.w700,
                ),
              ),
            ),
          ],
        ),
        const SizedBox(height: 8),
        Card(
          key: const Key('uac_boundary_notice'),
          color: colors.surfaceContainerHighest.withValues(alpha: 0.65),
          child: Padding(
            padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 10),
            child: Row(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Icon(Icons.shield_outlined, size: 18, color: colors.secondary),
                const SizedBox(width: 10),
                Expanded(
                  child: Text(
                    'Удалённое подтверждение NexDeck действует только для разрешённых действий приложений и никогда не заменяет системный Windows UAC.',
                    style: theme.textTheme.bodySmall?.copyWith(
                      color: colors.onSurfaceVariant,
                    ),
                  ),
                ),
              ],
            ),
          ),
        ),
        const SizedBox(height: 10),
        if (approvals.isEmpty)
          Card(
            key: const Key('empty_approvals_card'),
            child: Padding(
              padding: const EdgeInsets.all(16),
              child: Row(
                children: [
                  Icon(
                    Icons.check_circle_outline,
                    color: colors.onSurfaceVariant,
                  ),
                  const SizedBox(width: 12),
                  const Expanded(
                    child: Text(
                      'Активных запросов на подтверждение сейчас нет.',
                    ),
                  ),
                ],
              ),
            ),
          )
        else
          for (final request in approvals) ...[
            _ApprovalRequestCard(
              request: request,
              connected: connected,
              isSubmitting: widget.controller.isSubmittingApproval(
                request.requestId,
              ),
              onDecision: (action) => widget.controller.submitApprovalDecision(
                request.requestId,
                action,
              ),
            ),
            const SizedBox(height: 10),
          ],
        const SizedBox(height: 12),
        Row(
          children: [
            Icon(Icons.notifications_active_outlined, color: colors.primary),
            const SizedBox(width: 8),
            Expanded(
              child: Text(
                'Центр уведомлений (${visibleNotifications.length})',
                style: theme.textTheme.titleMedium?.copyWith(
                  fontWeight: FontWeight.w700,
                ),
              ),
            ),
            TextButton.icon(
              key: const Key('clear_notifications_button'),
              onPressed: allNotifications.isEmpty
                  ? null
                  : widget.controller.clearNotifications,
              icon: const Icon(Icons.clear_all, size: 18),
              label: const Text('Очистить всё'),
            ),
          ],
        ),
        const SizedBox(height: 8),
        Wrap(
          spacing: 8,
          runSpacing: 6,
          children: [
            for (final category in NotificationCategoryFilter.values)
              FilterChip(
                key: Key('notif_filter_${category.name}'),
                label: Text(category.label),
                selected: _selectedCategory == category,
                onSelected: (_) {
                  setState(() => _selectedCategory = category);
                },
              ),
            FilterChip(
              key: const Key('sync_high_priority_filter'),
              label: const Text('С ПК только High+'),
              selected: hostFilterHighOnly,
              onSelected: (enabled) {
                final nextFilter = NotificationFilter(
                  minPriority: enabled
                      ? NotificationPriority.high
                      : NotificationPriority.low,
                  mutedOrigins:
                      widget.controller.notificationFilter.mutedOrigins,
                );
                widget.controller.updateNotificationFilter(nextFilter);
              },
            ),
          ],
        ),
        const SizedBox(height: 12),
        if (visibleNotifications.isEmpty)
          Card(
            key: const Key('empty_notifications_card'),
            child: Padding(
              padding: const EdgeInsets.all(16),
              child: Row(
                children: [
                  Icon(
                    Icons.notifications_off_outlined,
                    color: colors.onSurfaceVariant,
                  ),
                  const SizedBox(width: 12),
                  const Expanded(
                    child: Text(
                      'Новых или отложенных уведомлений по выбранному фильтру нет.',
                    ),
                  ),
                ],
              ),
            ),
          )
        else
          for (final event in visibleNotifications) ...[
            _NotificationItemCard(
              event: event,
              onDismiss: () => widget.controller.dismissNotification(event.id),
            ),
            const SizedBox(height: 8),
          ],
      ],
    );
  }
}

class _ApprovalRequestCard extends StatelessWidget {
  const _ApprovalRequestCard({
    required this.request,
    required this.connected,
    required this.isSubmitting,
    required this.onDecision,
  });

  final ApprovalRequest request;
  final bool connected;
  final bool isSubmitting;
  final ValueChanged<String> onDecision;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final colors = theme.colorScheme;
    final riskColor = switch (request.riskLevel) {
      RiskLevel.low => Colors.teal,
      RiskLevel.medium => Colors.amber.shade800,
      RiskLevel.high || RiskLevel.critical => colors.error,
    };

    return Card(
      key: Key('approval_card_${request.requestId}'),
      shape: RoundedRectangleBorder(
        borderRadius: BorderRadius.circular(14),
        side: BorderSide(
          color: request.isPending
              ? riskColor.withValues(alpha: 0.65)
              : colors.outlineVariant,
          width: request.isPending ? 1.5 : 1.0,
        ),
      ),
      child: Padding(
        padding: const EdgeInsets.all(14),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Wrap(
              spacing: 8,
              runSpacing: 6,
              crossAxisAlignment: WrapCrossAlignment.center,
              children: [
                Chip(
                  avatar: const Icon(Icons.apps_outlined, size: 16),
                  label: Text(request.application),
                  visualDensity: VisualDensity.compact,
                ),
                Chip(
                  label: Text(
                    request.riskLevel.displayLabel,
                    style: TextStyle(
                      color: riskColor,
                      fontWeight: FontWeight.w600,
                    ),
                  ),
                  visualDensity: VisualDensity.compact,
                ),
                Chip(
                  label: Text(request.status.state.displayLabel),
                  visualDensity: VisualDensity.compact,
                ),
              ],
            ),
            const SizedBox(height: 8),
            Text(
              request.title,
              style: theme.textTheme.titleMedium?.copyWith(
                fontWeight: FontWeight.w700,
              ),
            ),
            const SizedBox(height: 4),
            Text(request.description, style: theme.textTheme.bodyMedium),
            const SizedBox(height: 6),
            Text(
              'ID: ${request.requestId} • Действует до ${formatEpochTime(request.expiresAtEpochMs)}',
              style: theme.textTheme.bodySmall?.copyWith(
                color: colors.onSurfaceVariant,
              ),
            ),
            const SizedBox(height: 12),
            if (request.isPending)
              Wrap(
                spacing: 8,
                runSpacing: 8,
                children: [
                  for (final action in request.allowedActions)
                    _buildDecisionButton(context, action),
                ],
              )
            else
              Text(
                _resolvedSummaryText(request.status),
                style: theme.textTheme.bodyMedium?.copyWith(
                  color: colors.onSurfaceVariant,
                  fontWeight: FontWeight.w600,
                ),
              ),
          ],
        ),
      ),
    );
  }

  Widget _buildDecisionButton(BuildContext context, String action) {
    final normalized = action.trim().toLowerCase();
    final isPrimary = normalized == 'approve' || normalized == 'allow';
    final isDestructive =
        normalized == 'deny' || normalized == 'reject' || normalized == 'block';
    final label = localizedApprovalActionLabel(action);
    final buttonKey = Key('approval_action_${request.requestId}_$action');
    final enabled = connected && !isSubmitting;

    if (isPrimary) {
      return FilledButton.icon(
        key: buttonKey,
        onPressed: enabled ? () => onDecision(action) : null,
        icon: const Icon(Icons.check, size: 18),
        label: Text(label),
      );
    }
    return OutlinedButton.icon(
      key: buttonKey,
      onPressed: enabled ? () => onDecision(action) : null,
      icon: Icon(isDestructive ? Icons.close : Icons.tune, size: 18),
      label: Text(label),
      style: isDestructive
          ? OutlinedButton.styleFrom(
              foregroundColor: Theme.of(context).colorScheme.error,
            )
          : null,
    );
  }

  String _resolvedSummaryText(ApprovalStatus status) {
    if (status.state == ApprovalStatusState.resolved &&
        status.selectedAction != null) {
      return 'Решение принято: ${localizedApprovalActionLabel(status.selectedAction!)}';
    }
    return 'Статус: ${status.state.displayLabel}';
  }
}

/// Maps wire approval action identifiers to friendly Russian labels.
String localizedApprovalActionLabel(String action) {
  return switch (action.trim().toLowerCase()) {
    'approve' => 'Подтвердить',
    'allow' => 'Разрешить',
    'deny' => 'Отклонить',
    'reject' => 'Отказать',
    'cancel' => 'Отменить',
    _ => action,
  };
}

class _NotificationItemCard extends StatelessWidget {
  const _NotificationItemCard({required this.event, required this.onDismiss});

  final NotificationEvent event;
  final VoidCallback onDismiss;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final colors = theme.colorScheme;
    final accent = switch (event.priority) {
      NotificationPriority.low => colors.outline,
      NotificationPriority.normal => colors.primary,
      NotificationPriority.high => Colors.orange.shade700,
      NotificationPriority.critical => colors.error,
    };

    return Card(
      key: Key('notification_card_${event.id}'),
      child: Padding(
        padding: const EdgeInsets.fromLTRB(12, 10, 8, 12),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Row(
              children: [
                Container(
                  width: 10,
                  height: 10,
                  decoration: BoxDecoration(
                    color: accent,
                    shape: BoxShape.circle,
                  ),
                ),
                const SizedBox(width: 8),
                Text(
                  event.source.originLabel,
                  style: theme.textTheme.labelMedium?.copyWith(
                    color: accent,
                    fontWeight: FontWeight.w700,
                  ),
                ),
                const SizedBox(width: 8),
                Text(
                  '• ${event.priority.displayLabel} • ${formatEpochTime(event.createdAtEpochMs)}',
                  style: theme.textTheme.labelSmall?.copyWith(
                    color: colors.onSurfaceVariant,
                  ),
                ),
                const Spacer(),
                IconButton(
                  key: Key('dismiss_notification_${event.id}'),
                  tooltip: 'Скрыть уведомление',
                  visualDensity: VisualDensity.compact,
                  onPressed: onDismiss,
                  icon: const Icon(Icons.close, size: 18),
                ),
              ],
            ),
            const SizedBox(height: 4),
            Text(
              event.title,
              style: theme.textTheme.titleSmall?.copyWith(
                fontWeight: FontWeight.w700,
              ),
            ),
            const SizedBox(height: 2),
            Text(event.body, style: theme.textTheme.bodyMedium),
          ],
        ),
      ),
    );
  }
}
