import 'json_helpers.dart';

/// Processor telemetry snapshot matching `CpuMetrics` from `crates/monitoring`.
class CpuMetrics {
  /// Creates a [CpuMetrics] snapshot.
  const CpuMetrics({
    required this.totalUsagePct,
    required this.perCoreUsagePct,
    this.frequencyMhz,
    this.temperatureCelsius,
  });

  /// Decodes a JSON map produced by `nexdeck-agent`.
  factory CpuMetrics.fromJson(Map<String, Object?> json) {
    return CpuMetrics(
      totalUsagePct: readRequiredDouble(json, 'total_usage_pct'),
      perCoreUsagePct: readDoubleList(json, 'per_core_usage_pct'),
      frequencyMhz: readOptional<int>(json, 'frequency_mhz'),
      temperatureCelsius: readOptionalDouble(json, 'temperature_celsius'),
    );
  }

  /// Total package utilization percentage (`0.0..=100.0`).
  final double totalUsagePct;

  /// Per-logical-core utilization percentages.
  final List<double> perCoreUsagePct;

  /// Current clock frequency in MHz, if available.
  final int? frequencyMhz;

  /// Package temperature in Celsius, if exposed by sensors.
  final double? temperatureCelsius;
}

/// System RAM telemetry snapshot matching `MemoryMetrics`.
class MemoryMetrics {
  /// Creates a [MemoryMetrics] snapshot.
  const MemoryMetrics({required this.totalBytes, required this.usedBytes});

  /// Decodes a JSON map produced by `nexdeck-agent`.
  factory MemoryMetrics.fromJson(Map<String, Object?> json) {
    return MemoryMetrics(
      totalBytes: readRequired<int>(json, 'total_bytes'),
      usedBytes: readRequired<int>(json, 'used_bytes'),
    );
  }

  /// Total physical RAM in bytes.
  final int totalBytes;

  /// Currently used RAM in bytes.
  final int usedBytes;

  /// Available RAM in bytes using saturating subtraction.
  int get freeBytes => totalBytes > usedBytes ? totalBytes - usedBytes : 0;

  /// Memory utilization percentage (`0.0..=100.0`).
  double get usagePct {
    if (totalBytes <= 0) {
      return 0.0;
    }
    final clampedUsed = usedBytes.clamp(0, totalBytes);
    return (clampedUsed / totalBytes) * 100.0;
  }
}

/// Graphics adapter telemetry snapshot matching `GpuMetrics`.
class GpuMetrics {
  /// Creates a [GpuMetrics] snapshot.
  const GpuMetrics({
    required this.name,
    this.usagePct,
    this.temperatureCelsius,
    this.clockMhz,
    this.vramUsedBytes,
    this.vramTotalBytes,
  });

  /// Decodes a JSON map produced by `nexdeck-agent`.
  factory GpuMetrics.fromJson(Map<String, Object?> json) {
    return GpuMetrics(
      name: readRequired<String>(json, 'name'),
      usagePct: readOptionalDouble(json, 'usage_pct'),
      temperatureCelsius: readOptionalDouble(json, 'temperature_celsius'),
      clockMhz: readOptional<int>(json, 'clock_mhz'),
      vramUsedBytes: readOptional<int>(json, 'vram_used_bytes'),
      vramTotalBytes: readOptional<int>(json, 'vram_total_bytes'),
    );
  }

  /// Adapter display name.
  final String name;

  /// Core load percentage (`0.0..=100.0`), if available.
  final double? usagePct;

  /// Core temperature in Celsius, if available.
  final double? temperatureCelsius;

  /// Core clock in MHz, if available.
  final int? clockMhz;

  /// Used VRAM in bytes, if available.
  final int? vramUsedBytes;

  /// Total VRAM in bytes, if available.
  final int? vramTotalBytes;
}

/// Storage volume telemetry snapshot matching `DiskMetrics`.
class DiskMetrics {
  /// Creates a [DiskMetrics] snapshot.
  const DiskMetrics({
    required this.mountPoint,
    required this.totalBytes,
    required this.freeBytes,
    required this.readBytesPerSec,
    required this.writeBytesPerSec,
  });

  /// Decodes a JSON map produced by `nexdeck-agent`.
  factory DiskMetrics.fromJson(Map<String, Object?> json) {
    return DiskMetrics(
      mountPoint: readRequired<String>(json, 'mount_point'),
      totalBytes: readRequired<int>(json, 'total_bytes'),
      freeBytes: readRequired<int>(json, 'free_bytes'),
      readBytesPerSec: readRequired<int>(json, 'read_bytes_per_sec'),
      writeBytesPerSec: readRequired<int>(json, 'write_bytes_per_sec'),
    );
  }

  /// Drive letter or mount point (e.g., `C:\`).
  final String mountPoint;

  /// Total capacity in bytes.
  final int totalBytes;

  /// Free capacity in bytes.
  final int freeBytes;

  /// Current read throughput in bytes/sec.
  final int readBytesPerSec;

  /// Current write throughput in bytes/sec.
  final int writeBytesPerSec;
}

/// Network interface telemetry snapshot matching `NetworkMetrics`.
class NetworkMetrics {
  /// Creates a [NetworkMetrics] snapshot.
  const NetworkMetrics({
    required this.interfaceName,
    required this.txBytesPerSec,
    required this.rxBytesPerSec,
    required this.totalTxBytes,
    required this.totalRxBytes,
  });

  /// Decodes a JSON map produced by `nexdeck-agent`.
  factory NetworkMetrics.fromJson(Map<String, Object?> json) {
    return NetworkMetrics(
      interfaceName: readRequired<String>(json, 'interface_name'),
      txBytesPerSec: readRequired<int>(json, 'tx_bytes_per_sec'),
      rxBytesPerSec: readRequired<int>(json, 'rx_bytes_per_sec'),
      totalTxBytes: readRequired<int>(json, 'total_tx_bytes'),
      totalRxBytes: readRequired<int>(json, 'total_rx_bytes'),
    );
  }

  /// Network adapter display name.
  final String interfaceName;

  /// Upload throughput in bytes/sec.
  final int txBytesPerSec;

  /// Download throughput in bytes/sec.
  final int rxBytesPerSec;

  /// Cumulative transmitted bytes.
  final int totalTxBytes;

  /// Cumulative received bytes.
  final int totalRxBytes;
}

/// Battery telemetry snapshot matching `BatteryMetrics`.
class BatteryMetrics {
  /// Creates a [BatteryMetrics] snapshot.
  const BatteryMetrics({required this.chargePct, required this.acConnected});

  /// Decodes a JSON map produced by `nexdeck-agent`.
  factory BatteryMetrics.fromJson(Map<String, Object?> json) {
    return BatteryMetrics(
      chargePct: readRequiredDouble(json, 'charge_pct'),
      acConnected: readRequired<bool>(json, 'ac_connected'),
    );
  }

  /// Charge percentage (`0.0..=100.0`).
  final double chargePct;

  /// Whether AC power is connected.
  final bool acConnected;
}

/// Aggregated hardware telemetry sample matching `SystemSnapshot`.
class SystemSnapshot {
  /// Creates a [SystemSnapshot].
  const SystemSnapshot({
    required this.capturedAtEpochMs,
    required this.uptimeSecs,
    required this.cpu,
    required this.memory,
    required this.gpus,
    required this.disks,
    required this.networks,
    this.battery,
  });

  /// Decodes a JSON map produced by `nexdeck-agent`.
  factory SystemSnapshot.fromJson(Map<String, Object?> json) {
    final rawBattery = json['battery'];
    return SystemSnapshot(
      capturedAtEpochMs: readRequired<int>(json, 'captured_at_epoch_ms'),
      uptimeSecs: readRequired<int>(json, 'uptime_secs'),
      cpu: CpuMetrics.fromJson(readRequiredMap(json, 'cpu')),
      memory: MemoryMetrics.fromJson(readRequiredMap(json, 'memory')),
      gpus: [
        for (final item in readObjectList(json, 'gpus'))
          GpuMetrics.fromJson(item),
      ],
      disks: [
        for (final item in readObjectList(json, 'disks'))
          DiskMetrics.fromJson(item),
      ],
      networks: [
        for (final item in readObjectList(json, 'networks'))
          NetworkMetrics.fromJson(item),
      ],
      battery: rawBattery == null
          ? null
          : BatteryMetrics.fromJson(asJsonMap(rawBattery, context: 'battery')),
    );
  }

  /// Capture timestamp in Unix epoch milliseconds.
  final int capturedAtEpochMs;

  /// Host uptime in seconds.
  final int uptimeSecs;

  /// CPU telemetry.
  final CpuMetrics cpu;

  /// Memory telemetry.
  final MemoryMetrics memory;

  /// Detected GPUs.
  final List<GpuMetrics> gpus;

  /// Detected storage volumes.
  final List<DiskMetrics> disks;

  /// Active network interfaces.
  final List<NetworkMetrics> networks;

  /// Optional battery status.
  final BatteryMetrics? battery;
}
