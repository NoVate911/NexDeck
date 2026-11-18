import 'dart:async';

import 'package:flutter_test/flutter_test.dart';
import 'package:material_ui/material_ui.dart';
import 'package:nexdeck_mobile/main.dart';
import 'package:nexdeck_mobile/src/models/protocol_models.dart';
import 'package:nexdeck_mobile/src/state/agent_controller.dart';
import 'package:nexdeck_mobile/src/storage/session_store.dart';
import 'package:nexdeck_mobile/src/transport/pinned_http_client.dart';
import 'package:nexdeck_mobile/src/transport/transport_client.dart';

class _TestPairingClient implements PairingClient {
  @override
  Future<HealthResponse> checkHealth(
    Uri baseUri, {
    required String expectedFingerprintSha256,
  }) async {
    return HealthResponse(
      status: 'ok',
      protocolVersion: nexdeckProtocolVersion,
      fingerprintSha256: expectedFingerprintSha256,
    );
  }

  @override
  Future<PairedCredentials> pair({
    required Uri baseUri,
    required String expectedFingerprintSha256,
    required PairRequest request,
  }) async {
    return PairedCredentials(
      deviceId: request.deviceId,
      deviceToken: 'example-session-token',
      fingerprintSha256: expectedFingerprintSha256,
    );
  }
}

class _TestTransport implements NexDeckTransport {
  final StreamController<TransportConnectionState> _states =
      StreamController<TransportConnectionState>.broadcast();
  final StreamController<Envelope> _envelopes =
      StreamController<Envelope>.broadcast();

  TransportConnectionState _state = TransportConnectionState.disconnected;
  final List<Envelope> sentEnvelopes = <Envelope>[];

  @override
  TransportConnectionState get currentState => _state;

  @override
  Stream<TransportConnectionState> get states => _states.stream;

  @override
  Stream<Envelope> get envelopes => _envelopes.stream;

  @override
  Future<void> connect(HostSession session) async {
    _state = TransportConnectionState.connected;
    _states.add(_state);
  }

  void pushEnvelope(Envelope envelope) {
    _envelopes.add(envelope);
  }

  @override
  Future<void> sendEnvelope(Envelope envelope) async {
    sentEnvelopes.add(envelope);
  }

  @override
  Future<void> disconnect() async {
    _state = TransportConnectionState.disconnected;
    _states.add(_state);
  }

  @override
  Future<void> dispose() async {
    await _states.close();
    await _envelopes.close();
  }
}

HostSession _exampleSession() {
  return HostSession(
    host: '127.0.0.1',
    port: 9443,
    deviceId: 'mobile-example',
    deviceName: 'ExamplePhone',
    deviceToken: 'example-device-secret',
    fingerprintSha256: 'a' * 64,
  );
}

Envelope _notificationEnvelope({
  required int messageId,
  required int eventId,
  required String title,
  String body = 'Example notification details',
  String priority = 'High',
  Map<String, Object?> source = const {'kind': 'HardwareAlert'},
}) {
  return Envelope.json(
    messageId: messageId,
    kind: MessageKind.notification,
    body: {
      'id': eventId,
      'source': source,
      'title': title,
      'body': body,
      'priority': priority,
      'created_at_epoch_ms': 1700000000000 + eventId * 1000,
    },
  );
}

Envelope _approvalEnvelope({
  required int messageId,
  required String requestId,
  String application = 'ExampleApp',
  String title = 'Подтвердить действие ExampleApp',
  String description = 'Разрешить запуск безопасного сценария',
  String riskLevel = 'Medium',
  String state = 'Pending',
  String? selectedAction,
}) {
  return Envelope.json(
    messageId: messageId,
    kind: MessageKind.approval,
    body: {
      'request_id': requestId,
      'application': application,
      'title': title,
      'description': description,
      'expires_at_epoch_ms': 1900000000000,
      'risk_level': riskLevel,
      'allowed_actions': ['approve', 'deny'],
      'status': {'state': state, 'selected_action': ?selectedAction},
    },
  );
}

void main() {
  test('controller tracks notifications, syncs dismiss/clear/filter, and guards approval replay', () async {
    final store = InMemorySessionStore();
    await store.saveSession(_exampleSession());
    final transport = _TestTransport();
    final controller = AgentController(
      sessionStore: store,
      pairingClient: _TestPairingClient(),
      transport: transport,
    );
    addTearDown(controller.dispose);

    await controller.initialize();
    expect(controller.connectionState, TransportConnectionState.connected);

    // Push initial sleep buffer notification + duplicate update on id=10.
    transport.pushEnvelope(
      _notificationEnvelope(
        messageId: 1,
        eventId: 10,
        title: 'First alert',
        priority: 'Normal',
        source: const {'kind': 'Application', 'name': 'ExampleApp'},
      ),
    );
    transport.pushEnvelope(
      _notificationEnvelope(
        messageId: 2,
        eventId: 11,
        title: 'CPU alert',
        priority: 'High',
      ),
    );
    transport.pushEnvelope(
      _notificationEnvelope(
        messageId: 3,
        eventId: 10,
        title: 'Updated alert',
        priority: 'Normal',
        source: const {'kind': 'Application', 'name': 'ExampleApp'},
      ),
    );
    await pumpEventQueue();

    expect(controller.notifications.length, 2);
    expect(controller.notifications.first.id, 11);
    expect(controller.notifications.last.title, 'Updated alert');

    // Update filter to High+ and sync to agent.
    await controller.updateNotificationFilter(
      const NotificationFilter(minPriority: NotificationPriority.high),
    );
    expect(controller.notifications.length, 1);
    expect(controller.notifications.single.id, 11);
    final filterCmd = transport.sentEnvelopes.last.decodePayloadJson();
    expect(filterCmd['op'], 'update_filter');

    // Dismiss single notification.
    await controller.dismissNotification(11);
    expect(controller.notifications, isEmpty);
    final dismissCmd = transport.sentEnvelopes.last.decodePayloadJson();
    expect(dismissCmd['op'], 'dismiss');
    expect(dismissCmd['event_id'], 11);

    // Clear all notifications.
    await controller.clearNotifications();
    final clearCmd = transport.sentEnvelopes.last.decodePayloadJson();
    expect(clearCmd['op'], 'clear_all');

    // Approval prompt lifecycle & replay prevention.
    transport.pushEnvelope(
      _approvalEnvelope(messageId: 4, requestId: 'req-example-1'),
    );
    await pumpEventQueue();
    expect(controller.pendingApprovalCount, 1);

    await controller.submitApprovalDecision('req-example-1', 'approve');
    expect(transport.sentEnvelopes.last.kind, MessageKind.approval);
    expect(transport.sentEnvelopes.last.decodePayloadJson(), {
      'request_id': 'req-example-1',
      'action': 'approve',
    });

    // Authoritative Resolved reply arrives from host.
    transport.pushEnvelope(
      _approvalEnvelope(
        messageId: 5,
        requestId: 'req-example-1',
        state: 'Resolved',
        selectedAction: 'approve',
      ),
    );
    await pumpEventQueue();
    expect(controller.pendingApprovalCount, 0);
    expect(
      controller.approvals.single.status.state,
      ApprovalStatusState.resolved,
    );

    // Replay attempt after resolution must be rejected locally without sending wire frame.
    final sentBefore = transport.sentEnvelopes.length;
    await controller.submitApprovalDecision('req-example-1', 'deny');
    expect(transport.sentEnvelopes.length, sentBefore);
    expect(controller.errorMessage, isNotNull);
  });

  testWidgets(
    'dashboard renders urgent approval banner, events tab, notification filters, and action buttons',
    (WidgetTester tester) async {
      final store = InMemorySessionStore();
      await store.saveSession(_exampleSession());
      final transport = _TestTransport();
      final controller = AgentController(
        sessionStore: store,
        pairingClient: _TestPairingClient(),
        transport: transport,
      );
      addTearDown(controller.dispose);

      await tester.pumpWidget(
        NexDeckApp(controller: controller, autoInitialize: true),
      );
      await tester.pumpAndSettle();

      transport.pushEnvelope(
        _approvalEnvelope(
          messageId: 10,
          requestId: 'req-example-2',
          application: 'ExampleBackup',
          title: 'Разрешить архивацию',
          riskLevel: 'High',
        ),
      );
      transport.pushEnvelope(
        _notificationEnvelope(
          messageId: 11,
          eventId: 21,
          title: 'Высокая температура CPU',
          priority: 'High',
          source: const {'kind': 'HardwareAlert'},
        ),
      );
      transport.pushEnvelope(
        _notificationEnvelope(
          messageId: 12,
          eventId: 22,
          title: 'Фоновая задача завершена',
          priority: 'Low',
          source: const {'kind': 'Application', 'name': 'ExampleApp'},
        ),
      );
      await tester.pumpAndSettle();

      // Urgent banner is visible from Monitoring tab and jumps directly to Events tab.
      expect(find.byKey(const Key('urgent_approval_banner')), findsOneWidget);
      await tester.tap(find.byKey(const Key('urgent_approval_open_button')));
      await tester.pumpAndSettle();

      expect(find.byKey(const Key('uac_boundary_notice')), findsOneWidget);
      expect(find.textContaining('Windows UAC'), findsOneWidget);
      expect(
        find.byKey(const Key('approval_card_req-example-2')),
        findsOneWidget,
      );

      final eventsScrollable = find.descendant(
        of: find.byKey(const ValueKey('events_tab_view')),
        matching: find.byType(Scrollable),
      );
      await tester.scrollUntilVisible(
        find.text('Фоновая задача завершена'),
        200,
        scrollable: eventsScrollable,
      );
      await tester.pumpAndSettle();
      expect(find.text('Высокая температура CPU'), findsOneWidget);
      expect(find.text('Фоновая задача завершена'), findsOneWidget);

      // Filter by High+ notifications hides Low notification cleanly.
      final highChip = find.byKey(const Key('notif_filter_highOnly'));
      await tester.scrollUntilVisible(
        highChip,
        -200,
        scrollable: eventsScrollable,
      );
      await tester.ensureVisible(highChip);
      await tester.pumpAndSettle();
      await tester.tap(highChip);
      await tester.pumpAndSettle();
      await tester.scrollUntilVisible(
        find.text('Высокая температура CPU'),
        200,
        scrollable: eventsScrollable,
      );
      await tester.ensureVisible(find.text('Высокая температура CPU'));
      await tester.pumpAndSettle();
      expect(find.text('Высокая температура CPU'), findsOneWidget);
      expect(find.text('Фоновая задача завершена'), findsNothing);

      // Reset to All and dismiss single notification.
      final allChip = find.byKey(const Key('notif_filter_all'));
      await tester.scrollUntilVisible(
        allChip,
        -200,
        scrollable: eventsScrollable,
      );
      await tester.ensureVisible(allChip);
      await tester.pumpAndSettle();
      await tester.tap(allChip);
      await tester.pumpAndSettle();
      final dismissBtn = find.byKey(const Key('dismiss_notification_21'));
      await tester.scrollUntilVisible(
        dismissBtn,
        150,
        scrollable: eventsScrollable,
      );
      await tester.ensureVisible(dismissBtn);
      await tester.pumpAndSettle();
      await tester.tap(dismissBtn);
      await tester.pumpAndSettle();
      expect(find.text('Высокая температура CPU'), findsNothing);
      expect(find.text('Фоновая задача завершена'), findsOneWidget);

      // Approve remote request from Events tab.
      final approveBtn = find.byKey(
        const Key('approval_action_req-example-2_approve'),
      );
      await tester.scrollUntilVisible(
        approveBtn,
        -200,
        scrollable: eventsScrollable,
      );
      await tester.ensureVisible(approveBtn);
      await tester.pumpAndSettle();
      await tester.tap(approveBtn);
      await tester.pump();
      expect(
        transport.sentEnvelopes.where((e) => e.kind == MessageKind.approval),
        isNotEmpty,
      );
    },
  );
}
