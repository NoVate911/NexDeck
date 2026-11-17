import 'dart:async';
import 'dart:convert';
import 'dart:io';

import 'package:flutter_test/flutter_test.dart';
import 'package:material_ui/material_ui.dart';
import 'package:nexdeck_mobile/main.dart';
import 'package:nexdeck_mobile/src/models/monitoring_models.dart';
import 'package:nexdeck_mobile/src/models/panel_models.dart';
import 'package:nexdeck_mobile/src/models/protocol_models.dart';
import 'package:nexdeck_mobile/src/state/agent_controller.dart';
import 'package:nexdeck_mobile/src/state/deck_controller.dart';
import 'package:nexdeck_mobile/src/storage/deck_store.dart';
import 'package:nexdeck_mobile/src/storage/session_store.dart';
import 'package:nexdeck_mobile/src/transport/pinned_http_client.dart';
import 'package:nexdeck_mobile/src/transport/transport_client.dart';
import 'package:nexdeck_mobile/src/ui/widget_card.dart';

class _NoopPairingClient implements PairingClient {
  @override
  Future<HealthResponse> checkHealth(
    Uri baseUri, {
    required String expectedFingerprintSha256,
  }) async {
    throw UnimplementedError();
  }

  @override
  Future<PairedCredentials> pair({
    required Uri baseUri,
    required String expectedFingerprintSha256,
    required PairRequest request,
  }) async {
    throw UnimplementedError();
  }
}

class _FakeWidgetTransport implements NexDeckTransport {
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

  void pushTelemetry({required double cpuPct}) {
    _envelopes.add(
      Envelope.json(
        messageId: 100,
        kind: MessageKind.telemetry,
        body: {
          'captured_at_epoch_ms': 1700000001000,
          'uptime_secs': 3665,
          'cpu': {
            'total_usage_pct': cpuPct,
            'per_core_usage_pct': [cpuPct, cpuPct],
            'frequency_mhz': 4200,
            'temperature_celsius': 58.5,
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
          'disks': const [],
          'networks': [
            {
              'interface_name': 'ExampleWiFi',
              'tx_bytes_per_sec': 1024,
              'rx_bytes_per_sec': 4096,
              'total_tx_bytes': 1024,
              'total_rx_bytes': 4096,
            },
          ],
          'battery': {'charge_pct': 82.0, 'ac_connected': true},
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

SystemSnapshot _sampleSnapshot({required double cpuPct}) {
  return SystemSnapshot(
    capturedAtEpochMs: 1700000001000,
    uptimeSecs: 120,
    cpu: CpuMetrics(
      totalUsagePct: cpuPct,
      perCoreUsagePct: [cpuPct, cpuPct],
      frequencyMhz: 4000,
      temperatureCelsius: 55.0,
    ),
    memory: const MemoryMetrics(
      totalBytes: 16 * 1024 * 1024 * 1024,
      usedBytes: 8 * 1024 * 1024 * 1024,
    ),
    gpus: const [
      GpuMetrics(
        name: 'ExampleGPU',
        usagePct: 24.0,
        vramUsedBytes: 2 * 1024 * 1024 * 1024,
        vramTotalBytes: 8 * 1024 * 1024 * 1024,
      ),
    ],
    disks: const [],
    networks: const [],
    battery: const BatteryMetrics(chargePct: 90.0, acConnected: true),
  );
}

void main() {
  test(
    'FileDeckStore saves, reloads, and clears DeckConfig atomically',
    () async {
      final tempDir = await Directory.systemTemp.createTemp(
        'nexdeck_deck_test_',
      );
      addTearDown(() async {
        if (await tempDir.exists()) {
          await tempDir.delete(recursive: true);
        }
      });

      final file = File('${tempDir.path}${Platform.pathSeparator}panels.json');
      final store = FileDeckStore(file);
      expect(await store.loadDeck(), isNull);

      final starter = DeckConfig.defaultStarter();
      await store.saveDeck(starter);
      expect(await file.exists(), isTrue);

      final loaded = await store.loadDeck();
      expect(loaded, isNotNull);
      expect(loaded!.exportJson(), starter.exportJson());

      await store.clearDeck();
      expect(await store.loadDeck(), isNull);
    },
  );

  test('DeckController handles panel/widget CRUD, collision guards, throttling, and JSON', () async {
    final store = InMemoryDeckStore();
    final controller = DeckController(deckStore: store);
    addTearDown(controller.dispose);
    await controller.initialize();

    // Creating panel selects it and honors bounds/cadence.
    expect(
      await controller.addPanel(
        name: 'Медиа профиль',
        columns: 3,
        refreshIntervalMs: 2000,
      ),
      isTrue,
    );
    expect(controller.activePanel.columns, 3);
    expect(controller.activePanel.refreshIntervalMs, 2000);

    // Refresh cadence throttling ignores intermediate packets before 2000 ms.
    final t0 = DateTime.fromMillisecondsSinceEpoch(1_700_000_000_000);
    controller.syncTelemetry(_sampleSnapshot(cpuPct: 30.0), now: t0);
    expect(controller.displayedSnapshot?.cpu.totalUsagePct, 30.0);

    controller.syncTelemetry(
      _sampleSnapshot(cpuPct: 80.0),
      now: t0.add(const Duration(milliseconds: 500)),
    );
    expect(controller.displayedSnapshot?.cpu.totalUsagePct, 30.0);

    controller.syncTelemetry(
      _sampleSnapshot(cpuPct: 80.0),
      now: t0.add(const Duration(milliseconds: 2100)),
    );
    expect(controller.displayedSnapshot?.cpu.totalUsagePct, 80.0);

    // Add 2 widgets without overlaps and enforce collision guard.
    expect(await controller.addWidget(WidgetKind.button), isTrue);
    expect(await controller.addWidget(WidgetKind.toggle), isTrue);
    final secondWidget = controller.selectedWidget!;
    expect((secondWidget.col, secondWidget.row), (1, 0));

    // Moving left onto (0, 0) overlaps first button -> rejected cleanly.
    expect(await controller.moveSelectedWidget(-1, 0), isFalse);
    expect(controller.errorMessage, isNotNull);
    expect(controller.selectedWidget!.col, 1);

    // Moving down to (1, 1) succeeds cleanly.
    expect(await controller.moveSelectedWidget(0, 1), isTrue);
    expect(controller.selectedWidget!.row, 1);

    // Export panel, reset to starter, then import panel back without overwriting IDs.
    final panelJson = controller.exportActivePanelJson();
    expect(await controller.resetToStarter(), isTrue);
    expect(controller.deck.panels, hasLength(1));

    expect(await controller.importPanelJson(panelJson), isTrue);
    expect(controller.deck.panels, hasLength(2));
    expect(await controller.deleteActivePanel(), isTrue);
    expect(controller.deck.panels, hasLength(1));

    // Cannot delete last remaining panel.
    expect(await controller.deleteActivePanel(), isFalse);
  });

  testWidgets(
    'DeckPanelView renders all widget types, dispatches actions, and supports editing',
    (WidgetTester tester) async {
      final fingerprint = computeCertificateSha256(utf8.encode('example-cert'));
      final sessionStore = InMemorySessionStore(
        HostSession(
          host: '127.0.0.1',
          port: 9443,
          fingerprintSha256: fingerprint,
          deviceId: 'mobile-test',
          deviceName: 'ExamplePhone',
          deviceToken: 'session-token',
        ),
      );
      final deckStore = InMemoryDeckStore(
        DeckConfig(
          activePanelId: 'all-kinds',
          panels: [
            DeckConfig.defaultStarter().panels.single,
            const PanelConfig(
              id: 'all-kinds',
              name: 'Все виджеты',
              columns: 4,
              refreshIntervalMs: 100,
              widgets: [
                WidgetSpec(
                  id: 'w-text',
                  title: 'ExampleNote',
                  kind: WidgetKind.text,
                  col: 0,
                  row: 0,
                  colSpan: 2,
                  rowSpan: 1,
                  text: 'Синтетическая заметка',
                ),
                WidgetSpec(
                  id: 'w-metric',
                  title: 'CPU %',
                  kind: WidgetKind.metricNumber,
                  col: 2,
                  row: 0,
                  colSpan: 2,
                  rowSpan: 1,
                  telemetry: TelemetryBinding.cpuUsagePct,
                  icon: WidgetIcon.cpu,
                ),
                WidgetSpec(
                  id: 'w-chart',
                  title: 'RAM График',
                  kind: WidgetKind.chart,
                  col: 0,
                  row: 1,
                  colSpan: 2,
                  rowSpan: 2,
                  telemetry: TelemetryBinding.ramUsagePct,
                ),
                WidgetSpec(
                  id: 'w-gauge',
                  title: 'GPU %',
                  kind: WidgetKind.gauge,
                  col: 2,
                  row: 1,
                  colSpan: 2,
                  rowSpan: 1,
                  telemetry: TelemetryBinding.gpuUsagePct,
                ),
                WidgetSpec(
                  id: 'w-button',
                  title: 'Пауза',
                  kind: WidgetKind.button,
                  col: 2,
                  row: 2,
                  colSpan: 1,
                  rowSpan: 1,
                  action: WidgetAction.control(ControlAction.mediaPlayPause),
                  icon: WidgetIcon.playPause,
                ),
                WidgetSpec(
                  id: 'w-toggle',
                  title: 'Звук',
                  kind: WidgetKind.toggle,
                  col: 3,
                  row: 2,
                  colSpan: 1,
                  rowSpan: 1,
                  action: WidgetAction.control(ControlAction.volumeMuteToggle),
                  icon: WidgetIcon.volumeMute,
                ),
                WidgetSpec(
                  id: 'w-icon',
                  title: 'Щит',
                  kind: WidgetKind.icon,
                  col: 0,
                  row: 3,
                  colSpan: 1,
                  rowSpan: 1,
                  icon: WidgetIcon.shield,
                  action: WidgetAction.heartbeat(),
                ),
                WidgetSpec(
                  id: 'w-image',
                  title: 'Баннер',
                  kind: WidgetKind.image,
                  col: 1,
                  row: 3,
                  colSpan: 1,
                  rowSpan: 1,
                  assetKey: 'example-banner',
                ),
                WidgetSpec(
                  id: 'w-custom',
                  title: 'Плагин',
                  kind: WidgetKind.custom,
                  col: 2,
                  row: 3,
                  colSpan: 2,
                  rowSpan: 1,
                  customComponent: 'org.example.panel',
                ),
              ],
            ),
          ],
        ),
      );

      final transport = _FakeWidgetTransport();
      final agentController = AgentController(
        sessionStore: sessionStore,
        pairingClient: _NoopPairingClient(),
        transport: transport,
      );
      final deckController = DeckController(deckStore: deckStore);
      addTearDown(agentController.dispose);
      addTearDown(deckController.dispose);

      await agentController.initialize();
      await deckController.initialize();

      await tester.pumpWidget(
        NexDeckApp(
          controller: agentController,
          deckController: deckController,
          autoInitialize: false,
        ),
      );
      await tester.pumpAndSettle();

      transport.pushTelemetry(cpuPct: 44.5);
      await tester.pumpAndSettle();

      // Switch to dynamic Panels tab.
      await tester.tap(find.byKey(const Key('nav_panels_tab')));
      await tester.pumpAndSettle();

      expect(find.text('NexDeck — Панели виджетов'), findsOneWidget);
      expect(find.text('44.5%'), findsOneWidget);
      expect(find.text('18.0%'), findsOneWidget);
      expect(find.textContaining('org.example.panel'), findsOneWidget);

      // Scroll down naturally so row 2/3 widgets sit clearly above NavigationBar.
      await tester.drag(find.byType(ListView), const Offset(0, -240));
      await tester.pumpAndSettle();

      // Tapping dynamic button dispatches ControlAction.mediaPlayPause.
      await tester.tap(find.byKey(const Key('panel_widget_w-button')));
      await tester.pump();
      expect(transport.sentEnvelopes.last.kind, MessageKind.controlCommand);
      expect(
        transport.sentEnvelopes.last.decodePayloadJson()['action'],
        'MediaPlayPause',
      );

      // Tapping toggle widget flips local pill label and sends VolumeMuteToggle.
      expect(find.text('ВЫКЛ'), findsOneWidget);
      await tester.tap(find.byKey(const Key('panel_widget_w-toggle')));
      await tester.pump();
      expect(find.text('ВКЛ'), findsOneWidget);
      expect(
        transport.sentEnvelopes.last.decodePayloadJson()['action'],
        'VolumeMuteToggle',
      );

      // Scroll back to top, enter edit mode, select w-button, and customize title.
      await tester.drag(find.byType(ListView), const Offset(0, 240));
      await tester.pumpAndSettle();

      await tester.tap(find.byKey(const Key('toggle_edit_mode_button')));
      await tester.pumpAndSettle();

      await tester.drag(find.byType(ListView), const Offset(0, -320));
      await tester.pumpAndSettle();

      await tester.tap(find.byKey(const Key('panel_widget_w-button')));
      await tester.pumpAndSettle();

      await tester.drag(find.byType(ListView), const Offset(0, 260));
      await tester.pumpAndSettle();

      await tester.ensureVisible(
        find.byKey(const Key('customize_selected_widget_button')),
      );
      await tester.tap(
        find.byKey(const Key('customize_selected_widget_button')),
      );
      await tester.pumpAndSettle();

      await tester.enterText(
        find.byKey(const Key('widget_editor_title_field')),
        'Старт/Пауза',
      );
      await tester.ensureVisible(
        find.byKey(const Key('widget_editor_save_button')),
      );
      await tester.tap(find.byKey(const Key('widget_editor_save_button')));
      await tester.pumpAndSettle();

      expect(find.text('Старт/Пауза'), findsWidgets);
    },
  );

  test('parsePanelHexColor handles #RRGGBB, #AARRGGBB, and invalid inputs', () {
    expect(parsePanelHexColor('#38BDF8'), const Color(0xFF38BDF8));
    expect(parsePanelHexColor('#801E293B'), const Color(0x801E293B));
    expect(parsePanelHexColor('bad'), const Color(0xFF1E293B));
  });
}
