import 'dart:io';

import 'package:flutter_test/flutter_test.dart';
import 'package:nexdeck_mobile/src/models/panel_models.dart';
import 'package:nexdeck_mobile/src/models/protocol_models.dart';

void main() {
  group('Panel & Deck models', () {
    test(
      'parses repository examples/panel_default.json identically to starter',
      () {
        final file = File('../../examples/panel_default.json');
        expect(file.existsSync(), isTrue);

        final parsed = DeckConfig.importJson(file.readAsStringSync());
        expect(parsed.exportJson(), DeckConfig.defaultStarter().exportJson());
      },
    );

    test('defaultStarter validates and round-trips JSON identically', () {
      final deck = DeckConfig.defaultStarter();
      deck.validate();

      final encoded = deck.exportJson();
      final decoded = DeckConfig.importJson(encoded);
      expect(decoded.schemaVersion, panelSchemaVersion);
      expect(decoded.activePanelId, 'system-overview');
      expect(decoded.panels, hasLength(1));

      final panel = decoded.panels.single;
      expect(panel.columns, 4);
      expect(panel.widgets, hasLength(5));
      expect(
        panel.widgets[3].action.controlAction,
        ControlAction.mediaPlayPause,
      );
      expect(panel.widgets[4].kind, WidgetKind.toggle);
    });

    test('rejects overlapping grid cells, bad colors, and traversal keys', () {
      final base = DeckConfig.defaultStarter().panels.single;

      final overlapping = PanelConfig(
        id: base.id,
        name: base.name,
        columns: base.columns,
        refreshIntervalMs: base.refreshIntervalMs,
        widgets: [
          ...base.widgets,
          const WidgetSpec(
            id: 'overlap-widget',
            title: 'Overlap',
            kind: WidgetKind.text,
            col: 1,
            row: 1,
            colSpan: 1,
            rowSpan: 1,
            text: 'Sample',
          ),
        ],
      );
      expect(overlapping.validate, throwsFormatException);

      final badColor = PanelConfig(
        id: 'p1',
        name: 'Custom Panel',
        columns: 2,
        refreshIntervalMs: 500,
        widgets: const [
          WidgetSpec(
            id: 'w1',
            title: 'Text',
            kind: WidgetKind.text,
            col: 0,
            row: 0,
            colSpan: 1,
            rowSpan: 1,
            style: WidgetStyle(accentHex: 'not-a-hex'),
          ),
        ],
      );
      expect(badColor.validate, throwsFormatException);

      final badAsset = PanelConfig(
        id: 'p2',
        name: 'Image Panel',
        columns: 2,
        refreshIntervalMs: 500,
        widgets: const [
          WidgetSpec(
            id: 'img1',
            title: 'Logo',
            kind: WidgetKind.image,
            col: 0,
            row: 0,
            colSpan: 1,
            rowSpan: 1,
            assetKey: '../forbidden',
          ),
        ],
      );
      expect(badAsset.validate, throwsFormatException);
    });

    test('imports single PanelConfig exported JSON cleanly', () {
      final panel = DeckConfig.defaultStarter().panels.single;
      final exported = panel.exportJson();
      final imported = PanelConfig.importJson(exported);
      expect(imported.id, panel.id);
      expect(imported.widgets.length, panel.widgets.length);
    });
  });
}
