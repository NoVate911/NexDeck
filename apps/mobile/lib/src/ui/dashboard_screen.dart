import 'package:material_ui/material_ui.dart';

import '../models/monitoring_models.dart';
import '../models/protocol_models.dart';
import '../state/agent_controller.dart';
import '../storage/session_store.dart';
import '../transport/transport_client.dart';
import 'formatters.dart';
import 'telemetry_chart.dart';

/// Connected host telemetry and remote media control screen for Stage 2.
class DashboardScreen extends StatelessWidget {
  /// Creates a [DashboardScreen].
  const DashboardScreen({required this.controller, super.key});

  /// Controller supplying active [HostSession], transport status, metrics, and control actions.
  final AgentController controller;

  @override
  Widget build(BuildContext context) {
    final session = controller.session;
    if (session == null) {
      return const SizedBox.shrink();
    }

    final snapshot = controller.latestSnapshot;
    final state = controller.connectionState;

    return Scaffold(
      appBar: AppBar(
        title: const Text('NexDeck — Мониторинг ПК'),
        actions: [
          IconButton(
            key: const Key('heartbeat_button'),
            tooltip: 'Отправить Heartbeat',
            onPressed: state == TransportConnectionState.connected
                ? () => controller.sendHeartbeat()
                : null,
            icon: const Icon(Icons.monitor_heart_outlined),
          ),
          IconButton(
            key: const Key('unpair_button'),
            tooltip: 'Отвязать хост',
            onPressed: controller.unpairHost,
            icon: const Icon(Icons.link_off),
          ),
        ],
      ),
      body: SafeArea(
        child: ListView(
          padding: const EdgeInsets.all(16),
          children: [
            _ConnectionBanner(session: session, controller: controller),
            const SizedBox(height: 12),
            _MediaControlsCard(controller: controller),
            const SizedBox(height: 12),
            if (snapshot == null)
              const Card(
                child: Padding(
                  padding: EdgeInsets.all(20),
                  child: Text(
                    'Ожидание первого снимка телеметрии от nexdeck-agent…',
                  ),
                ),
              )
            else ...[
              _CpuCard(
                cpu: snapshot.cpu,
                uptimeSecs: snapshot.uptimeSecs,
                history: controller.cpuHistory,
              ),
              const SizedBox(height: 12),
              _MemoryCard(
                memory: snapshot.memory,
                history: controller.ramHistory,
              ),
              if (snapshot.gpus.isNotEmpty) ...[
                const SizedBox(height: 12),
                _GpuSection(gpus: snapshot.gpus),
              ],
              if (snapshot.disks.isNotEmpty) ...[
                const SizedBox(height: 12),
                _DiskSection(disks: snapshot.disks),
              ],
              if (snapshot.networks.isNotEmpty) ...[
                const SizedBox(height: 12),
                _NetworkSection(
                  networks: snapshot.networks,
                  rxHistoryKib: controller.networkRxKibHistory,
                  txHistoryKib: controller.networkTxKibHistory,
                ),
              ],
              if (snapshot.battery != null) ...[
                const SizedBox(height: 12),
                _BatteryCard(battery: snapshot.battery!),
              ],
            ],
          ],
        ),
      ),
    );
  }
}

class _ConnectionBanner extends StatelessWidget {
  const _ConnectionBanner({required this.session, required this.controller});

  final HostSession session;
  final AgentController controller;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final state = controller.connectionState;
    final (label, color) = switch (state) {
      TransportConnectionState.connected => ('Подключено', Colors.green),
      TransportConnectionState.connecting => ('Подключение…', Colors.amber),
      TransportConnectionState.reconnecting => (
        'Переподключение…',
        Colors.orange,
      ),
      TransportConnectionState.disconnected => ('Отключено', Colors.redAccent),
    };

    return Card(
      child: Padding(
        padding: const EdgeInsets.all(16),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Row(
              children: [
                Container(
                  width: 10,
                  height: 10,
                  decoration: BoxDecoration(
                    color: color,
                    shape: BoxShape.circle,
                  ),
                ),
                const SizedBox(width: 8),
                Text(label, style: theme.textTheme.titleMedium),
                const Spacer(),
                if (state == TransportConnectionState.disconnected)
                  TextButton(
                    key: const Key('reconnect_button'),
                    onPressed: controller.connectActiveSession,
                    child: const Text('Подключиться'),
                  )
                else
                  TextButton(
                    key: const Key('disconnect_button'),
                    onPressed: controller.disconnect,
                    child: const Text('Отключить'),
                  ),
              ],
            ),
            const SizedBox(height: 6),
            Text(
              'Хост: ${session.host}:${session.port} (${session.deviceName})',
            ),
            Text(
              'TLS SHA-256: ${shortenFingerprint(session.fingerprintSha256)}',
              style: theme.textTheme.bodySmall,
            ),
            if (controller.statusMessage != null) ...[
              const SizedBox(height: 4),
              Text(controller.statusMessage!, style: theme.textTheme.bodySmall),
            ],
            if (controller.errorMessage != null) ...[
              const SizedBox(height: 6),
              Text(
                controller.errorMessage!,
                style: TextStyle(color: theme.colorScheme.error),
              ),
            ],
          ],
        ),
      ),
    );
  }
}

class _MediaControlsCard extends StatelessWidget {
  const _MediaControlsCard({required this.controller});

  final AgentController controller;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final enabled =
        controller.connectionState == TransportConnectionState.connected &&
        !controller.isSendingControlCommand;
    final lastResult = controller.lastControlResult;

    return Card(
      child: Padding(
        padding: const EdgeInsets.all(16),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Text(
              'Управление мультимедиа и громкостью',
              style: theme.textTheme.titleMedium,
            ),
            const SizedBox(height: 10),
            Wrap(
              spacing: 8,
              runSpacing: 8,
              children: [
                _ControlButton(
                  buttonKey: const Key('control_prev_track'),
                  icon: Icons.skip_previous,
                  label: ControlAction.mediaPreviousTrack.displayLabel,
                  onPressed: enabled
                      ? () => controller.sendControlAction(
                          ControlAction.mediaPreviousTrack,
                        )
                      : null,
                ),
                _ControlButton(
                  buttonKey: const Key('control_play_pause'),
                  icon: Icons.play_arrow,
                  label: ControlAction.mediaPlayPause.displayLabel,
                  onPressed: enabled
                      ? () => controller.sendControlAction(
                          ControlAction.mediaPlayPause,
                        )
                      : null,
                ),
                _ControlButton(
                  buttonKey: const Key('control_next_track'),
                  icon: Icons.skip_next,
                  label: ControlAction.mediaNextTrack.displayLabel,
                  onPressed: enabled
                      ? () => controller.sendControlAction(
                          ControlAction.mediaNextTrack,
                        )
                      : null,
                ),
                _ControlButton(
                  buttonKey: const Key('control_volume_down'),
                  icon: Icons.volume_down,
                  label: ControlAction.volumeDown.displayLabel,
                  onPressed: enabled
                      ? () => controller.sendControlAction(
                          ControlAction.volumeDown,
                        )
                      : null,
                ),
                _ControlButton(
                  buttonKey: const Key('control_volume_mute'),
                  icon: Icons.volume_off,
                  label: ControlAction.volumeMuteToggle.displayLabel,
                  onPressed: enabled
                      ? () => controller.sendControlAction(
                          ControlAction.volumeMuteToggle,
                        )
                      : null,
                ),
                _ControlButton(
                  buttonKey: const Key('control_volume_up'),
                  icon: Icons.volume_up,
                  label: ControlAction.volumeUp.displayLabel,
                  onPressed: enabled
                      ? () =>
                            controller.sendControlAction(ControlAction.volumeUp)
                      : null,
                ),
              ],
            ),
            if (lastResult != null) ...[
              const SizedBox(height: 8),
              Text(
                lastResult.ok
                    ? 'Последняя команда: ${lastResult.action.displayLabel} (${lastResult.message})'
                    : 'Ошибка команды ${lastResult.action.displayLabel}: ${lastResult.message}',
                style: theme.textTheme.bodySmall,
              ),
            ],
          ],
        ),
      ),
    );
  }
}

class _ControlButton extends StatelessWidget {
  const _ControlButton({
    required this.buttonKey,
    required this.icon,
    required this.label,
    required this.onPressed,
  });

  final Key buttonKey;
  final IconData icon;
  final String label;
  final VoidCallback? onPressed;

  @override
  Widget build(BuildContext context) {
    return OutlinedButton.icon(
      key: buttonKey,
      onPressed: onPressed,
      icon: Icon(icon, size: 18),
      label: Text(label),
    );
  }
}

class _CpuCard extends StatefulWidget {
  const _CpuCard({
    required this.cpu,
    required this.uptimeSecs,
    required this.history,
  });

  final CpuMetrics cpu;
  final int uptimeSecs;
  final List<double> history;

  @override
  State<_CpuCard> createState() => _CpuCardState();
}

class _CpuCardState extends State<_CpuCard> {
  bool _showPerCore = false;

  @override
  Widget build(BuildContext context) {
    final cpu = widget.cpu;
    final pct = cpu.totalUsagePct.clamp(0.0, 100.0);
    final theme = Theme.of(context);

    return Card(
      child: Padding(
        padding: const EdgeInsets.all(16),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Row(
              children: [
                Text(
                  'CPU: ${pct.toStringAsFixed(1)}%',
                  style: theme.textTheme.titleMedium,
                ),
                const Spacer(),
                Text('Uptime: ${formatUptime(widget.uptimeSecs)}'),
              ],
            ),
            const SizedBox(height: 8),
            TelemetryLineChart(
              key: const Key('cpu_history_chart'),
              fixedMaxY: 100.0,
              unitSuffix: '%',
              series: [
                ChartSeries(
                  label: 'Загрузка CPU',
                  color: theme.colorScheme.primary,
                  values: widget.history,
                ),
              ],
            ),
            const SizedBox(height: 10),
            Wrap(
              spacing: 16,
              runSpacing: 4,
              crossAxisAlignment: WrapCrossAlignment.center,
              children: [
                Text('Потоков: ${cpu.perCoreUsagePct.length}'),
                if (cpu.frequencyMhz != null)
                  Text('Частота: ${cpu.frequencyMhz} МГц'),
                if (cpu.temperatureCelsius != null)
                  Text(
                    'Температура: ${cpu.temperatureCelsius!.toStringAsFixed(1)} °C',
                  ),
                if (cpu.perCoreUsagePct.isNotEmpty)
                  TextButton(
                    key: const Key('toggle_cpu_cores_button'),
                    onPressed: () {
                      setState(() {
                        _showPerCore = !_showPerCore;
                      });
                    },
                    child: Text(
                      _showPerCore
                          ? 'Скрыть ядра'
                          : 'Показать ядра (${cpu.perCoreUsagePct.length})',
                    ),
                  ),
              ],
            ),
            if (_showPerCore && cpu.perCoreUsagePct.isNotEmpty) ...[
              const SizedBox(height: 8),
              Wrap(
                spacing: 8,
                runSpacing: 8,
                children: [
                  for (var i = 0; i < cpu.perCoreUsagePct.length; i++)
                    SizedBox(
                      width: 112,
                      child: Container(
                        padding: const EdgeInsets.all(8),
                        decoration: BoxDecoration(
                          color: theme.colorScheme.surfaceContainerLow,
                          borderRadius: BorderRadius.circular(8),
                        ),
                        child: Column(
                          crossAxisAlignment: CrossAxisAlignment.start,
                          children: [
                            Text(
                              'CPU $i: ${cpu.perCoreUsagePct[i].clamp(0.0, 100.0).toStringAsFixed(0)}%',
                              style: theme.textTheme.bodySmall,
                            ),
                            const SizedBox(height: 4),
                            LinearProgressIndicator(
                              value:
                                  cpu.perCoreUsagePct[i].clamp(0.0, 100.0) /
                                  100.0,
                            ),
                          ],
                        ),
                      ),
                    ),
                ],
              ),
            ],
          ],
        ),
      ),
    );
  }
}

class _MemoryCard extends StatelessWidget {
  const _MemoryCard({required this.memory, required this.history});

  final MemoryMetrics memory;
  final List<double> history;

  @override
  Widget build(BuildContext context) {
    final pct = memory.usagePct.clamp(0.0, 100.0);
    final theme = Theme.of(context);

    return Card(
      child: Padding(
        padding: const EdgeInsets.all(16),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Text(
              'RAM: ${pct.toStringAsFixed(1)}%',
              style: theme.textTheme.titleMedium,
            ),
            const SizedBox(height: 8),
            TelemetryLineChart(
              key: const Key('ram_history_chart'),
              fixedMaxY: 100.0,
              unitSuffix: '%',
              series: [
                ChartSeries(
                  label: 'Занято RAM',
                  color: Colors.tealAccent,
                  values: history,
                ),
              ],
            ),
            const SizedBox(height: 8),
            Text(
              'Занято ${formatBytes(memory.usedBytes)} из ${formatBytes(memory.totalBytes)} (свободно ${formatBytes(memory.freeBytes)})',
            ),
          ],
        ),
      ),
    );
  }
}

class _GpuSection extends StatelessWidget {
  const _GpuSection({required this.gpus});

  final List<GpuMetrics> gpus;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return Card(
      child: Padding(
        padding: const EdgeInsets.all(16),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Text('GPU', style: theme.textTheme.titleMedium),
            const SizedBox(height: 8),
            for (final gpu in gpus) ...[
              Text(
                '${gpu.name}: ${gpu.usagePct?.toStringAsFixed(1) ?? '—'}%'
                '${gpu.clockMhz != null ? ' • ${gpu.clockMhz} МГц' : ''}'
                '${gpu.temperatureCelsius != null ? ' • ${gpu.temperatureCelsius!.toStringAsFixed(1)} °C' : ''}',
              ),
              if (gpu.vramUsedBytes != null &&
                  gpu.vramTotalBytes != null &&
                  gpu.vramTotalBytes! > 0) ...[
                const SizedBox(height: 4),
                Text(
                  'VRAM: ${formatBytes(gpu.vramUsedBytes!)} из ${formatBytes(gpu.vramTotalBytes!)}',
                  style: theme.textTheme.bodySmall,
                ),
                const SizedBox(height: 4),
                LinearProgressIndicator(
                  value: (gpu.vramUsedBytes! / gpu.vramTotalBytes!).clamp(
                    0.0,
                    1.0,
                  ),
                ),
              ],
              const SizedBox(height: 8),
            ],
          ],
        ),
      ),
    );
  }
}

class _DiskSection extends StatelessWidget {
  const _DiskSection({required this.disks});

  final List<DiskMetrics> disks;

  @override
  Widget build(BuildContext context) {
    return Card(
      child: Padding(
        padding: const EdgeInsets.all(16),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Text('Диски', style: Theme.of(context).textTheme.titleMedium),
            const SizedBox(height: 8),
            for (final disk in disks)
              Padding(
                padding: const EdgeInsets.only(bottom: 6),
                child: Text(
                  '${disk.mountPoint} — свободно ${formatBytes(disk.freeBytes)} из ${formatBytes(disk.totalBytes)} '
                  '(R: ${formatThroughput(disk.readBytesPerSec)}, W: ${formatThroughput(disk.writeBytesPerSec)})',
                ),
              ),
          ],
        ),
      ),
    );
  }
}

class _NetworkSection extends StatelessWidget {
  const _NetworkSection({
    required this.networks,
    required this.rxHistoryKib,
    required this.txHistoryKib,
  });

  final List<NetworkMetrics> networks;
  final List<double> rxHistoryKib;
  final List<double> txHistoryKib;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return Card(
      child: Padding(
        padding: const EdgeInsets.all(16),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Text('Сеть', style: theme.textTheme.titleMedium),
            const SizedBox(height: 8),
            TelemetryLineChart(
              key: const Key('network_history_chart'),
              unitSuffix: 'КиБ/с',
              series: [
                ChartSeries(
                  label: '↓ RX',
                  color: Colors.lightGreenAccent,
                  values: rxHistoryKib,
                ),
                ChartSeries(
                  label: '↑ TX',
                  color: Colors.amberAccent,
                  values: txHistoryKib,
                ),
              ],
            ),
            const SizedBox(height: 10),
            for (final net in networks)
              Padding(
                padding: const EdgeInsets.only(bottom: 6),
                child: Text(
                  '${net.interfaceName}: ↑ ${formatThroughput(net.txBytesPerSec)} • ↓ ${formatThroughput(net.rxBytesPerSec)} '
                  '(всего ↑ ${formatBytes(net.totalTxBytes)} / ↓ ${formatBytes(net.totalRxBytes)})',
                ),
              ),
          ],
        ),
      ),
    );
  }
}

class _BatteryCard extends StatelessWidget {
  const _BatteryCard({required this.battery});

  final BatteryMetrics battery;

  @override
  Widget build(BuildContext context) {
    final source = battery.acConnected ? 'от сети' : 'от батареи';
    return Card(
      child: Padding(
        padding: const EdgeInsets.all(16),
        child: Text(
          'Питание: ${battery.chargePct.toStringAsFixed(0)}% ($source)',
          style: Theme.of(context).textTheme.titleMedium,
        ),
      ),
    );
  }
}
