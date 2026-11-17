import 'dart:async';
import 'dart:convert';

import 'package:flutter_test/flutter_test.dart';
import 'package:material_ui/material_ui.dart';
import 'package:nexdeck_mobile/main.dart';
import 'package:nexdeck_mobile/src/models/protocol_models.dart';
import 'package:nexdeck_mobile/src/state/agent_controller.dart';
import 'package:nexdeck_mobile/src/storage/session_store.dart';
import 'package:nexdeck_mobile/src/transport/pinned_http_client.dart';
import 'package:nexdeck_mobile/src/transport/transport_client.dart';

class _FakePairingClient implements PairingClient {
  _FakePairingClient({required this.expectedFingerprint});

  final String expectedFingerprint;
  PairRequest? lastRequest;

  @override
  Future<HealthResponse> checkHealth(
    Uri baseUri, {
    required String expectedFingerprintSha256,
  }) async {
    return HealthResponse(
      status: 'ok',
      protocolVersion: nexdeckProtocolVersion,
      fingerprintSha256: expectedFingerprint,
    );
  }

  @override
  Future<PairedCredentials> pair({
    required Uri baseUri,
    required String expectedFingerprintSha256,
    required PairRequest request,
  }) async {
    lastRequest = request;
    return PairedCredentials(
      deviceId: request.deviceId,
      deviceToken: 'paired-session-${DateTime.now().microsecondsSinceEpoch}',
      fingerprintSha256: expectedFingerprint,
    );
  }
}

class _FakeTransport implements NexDeckTransport {
  final StreamController<TransportConnectionState> _states =
      StreamController<TransportConnectionState>.broadcast();
  final StreamController<Envelope> _envelopes =
      StreamController<Envelope>.broadcast();

  TransportConnectionState _state = TransportConnectionState.disconnected;
  final List<Envelope> sentEnvelopes = [];

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

  void pushTelemetry({required double cpuPct, required int uptimeSecs}) {
    _envelopes.add(
      Envelope.json(
        messageId: 10,
        kind: MessageKind.telemetry,
        body: {
          'captured_at_epoch_ms': 1700000001000,
          'uptime_secs': uptimeSecs,
          'cpu': {
            'total_usage_pct': cpuPct,
            'per_core_usage_pct': [cpuPct, cpuPct],
            'frequency_mhz': 3900,
            'temperature_celsius': null,
          },
          'memory': {
            'total_bytes': 16 * 1024 * 1024 * 1024,
            'used_bytes': 4 * 1024 * 1024 * 1024,
          },
          'gpus': [
            {
              'name': 'ExampleGPU',
              'usage_pct': 18.0,
              'temperature_celsius': 52.0,
              'clock_mhz': 1650,
              'vram_used_bytes': 2 * 1024 * 1024 * 1024,
              'vram_total_bytes': 8 * 1024 * 1024 * 1024,
            },
          ],
          'disks': [
            {
              'mount_point': r'C:\',
              'total_bytes': 256 * 1024 * 1024 * 1024,
              'free_bytes': 128 * 1024 * 1024 * 1024,
              'read_bytes_per_sec': 0,
              'write_bytes_per_sec': 0,
            },
          ],
          'networks': [
            {
              'interface_name': 'ExampleWiFi',
              'tx_bytes_per_sec': 2048,
              'rx_bytes_per_sec': 8192,
              'total_tx_bytes': 1024 * 1024,
              'total_rx_bytes': 4 * 1024 * 1024,
            },
          ],
          'battery': null,
        },
      ),
    );
  }

  void pushControlAck(String commandId, ControlAction action) {
    _envelopes.add(
      Envelope.json(
        messageId: 20,
        kind: MessageKind.controlCommand,
        body: {
          'command_id': commandId,
          'action': action.wireName,
          'ok': true,
          'message': 'Executed ${action.wireName}',
        },
      ),
    );
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

void main() {
  testWidgets(
    'imports QR JSON, pairs, renders telemetry, and sends heartbeat',
    (WidgetTester tester) async {
      final fingerprint = computeCertificateSha256(utf8.encode('example-cert'));
      final store = InMemorySessionStore();
      final pairingClient = _FakePairingClient(
        expectedFingerprint: fingerprint,
      );
      final transport = _FakeTransport();
      final controller = AgentController(
        sessionStore: store,
        pairingClient: pairingClient,
        transport: transport,
      );
      addTearDown(controller.dispose);

      await tester.pumpWidget(
        NexDeckApp(controller: controller, autoInitialize: false),
      );

      expect(find.text('NexDeck — Сопряжение с ПК'), findsOneWidget);

      final qrJson = jsonEncode({
        'protocol_version': 1,
        'host': '127.0.0.1',
        'port': 9443,
        'fingerprint_sha256': fingerprint,
        'pin': '654321',
      });

      await tester.enterText(find.byKey(const Key('qr_json_field')), qrJson);
      await tester.tap(find.byKey(const Key('apply_qr_button')));
      await tester.pump();

      await tester.ensureVisible(find.byKey(const Key('pair_submit_button')));
      await tester.tap(find.byKey(const Key('pair_submit_button')));
      await tester.pumpAndSettle();

      expect(pairingClient.lastRequest?.pin, '654321');
      expect(pairingClient.lastRequest?.deviceName, 'ExamplePhone');
      expect(find.text('NexDeck — Мониторинг ПК'), findsOneWidget);
      expect(find.text('Подключено'), findsOneWidget);

      transport.pushTelemetry(cpuPct: 44.5, uptimeSecs: 125);
      await tester.pumpAndSettle();

      await tester.ensureVisible(find.byKey(const Key('control_play_pause')));
      await tester.tap(find.byKey(const Key('control_play_pause')));
      await tester.pump();
      expect(transport.sentEnvelopes.last.kind, MessageKind.controlCommand);
      final reqPayload = transport.sentEnvelopes.last.decodePayloadJson();
      expect(reqPayload['action'], 'MediaPlayPause');

      transport.pushControlAck(
        reqPayload['command_id']! as String,
        ControlAction.mediaPlayPause,
      );
      await tester.pumpAndSettle();
      expect(find.textContaining('Последняя команда:'), findsOneWidget);

      await tester.scrollUntilVisible(
        find.byKey(const Key('toggle_cpu_cores_button')),
        200,
        scrollable: find.byType(Scrollable),
      );
      expect(find.text('CPU: 44.5%'), findsOneWidget);
      expect(find.byKey(const Key('cpu_history_chart')), findsOneWidget);
      await tester.tap(find.byKey(const Key('toggle_cpu_cores_button')));
      await tester.pumpAndSettle();
      expect(find.textContaining('CPU 0:'), findsOneWidget);
      expect(find.textContaining('CPU 1:'), findsOneWidget);

      await tester.scrollUntilVisible(
        find.byKey(const Key('ram_history_chart')),
        200,
        scrollable: find.byType(Scrollable),
      );
      expect(find.text('RAM: 25.0%'), findsOneWidget);
      expect(find.byKey(const Key('ram_history_chart')), findsOneWidget);

      await tester.scrollUntilVisible(
        find.byKey(const Key('network_history_chart')),
        250,
        scrollable: find.byType(Scrollable),
      );
      expect(find.textContaining('VRAM:'), findsOneWidget);
      expect(find.textContaining(r'C:\'), findsOneWidget);
      expect(find.byKey(const Key('network_history_chart')), findsOneWidget);

      await tester.tap(find.byKey(const Key('heartbeat_button')));
      await tester.pump();
      expect(transport.sentEnvelopes.last.kind, MessageKind.heartbeat);
    },
  );
}
