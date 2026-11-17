import 'dart:math' as math;

import 'package:material_ui/material_ui.dart';

/// Single plotted metric series inside a [TelemetryLineChart].
class ChartSeries {
  /// Creates a [ChartSeries].
  const ChartSeries({
    required this.label,
    required this.color,
    required this.values,
  });

  /// Legend caption shown above the chart.
  final String label;

  /// Line and subtle area tint color.
  final Color color;

  /// Ordered chronological samples (oldest to newest).
  final List<double> values;
}

/// Lightweight custom-painted real-time sparkline/area chart with zero third-party deps.
class TelemetryLineChart extends StatelessWidget {
  /// Creates a [TelemetryLineChart].
  const TelemetryLineChart({
    required this.series,
    this.fixedMaxY,
    this.unitSuffix = '%',
    this.height = 76,
    super.key,
  });

  /// One or more chronological data series drawn on the same scale.
  final List<ChartSeries> series;

  /// Fixed upper Y bound (e.g. `100` for percentages) or `null` for auto-scaling.
  final double? fixedMaxY;

  /// Suffix appended to legend peak/current labels.
  final String unitSuffix;

  /// Fixed chart height in logical pixels.
  final double height;

  double _resolvedMaxY() {
    if (fixedMaxY != null) {
      return fixedMaxY!;
    }
    var peak = 1.0;
    for (final s in series) {
      for (final value in s.values) {
        if (value.isFinite && value > peak) {
          peak = value;
        }
      }
    }
    return peak * 1.15;
  }

  @override
  Widget build(BuildContext context) {
    final maxY = _resolvedMaxY();
    final theme = Theme.of(context);

    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        Wrap(
          spacing: 14,
          runSpacing: 4,
          children: [
            for (final item in series)
              Row(
                mainAxisSize: MainAxisSize.min,
                children: [
                  Container(
                    width: 8,
                    height: 8,
                    decoration: BoxDecoration(
                      color: item.color,
                      shape: BoxShape.circle,
                    ),
                  ),
                  const SizedBox(width: 6),
                  Text(
                    '${item.label}: ${item.values.isEmpty ? '0.0' : item.values.last.toStringAsFixed(1)} $unitSuffix',
                    style: theme.textTheme.bodySmall,
                  ),
                ],
              ),
          ],
        ),
        const SizedBox(height: 6),
        Container(
          height: height,
          decoration: BoxDecoration(
            color: theme.colorScheme.surfaceContainerLowest,
            borderRadius: BorderRadius.circular(8),
            border: Border.all(color: theme.colorScheme.outlineVariant),
          ),
          clipBehavior: Clip.antiAlias,
          child: CustomPaint(
            painter: _TelemetryChartPainter(
              series: series,
              maxY: maxY,
              gridColor: theme.colorScheme.outlineVariant.withValues(
                alpha: 0.45,
              ),
            ),
            child: const SizedBox.expand(),
          ),
        ),
      ],
    );
  }
}

class _TelemetryChartPainter extends CustomPainter {
  const _TelemetryChartPainter({
    required this.series,
    required this.maxY,
    required this.gridColor,
  });

  final List<ChartSeries> series;
  final double maxY;
  final Color gridColor;

  @override
  void paint(Canvas canvas, Size size) {
    final gridPaint = Paint()
      ..color = gridColor
      ..strokeWidth = 1;

    for (final fraction in const [0.25, 0.5, 0.75]) {
      final y = size.height * fraction;
      canvas.drawLine(Offset(0, y), Offset(size.width, y), gridPaint);
    }

    final safeMaxY = maxY <= 0 ? 1.0 : maxY;
    for (final item in series) {
      if (item.values.isEmpty) {
        continue;
      }

      final linePath = Path();
      final fillPath = Path();
      final count = item.values.length;

      if (count == 1) {
        final onlyY =
            size.height -
            (item.values.first.clamp(0.0, safeMaxY) / safeMaxY) *
                (size.height - 6) -
            3;
        linePath.moveTo(0, onlyY);
        linePath.lineTo(size.width, onlyY);
        fillPath
          ..moveTo(0, size.height)
          ..lineTo(0, onlyY)
          ..lineTo(size.width, onlyY)
          ..lineTo(size.width, size.height)
          ..close();
      } else {
        for (var i = 0; i < count; i++) {
          final x = (i / math.max(1, count - 1)) * size.width;
          final clamped = item.values[i].clamp(0.0, safeMaxY);
          final y = size.height - (clamped / safeMaxY) * (size.height - 6) - 3;
          if (i == 0) {
            linePath.moveTo(x, y);
            fillPath.moveTo(x, size.height);
            fillPath.lineTo(x, y);
          } else {
            linePath.lineTo(x, y);
            fillPath.lineTo(x, y);
          }
        }
        fillPath
          ..lineTo(size.width, size.height)
          ..close();
      }

      final fillPaint = Paint()
        ..color = item.color.withValues(alpha: 0.16)
        ..style = PaintingStyle.fill;
      canvas.drawPath(fillPath, fillPaint);

      final linePaint = Paint()
        ..color = item.color
        ..strokeWidth = 2.0
        ..style = PaintingStyle.stroke
        ..strokeCap = StrokeCap.round
        ..strokeJoin = StrokeJoin.round;
      canvas.drawPath(linePath, linePaint);
    }
  }

  @override
  bool shouldRepaint(covariant _TelemetryChartPainter oldDelegate) {
    return oldDelegate.maxY != maxY ||
        oldDelegate.gridColor != gridColor ||
        oldDelegate.series != series;
  }
}
