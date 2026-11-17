import 'package:material_ui/material_ui.dart';

import '../models/monitoring_models.dart';
import '../state/agent_controller.dart';
import '../storage/session_store.dart';
import '../transport/transport_client.dart';
import 'formatters.dart';

/// Connected host telemetry overview screen for Stage 2.1.
class DashboardScreen extends StatelessWidget {
  /// Creates a [DashboardScreen].
  const DashboardScreen({required this.controller, super.key});

  /// Controller supplying active [HostSession], transport status, and metrics.
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
            const SizedBox(height: 16),
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
              _CpuCard(cpu: snapshot.cpu, uptimeSecs: snapshot.uptimeSecs),
              const SizedBox(height: 12),
              _MemoryCard(memory: snapshot.memory),
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
                _NetworkSection(networks: snapshot.networks),
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

class _CpuCard extends StatelessWidget {
  const _CpuCard({required this.cpu, required this.uptimeSecs});

  final CpuMetrics cpu;
  final int uptimeSecs;

  @override
  Widget build(BuildContext context) {
    final pct = cpu.totalUsagePct.clamp(0.0, 100.0);
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
                  style: Theme.of(context).textTheme.titleMedium,
                ),
                const Spacer(),
                Text('Uptime: ${formatUptime(uptimeSecs)}'),
              ],
            ),
            const SizedBox(height: 8),
            LinearProgressIndicator(value: pct / 100.0),
            const SizedBox(height: 8),
            Wrap(
              spacing: 16,
              children: [
                Text('Ядер: ${cpu.perCoreUsagePct.length}'),
                if (cpu.frequencyMhz != null)
                  Text('Частота: ${cpu.frequencyMhz} МГц'),
                if (cpu.temperatureCelsius != null)
                  Text(
                    'Температура: ${cpu.temperatureCelsius!.toStringAsFixed(1)} °C',
                  ),
              ],
            ),
          ],
        ),
      ),
    );
  }
}

class _MemoryCard extends StatelessWidget {
  const _MemoryCard({required this.memory});

  final MemoryMetrics memory;

  @override
  Widget build(BuildContext context) {
    final pct = memory.usagePct.clamp(0.0, 100.0);
    return Card(
      child: Padding(
        padding: const EdgeInsets.all(16),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Text(
              'RAM: ${pct.toStringAsFixed(1)}%',
              style: Theme.of(context).textTheme.titleMedium,
            ),
            const SizedBox(height: 8),
            LinearProgressIndicator(value: pct / 100.0),
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
    return Card(
      child: Padding(
        padding: const EdgeInsets.all(16),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Text('GPU', style: Theme.of(context).textTheme.titleMedium),
            const SizedBox(height: 8),
            for (final gpu in gpus)
              Padding(
                padding: const EdgeInsets.only(bottom: 6),
                child: Text(
                  '${gpu.name}: ${gpu.usagePct?.toStringAsFixed(1) ?? '—'}%'
                  '${gpu.temperatureCelsius != null ? ' • ${gpu.temperatureCelsius!.toStringAsFixed(1)} °C' : ''}',
                ),
              ),
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
  const _NetworkSection({required this.networks});

  final List<NetworkMetrics> networks;

  @override
  Widget build(BuildContext context) {
    return Card(
      child: Padding(
        padding: const EdgeInsets.all(16),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Text('Сеть', style: Theme.of(context).textTheme.titleMedium),
            const SizedBox(height: 8),
            for (final net in networks)
              Padding(
                padding: const EdgeInsets.only(bottom: 6),
                child: Text(
                  '${net.interfaceName}: ↑ ${formatThroughput(net.txBytesPerSec)} • ↓ ${formatThroughput(net.rxBytesPerSec)}',
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
