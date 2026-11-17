import 'package:flutter/services.dart';
import 'package:material_ui/material_ui.dart';

import '../models/panel_models.dart';
import '../models/protocol_models.dart';
import '../state/deck_controller.dart';
import 'widget_card.dart';

const List<String> _accentPresets = [
  '#38BDF8',
  '#22D3EE',
  '#34D399',
  '#F59E0B',
  '#FB7185',
  '#A78BFA',
];

const List<String> _backgroundPresets = [
  '#1E293B',
  '#0F172A',
  '#172554',
  '#14532D',
  '#3B0764',
  '#3F1D28',
];

/// Opens a modal bottom sheet to customize a [WidgetSpec].
Future<void> showWidgetEditorSheet({
  required BuildContext context,
  required DeckController deckController,
  required WidgetSpec widgetSpec,
}) {
  return showModalBottomSheet<void>(
    context: context,
    isScrollControlled: true,
    useSafeArea: true,
    builder: (ctx) => _WidgetEditorSheetContent(
      deckController: deckController,
      initial: widgetSpec,
    ),
  );
}

class _WidgetEditorSheetContent extends StatefulWidget {
  const _WidgetEditorSheetContent({
    required this.deckController,
    required this.initial,
  });

  final DeckController deckController;
  final WidgetSpec initial;

  @override
  State<_WidgetEditorSheetContent> createState() =>
      _WidgetEditorSheetContentState();
}

class _WidgetEditorSheetContentState extends State<_WidgetEditorSheetContent> {
  late final TextEditingController _titleCtrl;
  late final TextEditingController _textCtrl;
  late final TextEditingController _assetCtrl;
  late final TextEditingController _customCtrl;
  late final TextEditingController _bgHexCtrl;
  late final TextEditingController _fgHexCtrl;
  late final TextEditingController _accentHexCtrl;

  late WidgetKind _kind;
  late TelemetryBinding? _telemetry;
  late WidgetAction _action;
  late WidgetIcon? _icon;
  late int _fontSizeSp;
  late int _cornerRadius;

  @override
  void initState() {
    super.initState();
    final spec = widget.initial;
    _titleCtrl = TextEditingController(text: spec.title);
    _textCtrl = TextEditingController(text: spec.text ?? '');
    _assetCtrl = TextEditingController(text: spec.assetKey ?? '');
    _customCtrl = TextEditingController(text: spec.customComponent ?? '');
    _bgHexCtrl = TextEditingController(text: spec.style.backgroundHex);
    _fgHexCtrl = TextEditingController(text: spec.style.foregroundHex);
    _accentHexCtrl = TextEditingController(text: spec.style.accentHex);

    _kind = spec.kind;
    _telemetry = spec.telemetry;
    _action = spec.action;
    _icon = spec.icon;
    _fontSizeSp = spec.style.fontSizeSp;
    _cornerRadius = spec.style.cornerRadius;
  }

  @override
  void dispose() {
    _titleCtrl.dispose();
    _textCtrl.dispose();
    _assetCtrl.dispose();
    _customCtrl.dispose();
    _bgHexCtrl.dispose();
    _fgHexCtrl.dispose();
    _accentHexCtrl.dispose();
    super.dispose();
  }

  Future<void> _save() async {
    final textValue = _textCtrl.text.trim();
    final assetValue = _assetCtrl.text.trim();
    final customValue = _customCtrl.text.trim();

    final updated = widget.initial.copyWith(
      title: _titleCtrl.text.trim(),
      kind: _kind,
      telemetry: _telemetry,
      clearTelemetry: _telemetry == null,
      action: _action,
      icon: _icon,
      clearIcon: _icon == null,
      text: textValue.isEmpty ? null : textValue,
      clearText: textValue.isEmpty,
      assetKey: assetValue.isEmpty ? null : assetValue,
      clearAssetKey: assetValue.isEmpty,
      customComponent: customValue.isEmpty ? null : customValue,
      clearCustomComponent: customValue.isEmpty,
      style: WidgetStyle(
        backgroundHex: _bgHexCtrl.text.trim(),
        foregroundHex: _fgHexCtrl.text.trim(),
        accentHex: _accentHexCtrl.text.trim(),
        fontSizeSp: _fontSizeSp,
        cornerRadius: _cornerRadius,
      ),
    );

    final ok = await widget.deckController.updateWidget(updated);
    if (ok && mounted) {
      Navigator.of(context).pop();
    }
  }

  @override
  Widget build(BuildContext context) {
    final bottomInset = MediaQuery.viewInsetsOf(context).bottom;
    return Padding(
      padding: EdgeInsets.fromLTRB(16, 16, 16, 16 + bottomInset),
      child: SingleChildScrollView(
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.stretch,
          mainAxisSize: MainAxisSize.min,
          children: [
            Row(
              children: [
                Text(
                  'Настройка виджета (${widget.initial.id})',
                  style: Theme.of(context).textTheme.titleMedium,
                ),
                const Spacer(),
                IconButton(
                  onPressed: () => Navigator.of(context).pop(),
                  icon: const Icon(Icons.close),
                ),
              ],
            ),
            const SizedBox(height: 8),
            TextField(
              key: const Key('widget_editor_title_field'),
              controller: _titleCtrl,
              decoration: const InputDecoration(
                labelText: 'Заголовок',
                border: OutlineInputBorder(),
              ),
            ),
            const SizedBox(height: 10),
            DropdownButtonFormField<WidgetKind>(
              initialValue: _kind,
              decoration: const InputDecoration(
                labelText: 'Тип виджета',
                border: OutlineInputBorder(),
              ),
              items: [
                for (final kind in WidgetKind.values)
                  DropdownMenuItem(value: kind, child: Text(kind.displayLabel)),
              ],
              onChanged: (next) {
                if (next != null) {
                  setState(() => _kind = next);
                }
              },
            ),
            const SizedBox(height: 10),
            DropdownButtonFormField<TelemetryBinding?>(
              initialValue: _telemetry,
              decoration: const InputDecoration(
                labelText: 'Привязка метрики',
                border: OutlineInputBorder(),
              ),
              items: [
                const DropdownMenuItem<TelemetryBinding?>(
                  value: null,
                  child: Text('Нет'),
                ),
                for (final binding in TelemetryBinding.values)
                  DropdownMenuItem<TelemetryBinding?>(
                    value: binding,
                    child: Text(binding.displayLabel),
                  ),
              ],
              onChanged: (next) => setState(() => _telemetry = next),
            ),
            const SizedBox(height: 10),
            DropdownButtonFormField<String>(
              initialValue: _actionKey(_action),
              decoration: const InputDecoration(
                labelText: 'Действие при нажатии',
                border: OutlineInputBorder(),
              ),
              items: [
                const DropdownMenuItem(
                  value: 'none',
                  child: Text('Нет действия'),
                ),
                const DropdownMenuItem(
                  value: 'heartbeat',
                  child: Text('Пинг (Heartbeat)'),
                ),
                for (final action in ControlAction.values)
                  DropdownMenuItem(
                    value: action.wireName,
                    child: Text(action.displayLabel),
                  ),
              ],
              onChanged: (next) {
                if (next != null) {
                  setState(() => _action = _actionFromKey(next));
                }
              },
            ),
            const SizedBox(height: 10),
            DropdownButtonFormField<WidgetIcon?>(
              initialValue: _icon,
              decoration: const InputDecoration(
                labelText: 'Иконка',
                border: OutlineInputBorder(),
              ),
              items: [
                const DropdownMenuItem<WidgetIcon?>(
                  value: null,
                  child: Text('Нет'),
                ),
                for (final icon in WidgetIcon.values)
                  DropdownMenuItem<WidgetIcon?>(
                    value: icon,
                    child: Row(
                      children: [
                        Icon(widgetIconToIconData(icon), size: 18),
                        const SizedBox(width: 8),
                        Text(icon.wireName),
                      ],
                    ),
                  ),
              ],
              onChanged: (next) => setState(() => _icon = next),
            ),
            if (_kind == WidgetKind.text) ...[
              const SizedBox(height: 10),
              TextField(
                controller: _textCtrl,
                maxLines: 3,
                decoration: const InputDecoration(
                  labelText: 'Текст карточки',
                  border: OutlineInputBorder(),
                ),
              ),
            ],
            if (_kind == WidgetKind.image) ...[
              const SizedBox(height: 10),
              TextField(
                controller: _assetCtrl,
                decoration: const InputDecoration(
                  labelText: 'Безопасный asset_key',
                  hintText: 'example-banner',
                  border: OutlineInputBorder(),
                ),
              ),
            ],
            if (_kind == WidgetKind.custom) ...[
              const SizedBox(height: 10),
              TextField(
                controller: _customCtrl,
                decoration: const InputDecoration(
                  labelText: 'Идентификатор компонента (org.example.*)',
                  border: OutlineInputBorder(),
                ),
              ),
            ],
            const SizedBox(height: 12),
            Text('Цвет фона', style: Theme.of(context).textTheme.labelMedium),
            const SizedBox(height: 4),
            _ColorPresetRow(
              presets: _backgroundPresets,
              controller: _bgHexCtrl,
              label: 'Фон (#RRGGBB)',
            ),
            const SizedBox(height: 8),
            Text('Акцент', style: Theme.of(context).textTheme.labelMedium),
            const SizedBox(height: 4),
            _ColorPresetRow(
              presets: _accentPresets,
              controller: _accentHexCtrl,
              label: 'Акцент (#RRGGBB)',
            ),
            const SizedBox(height: 8),
            TextField(
              controller: _fgHexCtrl,
              decoration: const InputDecoration(
                labelText: 'Цвет текста (#RRGGBB)',
                border: OutlineInputBorder(),
              ),
            ),
            const SizedBox(height: 10),
            Row(
              children: [
                Text('Шрифт: $_fontSizeSp sp'),
                Expanded(
                  child: Slider(
                    min: 10,
                    max: 48,
                    divisions: 38,
                    value: _fontSizeSp.toDouble(),
                    onChanged: (val) =>
                        setState(() => _fontSizeSp = val.round()),
                  ),
                ),
              ],
            ),
            Row(
              children: [
                Text('Скругление: $_cornerRadius px'),
                Expanded(
                  child: Slider(
                    min: 0,
                    max: 32,
                    divisions: 32,
                    value: _cornerRadius.toDouble(),
                    onChanged: (val) =>
                        setState(() => _cornerRadius = val.round()),
                  ),
                ),
              ],
            ),
            const SizedBox(height: 8),
            Row(
              children: [
                Expanded(
                  child: FilledButton(
                    key: const Key('widget_editor_save_button'),
                    onPressed: _save,
                    child: const Text('Сохранить'),
                  ),
                ),
              ],
            ),
          ],
        ),
      ),
    );
  }

  static String _actionKey(WidgetAction action) => switch (action.type) {
    WidgetActionType.none => 'none',
    WidgetActionType.heartbeat => 'heartbeat',
    WidgetActionType.control => action.controlAction?.wireName ?? 'none',
  };

  static WidgetAction _actionFromKey(String key) {
    if (key == 'none') {
      return const WidgetAction.none();
    }
    if (key == 'heartbeat') {
      return const WidgetAction.heartbeat();
    }
    return WidgetAction.control(ControlAction.fromWireName(key));
  }
}

class _ColorPresetRow extends StatelessWidget {
  const _ColorPresetRow({
    required this.presets,
    required this.controller,
    required this.label,
  });

  final List<String> presets;
  final TextEditingController controller;
  final String label;

  @override
  Widget build(BuildContext context) {
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Wrap(
          spacing: 8,
          runSpacing: 6,
          children: [
            for (final hex in presets)
              InkWell(
                onTap: () => controller.text = hex,
                borderRadius: BorderRadius.circular(14),
                child: Container(
                  width: 26,
                  height: 26,
                  decoration: BoxDecoration(
                    color: parsePanelHexColor(hex),
                    shape: BoxShape.circle,
                    border: Border.all(color: Colors.white54),
                  ),
                ),
              ),
          ],
        ),
        const SizedBox(height: 6),
        TextField(
          controller: controller,
          decoration: InputDecoration(
            labelText: label,
            border: const OutlineInputBorder(),
            isDense: true,
          ),
        ),
      ],
    );
  }
}

/// Opens a dialog to create a new panel or configure the active panel's grid/cadence.
Future<void> showPanelMetaDialog({
  required BuildContext context,
  required DeckController deckController,
  required bool isCreate,
}) {
  return showDialog<void>(
    context: context,
    builder: (ctx) =>
        _PanelMetaDialog(deckController: deckController, isCreate: isCreate),
  );
}

class _PanelMetaDialog extends StatefulWidget {
  const _PanelMetaDialog({
    required this.deckController,
    required this.isCreate,
  });

  final DeckController deckController;
  final bool isCreate;

  @override
  State<_PanelMetaDialog> createState() => _PanelMetaDialogState();
}

class _PanelMetaDialogState extends State<_PanelMetaDialog> {
  late final TextEditingController _nameCtrl;
  late final TextEditingController _refreshCtrl;
  late int _columns;

  @override
  void initState() {
    super.initState();
    final active = widget.deckController.activePanel;
    _nameCtrl = TextEditingController(
      text: widget.isCreate ? 'Новая панель' : active.name,
    );
    _refreshCtrl = TextEditingController(
      text: widget.isCreate ? '1000' : active.refreshIntervalMs.toString(),
    );
    _columns = widget.isCreate ? 4 : active.columns;
  }

  @override
  void dispose() {
    _nameCtrl.dispose();
    _refreshCtrl.dispose();
    super.dispose();
  }

  Future<void> _submit() async {
    final refreshMs = int.tryParse(_refreshCtrl.text.trim()) ?? -1;
    final ok = widget.isCreate
        ? await widget.deckController.addPanel(
            name: _nameCtrl.text,
            columns: _columns,
            refreshIntervalMs: refreshMs,
          )
        : await widget.deckController.updateActivePanelMeta(
            name: _nameCtrl.text,
            columns: _columns,
            refreshIntervalMs: refreshMs,
          );
    if (ok && mounted) {
      Navigator.of(context).pop();
    }
  }

  @override
  Widget build(BuildContext context) {
    final canDelete =
        !widget.isCreate && widget.deckController.deck.panels.length > 1;
    return AlertDialog(
      title: Text(widget.isCreate ? 'Новая панель' : 'Параметры панели'),
      content: SingleChildScrollView(
        child: Column(
          mainAxisSize: MainAxisSize.min,
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            TextField(
              key: const Key('panel_meta_name_field'),
              controller: _nameCtrl,
              decoration: const InputDecoration(
                labelText: 'Название панели',
                border: OutlineInputBorder(),
              ),
            ),
            const SizedBox(height: 12),
            Text('Колонок в сетке: $_columns'),
            Slider(
              min: minPanelColumns.toDouble(),
              max: maxPanelColumns.toDouble(),
              divisions: maxPanelColumns - minPanelColumns,
              value: _columns.toDouble(),
              onChanged: (v) => setState(() => _columns = v.round()),
            ),
            TextField(
              key: const Key('panel_meta_refresh_field'),
              controller: _refreshCtrl,
              keyboardType: TextInputType.number,
              decoration: const InputDecoration(
                labelText: 'Интервал обновления (100..60000 мс)',
                border: OutlineInputBorder(),
              ),
            ),
          ],
        ),
      ),
      actions: [
        if (canDelete)
          TextButton(
            key: const Key('panel_meta_delete_button'),
            onPressed: () async {
              final ok = await widget.deckController.deleteActivePanel();
              if (ok && context.mounted) {
                Navigator.of(context).pop();
              }
            },
            child: Text(
              'Удалить панель',
              style: TextStyle(color: Theme.of(context).colorScheme.error),
            ),
          ),
        TextButton(
          onPressed: () => Navigator.of(context).pop(),
          child: const Text('Отмена'),
        ),
        FilledButton(
          key: const Key('panel_meta_submit_button'),
          onPressed: _submit,
          child: const Text('Сохранить'),
        ),
      ],
    );
  }
}

/// Opens a dialog for JSON import/export (`DeckConfig` or single `PanelConfig`) and reset.
Future<void> showJsonImportExportDialog({
  required BuildContext context,
  required DeckController deckController,
}) {
  return showDialog<void>(
    context: context,
    builder: (ctx) => _JsonImportExportDialog(deckController: deckController),
  );
}

class _JsonImportExportDialog extends StatefulWidget {
  const _JsonImportExportDialog({required this.deckController});

  final DeckController deckController;

  @override
  State<_JsonImportExportDialog> createState() =>
      _JsonImportExportDialogState();
}

class _JsonImportExportDialogState extends State<_JsonImportExportDialog> {
  late final TextEditingController _jsonCtrl;
  String? _localNote;

  @override
  void initState() {
    super.initState();
    _jsonCtrl = TextEditingController(
      text: widget.deckController.exportDeckJson(),
    );
  }

  @override
  void dispose() {
    _jsonCtrl.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    return AlertDialog(
      title: const Text('Импорт и экспорт JSON'),
      content: SizedBox(
        width: 420,
        child: SingleChildScrollView(
          child: Column(
            mainAxisSize: MainAxisSize.min,
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [
              Wrap(
                spacing: 8,
                runSpacing: 6,
                children: [
                  OutlinedButton(
                    onPressed: () {
                      setState(() {
                        _jsonCtrl.text = widget.deckController.exportDeckJson();
                        _localNote = 'Выгружен весь Deck JSON.';
                      });
                    },
                    child: const Text('Экспорт Deck'),
                  ),
                  OutlinedButton(
                    onPressed: () {
                      setState(() {
                        _jsonCtrl.text = widget.deckController
                            .exportActivePanelJson();
                        _localNote = 'Выгружена текущая панель.';
                      });
                    },
                    child: const Text('Экспорт панели'),
                  ),
                  OutlinedButton(
                    onPressed: () async {
                      await Clipboard.setData(
                        ClipboardData(text: _jsonCtrl.text),
                      );
                      if (mounted) {
                        setState(() => _localNote = 'Скопировано в буфер.');
                      }
                    },
                    child: const Text('Копировать'),
                  ),
                ],
              ),
              const SizedBox(height: 8),
              TextField(
                key: const Key('panel_json_dialog_field'),
                controller: _jsonCtrl,
                maxLines: 8,
                style: const TextStyle(fontFamily: 'monospace', fontSize: 12),
                decoration: const InputDecoration(
                  border: OutlineInputBorder(),
                  labelText: 'Конфигурация JSON',
                ),
              ),
              if (_localNote != null) ...[
                const SizedBox(height: 6),
                Text(_localNote!, style: Theme.of(context).textTheme.bodySmall),
              ],
            ],
          ),
        ),
      ),
      actions: [
        TextButton(
          onPressed: () async {
            final ok = await widget.deckController.resetToStarter();
            if (ok && context.mounted) {
              Navigator.of(context).pop();
            }
          },
          child: const Text('Сбросить'),
        ),
        TextButton(
          onPressed: () async {
            final ok = await widget.deckController.importPanelJson(
              _jsonCtrl.text,
            );
            if (ok && context.mounted) {
              Navigator.of(context).pop();
            }
          },
          child: const Text('Импорт панели'),
        ),
        FilledButton(
          key: const Key('import_deck_json_submit_button'),
          onPressed: () async {
            final ok = await widget.deckController.importDeckJson(
              _jsonCtrl.text,
            );
            if (ok && context.mounted) {
              Navigator.of(context).pop();
            }
          },
          child: const Text('Импорт Deck'),
        ),
      ],
    );
  }
}
