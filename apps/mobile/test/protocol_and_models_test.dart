import 'dart:convert';
import 'dart:io';

import 'package:flutter_test/flutter_test.dart';
import 'package:nexdeck_mobile/src/models/protocol_models.dart';
import 'package:nexdeck_mobile/src/storage/session_store.dart';

String _syntheticHex(int byteValue) =>
    byteValue.toRadixString(16).padLeft(2, '0') * 32;

void main() {
  group('PairingQrPayload', () {
    test('parses valid QR JSON payload exported by nexdeck-agent', () {
      final fp = _syntheticHex(0xab);
      final raw = jsonEncode({
        'protocol_version': 1,
        'host': '127.0.0.1',
        'port': 9443,
        'fingerprint_sha256': fp,
        'pin': '482910',
      });

      final parsed = PairingQrPayload.fromJsonString(raw);
      expect(parsed.protocolVersion, 1);
      expect(parsed.host, '127.0.0.1');
      expect(parsed.port, 9443);
      expect(parsed.fingerprintSha256, fp);
      expect(parsed.pin, '482910');
    });

    test('rejects invalid protocol, port, fingerprint, or PIN', () {
      final fp = _syntheticHex(0x11);
      expect(
        () => PairingQrPayload.fromJson({
          'protocol_version': 2,
          'host': '127.0.0.1',
          'port': 9443,
          'fingerprint_sha256': fp,
          'pin': '123456',
        }),
        throwsFormatException,
      );
      expect(
        () => PairingQrPayload.fromJson({
          'protocol_version': 1,
          'host': '127.0.0.1',
          'port': 99999,
          'fingerprint_sha256': fp,
          'pin': '123456',
        }),
        throwsFormatException,
      );
      expect(
        () => PairingQrPayload.fromJson({
          'protocol_version': 1,
          'host': '127.0.0.1',
          'port': 9443,
          'fingerprint_sha256': 'not-hex',
          'pin': '123456',
        }),
        throwsFormatException,
      );
      expect(
        () => PairingQrPayload.fromJson({
          'protocol_version': 1,
          'host': '127.0.0.1',
          'port': 9443,
          'fingerprint_sha256': fp,
          'pin': '12ab',
        }),
        throwsFormatException,
      );
    });
  });

  group('Envelope & SystemSnapshot', () {
    test('decodes Telemetry Envelope generated with serde_json layout', () {
      final snapshotMap = {
        'captured_at_epoch_ms': 1700000000000,
        'uptime_secs': 3665,
        'cpu': {
          'total_usage_pct': 31.5,
          'per_core_usage_pct': [28.0, 35.0],
          'frequency_mhz': 4100,
          'temperature_celsius': 58.2,
        },
        'memory': {
          'total_bytes': 16 * 1024 * 1024 * 1024,
          'used_bytes': 6 * 1024 * 1024 * 1024,
        },
        'gpus': [
          {
            'name': 'ExampleGPU',
            'usage_pct': 12.0,
            'temperature_celsius': 45.0,
            'clock_mhz': 1500,
            'vram_used_bytes': 1024 * 1024 * 1024,
            'vram_total_bytes': 8 * 1024 * 1024 * 1024,
          },
        ],
        'disks': [
          {
            'mount_point': r'C:\',
            'total_bytes': 512 * 1024 * 1024 * 1024,
            'free_bytes': 200 * 1024 * 1024 * 1024,
            'read_bytes_per_sec': 1048576,
            'write_bytes_per_sec': 524288,
          },
        ],
        'networks': [
          {
            'interface_name': 'ExampleWiFi',
            'tx_bytes_per_sec': 2048,
            'rx_bytes_per_sec': 8192,
            'total_tx_bytes': 100000,
            'total_rx_bytes': 200000,
          },
        ],
        'battery': {'charge_pct': 87.0, 'ac_connected': true},
      };

      final envelopeJson = {
        'protocol_version': 1,
        'message_id': 7,
        'kind': 'Telemetry',
        'payload': utf8.encode(jsonEncode(snapshotMap)),
      };

      final envelope = Envelope.fromJson(envelopeJson);
      final snapshot = envelope.decodeTelemetrySnapshot();
      expect(snapshot.uptimeSecs, 3665);
      expect(snapshot.cpu.totalUsagePct, closeTo(31.5, 0.01));
      expect(snapshot.cpu.perCoreUsagePct, hasLength(2));
      expect(snapshot.memory.usagePct, closeTo(37.5, 0.01));
      expect(snapshot.gpus.single.name, 'ExampleGPU');
      expect(snapshot.disks.single.mountPoint, r'C:\');
      expect(snapshot.networks.single.interfaceName, 'ExampleWiFi');
      expect(snapshot.battery?.acConnected, isTrue);
    });

    test('rejects non-heartbeat Envelope with empty payload', () {
      expect(
        () => const Envelope(
          messageId: 1,
          kind: MessageKind.telemetry,
          payload: [],
        ).validate(),
        throwsFormatException,
      );
    });

    test(
      'round-trips ControlAction requests and decodes ControlCommandResponse',
      () {
        for (final action in ControlAction.values) {
          final req = ControlCommandRequest(commandId: 'cmd-1', action: action);
          expect(req.toJson(), {
            'command_id': 'cmd-1',
            'action': action.wireName,
          });
          expect(ControlAction.fromWireName(action.wireName), action);
        }

        final responseEnvelope = Envelope.json(
          messageId: 15,
          kind: MessageKind.controlCommand,
          body: {
            'command_id': 'cmd-15',
            'action': 'VolumeUp',
            'ok': true,
            'message': 'Executed VolumeUp',
          },
        );
        final response = responseEnvelope.decodeControlResponse();
        expect(response.commandId, 'cmd-15');
        expect(response.action, ControlAction.volumeUp);
        expect(response.ok, isTrue);
        expect(response.message, 'Executed VolumeUp');
      },
    );
  });

  group('FileSessionStore', () {
    test('saves, reloads, and clears HostSession atomically', () async {
      final tempDir = await Directory.systemTemp.createTemp('nexdeck_mobile_');
      try {
        final store = FileSessionStore(File('${tempDir.path}/session.json'));
        expect(await store.loadSession(), isNull);

        final session = HostSession(
          host: '127.0.0.1',
          port: 9443,
          fingerprintSha256: _syntheticHex(0xcd),
          deviceId: 'example-device-1',
          deviceName: 'ExamplePhone',
          deviceToken:
              'example-bearer-value-${DateTime.now().microsecondsSinceEpoch}',
        );

        await store.saveSession(session);
        final reloaded = await store.loadSession();
        expect(reloaded, isNotNull);
        expect(reloaded!.host, '127.0.0.1');
        expect(reloaded.port, 9443);
        expect(reloaded.deviceId, 'example-device-1');
        expect(reloaded.fingerprintSha256, session.fingerprintSha256);

        await store.clearSession();
        expect(await store.loadSession(), isNull);
      } finally {
        await tempDir.delete(recursive: true);
      }
    });
  });
}
