import 'dart:math' as math;

import 'package:material_ui/material_ui.dart';

import '../models/monitoring_models.dart';
import '../models/panel_models.dart';
import '../state/agent_controller.dart';
import '../state/deck_controller.dart';
import '../transport/transport_client.dart';
import 'widget_card.dart';
import 'widget_editor_sheet.dart';

const double _gridGap = 8.0;

/// Dynamic panel renderer and on-device layout editor (`Stage 3`).
class DeckPanelView extends StatelessWidget {
  /// Creates a [DeckPanelView].
  const DeckPanelView({
    required this.agentController,
    required this.deckController,
    super.key,
  });

  /// Provides connection status, control dispatch, and telemetry history.
  final AgentController agentController;

  /// Manages [DeckConfig], widget layout editing, and throttled telemetry snapshots.
  final DeckController deckController;

  @override
  Widget build(BuildContext context) {
    return AnimatedBuilder(
      animation: Listenable.merge([agentController, deckController]),
      builder: (context, _) {
        final panel = deckController.activePanel;
        final isEditMode = deckController.editMode;
        final isConnected =
            agentController.connectionState ==
            TransportConnectionState.connected;
        final snapshot =
            deckController.displayedSnapshot ?? agentController.latestSnapshot;

        return ListView(
          padding: const EdgeInsets.all(16),
          children: [
            _buildPanelHeaderBar(context, panel, isEditMode),
            if (isEditMode) ...[
              const SizedBox(height: 10),
              _buildEditorToolbar(context, panel),
              if (deckController.selectedWidget != null) ...[
                const SizedBox(height: 8),
                _buildSelectedWidgetControls(
                  context,
                  deckController.selectedWidget!,
                ),
              ],
            ],
            if (deckController.errorMessage != null) ...[
              const SizedBox(height: 8),
              _StatusStrip(
                message: deckController.errorMessage!,
                isError: true,
              ),
            ] else if (deckController.statusMessage != null) ...[
              const SizedBox(height: 8),
              _StatusStrip(
                message: deckController.statusMessage!,
                isError: false,
              ),
            ],
            const SizedBox(height: 12),
            _DeckGridCanvas(
              panel: panel,
              snapshot: snapshot,
              agentController: agentController,
              deckController: deckController,
              isEditMode: isEditMode,
              isConnected: isConnected,
            ),
          ],
        );
      },
    );
  }

  Widget _buildPanelHeaderBar(
    BuildContext context,
    PanelConfig activePanel,
    bool isEditMode,
  ) {
    final theme = Theme.of(context);
    return Card(
      child: Padding(
        padding: const EdgeInsets.all(12),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Row(
              children: [
                Expanded(
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      Text(
                        activePanel.name,
                        style: theme.textTheme.titleMedium,
                        maxLines: 1,
                        overflow: TextOverflow.ellipsis,
                      ),
                      Text(
                        '${activePanel.columns} кол. • Обновление ${activePanel.refreshIntervalMs} мс',
                        style: theme.textTheme.bodySmall,
                      ),
                    ],
                  ),
                ),
                FilterChip(
                  key: const Key('toggle_edit_mode_button'),
                  label: Text(isEditMode ? 'Готово' : 'Редактировать'),
                  avatar: Icon(isEditMode ? Icons.check : Icons.tune, size: 18),
                  selected: isEditMode,
                  onSelected: deckController.setEditMode,
                ),
              ],
            ),
            const SizedBox(height: 8),
            SingleChildScrollView(
              scrollDirection: Axis.horizontal,
              child: Row(
                children: [
                  for (final panel in deckController.deck.panels)
                    Padding(
                      padding: const EdgeInsets.only(right: 8),
                      child: ChoiceChip(
                        key: Key('panel_chip_${panel.id}'),
                        label: Text(panel.name),
                        selected: panel.id == activePanel.id,
                        onSelected: (_) {
                          deckController.selectPanel(
                            panel.id,
                            latestLiveSnapshot: agentController.latestSnapshot,
                          );
                        },
                      ),
                    ),
                ],
              ),
            ),
          ],
        ),
      ),
    );
  }

  Widget _buildEditorToolbar(BuildContext context, PanelConfig panel) {
    return Wrap(
      spacing: 8,
      runSpacing: 8,
      children: [
        OutlinedButton.icon(
          key: const Key('add_panel_button'),
          onPressed: () => showPanelMetaDialog(
            context: context,
            deckController: deckController,
            isCreate: true,
          ),
          icon: const Icon(Icons.add_box_outlined, size: 18),
          label: const Text('Новая панель'),
        ),
        PopupMenuButton<WidgetKind>(
          key: const Key('add_widget_menu_button'),
          tooltip: 'Добавить виджет',
          onSelected: deckController.addWidget,
          itemBuilder: (context) => [
            for (final kind in WidgetKind.values)
              PopupMenuItem(
                key: Key('add_widget_option_${kind.wireName}'),
                value: kind,
                child: Text(kind.displayLabel),
              ),
          ],
          child: Container(
            padding: const EdgeInsets.symmetric(horizontal: 14, vertical: 10),
            decoration: BoxDecoration(
              borderRadius: BorderRadius.circular(20),
              border: Border.all(color: Theme.of(context).colorScheme.outline),
            ),
            child: const Row(
              mainAxisSize: MainAxisSize.min,
              children: [
                Icon(Icons.widgets_outlined, size: 18),
                SizedBox(width: 6),
                Text('+ Виджет'),
              ],
            ),
          ),
        ),
        OutlinedButton.icon(
          key: const Key('edit_panel_meta_button'),
          onPressed: () => showPanelMetaDialog(
            context: context,
            deckController: deckController,
            isCreate: false,
          ),
          icon: const Icon(Icons.settings_outlined, size: 18),
          label: Text('Сетка (${panel.columns}к)'),
        ),
        OutlinedButton.icon(
          key: const Key('open_panel_json_button'),
          onPressed: () => showJsonImportExportDialog(
            context: context,
            deckController: deckController,
          ),
          icon: const Icon(Icons.code, size: 18),
          label: const Text('JSON'),
        ),
      ],
    );
  }

  Widget _buildSelectedWidgetControls(
    BuildContext context,
    WidgetSpec selected,
  ) {
    return Card(
      color: Theme.of(context).colorScheme.surfaceContainerHigh,
      child: Padding(
        padding: const EdgeInsets.all(10),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Text(
              'Виджет: ${selected.title} (${selected.col},${selected.row} • ${selected.colSpan}×${selected.rowSpan})',
              style: Theme.of(context).textTheme.labelLarge,
              maxLines: 1,
              overflow: TextOverflow.ellipsis,
            ),
            const SizedBox(height: 8),
            Wrap(
              spacing: 6,
              runSpacing: 6,
              children: [
                _CompactToolButton(
                  buttonKey: const Key('widget_move_left'),
                  label: '◀',
                  onPressed: () => deckController.moveSelectedWidget(-1, 0),
                ),
                _CompactToolButton(
                  buttonKey: const Key('widget_move_right'),
                  label: '▶',
                  onPressed: () => deckController.moveSelectedWidget(1, 0),
                ),
                _CompactToolButton(
                  buttonKey: const Key('widget_move_up'),
                  label: '▲',
                  onPressed: () => deckController.moveSelectedWidget(0, -1),
                ),
                _CompactToolButton(
                  buttonKey: const Key('widget_move_down'),
                  label: '▼',
                  onPressed: () => deckController.moveSelectedWidget(0, 1),
                ),
                _CompactToolButton(
                  buttonKey: const Key('widget_width_minus'),
                  label: 'Ш−',
                  onPressed: () => deckController.resizeSelectedWidget(-1, 0),
                ),
                _CompactToolButton(
                  buttonKey: const Key('widget_width_plus'),
                  label: 'Ш+',
                  onPressed: () => deckController.resizeSelectedWidget(1, 0),
                ),
                _CompactToolButton(
                  buttonKey: const Key('widget_height_minus'),
                  label: 'В−',
                  onPressed: () => deckController.resizeSelectedWidget(0, -1),
                ),
                _CompactToolButton(
                  buttonKey: const Key('widget_height_plus'),
                  label: 'В+',
                  onPressed: () => deckController.resizeSelectedWidget(0, 1),
                ),
                FilledButton.tonal(
                  key: const Key('customize_selected_widget_button'),
                  onPressed: () => showWidgetEditorSheet(
                    context: context,
                    deckController: deckController,
                    widgetSpec: selected,
                  ),
                  child: const Text('Стиль / Привязка'),
                ),
                TextButton(
                  key: const Key('delete_selected_widget_button'),
                  onPressed: deckController.deleteSelectedWidget,
                  child: Text(
                    'Удалить',
                    style: TextStyle(
                      color: Theme.of(context).colorScheme.error,
                    ),
                  ),
                ),
              ],
            ),
          ],
        ),
      ),
    );
  }
}

class _DeckGridCanvas extends StatelessWidget {
  const _DeckGridCanvas({
    required this.panel,
    required this.snapshot,
    required this.agentController,
    required this.deckController,
    required this.isEditMode,
    required this.isConnected,
  });

  final PanelConfig panel;
  final SystemSnapshot? snapshot;
  final AgentController agentController;
  final DeckController deckController;
  final bool isEditMode;
  final bool isConnected;

  @override
  Widget build(BuildContext context) {
    final cols = math.max(minPanelColumns, panel.columns);
    var maxRow = isEditMode ? 4 : 3;
    for (final widget in panel.widgets) {
      maxRow = math.max(maxRow, widget.row + widget.rowSpan);
    }
    if (isEditMode) {
      maxRow += 1;
    }

    return LayoutBuilder(
      builder: (context, constraints) {
        final width = constraints.maxWidth.isFinite
            ? constraints.maxWidth
            : 360.0;
        final cellWidth = ((width - _gridGap * (cols - 1)) / cols).clamp(
          42.0,
          240.0,
        );
        final cellHeight = (cellWidth * 0.92).clamp(76.0, 124.0);
        final canvasHeight = maxRow * cellHeight + (maxRow - 1) * _gridGap;

        return SizedBox(
          width: width,
          height: canvasHeight,
          child: Stack(
            children: [
              if (isEditMode)
                for (var r = 0; r < maxRow; r++)
                  for (var c = 0; c < cols; c++)
                    Positioned(
                      left: c * (cellWidth + _gridGap),
                      top: r * (cellHeight + _gridGap),
                      width: cellWidth,
                      height: cellHeight,
                      child: DragTarget<String>(
                        onWillAcceptWithDetails: (details) =>
                            details.data.isNotEmpty,
                        onAcceptWithDetails: (details) {
                          deckController.moveWidgetToCell(details.data, c, r);
                        },
                        builder: (context, candidateData, rejectedData) {
                          final hovering = candidateData.isNotEmpty;
                          return AnimatedContainer(
                            duration: const Duration(milliseconds: 140),
                            decoration: BoxDecoration(
                              color: hovering
                                  ? Theme.of(context).colorScheme.primary
                                        .withValues(alpha: 0.22)
                                  : Colors.transparent,
                              borderRadius: BorderRadius.circular(12),
                              border: Border.all(
                                width: hovering ? 2.0 : 1.0,
                                color: hovering
                                    ? Theme.of(context).colorScheme.primary
                                    : Theme.of(context)
                                          .colorScheme
                                          .outlineVariant
                                          .withValues(alpha: 0.4),
                              ),
                            ),
                          );
                        },
                      ),
                    ),
              for (final widgetSpec in panel.widgets)
                Positioned(
                  left: widgetSpec.col * (cellWidth + _gridGap),
                  top: widgetSpec.row * (cellHeight + _gridGap),
                  width:
                      widgetSpec.colSpan * cellWidth +
                      (widgetSpec.colSpan - 1) * _gridGap,
                  height:
                      widgetSpec.rowSpan * cellHeight +
                      (widgetSpec.rowSpan - 1) * _gridGap,
                  child: isEditMode
                      ? LongPressDraggable<String>(
                          data: widgetSpec.id,
                          delay: const Duration(milliseconds: 220),
                          feedback: Material(
                            color: Colors.transparent,
                            child: SizedBox(
                              width:
                                  widgetSpec.colSpan * cellWidth +
                                  (widgetSpec.colSpan - 1) * _gridGap,
                              height:
                                  widgetSpec.rowSpan * cellHeight +
                                  (widgetSpec.rowSpan - 1) * _gridGap,
                              child: Opacity(
                                opacity: 0.88,
                                child: DeckWidgetCard(
                                  spec: widgetSpec,
                                  snapshot: snapshot,
                                  history: _chartHistoryFor(
                                    widgetSpec.telemetry,
                                  ),
                                  isConnected: isConnected,
                                  isEditMode: true,
                                  isSelected: true,
                                  isToggleActive: deckController.isToggleActive(
                                    widgetSpec.id,
                                  ),
                                  onTap: null,
                                ),
                              ),
                            ),
                          ),
                          childWhenDragging: Opacity(
                            opacity: 0.32,
                            child: DeckWidgetCard(
                              spec: widgetSpec,
                              snapshot: snapshot,
                              history: _chartHistoryFor(widgetSpec.telemetry),
                              isConnected: isConnected,
                              isEditMode: isEditMode,
                              isSelected:
                                  deckController.selectedWidgetId ==
                                  widgetSpec.id,
                              isToggleActive: deckController.isToggleActive(
                                widgetSpec.id,
                              ),
                              onTap: () =>
                                  deckController.selectWidget(widgetSpec.id),
                            ),
                          ),
                          child: DeckWidgetCard(
                            spec: widgetSpec,
                            snapshot: snapshot,
                            history: _chartHistoryFor(widgetSpec.telemetry),
                            isConnected: isConnected,
                            isEditMode: isEditMode,
                            isSelected:
                                deckController.selectedWidgetId ==
                                widgetSpec.id,
                            isToggleActive: deckController.isToggleActive(
                              widgetSpec.id,
                            ),
                            onTap: () =>
                                deckController.selectWidget(widgetSpec.id),
                          ),
                        )
                      : DeckWidgetCard(
                          spec: widgetSpec,
                          snapshot: snapshot,
                          history: _chartHistoryFor(widgetSpec.telemetry),
                          isConnected: isConnected,
                          isEditMode: isEditMode,
                          isSelected: false,
                          isToggleActive: deckController.isToggleActive(
                            widgetSpec.id,
                          ),
                          onTap: () => deckController.activateWidgetAction(
                            widgetSpec,
                            agentController,
                          ),
                        ),
                ),
              if (panel.widgets.isEmpty)
                Center(
                  child: Text(
                    isEditMode ? 'Нажмите «+ Виджет», чтобы заполнить панель.' : 'Панель пуста. Включите «Редактировать», чтобы добавить виджеты.',
                    textAlign: TextAlign.center,
                  ),
                ),
            ],
          ),
        );
      },
    );
  }

  List<double> _chartHistoryFor(TelemetryBinding? binding) {
    final target = binding ?? TelemetryBinding.cpuUsagePct;
    final fromDeck = deckController.historyFor(target);
    if (fromDeck.isNotEmpty) {
      return fromDeck;
    }
    return switch (target) {
      TelemetryBinding.cpuUsagePct => agentController.cpuHistory,
      TelemetryBinding.ramUsagePct => agentController.ramHistory,
      _ => const [],
    };
  }
}

class _CompactToolButton extends StatelessWidget {
  const _CompactToolButton({
    required this.buttonKey,
    required this.label,
    required this.onPressed,
  });

  final Key buttonKey;
  final String label;
  final VoidCallback onPressed;

  @override
  Widget build(BuildContext context) {
    return OutlinedButton(
      key: buttonKey,
      style: OutlinedButton.styleFrom(
        minimumSize: const Size(40, 36),
        padding: const EdgeInsets.symmetric(horizontal: 10, vertical: 6),
      ),
      onPressed: onPressed,
      child: Text(label),
    );
  }
}

class _StatusStrip extends StatelessWidget {
  const _StatusStrip({required this.message, required this.isError});

  final String message;
  final bool isError;

  @override
  Widget build(BuildContext context) {
    final color = isError
        ? Theme.of(context).colorScheme.error
        : const Color(0xFF22C55E);
    return Container(
      padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 8),
      decoration: BoxDecoration(
        color: color.withValues(alpha: 0.14),
        borderRadius: BorderRadius.circular(10),
        border: Border.all(color: color.withValues(alpha: 0.5)),
      ),
      child: Text(message, style: TextStyle(color: color, fontSize: 13)),
    );
  }
}
