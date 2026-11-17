import 'dart:convert';

import 'json_helpers.dart';
import 'protocol_models.dart';

/// Current JSON schema version aligned with `crates/api/src/panels.rs`.
const int panelSchemaVersion = 1;

/// Minimum allowed grid column count per panel.
const int minPanelColumns = 2;

/// Maximum allowed grid column count per panel.
const int maxPanelColumns = 8;

/// Minimum refresh interval in milliseconds.
const int minRefreshIntervalMs = 100;

/// Maximum refresh interval in milliseconds (`60s`).
const int maxRefreshIntervalMs = 60000;

final RegExp _safeIdPattern = RegExp(r'^[A-Za-z0-9_.-]{1,64}$');
final RegExp _hexColorPattern = RegExp(r'^#(?:[0-9A-Fa-f]{6}|[0-9A-Fa-f]{8})$');

/// Supported visual element types matching `WidgetKind` in Rust.
enum WidgetKind {
  /// Static or labeled text block.
  text('Text', 'Текст'),

  /// Numeric readout bound to a [TelemetryBinding].
  metricNumber('MetricNumber', 'Число'),

  /// Push button invoking a [WidgetAction].
  button('Button', 'Кнопка'),

  /// Toggle switch tracking state or mute action.
  toggle('Toggle', 'Переключатель'),

  /// Real-time chart bound to telemetry history.
  chart('Chart', 'График'),

  /// Percentage gauge bar.
  gauge('Gauge', 'Индикатор'),

  /// Built-in semantic vector icon.
  icon('Icon', 'Иконка'),

  /// Built-in asset reference identified by safe `asset_key`.
  image('Image', 'Изображение'),

  /// Plugin-defined custom component (Stage 6).
  custom('Custom', 'Custom UI');

  const WidgetKind(this.wireName, this.displayLabel);

  /// Exact enum variant name used by Rust `serde`.
  final String wireName;

  /// Concise Russian UI caption.
  final String displayLabel;

  /// Parses [wireName] into [WidgetKind].
  static WidgetKind fromWireName(String wireName) {
    for (final kind in values) {
      if (kind.wireName == wireName) {
        return kind;
      }
    }
    throw FormatException('Unsupported WidgetKind "$wireName".');
  }
}

/// Bindable hardware metrics matching `TelemetryBinding` in Rust.
enum TelemetryBinding {
  /// Total CPU utilization percentage.
  cpuUsagePct('CpuUsagePct', 'CPU %'),

  /// CPU frequency in MHz.
  cpuFrequencyMhz('CpuFrequencyMhz', 'Частота CPU'),

  /// CPU package temperature in Celsius.
  cpuTemperatureC('CpuTemperatureC', 'Температура CPU'),

  /// Physical RAM utilization percentage.
  ramUsagePct('RamUsagePct', 'RAM %'),

  /// Used physical RAM bytes.
  ramUsedBytes('RamUsedBytes', 'Занято RAM'),

  /// Primary GPU utilization percentage.
  gpuUsagePct('GpuUsagePct', 'GPU %'),

  /// Primary GPU VRAM utilization percentage.
  gpuVramPct('GpuVramPct', 'VRAM %'),

  /// Network download throughput in bytes/sec.
  networkRxBps('NetworkRxBps', 'Сеть ↓'),

  /// Network upload throughput in bytes/sec.
  networkTxBps('NetworkTxBps', 'Сеть ↑'),

  /// Battery percentage.
  batteryPct('BatteryPct', 'Батарея %'),

  /// System uptime in seconds.
  uptimeSecs('UptimeSecs', 'Uptime');

  const TelemetryBinding(this.wireName, this.displayLabel);

  /// Exact enum variant name used by Rust `serde`.
  final String wireName;

  /// Human-readable Russian caption.
  final String displayLabel;

  /// Parses [wireName] into [TelemetryBinding].
  static TelemetryBinding fromWireName(String wireName) {
    for (final item in values) {
      if (item.wireName == wireName) {
        return item;
      }
    }
    throw FormatException('Unsupported TelemetryBinding "$wireName".');
  }
}

/// Discriminator for [WidgetAction].
enum WidgetActionType {
  /// Non-interactive display widget.
  none('None'),

  /// Allowlisted host control command.
  control('Control'),

  /// WebSocket keepalive heartbeat.
  heartbeat('Heartbeat');

  const WidgetActionType(this.wireName);

  /// Exact enum variant tag used by Rust `serde`.
  final String wireName;

  /// Parses [wireName] into [WidgetActionType].
  static WidgetActionType fromWireName(String wireName) {
    for (final item in values) {
      if (item.wireName == wireName) {
        return item;
      }
    }
    throw FormatException('Unsupported WidgetAction type "$wireName".');
  }
}

/// Interactive action matching tagged `WidgetAction` in `crates/api`.
class WidgetAction {
  /// Creates a [WidgetAction].
  const WidgetAction({required this.type, this.controlAction});

  /// Non-interactive default action.
  const WidgetAction.none() : this(type: WidgetActionType.none);

  /// Host control command action.
  const WidgetAction.control(ControlAction action)
    : this(type: WidgetActionType.control, controlAction: action);

  /// WebSocket keepalive action.
  const WidgetAction.heartbeat() : this(type: WidgetActionType.heartbeat);

  /// Decodes a tagged `WidgetAction` JSON object.
  factory WidgetAction.fromJson(Map<String, Object?> json) {
    final type = WidgetActionType.fromWireName(
      readRequired<String>(json, 'type'),
    );
    if (type == WidgetActionType.control) {
      final rawAction = readRequired<String>(json, 'action');
      return WidgetAction.control(ControlAction.fromWireName(rawAction));
    }
    return WidgetAction(type: type);
  }

  /// Action tag type.
  final WidgetActionType type;

  /// Bound control action when [type] is [WidgetActionType.control].
  final ControlAction? controlAction;

  /// Serializes into Rust-compatible tagged JSON.
  Map<String, Object?> toJson() {
    if (type == WidgetActionType.control && controlAction != null) {
      return {'type': type.wireName, 'action': controlAction!.wireName};
    }
    return {'type': type.wireName};
  }
}

/// Curated built-in vector icons matching `WidgetIcon` in Rust.
enum WidgetIcon {
  /// Processor icon.
  cpu('Cpu'),

  /// Memory module icon.
  memory('Memory'),

  /// Graphics card icon.
  gpu('Gpu'),

  /// Disk icon.
  disk('Disk'),

  /// Network icon.
  network('Network'),

  /// Battery icon.
  battery('Battery'),

  /// Play/pause icon.
  playPause('PlayPause'),

  /// Previous track icon.
  previousTrack('PreviousTrack'),

  /// Next track icon.
  nextTrack('NextTrack'),

  /// Volume up icon.
  volumeUp('VolumeUp'),

  /// Volume down icon.
  volumeDown('VolumeDown'),

  /// Mute icon.
  volumeMute('VolumeMute'),

  /// Terminal icon.
  terminal('Terminal'),

  /// Shield icon.
  shield('Shield');

  const WidgetIcon(this.wireName);

  /// Exact enum variant name used by Rust `serde`.
  final String wireName;

  /// Parses [wireName] into [WidgetIcon].
  static WidgetIcon fromWireName(String wireName) {
    for (final icon in values) {
      if (icon.wireName == wireName) {
        return icon;
      }
    }
    throw FormatException('Unsupported WidgetIcon "$wireName".');
  }
}

/// Visual styling applied to a [WidgetSpec].
class WidgetStyle {
  /// Creates a [WidgetStyle].
  const WidgetStyle({
    this.backgroundHex = '#1E293B',
    this.foregroundHex = '#F8FAFC',
    this.accentHex = '#38BDF8',
    this.fontSizeSp = 14,
    this.cornerRadius = 12,
  });

  /// Decodes a [WidgetStyle] JSON map.
  factory WidgetStyle.fromJson(Map<String, Object?> json) {
    return WidgetStyle(
      backgroundHex: readOptional<String>(json, 'background_hex') ?? '#1E293B',
      foregroundHex: readOptional<String>(json, 'foreground_hex') ?? '#F8FAFC',
      accentHex: readOptional<String>(json, 'accent_hex') ?? '#38BDF8',
      fontSizeSp: readOptional<int>(json, 'font_size_sp') ?? 14,
      cornerRadius: readOptional<int>(json, 'corner_radius') ?? 12,
    );
  }

  /// Background color hex string (`#RRGGBB` or `#AARRGGBB`).
  final String backgroundHex;

  /// Foreground color hex string (`#RRGGBB` or `#AARRGGBB`).
  final String foregroundHex;

  /// Accent color hex string (`#RRGGBB` or `#AARRGGBB`).
  final String accentHex;

  /// Base font size (`10..=48`).
  final int fontSizeSp;

  /// Card border radius (`0..=32`).
  final int cornerRadius;

  /// Validates hex color formatting and range bounds.
  void validate() {
    _validateHexColor('background_hex', backgroundHex);
    _validateHexColor('foreground_hex', foregroundHex);
    _validateHexColor('accent_hex', accentHex);
    if (fontSizeSp < 10 || fontSizeSp > 48) {
      throw const FormatException('font_size_sp must be between 10 and 48.');
    }
    if (cornerRadius < 0 || cornerRadius > 32) {
      throw const FormatException('corner_radius must be between 0 and 32.');
    }
  }

  /// Serializes styling properties into JSON matching Rust `WidgetStyle`.
  Map<String, Object?> toJson() => {
    'background_hex': backgroundHex,
    'foreground_hex': foregroundHex,
    'accent_hex': accentHex,
    'font_size_sp': fontSizeSp,
    'corner_radius': cornerRadius,
  };
}

/// Positioned and styled widget on a [PanelConfig] grid.
class WidgetSpec {
  /// Creates a [WidgetSpec].
  const WidgetSpec({
    required this.id,
    required this.title,
    required this.kind,
    required this.col,
    required this.row,
    required this.colSpan,
    required this.rowSpan,
    this.telemetry,
    this.action = const WidgetAction.none(),
    this.text,
    this.icon,
    this.assetKey,
    this.customComponent,
    this.style = const WidgetStyle(),
  });

  /// Decodes a [WidgetSpec] JSON map.
  factory WidgetSpec.fromJson(Map<String, Object?> json) {
    final rawTelemetry = readOptional<String>(json, 'telemetry');
    final rawAction = json['action'];
    final rawIcon = readOptional<String>(json, 'icon');
    final rawStyle = json['style'];

    return WidgetSpec(
      id: readRequired<String>(json, 'id'),
      title: readRequired<String>(json, 'title'),
      kind: WidgetKind.fromWireName(readRequired<String>(json, 'kind')),
      col: readRequired<int>(json, 'col'),
      row: readRequired<int>(json, 'row'),
      colSpan: readRequired<int>(json, 'col_span'),
      rowSpan: readRequired<int>(json, 'row_span'),
      telemetry: rawTelemetry == null
          ? null
          : TelemetryBinding.fromWireName(rawTelemetry),
      action: rawAction == null
          ? const WidgetAction.none()
          : WidgetAction.fromJson(asJsonMap(rawAction, context: 'action')),
      text: readOptional<String>(json, 'text'),
      icon: rawIcon == null ? null : WidgetIcon.fromWireName(rawIcon),
      assetKey: readOptional<String>(json, 'asset_key'),
      customComponent: readOptional<String>(json, 'custom_component'),
      style: rawStyle == null
          ? const WidgetStyle()
          : WidgetStyle.fromJson(asJsonMap(rawStyle, context: 'style')),
    );
  }

  /// Unique widget ID within the panel.
  final String id;

  /// Display title shown on the widget.
  final String title;

  /// Rendering element kind.
  final WidgetKind kind;

  /// Zero-based column index.
  final int col;

  /// Zero-based row index.
  final int row;

  /// Horizontal grid span (`>= 1`).
  final int colSpan;

  /// Vertical grid span (`>= 1`).
  final int rowSpan;

  /// Optional bound metric.
  final TelemetryBinding? telemetry;

  /// Interactive action triggered by buttons/toggles.
  final WidgetAction action;

  /// Static text body for text widgets.
  final String? text;

  /// Optional built-in icon.
  final WidgetIcon? icon;

  /// Optional safe asset identifier.
  final String? assetKey;

  /// Optional plugin UI component identifier.
  final String? customComponent;

  /// Per-widget appearance settings.
  final WidgetStyle style;

  /// Validates widget fields and bounds within [panelColumns].
  void validate(int panelColumns) {
    _validateSafeId('widget.id', id);
    if (title.trim().isEmpty || title.length > 80) {
      throw const FormatException('widget.title must be 1..=80 characters.');
    }
    if (col < 0 || row < 0 || colSpan < 1 || rowSpan < 1) {
      throw const FormatException('Widget coordinates and spans must be >= 1.');
    }
    if (col + colSpan > panelColumns) {
      throw const FormatException('Widget extends beyond panel column bounds.');
    }
    if (assetKey != null) {
      _validateSafeId('asset_key', assetKey!);
    }
    if (customComponent != null) {
      _validateSafeId('custom_component', customComponent!);
    }
    if (text != null && text!.length > 280) {
      throw const FormatException(
        'widget.text must not exceed 280 characters.',
      );
    }
    style.validate();
  }

  /// Returns `true` if `this` shares at least one grid cell with [other].
  bool overlaps(WidgetSpec other) {
    final aRight = col + colSpan;
    final bRight = other.col + other.colSpan;
    final aBottom = row + rowSpan;
    final bBottom = other.row + other.rowSpan;

    return !(aRight <= other.col ||
        bRight <= col ||
        aBottom <= other.row ||
        bBottom <= row);
  }

  /// Serializes this widget matching Rust `WidgetSpec`.
  Map<String, Object?> toJson() => {
    'id': id,
    'title': title,
    'kind': kind.wireName,
    'col': col,
    'row': row,
    'col_span': colSpan,
    'row_span': rowSpan,
    if (telemetry != null) 'telemetry': telemetry!.wireName,
    'action': action.toJson(),
    if (text != null) 'text': text,
    if (icon != null) 'icon': icon!.wireName,
    if (assetKey != null) 'asset_key': assetKey,
    if (customComponent != null) 'custom_component': customComponent,
    'style': style.toJson(),
  };
}

/// Complete dashboard panel layout matching Rust `PanelConfig`.
class PanelConfig {
  /// Creates a [PanelConfig].
  const PanelConfig({
    required this.id,
    required this.name,
    required this.columns,
    required this.refreshIntervalMs,
    required this.widgets,
  });

  /// Decodes a [PanelConfig] JSON map and validates grid invariants.
  factory PanelConfig.fromJson(Map<String, Object?> json) {
    final panel = PanelConfig(
      id: readRequired<String>(json, 'id'),
      name: readRequired<String>(json, 'name'),
      columns: readRequired<int>(json, 'columns'),
      refreshIntervalMs: readRequired<int>(json, 'refresh_interval_ms'),
      widgets: [
        for (final entry in readObjectList(json, 'widgets'))
          WidgetSpec.fromJson(entry),
      ],
    );
    panel.validate();
    return panel;
  }

  /// Parses and validates a single exported panel JSON string.
  factory PanelConfig.importJson(String rawJson) {
    final decoded = jsonDecode(rawJson);
    return PanelConfig.fromJson(asJsonMap(decoded, context: 'panel'));
  }

  /// Unique panel identifier.
  final String id;

  /// Human-readable panel name.
  final String name;

  /// Grid column count (`2..=8`).
  final int columns;

  /// Target refresh cadence in milliseconds (`100..=60000`).
  final int refreshIntervalMs;

  /// Widgets placed on this panel.
  final List<WidgetSpec> widgets;

  /// Validates bounds, unique widget IDs, and non-overlapping cell placements.
  void validate() {
    _validateSafeId('panel.id', id);
    if (name.trim().isEmpty || name.length > 64) {
      throw const FormatException('panel.name must be 1..=64 characters.');
    }
    if (columns < minPanelColumns || columns > maxPanelColumns) {
      throw const FormatException('panel.columns must be between 2 and 8.');
    }
    if (refreshIntervalMs < minRefreshIntervalMs ||
        refreshIntervalMs > maxRefreshIntervalMs) {
      throw const FormatException(
        'panel.refresh_interval_ms must be between 100 and 60000.',
      );
    }

    final seenIds = <String>{};
    for (var i = 0; i < widgets.length; i++) {
      final widget = widgets[i];
      if (!seenIds.add(widget.id)) {
        throw FormatException('Duplicate widget id "${widget.id}".');
      }
      widget.validate(columns);
      for (var j = 0; j < i; j++) {
        if (widget.overlaps(widgets[j])) {
          throw FormatException(
            'Widget "${widget.id}" overlaps "${widgets[j].id}" on grid.',
          );
        }
      }
    }
  }

  /// Serializes this panel to pretty JSON for export.
  String exportJson() {
    validate();
    return const JsonEncoder.withIndent('  ').convert(toJson());
  }

  /// Serializes this panel into a JSON map.
  Map<String, Object?> toJson() => {
    'id': id,
    'name': name,
    'columns': columns,
    'refresh_interval_ms': refreshIntervalMs,
    'widgets': [for (final w in widgets) w.toJson()],
  };
}

/// Multi-panel workspace configuration matching Rust `DeckConfig`.
class DeckConfig {
  /// Creates a [DeckConfig].
  const DeckConfig({
    this.schemaVersion = panelSchemaVersion,
    required this.activePanelId,
    required this.panels,
  });

  /// Builds the default starter deck matching `DeckConfig::default_starter`.
  factory DeckConfig.defaultStarter() {
    return const DeckConfig(
      schemaVersion: panelSchemaVersion,
      activePanelId: 'system-overview',
      panels: [
        PanelConfig(
          id: 'system-overview',
          name: 'Системный обзор',
          columns: 4,
          refreshIntervalMs: 1000,
          widgets: [
            WidgetSpec(
              id: 'cpu-chart',
              title: 'CPU Нагрузка',
              kind: WidgetKind.chart,
              col: 0,
              row: 0,
              colSpan: 2,
              rowSpan: 2,
              telemetry: TelemetryBinding.cpuUsagePct,
              icon: WidgetIcon.cpu,
            ),
            WidgetSpec(
              id: 'ram-gauge',
              title: 'RAM %',
              kind: WidgetKind.gauge,
              col: 2,
              row: 0,
              colSpan: 2,
              rowSpan: 1,
              telemetry: TelemetryBinding.ramUsagePct,
              icon: WidgetIcon.memory,
            ),
            WidgetSpec(
              id: 'gpu-usage',
              title: 'GPU %',
              kind: WidgetKind.metricNumber,
              col: 2,
              row: 1,
              colSpan: 2,
              rowSpan: 1,
              telemetry: TelemetryBinding.gpuUsagePct,
              icon: WidgetIcon.gpu,
            ),
            WidgetSpec(
              id: 'media-play',
              title: 'Пауза / Пуск',
              kind: WidgetKind.button,
              col: 0,
              row: 2,
              colSpan: 2,
              rowSpan: 1,
              action: WidgetAction.control(ControlAction.mediaPlayPause),
              icon: WidgetIcon.playPause,
              style: WidgetStyle(accentHex: '#22D3EE'),
            ),
            WidgetSpec(
              id: 'volume-mute',
              title: 'Без звука',
              kind: WidgetKind.toggle,
              col: 2,
              row: 2,
              colSpan: 2,
              rowSpan: 1,
              action: WidgetAction.control(ControlAction.volumeMuteToggle),
              icon: WidgetIcon.volumeMute,
              style: WidgetStyle(accentHex: '#F59E0B'),
            ),
          ],
        ),
      ],
    );
  }

  /// Decodes a [DeckConfig] map and validates schema and panels.
  factory DeckConfig.fromJson(Map<String, Object?> json) {
    final deck = DeckConfig(
      schemaVersion: readRequired<int>(json, 'schema_version'),
      activePanelId: readRequired<String>(json, 'active_panel_id'),
      panels: [
        for (final entry in readObjectList(json, 'panels'))
          PanelConfig.fromJson(entry),
      ],
    );
    deck.validate();
    return deck;
  }

  /// Parses and validates an exported [DeckConfig] JSON string.
  factory DeckConfig.importJson(String rawJson) {
    final decoded = jsonDecode(rawJson);
    return DeckConfig.fromJson(asJsonMap(decoded, context: 'deck'));
  }

  /// Schema version (`1`).
  final int schemaVersion;

  /// Active panel ID.
  final String activePanelId;

  /// Ordered list of panels.
  final List<PanelConfig> panels;

  /// Validates schema version, unique panel IDs, and panel layouts.
  void validate() {
    if (schemaVersion != panelSchemaVersion) {
      throw FormatException('Unsupported schema_version $schemaVersion.');
    }
    if (panels.isEmpty) {
      throw const FormatException('DeckConfig must contain at least 1 panel.');
    }
    final seenIds = <String>{};
    for (final panel in panels) {
      if (!seenIds.add(panel.id)) {
        throw FormatException('Duplicate panel id "${panel.id}".');
      }
      panel.validate();
    }
    if (!seenIds.contains(activePanelId)) {
      throw FormatException('Unknown active_panel_id "$activePanelId".');
    }
  }

  /// Serializes this workspace configuration to pretty JSON.
  String exportJson() {
    validate();
    return const JsonEncoder.withIndent('  ').convert(toJson());
  }

  /// Serializes this workspace into a JSON map.
  Map<String, Object?> toJson() => {
    'schema_version': schemaVersion,
    'active_panel_id': activePanelId,
    'panels': [for (final p in panels) p.toJson()],
  };
}

void _validateSafeId(String field, String value) {
  if (value.contains('..') || !_safeIdPattern.hasMatch(value)) {
    throw FormatException(
      '$field must use 1..=64 safe ASCII characters without "..".',
    );
  }
}

void _validateHexColor(String field, String value) {
  if (!_hexColorPattern.hasMatch(value)) {
    throw FormatException('$field must be #RRGGBB or #AARRGGBB.');
  }
}
