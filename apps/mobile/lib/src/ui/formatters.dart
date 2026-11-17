/// Human-readable formatting helpers for telemetry values.
library;

/// Formats a byte count into B, KiB, MiB, GiB, or TiB.
String formatBytes(int bytes) {
  if (bytes < 1024) {
    return '$bytes Б';
  }
  const units = ['КиБ', 'МиБ', 'ГиБ', 'ТиБ'];
  var value = bytes.toDouble();
  var unitIndex = -1;
  while (value >= 1024.0 && unitIndex + 1 < units.length) {
    value /= 1024.0;
    unitIndex++;
  }
  return '${value.toStringAsFixed(1)} ${units[unitIndex]}';
}

/// Formats throughput bytes/sec into readable speed units.
String formatThroughput(int bytesPerSec) => '${formatBytes(bytesPerSec)}/с';

/// Formats uptime seconds into `Xd Xh Xm Xs`.
String formatUptime(int totalSeconds) {
  final clamped = totalSeconds < 0 ? 0 : totalSeconds;
  final days = clamped ~/ 86400;
  final hours = (clamped % 86400) ~/ 3600;
  final minutes = (clamped % 3600) ~/ 60;
  final seconds = clamped % 60;
  if (days > 0) {
    return '$daysд $hoursч $minutesм';
  }
  if (hours > 0) {
    return '$hoursч $minutesм';
  }
  if (minutes > 0) {
    return '$minutesм $secondsс';
  }
  return '$secondsс';
}

/// Returns a short display prefix/suffix of a 64-char SHA-256 fingerprint.
String shortenFingerprint(String fingerprint) {
  if (fingerprint.length <= 16) {
    return fingerprint;
  }
  return '${fingerprint.substring(0, 8)}…${fingerprint.substring(fingerprint.length - 8)}';
}
