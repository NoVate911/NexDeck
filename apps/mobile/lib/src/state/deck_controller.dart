import 'dart:collection';

import 'package:flutter/foundation.dart';

import '../models/monitoring_models.dart';
import '../models/panel_models.dart';
import '../models/protocol_models.dart';
import '../storage/deck_store.dart';
import 'agent_controller.dart';

const int _gridSearchMaxRows = 128;

/// Coordinates customizable panel layouts, widget editing, persistence, and throttled telemetry.
class DeckController extends ChangeNotifier {
  /// Creates a [DeckController] backed by [_deckStore].
  DeckController({
    required this._deckStore,
    DeckConfig? initialDeck,
    this._maxHistorySamples = 30,
  }) : _deck = initialDeck ?? DeckConfig.defaultStarter() {
    _deck.validate();
    _selectedWidgetId = activePanel.widgets.isNotEmpty
        ? activePanel.widgets.first.id
        : null;
  }

  final DeckStore _deckStore;
  final int _maxHistorySamples;

  DeckConfig _deck;
  String? _selectedWidgetId;
  bool _editMode = false;
  bool _initialized = false;
  bool _disposed = false;
  SystemSnapshot? _displayedSnapshot;
  int? _lastTelemetrySyncEpochMs;
  final Map<TelemetryBinding, ListQueue<double>> _metricHistory = {};
  final Map<String, bool> _toggleStates = {};
  String? _errorMessage;
  String? _statusMessage;

  /// Current validated deck configuration.
  DeckConfig get deck => _deck;

  /// Currently active [PanelConfig].
  PanelConfig get activePanel {
    for (final panel in _deck.panels) {
      if (panel.id == _deck.activePanelId) {
        return panel;
      }
    }
    return _deck.panels.first;
  }

  /// Currently selected widget identifier in edit mode, if any.
  String? get selectedWidgetId => _selectedWidgetId;

  /// Currently selected [WidgetSpec], if any.
  WidgetSpec? get selectedWidget {
    final target = _selectedWidgetId;
    if (target == null) {
      return null;
    }
    for (final widget in activePanel.widgets) {
      if (widget.id == target) {
        return widget;
      }
    }
    return null;
  }

  /// Whether layout editing controls are currently active.
  bool get editMode => _editMode;

  /// Whether [initialize] has completed at least once.
  bool get initialized => _initialized;

  /// Latest hardware snapshot throttled by [PanelConfig.refreshIntervalMs].
  SystemSnapshot? get displayedSnapshot => _displayedSnapshot;

  /// Latest validation or persistence error banner.
  String? get errorMessage => _errorMessage;

  /// Latest confirmation banner.
  String? get statusMessage => _statusMessage;

  /// Returns recorded numeric history samples for [binding].
  List<double> historyFor(TelemetryBinding binding) {
    final queue = _metricHistory[binding];
    if (queue == null) {
      return const [];
    }
    return List<double>.unmodifiable(queue);
  }

  /// Returns current visual boolean state for toggle widget [widgetId].
  bool isToggleActive(String widgetId) => _toggleStates[widgetId] ?? false;

  /// Loads persisted `panels.json` or initializes storage with the starter deck.
  Future<void> initialize() async {
    try {
      final loaded = await _deckStore.loadDeck();
      if (loaded != null) {
        _deck = loaded;
        _selectedWidgetId = activePanel.widgets.isNotEmpty
            ? activePanel.widgets.first.id
            : null;
      } else {
        await _deckStore.saveDeck(_deck);
      }
      _errorMessage = null;
    } on Object catch (err) {
      _errorMessage = 'Не удалось загрузить панели: $err';
    } finally {
      _initialized = true;
      _safeNotify();
    }
  }

  /// Enables or disables inline grid editing controls.
  void setEditMode(bool enabled) {
    if (_editMode == enabled) {
      return;
    }
    _editMode = enabled;
    if (enabled &&
        _selectedWidgetId == null &&
        activePanel.widgets.isNotEmpty) {
      _selectedWidgetId = activePanel.widgets.first.id;
    }
    _clearBanners();
    _safeNotify();
  }

  /// Selects a widget tile for move/resize/style actions.
  void selectWidget(String? widgetId) {
    _selectedWidgetId = widgetId;
    _clearBanners();
    _safeNotify();
  }

  /// Updates displayed telemetry snapshot respecting [PanelConfig.refreshIntervalMs].
  void syncTelemetry(SystemSnapshot? liveSnapshot, {DateTime? now}) {
    if (liveSnapshot == null) {
      if (_displayedSnapshot != null || _metricHistory.isNotEmpty) {
        _displayedSnapshot = null;
        _lastTelemetrySyncEpochMs = null;
        _metricHistory.clear();
        _safeNotify();
      }
      return;
    }

    final nowMs = (now ?? DateTime.now()).millisecondsSinceEpoch;
    final previousMs = _lastTelemetrySyncEpochMs;
    if (_displayedSnapshot != null &&
        previousMs != null &&
        (nowMs - previousMs) < activePanel.refreshIntervalMs) {
      return;
    }

    _displayedSnapshot = liveSnapshot;
    _lastTelemetrySyncEpochMs = nowMs;
    for (final binding in TelemetryBinding.values) {
      final sample = extractTelemetryNumericValue(liveSnapshot, binding);
      if (sample != null && sample.isFinite) {
        final queue = _metricHistory.putIfAbsent(
          binding,
          ListQueue<double>.new,
        );
        queue.addLast(sample);
        while (queue.length > _maxHistorySamples) {
          queue.removeFirst();
        }
      }
    }
    _safeNotify();
  }

  /// Switches active panel tab and persists `active_panel_id`.
  Future<bool> selectPanel(
    String panelId, {
    SystemSnapshot? latestLiveSnapshot,
  }) async {
    if (panelId == _deck.activePanelId) {
      return true;
    }
    if (!_deck.panels.any((panel) => panel.id == panelId)) {
      _setError('Панель "$panelId" не найдена.');
      return false;
    }

    final next = _deck.copyWith(activePanelId: panelId);
    final saved = await _commitDeck(next, statusNote: null);
    if (saved) {
      _selectedWidgetId = activePanel.widgets.isNotEmpty
          ? activePanel.widgets.first.id
          : null;
      _lastTelemetrySyncEpochMs = null;
      if (latestLiveSnapshot != null) {
        syncTelemetry(latestLiveSnapshot);
      } else {
        _safeNotify();
      }
    }
    return saved;
  }

  /// Creates and activates a new customizable panel.
  Future<bool> addPanel({
    required String name,
    required int columns,
    required int refreshIntervalMs,
  }) async {
    final trimmed = name.trim();
    final safeName = trimmed.isEmpty ? 'ExamplePanel' : trimmed;
    final existingIds = _deck.panels.map((p) => p.id).toList(growable: false);
    final id = _nextUniqueSafeId('panel', safeName, existingIds);

    final panel = PanelConfig(
      id: id,
      name: safeName.length > 64 ? safeName.substring(0, 64) : safeName,
      columns: columns,
      refreshIntervalMs: refreshIntervalMs,
      widgets: const [],
    );

    final next = _deck.copyWith(
      activePanelId: id,
      panels: [..._deck.panels, panel],
    );
    final saved = await _commitDeck(
      next,
      statusNote: 'Панель «${panel.name}» создана.',
    );
    if (saved) {
      _selectedWidgetId = null;
      _safeNotify();
    }
    return saved;
  }

  /// Updates active panel metadata (`name`, `columns`, `refreshIntervalMs`).
  Future<bool> updateActivePanelMeta({
    required String name,
    required int columns,
    required int refreshIntervalMs,
  }) async {
    final updatedPanel = activePanel.copyWith(
      name: name.trim(),
      columns: columns,
      refreshIntervalMs: refreshIntervalMs,
    );
    return _replacePanel(
      updatedPanel,
      statusNote: 'Параметры панели обновлены.',
    );
  }

  /// Deletes the active panel if more than one panel exists.
  Future<bool> deleteActivePanel() async {
    if (_deck.panels.length <= 1) {
      _setError('В конфигурации должна оставаться хотя бы одна панель.');
      return false;
    }

    final currentIndex = _deck.panels.indexWhere(
      (panel) => panel.id == activePanel.id,
    );
    final removedName = activePanel.name;
    final remaining = _deck.panels
        .where((panel) => panel.id != activePanel.id)
        .toList(growable: false);
    final fallbackIdx = currentIndex >= remaining.length
        ? remaining.length - 1
        : currentIndex;
    final nextActiveId = remaining[fallbackIdx].id;

    final next = _deck.copyWith(activePanelId: nextActiveId, panels: remaining);
    final saved = await _commitDeck(
      next,
      statusNote: 'Панель «$removedName» удалена.',
    );
    if (saved) {
      _selectedWidgetId = activePanel.widgets.isNotEmpty
          ? activePanel.widgets.first.id
          : null;
      _safeNotify();
    }
    return saved;
  }

  /// Restores the starter layout configuration.
  Future<bool> resetToStarter() async {
    final starter = DeckConfig.defaultStarter();
    final saved = await _commitDeck(
      starter,
      statusNote: 'Восстановлена стартовая конфигурация панелей.',
    );
    if (saved) {
      _selectedWidgetId = activePanel.widgets.isNotEmpty
          ? activePanel.widgets.first.id
          : null;
      _safeNotify();
    }
    return saved;
  }

  /// Finds the earliest non-overlapping `(col, row)` placement on [panel].
  static (int, int)? findFreeCell(
    PanelConfig panel, {
    required int colSpan,
    required int rowSpan,
  }) {
    if (colSpan < 1 || rowSpan < 1 || colSpan > panel.columns) {
      return null;
    }
    final maxStartCol = panel.columns - colSpan;
    for (var row = 0; row < _gridSearchMaxRows; row++) {
      for (var col = 0; col <= maxStartCol; col++) {
        final probe = WidgetSpec(
          id: '__probe__',
          title: 'probe',
          kind: WidgetKind.text,
          col: col,
          row: row,
          colSpan: colSpan,
          rowSpan: rowSpan,
        );
        final free = panel.widgets.every(
          (existing) => !probe.overlaps(existing),
        );
        if (free) {
          return (col, row);
        }
      }
    }
    return null;
  }

  /// Adds a new default-configured widget of [kind] into the first free cell.
  Future<bool> addWidget(WidgetKind kind) async {
    final panel = activePanel;
    final (colSpan, rowSpan) = _defaultSpansForKind(kind, panel.columns);
    final placement = findFreeCell(panel, colSpan: colSpan, rowSpan: rowSpan);
    if (placement == null) {
      _setError('Не удалось найти свободную ячейку на сетке.');
      return false;
    }

    final existingIds = panel.widgets.map((w) => w.id).toList(growable: false);
    final newId = _nextUniqueSafeId('widget', _kindSlug(kind), existingIds);
    final spec = _defaultWidgetSpec(
      id: newId,
      kind: kind,
      col: placement.$1,
      row: placement.$2,
      colSpan: colSpan,
      rowSpan: rowSpan,
    );

    final updatedPanel = panel.copyWith(widgets: [...panel.widgets, spec]);
    final saved = await _replacePanel(
      updatedPanel,
      statusNote: 'Виджет «${spec.title}» добавлен.',
    );
    if (saved) {
      _selectedWidgetId = newId;
      _safeNotify();
    }
    return saved;
  }

  /// Validates and saves updated [spec] inside the active panel.
  Future<bool> updateWidget(WidgetSpec spec) async {
    final panel = activePanel;
    final index = panel.widgets.indexWhere((w) => w.id == spec.id);
    if (index < 0) {
      _setError('Виджет "${spec.id}" не найден.');
      return false;
    }

    final updatedWidgets = List<WidgetSpec>.of(panel.widgets);
    updatedWidgets[index] = spec;
    final updatedPanel = panel.copyWith(widgets: updatedWidgets);
    return _replacePanel(
      updatedPanel,
      statusNote: 'Настройки виджета «${spec.title}» сохранены.',
    );
  }

  /// Moves the currently selected widget without leaving bounds or overlapping others.
  Future<bool> moveSelectedWidget(int deltaCol, int deltaRow) async {
    final widget = selectedWidget;
    if (widget == null) {
      _setError('Сначала выберите виджет на сетке.');
      return false;
    }
    final nextCol = widget.col + deltaCol;
    final nextRow = widget.row + deltaRow;
    if (nextCol < 0 || nextRow < 0) {
      _setError('Нельзя сдвинуть виджет за левый или верхний край.');
      return false;
    }
    return updateWidget(widget.copyWith(col: nextCol, row: nextRow));
  }

  /// Resizes the currently selected widget without leaving bounds or overlapping others.
  Future<bool> resizeSelectedWidget(int deltaColSpan, int deltaRowSpan) async {
    final widget = selectedWidget;
    if (widget == null) {
      _setError('Сначала выберите виджет на сетке.');
      return false;
    }
    final nextColSpan = widget.colSpan + deltaColSpan;
    final nextRowSpan = widget.rowSpan + deltaRowSpan;
    if (nextColSpan < 1 || nextRowSpan < 1) {
      _setError('Минимальный размер виджета — 1×1.');
      return false;
    }
    return updateWidget(
      widget.copyWith(colSpan: nextColSpan, rowSpan: nextRowSpan),
    );
  }

  /// Removes the selected widget from the active panel.
  Future<bool> deleteSelectedWidget() async {
    final target = selectedWidget;
    if (target == null) {
      return false;
    }
    final remaining = activePanel.widgets
        .where((w) => w.id != target.id)
        .toList(growable: false);
    final saved = await _replacePanel(
      activePanel.copyWith(widgets: remaining),
      statusNote: 'Виджет «${target.title}» удалён.',
    );
    if (saved) {
      _selectedWidgetId = remaining.isNotEmpty ? remaining.first.id : null;
      _safeNotify();
    }
    return saved;
  }

  /// Exports the full [DeckConfig] as validated pretty JSON.
  String exportDeckJson() => _deck.exportJson();

  /// Exports only the active [PanelConfig] as validated pretty JSON.
  String exportActivePanelJson() => activePanel.exportJson();

  /// Parses, validates, and persists a full [DeckConfig] JSON string.
  Future<bool> importDeckJson(String rawJson) async {
    try {
      final parsed = DeckConfig.importJson(rawJson);
      final saved = await _commitDeck(
        parsed,
        statusNote: 'Конфигурация панелей импортирована.',
      );
      if (saved) {
        _selectedWidgetId = activePanel.widgets.isNotEmpty
            ? activePanel.widgets.first.id
            : null;
        _lastTelemetrySyncEpochMs = null;
        _safeNotify();
      }
      return saved;
    } on Object catch (err) {
      _setError('Некорректный JSON конфигурации: $err');
      return false;
    }
  }

  /// Parses, validates, and appends a single exported [PanelConfig] JSON string.
  Future<bool> importPanelJson(String rawJson) async {
    try {
      var imported = PanelConfig.importJson(rawJson);
      final existingIds = _deck.panels.map((p) => p.id).toList(growable: false);
      if (existingIds.contains(imported.id)) {
        imported = imported.copyWith(
          id: _nextUniqueSafeId('panel', imported.id, existingIds),
        );
      }

      final next = _deck.copyWith(
        activePanelId: imported.id,
        panels: [..._deck.panels, imported],
      );
      final saved = await _commitDeck(
        next,
        statusNote: 'Панель «${imported.name}» импортирована.',
      );
      if (saved) {
        _selectedWidgetId = imported.widgets.isNotEmpty
            ? imported.widgets.first.id
            : null;
        _lastTelemetrySyncEpochMs = null;
        _safeNotify();
      }
      return saved;
    } on Object catch (err) {
      _setError('Некорректный JSON панели: $err');
      return false;
    }
  }

  /// Dispatches an interactive button/toggle widget's allowlisted action over [agentController].
  Future<void> activateWidgetAction(
    WidgetSpec spec,
    AgentController agentController,
  ) async {
    if (spec.kind == WidgetKind.toggle) {
      _toggleStates[spec.id] = !(_toggleStates[spec.id] ?? false);
      _safeNotify();
    }

    switch (spec.action.type) {
      case WidgetActionType.none:
        return;
      case WidgetActionType.control:
        final controlAction = spec.action.controlAction;
        if (controlAction != null) {
          await agentController.sendControlAction(controlAction);
        }
      case WidgetActionType.heartbeat:
        await agentController.sendHeartbeat();
    }
  }

  Future<bool> _replacePanel(
    PanelConfig updatedPanel, {
    required String statusNote,
  }) async {
    final nextPanels = [
      for (final panel in _deck.panels)
        if (panel.id == updatedPanel.id) updatedPanel else panel,
    ];
    final next = _deck.copyWith(panels: nextPanels);
    return _commitDeck(next, statusNote: statusNote);
  }

  Future<bool> _commitDeck(DeckConfig candidate, {String? statusNote}) async {
    try {
      candidate.validate();
      await _deckStore.saveDeck(candidate);
      _deck = candidate;
      _errorMessage = null;
      _statusMessage = statusNote;
      _safeNotify();
      return true;
    } on Object catch (err) {
      _setError('Ошибка конфигурации панели: $err');
      return false;
    }
  }

  void _clearBanners() {
    _errorMessage = null;
    _statusMessage = null;
  }

  void _setError(String message) {
    _errorMessage = message;
    _statusMessage = null;
    _safeNotify();
  }

  void _safeNotify() {
    if (!_disposed) {
      notifyListeners();
    }
  }

  @override
  void dispose() {
    _disposed = true;
    super.dispose();
  }
}

/// Extracts raw numeric metric value for [binding] from [snapshot], if available.
double? extractTelemetryNumericValue(
  SystemSnapshot? snapshot,
  TelemetryBinding binding,
) {
  if (snapshot == null) {
    return null;
  }
  switch (binding) {
    case TelemetryBinding.cpuUsagePct:
      return snapshot.cpu.totalUsagePct;
    case TelemetryBinding.cpuFrequencyMhz:
      return snapshot.cpu.frequencyMhz?.toDouble();
    case TelemetryBinding.cpuTemperatureC:
      return snapshot.cpu.temperatureCelsius;
    case TelemetryBinding.ramUsagePct:
      return snapshot.memory.usagePct;
    case TelemetryBinding.ramUsedBytes:
      return snapshot.memory.usedBytes.toDouble();
    case TelemetryBinding.gpuUsagePct:
      if (snapshot.gpus.isEmpty) {
        return null;
      }
      return snapshot.gpus.first.usagePct;
    case TelemetryBinding.gpuVramPct:
      if (snapshot.gpus.isEmpty) {
        return null;
      }
      final gpu = snapshot.gpus.first;
      final used = gpu.vramUsedBytes;
      final total = gpu.vramTotalBytes;
      if (used == null || total == null || total <= 0) {
        return null;
      }
      return (used.clamp(0, total) / total) * 100.0;
    case TelemetryBinding.networkRxBps:
      var rx = 0;
      for (final net in snapshot.networks) {
        rx += net.rxBytesPerSec;
      }
      return rx.toDouble();
    case TelemetryBinding.networkTxBps:
      var tx = 0;
      for (final net in snapshot.networks) {
        tx += net.txBytesPerSec;
      }
      return tx.toDouble();
    case TelemetryBinding.batteryPct:
      return snapshot.battery?.chargePct;
    case TelemetryBinding.uptimeSecs:
      return snapshot.uptimeSecs.toDouble();
  }
}

String _nextUniqueSafeId(
  String fallbackPrefix,
  String rawHint,
  List<String> existing,
) {
  final buffer = StringBuffer();
  var previousDash = false;
  for (final codeUnit in rawHint.codeUnits) {
    final isAsciiAlphaNum =
        (codeUnit >= 0x30 && codeUnit <= 0x39) ||
        (codeUnit >= 0x41 && codeUnit <= 0x5A) ||
        (codeUnit >= 0x61 && codeUnit <= 0x7A);
    if (isAsciiAlphaNum) {
      buffer.write(String.fromCharCode(codeUnit).toLowerCase());
      previousDash = false;
    } else if ((codeUnit == 0x2D || codeUnit == 0x5F || codeUnit == 0x20) &&
        buffer.isNotEmpty &&
        !previousDash) {
      buffer.write('-');
      previousDash = true;
    }
  }

  var base = buffer.toString();
  while (base.endsWith('-')) {
    base = base.substring(0, base.length - 1);
  }
  if (base.isEmpty) {
    base = fallbackPrefix;
  }
  if (base.length > 48) {
    base = base.substring(0, 48);
  }
  if (!existing.contains(base)) {
    return base;
  }
  for (var i = 2; i <= 9999; i++) {
    final candidate = '$base-$i';
    if (candidate.length <= 64 && !existing.contains(candidate)) {
      return candidate;
    }
  }
  return '$fallbackPrefix-${existing.length + 1}';
}

String _kindSlug(WidgetKind kind) => switch (kind) {
  WidgetKind.text => 'text',
  WidgetKind.metricNumber => 'metric',
  WidgetKind.button => 'button',
  WidgetKind.toggle => 'toggle',
  WidgetKind.chart => 'chart',
  WidgetKind.gauge => 'gauge',
  WidgetKind.icon => 'icon',
  WidgetKind.image => 'image',
  WidgetKind.custom => 'custom',
};

(int, int) _defaultSpansForKind(WidgetKind kind, int panelColumns) {
  final (colSpan, rowSpan) = switch (kind) {
    WidgetKind.chart || WidgetKind.text => (2, 2),
    WidgetKind.metricNumber ||
    WidgetKind.gauge ||
    WidgetKind.image ||
    WidgetKind.custom => (2, 1),
    WidgetKind.button || WidgetKind.toggle || WidgetKind.icon => (1, 1),
  };
  return (colSpan.clamp(1, panelColumns), rowSpan);
}

WidgetSpec _defaultWidgetSpec({
  required String id,
  required WidgetKind kind,
  required int col,
  required int row,
  required int colSpan,
  required int rowSpan,
}) {
  return switch (kind) {
    WidgetKind.chart => WidgetSpec(
      id: id,
      title: 'CPU График',
      kind: kind,
      col: col,
      row: row,
      colSpan: colSpan,
      rowSpan: rowSpan,
      telemetry: TelemetryBinding.cpuUsagePct,
      icon: WidgetIcon.cpu,
    ),
    WidgetKind.metricNumber => WidgetSpec(
      id: id,
      title: 'RAM %',
      kind: kind,
      col: col,
      row: row,
      colSpan: colSpan,
      rowSpan: rowSpan,
      telemetry: TelemetryBinding.ramUsagePct,
      icon: WidgetIcon.memory,
    ),
    WidgetKind.gauge => WidgetSpec(
      id: id,
      title: 'GPU %',
      kind: kind,
      col: col,
      row: row,
      colSpan: colSpan,
      rowSpan: rowSpan,
      telemetry: TelemetryBinding.gpuUsagePct,
      icon: WidgetIcon.gpu,
    ),
    WidgetKind.button => WidgetSpec(
      id: id,
      title: 'Пауза / Старт',
      kind: kind,
      col: col,
      row: row,
      colSpan: colSpan,
      rowSpan: rowSpan,
      action: const WidgetAction.control(ControlAction.mediaPlayPause),
      icon: WidgetIcon.playPause,
      style: const WidgetStyle(accentHex: '#22D3EE'),
    ),
    WidgetKind.toggle => WidgetSpec(
      id: id,
      title: 'Без звука',
      kind: kind,
      col: col,
      row: row,
      colSpan: colSpan,
      rowSpan: rowSpan,
      action: const WidgetAction.control(ControlAction.volumeMuteToggle),
      icon: WidgetIcon.volumeMute,
      style: const WidgetStyle(accentHex: '#F59E0B'),
    ),
    WidgetKind.text => WidgetSpec(
      id: id,
      title: 'ExampleNote',
      kind: kind,
      col: col,
      row: row,
      colSpan: colSpan,
      rowSpan: rowSpan,
      text: 'Пример текстового виджета NexDeck',
    ),
    WidgetKind.icon => WidgetSpec(
      id: id,
      title: 'Статус узла',
      kind: kind,
      col: col,
      row: row,
      colSpan: colSpan,
      rowSpan: rowSpan,
      action: const WidgetAction.heartbeat(),
      icon: WidgetIcon.shield,
    ),
    WidgetKind.image => WidgetSpec(
      id: id,
      title: 'ExampleBanner',
      kind: kind,
      col: col,
      row: row,
      colSpan: colSpan,
      rowSpan: rowSpan,
      assetKey: 'example-banner',
    ),
    WidgetKind.custom => WidgetSpec(
      id: id,
      title: 'ExamplePlugin',
      kind: kind,
      col: col,
      row: row,
      colSpan: colSpan,
      rowSpan: rowSpan,
      icon: WidgetIcon.terminal,
      customComponent: 'org.example.widget',
    ),
  };
}
