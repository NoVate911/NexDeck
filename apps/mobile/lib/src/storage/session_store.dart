import 'dart:convert';
import 'dart:io';

import '../models/json_helpers.dart';
import '../models/protocol_models.dart';

/// Persisted connection credentials for an authorized NexDeck host.
class HostSession {
  /// Creates a [HostSession].
  const HostSession({
    required this.host,
    required this.port,
    required this.fingerprintSha256,
    required this.deviceId,
    required this.deviceName,
    required this.deviceToken,
  });

  /// Decodes a persisted [HostSession] from JSON.
  factory HostSession.fromJson(Map<String, Object?> json) {
    final fingerprint = normalizeFingerprintSha256(
      readRequired<String>(json, 'fingerprint_sha256'),
    );
    if (!isValidFingerprintSha256(fingerprint)) {
      throw const FormatException('Invalid stored TLS fingerprint.');
    }
    return HostSession(
      host: readRequired<String>(json, 'host'),
      port: readRequired<int>(json, 'port'),
      fingerprintSha256: fingerprint,
      deviceId: readRequired<String>(json, 'device_id'),
      deviceName: readRequired<String>(json, 'device_name'),
      deviceToken: readRequired<String>(json, 'device_token'),
    );
  }

  /// Host IP or DNS name.
  final String host;

  /// Listening TCP port.
  final int port;

  /// Pinned SHA-256 hex digest of the host TLS certificate.
  final String fingerprintSha256;

  /// Paired device identifier.
  final String deviceId;

  /// Paired device display label.
  final String deviceName;

  /// Bearer token granted by `/pair`.
  final String deviceToken;

  /// HTTPS base URI used for control endpoints (`/health`, `/pair`).
  Uri get httpsBaseUri => Uri(scheme: 'https', host: host, port: port);

  /// WSS WebSocket URI for streaming telemetry and events (`/ws`).
  Uri get wssUri => Uri(scheme: 'wss', host: host, port: port, path: '/ws');

  /// Serializes this session into JSON.
  Map<String, Object?> toJson() => {
    'host': host,
    'port': port,
    'fingerprint_sha256': normalizeFingerprintSha256(fingerprintSha256),
    'device_id': deviceId,
    'device_name': deviceName,
    'device_token': deviceToken,
  };
}

/// Persistence abstraction for storing and clearing a paired [HostSession].
abstract interface class SessionStore {
  /// Loads the currently persisted session, or `null` if unpaired.
  Future<HostSession?> loadSession();

  /// Persists [session].
  Future<void> saveSession(HostSession session);

  /// Removes any saved pairing session.
  Future<void> clearSession();
}

/// Volatile [SessionStore] implementation tailored for unit and widget tests.
class InMemorySessionStore implements SessionStore {
  /// Creates an [InMemorySessionStore] with optional [initialSession].
  InMemorySessionStore([this._session]);

  HostSession? _session;

  @override
  Future<HostSession?> loadSession() async => _session;

  @override
  Future<void> saveSession(HostSession session) async {
    _session = session;
  }

  @override
  Future<void> clearSession() async {
    _session = null;
  }
}

/// JSON file-backed [SessionStore] using atomic replacement in app data.
class FileSessionStore implements SessionStore {
  /// Creates a [FileSessionStore] targeting [file].
  FileSessionStore(this.file);

  /// Creates a [FileSessionStore] inside an OS-appropriate user app data folder.
  factory FileSessionStore.defaultLocation() {
    final separator = Platform.pathSeparator;
    final home =
        Platform.environment['LOCALAPPDATA'] ??
        Platform.environment['XDG_DATA_HOME'] ??
        Platform.environment['HOME'] ??
        Directory.systemTemp.path;
    final dir = '$home${separator}NexDeckMobile';
    return FileSessionStore(File('$dir${separator}session.json'));
  }

  /// Target session file.
  final File file;

  @override
  Future<HostSession?> loadSession() async {
    if (!await file.exists()) {
      return null;
    }
    final content = await file.readAsString();
    if (content.trim().isEmpty) {
      return null;
    }
    final decoded = jsonDecode(content);
    return HostSession.fromJson(asJsonMap(decoded, context: 'session'));
  }

  @override
  Future<void> saveSession(HostSession session) async {
    final parent = file.parent;
    if (!await parent.exists()) {
      await parent.create(recursive: true);
    }
    final nonce = DateTime.now().microsecondsSinceEpoch;
    final tempFile = File('${file.path}.tmp.$pid.$nonce');
    final encoded = const JsonEncoder.withIndent('  ')
        .convert(session.toJson());
    await tempFile.writeAsString(encoded, flush: true);
    await tempFile.rename(file.path);
  }

  @override
  Future<void> clearSession() async {
    if (await file.exists()) {
      await file.delete();
    }
  }
}
