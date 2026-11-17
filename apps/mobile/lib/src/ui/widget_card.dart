import 'package:material_ui/material_ui.dart';

import '../models/monitoring_models.dart';
import '../models/panel_models.dart';
import '../state/deck_controller.dart';
import 'formatters.dart';
import 'telemetry_chart.dart';

/// Parses `#RRGGBB` or `#AARRGGBB` hex colors used by [WidgetStyle].
Color parsePanelHexColor(
  String hex, {
  Color fallback = const Color(0xFF1E293B),
}) {
  if (!hex.startsWith('#')) {
    return fallback;
  }
  final digits = hex.substring(1);
  if (digits.length == 6) {
    final parsed = int.tryParse(digits, radix: 16);
    return parsed == null ? fallback : Color(0xFF000000 | parsed);
  }
  if (digits.length == 8) {
    final parsed = int.tryParse(digits, radix: 16);
    return parsed == null ? fallback : Color(parsed);
  }
  return fallback;
}

/// Maps curated cross-platform [WidgetIcon] identifiers to Material icons.
IconData widgetIconToIconData(WidgetIcon icon) => switch (icon) {
  WidgetIcon.cpu => Icons.memory,
  WidgetIcon.memory => Icons.sd_storage_outlined,
  WidgetIcon.gpu => Icons.videogame_asset_outlined,
  WidgetIcon.disk => Icons.storage,
  WidgetIcon.network => Icons.wifi,
  WidgetIcon.battery => Icons.battery_charging_full,
  WidgetIcon.playPause => Icons.play_arrow_rounded,
  WidgetIcon.previousTrack => Icons.skip_previous,
  WidgetIcon.nextTrack => Icons.skip_next,
  WidgetIcon.volumeUp => Icons.volume_up,
  WidgetIcon.volumeDown => Icons.volume_down,
  WidgetIcon.volumeMute => Icons.volume_off,
  WidgetIcon.terminal => Icons.terminal,
  WidgetIcon.shield => Icons.verified_user_outlined,
};

/// Formats a bound hardware metric from [snapshot] for display inside a card.
String formatTelemetryValue(
  SystemSnapshot? snapshot,
  TelemetryBinding? binding,
) {
  if (binding == null) {
    return '—';
  }
  final numeric = extractTelemetryNumericValue(snapshot, binding);
  if (numeric == null) {
    return '—';
  }
  return switch (binding) {
    TelemetryBinding.cpuUsagePct ||
    TelemetryBinding.ramUsagePct ||
    TelemetryBinding.gpuUsagePct ||
    TelemetryBinding.gpuVramPct ||
    TelemetryBinding.batteryPct => '${numeric.toStringAsFixed(1)}%',
    TelemetryBinding.cpuFrequencyMhz => '${numeric.round()} МГц',
    TelemetryBinding.cpuTemperatureC => '${numeric.toStringAsFixed(1)} °C',
    TelemetryBinding.ramUsedBytes => formatBytes(numeric.round()),
    TelemetryBinding.networkRxBps ||
    TelemetryBinding.networkTxBps => formatThroughput(numeric.round()),
    TelemetryBinding.uptimeSecs => formatUptime(numeric.round()),
  };
}

/// Renders a single [WidgetSpec] inside the dynamic panel grid.
class DeckWidgetCard extends StatelessWidget {
  /// Creates a [DeckWidgetCard].
  const DeckWidgetCard({
    required this.spec,
    required this.snapshot,
    required this.history,
    required this.isConnected,
    required this.isEditMode,
    required this.isSelected,
    required this.isToggleActive,
    required this.onTap,
    super.key,
  });

  /// Declarative widget specification.
  final WidgetSpec spec;

  /// Latest throttled telemetry snapshot for this panel.
  final SystemSnapshot? snapshot;

  /// Numeric history samples for [WidgetKind.chart].
  final List<double> history;

  /// Whether WSS connection is active for dispatching actions.
  final bool isConnected;

  /// Whether grid layout editing mode is enabled.
  final bool isEditMode;

  /// Whether this card is currently selected in edit mode.
  final bool isSelected;

  /// Visual state for [WidgetKind.toggle] tiles.
  final bool isToggleActive;

  /// Invoked when tapping the widget card (selects in edit mode or triggers action live).
  final VoidCallback? onTap;

  @override
  Widget build(BuildContext context) {
    final bgColor = parsePanelHexColor(spec.style.backgroundHex);
    final fgColor = parsePanelHexColor(
      spec.style.foregroundHex,
      fallback: const Color(0xFFF8FAFC),
    );
    final accentColor = parsePanelHexColor(
      spec.style.accentHex,
      fallback: const Color(0xFF38BDF8),
    );
    final radius = BorderRadius.circular(spec.style.cornerRadius.toDouble());
    final interactive =
        isEditMode ||
        spec.kind == WidgetKind.button ||
        spec.kind == WidgetKind.toggle ||
        spec.action.type != WidgetActionType.none;

    return Material(
      color: bgColor,
      borderRadius: radius,
      clipBehavior: Clip.antiAlias,
      child: InkWell(
        key: Key('panel_widget_${spec.id}'),
        onTap: interactive ? onTap : null,
        borderRadius: radius,
        child: Container(
          decoration: BoxDecoration(
            borderRadius: radius,
            border: Border.all(
              color: isSelected
                  ? Colors.white
                  : accentColor.withValues(alpha: isEditMode ? 0.85 : 0.45),
              width: isSelected ? 2.4 : 1.1,
            ),
          ),
          padding: const EdgeInsets.all(10),
          child: _buildBody(context, fgColor, accentColor),
        ),
      ),
    );
  }

  Widget _buildBody(BuildContext context, Color fgColor, Color accentColor) {
    final titleFontSize = spec.style.fontSizeSp.toDouble().clamp(11.0, 22.0);
    final titleStyle = TextStyle(
      color: fgColor,
      fontSize: titleFontSize,
      fontWeight: FontWeight.w600,
    );
    final subtitleStyle = TextStyle(
      color: accentColor,
      fontSize: (titleFontSize - 2.0).clamp(10.0, 16.0),
      fontWeight: FontWeight.w500,
    );

    return switch (spec.kind) {
      WidgetKind.text => _buildTextBody(titleStyle, fgColor),
      WidgetKind.metricNumber => _buildMetricBody(titleStyle, accentColor),
      WidgetKind.gauge => _buildGaugeBody(titleStyle, accentColor),
      WidgetKind.chart => _buildChartBody(titleStyle, accentColor),
      WidgetKind.button => _buildButtonBody(
        titleStyle,
        subtitleStyle,
        accentColor,
      ),
      WidgetKind.toggle => _buildToggleBody(titleStyle, accentColor),
      WidgetKind.icon => _buildIconBody(titleStyle, accentColor),
      WidgetKind.image => _buildImageBody(titleStyle, accentColor),
      WidgetKind.custom => _buildCustomBody(
        titleStyle,
        subtitleStyle,
        accentColor,
      ),
    };
  }

  Widget _buildHeaderRow(TextStyle titleStyle, Color accentColor) {
    return Row(
      children: [
        if (spec.icon != null) ...[
          Icon(widgetIconToIconData(spec.icon!), size: 16, color: accentColor),
          const SizedBox(width: 6),
        ],
        Expanded(
          child: Text(
            spec.title,
            style: titleStyle,
            maxLines: 1,
            overflow: TextOverflow.ellipsis,
          ),
        ),
      ],
    );
  }

  Widget _buildTextBody(TextStyle titleStyle, Color fgColor) {
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Text(
          spec.title,
          style: titleStyle,
          maxLines: 1,
          overflow: TextOverflow.ellipsis,
        ),
        const SizedBox(height: 6),
        Expanded(
          child: Text(
            spec.text ?? '',
            style: TextStyle(
              color: fgColor.withValues(alpha: 0.85),
              fontSize: (titleStyle.fontSize ?? 14.0) - 1.5,
            ),
            overflow: TextOverflow.fade,
          ),
        ),
      ],
    );
  }

  Widget _buildMetricBody(TextStyle titleStyle, Color accentColor) {
    final valueText = formatTelemetryValue(snapshot, spec.telemetry);
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      mainAxisAlignment: MainAxisAlignment.spaceBetween,
      children: [
        _buildHeaderRow(titleStyle, accentColor),
        FittedBox(
          fit: BoxFit.scaleDown,
          alignment: Alignment.centerLeft,
          child: Text(
            valueText,
            style: TextStyle(
              color: accentColor,
              fontSize: ((titleStyle.fontSize ?? 14.0) + 6.0).clamp(16.0, 28.0),
              fontWeight: FontWeight.bold,
            ),
          ),
        ),
      ],
    );
  }

  Widget _buildGaugeBody(TextStyle titleStyle, Color accentColor) {
    final numeric = spec.telemetry == null
        ? null
        : extractTelemetryNumericValue(snapshot, spec.telemetry!);
    final progress = numeric == null ? 0.0 : (numeric / 100.0).clamp(0.0, 1.0);
    final valueText = formatTelemetryValue(snapshot, spec.telemetry);

    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      mainAxisAlignment: MainAxisAlignment.spaceBetween,
      children: [
        _buildHeaderRow(titleStyle, accentColor),
        Row(
          children: [
            Expanded(
              child: ClipRRect(
                borderRadius: BorderRadius.circular(6),
                child: LinearProgressIndicator(
                  value: progress,
                  minHeight: 8,
                  color: accentColor,
                  backgroundColor: accentColor.withValues(alpha: 0.2),
                ),
              ),
            ),
            const SizedBox(width: 8),
            Text(
              valueText,
              style: TextStyle(
                color: accentColor,
                fontWeight: FontWeight.bold,
                fontSize: 13,
              ),
            ),
          ],
        ),
      ],
    );
  }

  Widget _buildChartBody(TextStyle titleStyle, Color accentColor) {
    final binding = spec.telemetry ?? TelemetryBinding.cpuUsagePct;
    final unitSuffix = switch (binding) {
      TelemetryBinding.cpuUsagePct ||
      TelemetryBinding.ramUsagePct ||
      TelemetryBinding.gpuUsagePct ||
      TelemetryBinding.gpuVramPct ||
      TelemetryBinding.batteryPct => '%',
      TelemetryBinding.cpuFrequencyMhz => 'МГц',
      TelemetryBinding.cpuTemperatureC => '°C',
      TelemetryBinding.networkRxBps || TelemetryBinding.networkTxBps => 'B/s',
      TelemetryBinding.ramUsedBytes => 'B',
      TelemetryBinding.uptimeSecs => 'с',
    };
    final fixedMaxY = switch (binding) {
      TelemetryBinding.cpuUsagePct ||
      TelemetryBinding.ramUsagePct ||
      TelemetryBinding.gpuUsagePct ||
      TelemetryBinding.gpuVramPct ||
      TelemetryBinding.batteryPct => 100.0,
      _ => null,
    };

    return LayoutBuilder(
      builder: (context, constraints) {
        final chartHeight = (constraints.maxHeight - 54.0).clamp(26.0, 88.0);
        return Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            _buildHeaderRow(titleStyle, accentColor),
            const SizedBox(height: 4),
            TelemetryLineChart(
              series: [
                ChartSeries(
                  label: binding.displayLabel,
                  color: accentColor,
                  values: history,
                ),
              ],
              fixedMaxY: fixedMaxY,
              unitSuffix: unitSuffix,
              height: chartHeight,
            ),
          ],
        );
      },
    );
  }

  Widget _buildButtonBody(
    TextStyle titleStyle,
    TextStyle subtitleStyle,
    Color accentColor,
  ) {
    final enabled = isConnected || isEditMode;
    final actionText = switch (spec.action.type) {
      WidgetActionType.none => 'Без действия',
      WidgetActionType.control =>
        spec.action.controlAction?.displayLabel ?? 'Команда',
      WidgetActionType.heartbeat => 'Heartbeat',
    };

    return Opacity(
      opacity: enabled ? 1.0 : 0.65,
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        mainAxisAlignment: MainAxisAlignment.spaceBetween,
        children: [
          Row(
            children: [
              Icon(
                widgetIconToIconData(spec.icon ?? WidgetIcon.playPause),
                color: accentColor,
                size: 20,
              ),
              const SizedBox(width: 6),
              Expanded(
                child: Text(
                  spec.title,
                  style: titleStyle,
                  maxLines: 1,
                  overflow: TextOverflow.ellipsis,
                ),
              ),
            ],
          ),
          Text(
            actionText,
            style: subtitleStyle,
            maxLines: 1,
            overflow: TextOverflow.ellipsis,
          ),
        ],
      ),
    );
  }

  Widget _buildToggleBody(TextStyle titleStyle, Color accentColor) {
    return Row(
      children: [
        if (spec.icon != null) ...[
          Icon(widgetIconToIconData(spec.icon!), color: accentColor, size: 20),
          const SizedBox(width: 6),
        ],
        Expanded(
          child: Text(
            spec.title,
            style: titleStyle,
            maxLines: 2,
            overflow: TextOverflow.ellipsis,
          ),
        ),
        Container(
          padding: const EdgeInsets.symmetric(horizontal: 8, vertical: 3),
          decoration: BoxDecoration(
            color: isToggleActive
                ? accentColor
                : accentColor.withValues(alpha: 0.18),
            borderRadius: BorderRadius.circular(12),
          ),
          child: Text(
            isToggleActive ? 'ВКЛ' : 'ВЫКЛ',
            style: TextStyle(
              fontSize: 11,
              fontWeight: FontWeight.bold,
              color: isToggleActive ? Colors.black : accentColor,
            ),
          ),
        ),
      ],
    );
  }

  Widget _buildIconBody(TextStyle titleStyle, Color accentColor) {
    return Column(
      mainAxisAlignment: MainAxisAlignment.center,
      children: [
        Icon(
          widgetIconToIconData(spec.icon ?? WidgetIcon.shield),
          color: accentColor,
          size: 28,
        ),
        const SizedBox(height: 4),
        Text(
          spec.title,
          style: titleStyle.copyWith(fontSize: 12),
          maxLines: 1,
          overflow: TextOverflow.ellipsis,
          textAlign: TextAlign.center,
        ),
      ],
    );
  }

  Widget _buildImageBody(TextStyle titleStyle, Color accentColor) {
    return Row(
      children: [
        Container(
          width: 42,
          height: 42,
          decoration: BoxDecoration(
            color: accentColor.withValues(alpha: 0.15),
            borderRadius: BorderRadius.circular(8),
            border: Border.all(color: accentColor.withValues(alpha: 0.5)),
          ),
          child: Icon(Icons.image_outlined, color: accentColor),
        ),
        const SizedBox(width: 10),
        Expanded(
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            mainAxisAlignment: MainAxisAlignment.center,
            children: [
              Text(
                spec.title,
                style: titleStyle,
                maxLines: 1,
                overflow: TextOverflow.ellipsis,
              ),
              Text(
                'Asset: ${spec.assetKey ?? 'example-banner'}',
                style: TextStyle(color: accentColor, fontSize: 11),
                maxLines: 1,
                overflow: TextOverflow.ellipsis,
              ),
            ],
          ),
        ),
      ],
    );
  }

  Widget _buildCustomBody(
    TextStyle titleStyle,
    TextStyle subtitleStyle,
    Color accentColor,
  ) {
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      mainAxisAlignment: MainAxisAlignment.spaceBetween,
      children: [
        _buildHeaderRow(titleStyle, accentColor),
        Text(
          'Плагин: ${spec.customComponent ?? 'org.example.widget'}',
          style: subtitleStyle,
          maxLines: 1,
          overflow: TextOverflow.ellipsis,
        ),
      ],
    );
  }
}
