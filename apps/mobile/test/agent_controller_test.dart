import 'dart:async';
import 'dart:convert';

import 'package:flutter_test/flutter_test.dart';
import 'package:nexdeck_mobile/src/models/protocol_models.dart';
import 'package:nexdeck_mobile/src/state/agent_controller.dart';
import 'package:nexdeck_mobile/src/storage/session_store.dart';
import 'package:nexdeck_mobile/src/transport/pinned_http_client.dart';
import 'package:nexdeck_mobile/src/transport/transport_client.dart';

class _StubPairingClient implements PairingClient {
  _StubPairingClient({required this.fingerprint, this.shouldFail = false});

  final String fingerprint;
  final bool shouldFail;

  @override
  Future<HealthResponse> checkHealth(
    Uri baseUri, {
    required String expectedFingerprintSha256,
  }) async {
    return HealthResponse(
      status: 'ok',
      protocolVersion: nexdeckProtocolVersion,
      fingerprintSha256: fingerprint,
    );
  }

  @override
  Future<PairedCredentials> pair({
    required Uri baseUri,
    required String expectedFingerprintSha256,
    required PairRequest request,
  }) async {
    if (shouldFail) {
      throw const PairingClientException(
        'Неверный PIN-код сопряжения.',
        statusCode: 401,
      );
    }
    return PairedCredentials(
      deviceId: request.deviceId,
      deviceToken: 'example-session-${DateTime.now().microsecondsSinceEpoch}',
      fingerprintSha256: fingerprint,
    );
  }
}

class _StubTransport implements NexDeckTransport {
  final StreamController<TransportConnectionState> _states =
      StreamController<TransportConnectionState>.broadcast();
  final StreamController<Envelope> _envelopes =
      StreamController<Envelope>.broadcast();

  TransportConnectionState _state = TransportConnectionState.disconnected;

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

  void simulateDrop() {
    _state = TransportConnectionState.disconnected;
    _states.add(_state);
  }

  final List<Envelope> sentEnvelopes = [];

  void emitTelemetry(
    double cpuPct,
    int seq, {
    int rxBytesPerSec = 4096,
    int txBytesPerSec = 1024,
  }) {
    _envelopes.add(
      Envelope.json(
        messageId: seq,
        kind: MessageKind.telemetry,
        body: {
          'captured_at_epoch_ms': 1700000000000 + seq * 1000,
          'uptime_secs': 60 + seq,
          'cpu': {
            'total_usage_pct': cpuPct,
            'per_core_usage_pct': [cpuPct],
            'frequency_mhz': 3500,
            'temperature_celsius': null,
          },
          'memory': {'total_bytes': 8192, 'used_bytes': 4096},
          'gpus': const <Object?>[],
          'disks': const <Object?>[],
          'networks': [
            {
              'interface_name': 'ExampleEthernet',
              'tx_bytes_per_sec': txBytesPerSec,
              'rx_bytes_per_sec': rxBytesPerSec,
              'total_tx_bytes': 10240,
              'total_rx_bytes': 40960,
            },
          ],
          'battery': null,
        },
      ),
    );
  }

  void emitControlAck(String commandId, ControlAction action) {
    _envelopes.add(
      Envelope.json(
        messageId: 99,
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
  test('caps CPU history samples and handles unpair cleanly', () async {
    final fp = computeCertificateSha256(utf8.encode('example-host-cert'));
    final store = InMemorySessionStore();
    final transport = _StubTransport();
    final controller = AgentController(
      sessionStore: store,
      pairingClient: _StubPairingClient(fingerprint: fp),
      transport: transport,
      maxHistorySamples: 3,
    );
    addTearDown(controller.dispose);

    final paired = await controller.pairAndConnect(
      host: '127.0.0.1',
      port: 9443,
      fingerprintSha256: fp,
      pin: '123456',
      deviceName: 'ExamplePhone',
      deviceId: 'example-device-1',
    );
    expect(paired, isTrue);
    expect(controller.connectionState, TransportConnectionState.connected);

    for (var i = 1; i <= 5; i++) {
      transport.emitTelemetry(i * 10.0, i);
    }
    await Future<void>.delayed(Duration.zero);

    expect(controller.cpuHistory, [30.0, 40.0, 50.0]);
    expect(controller.ramHistory, [50.0, 50.0, 50.0]);
    expect(controller.networkRxKibHistory, [4.0, 4.0, 4.0]);
    expect(controller.networkTxKibHistory, [1.0, 1.0, 1.0]);
    expect(controller.latestSnapshot?.uptimeSecs, 65);

    await controller.sendControlAction(ControlAction.mediaPlayPause);
    expect(transport.sentEnvelopes, hasLength(1));
    final sentPayload = transport.sentEnvelopes.single.decodePayloadJson();
    expect(sentPayload['action'], 'MediaPlayPause');

    transport.emitControlAck(
      sentPayload['command_id']! as String,
      ControlAction.mediaPlayPause,
    );
    await Future<void>.delayed(Duration.zero);
    expect(controller.lastControlResult?.ok, isTrue);
    expect(controller.lastControlResult?.action, ControlAction.mediaPlayPause);

    await controller.unpairHost();
    expect(controller.session, isNull);
    expect(await store.loadSession(), isNull);
    expect(controller.cpuHistory, isEmpty);
    expect(controller.ramHistory, isEmpty);
    expect(controller.networkRxKibHistory, isEmpty);
    expect(controller.networkTxKibHistory, isEmpty);
    expect(controller.lastControlResult, isNull);
  });

  test('schedules reconnect after unexpected socket drop', () async {
    final fp = computeCertificateSha256(utf8.encode('example-host-cert'));
    final transport = _StubTransport();
    final controller = AgentController(
      sessionStore: InMemorySessionStore(),
      pairingClient: _StubPairingClient(fingerprint: fp),
      transport: transport,
    );
    addTearDown(controller.dispose);

    await controller.pairAndConnect(
      host: '127.0.0.1',
      port: 9443,
      fingerprintSha256: fp,
      pin: '654321',
      deviceName: 'ExamplePhone',
    );
    expect(controller.connectionState, TransportConnectionState.connected);

    transport.simulateDrop();
    await Future<void>.delayed(Duration.zero);
    expect(controller.connectionState, TransportConnectionState.reconnecting);
  });

  test(
    'validates inputs and surfaces pairing failure without saving session',
    () async {
      final fp = computeCertificateSha256(utf8.encode('example-host-cert'));
      final store = InMemorySessionStore();
      final controller = AgentController(
        sessionStore: store,
        pairingClient: _StubPairingClient(fingerprint: fp, shouldFail: true),
        transport: _StubTransport(),
      );
      addTearDown(controller.dispose);

      final invalidPin = await controller.pairAndConnect(
        host: '127.0.0.1',
        port: 9443,
        fingerprintSha256: fp,
        pin: 'abc',
        deviceName: 'ExamplePhone',
      );
      expect(invalidPin, isFalse);
      expect(controller.errorMessage, isNotNull);

      final rejected = await controller.pairAndConnect(
        host: '127.0.0.1',
        port: 9443,
        fingerprintSha256: fp,
        pin: '111222',
        deviceName: 'ExamplePhone',
      );
      expect(rejected, isFalse);
      expect(controller.session, isNull);
      expect(await store.loadSession(), isNull);
    },
  );
}
