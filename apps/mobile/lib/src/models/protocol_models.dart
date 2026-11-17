import 'dart:convert';

import 'json_helpers.dart';
import 'monitoring_models.dart';

/// Wire protocol version expected by `nexdeck-transport`.
const int nexdeckProtocolVersion = 1;

final RegExp _sha256HexPattern = RegExp(r'^[0-9a-f]{64}$');
final RegExp _pinPattern = RegExp(r'^\d{6}$');

/// Normalizes a colon- or space-delimited SHA-256 hex digest to lowercase 64-char form.
String normalizeFingerprintSha256(String input) {
  return input.replaceAll(':', '').replaceAll(' ', '').trim().toLowerCase();
}

/// Returns `true` if [fingerprint] is a normalized 64-character SHA-256 hex string.
bool isValidFingerprintSha256(String fingerprint) {
  return _sha256HexPattern.hasMatch(normalizeFingerprintSha256(fingerprint));
}

/// Returns `true` if [pin] is a valid 6-digit numeric pairing PIN.
bool isValidPairingPin(String pin) => _pinPattern.hasMatch(pin.trim());

/// Logical channel topic carried inside a transport [Envelope].
enum MessageKind {
  /// System telemetry frame.
  telemetry('Telemetry'),

  /// Control command triggered from a client panel.
  controlCommand('ControlCommand'),

  /// Notification center event.
  notification('Notification'),

  /// Remote approval prompt or user decision.
  approval('Approval'),

  /// Heartbeat / keepalive frame.
  heartbeat('Heartbeat');

  const MessageKind(this.wireName);

  /// Exact enum name used by Rust `serde` serialization.
  final String wireName;

  /// Parses [wireName] into [MessageKind].
  static MessageKind fromWireName(String wireName) {
    for (final kind in values) {
      if (kind.wireName == wireName) {
        return kind;
      }
    }
    throw FormatException('Unsupported MessageKind "$wireName".');
  }
}

/// Framed message matching `Envelope` in `crates/transport`.
class Envelope {
  /// Creates a validated [Envelope].
  const Envelope({
    this.protocolVersion = nexdeckProtocolVersion,
    required this.messageId,
    required this.kind,
    required this.payload,
  });

  /// Decodes an [Envelope] JSON object received over WebSocket.
  factory Envelope.fromJson(Map<String, Object?> json) {
    final rawPayload = json['payload'];
    if (rawPayload is! List<Object?>) {
      throw const FormatException('Invalid "payload" byte array in Envelope.');
    }
    final bytes = <int>[];
    for (final entry in rawPayload) {
      if (entry is! int || entry < 0 || entry > 255) {
        throw const FormatException('Envelope payload contains invalid byte.');
      }
      bytes.add(entry);
    }

    final envelope = Envelope(
      protocolVersion: readRequired<int>(json, 'protocol_version'),
      messageId: readRequired<int>(json, 'message_id'),
      kind: MessageKind.fromWireName(readRequired<String>(json, 'kind')),
      payload: List<int>.unmodifiable(bytes),
    );
    envelope.validate();
    return envelope;
  }

  /// Creates an [Envelope] whose payload is UTF-8 encoded JSON.
  factory Envelope.json({
    required int messageId,
    required MessageKind kind,
    required Map<String, Object?> body,
  }) {
    final encoded = utf8.encode(jsonEncode(body));
    return Envelope(
      protocolVersion: nexdeckProtocolVersion,
      messageId: messageId,
      kind: kind,
      payload: encoded,
    );
  }

  /// Wire protocol version.
  final int protocolVersion;

  /// Correlation identifier unique per session.
  final int messageId;

  /// Logical category of the payload.
  final MessageKind kind;

  /// Serialized UTF-8 payload bytes.
  final List<int> payload;

  /// Validates protocol version and non-empty payload invariants.
  void validate() {
    if (protocolVersion != nexdeckProtocolVersion) {
      throw FormatException(
        'Unsupported protocol version: $protocolVersion (expected $nexdeckProtocolVersion).',
      );
    }
    if (kind != MessageKind.heartbeat && payload.isEmpty) {
      throw FormatException(
        'Non-heartbeat message kind "${kind.wireName}" must carry payload bytes.',
      );
    }
  }

  /// Decodes [payload] bytes as a UTF-8 JSON map.
  Map<String, Object?> decodePayloadJson() {
    final decoded = jsonDecode(utf8.decode(payload));
    return asJsonMap(decoded, context: 'envelope.payload');
  }

  /// Decodes a telemetry envelope directly into a [SystemSnapshot].
  SystemSnapshot decodeTelemetrySnapshot() {
    if (kind != MessageKind.telemetry) {
      throw FormatException(
        'Expected Telemetry envelope, received "${kind.wireName}".',
      );
    }
    return SystemSnapshot.fromJson(decodePayloadJson());
  }

  /// Serializes this envelope into wire JSON matching `crates/transport`.
  Map<String, Object?> toJson() {
    validate();
    return {
      'protocol_version': protocolVersion,
      'message_id': messageId,
      'kind': kind.wireName,
      'payload': payload,
    };
  }
}

/// Response returned by `GET /health` on `nexdeck-agent`.
class HealthResponse {
  /// Creates a [HealthResponse].
  const HealthResponse({
    required this.status,
    required this.protocolVersion,
    required this.fingerprintSha256,
  });

  /// Decodes JSON returned by `/health`.
  factory HealthResponse.fromJson(Map<String, Object?> json) {
    final fingerprint = normalizeFingerprintSha256(
      readRequired<String>(json, 'fingerprint_sha256'),
    );
    if (!isValidFingerprintSha256(fingerprint)) {
      throw const FormatException(
        'Invalid SHA-256 fingerprint in HealthResponse.',
      );
    }
    return HealthResponse(
      status: readRequired<String>(json, 'status'),
      protocolVersion: readRequired<int>(json, 'protocol_version'),
      fingerprintSha256: fingerprint,
    );
  }

  /// Service health string (`"ok"`).
  final String status;

  /// Supported protocol version.
  final int protocolVersion;

  /// Host TLS certificate SHA-256 hex digest.
  final String fingerprintSha256;
}

/// QR pairing bootstrap payload printed by `nexdeck-agent --daemon --pair`.
class PairingQrPayload {
  /// Creates a validated [PairingQrPayload].
  const PairingQrPayload({
    required this.protocolVersion,
    required this.host,
    required this.port,
    required this.fingerprintSha256,
    required this.pin,
  });

  /// Parses raw QR JSON text exported by `nexdeck-agent`.
  factory PairingQrPayload.fromJsonString(String rawJson) {
    final decoded = jsonDecode(rawJson);
    return PairingQrPayload.fromJson(asJsonMap(decoded, context: 'qr'));
  }

  /// Decodes a [PairingQrPayload] map and validates every field.
  factory PairingQrPayload.fromJson(Map<String, Object?> json) {
    final protocolVersion = readRequired<int>(json, 'protocol_version');
    if (protocolVersion != nexdeckProtocolVersion) {
      throw FormatException(
        'Unsupported protocol_version $protocolVersion in pairing payload.',
      );
    }
    final host = readRequired<String>(json, 'host').trim();
    if (host.isEmpty) {
      throw const FormatException('Pairing host cannot be empty.');
    }
    final port = readRequired<int>(json, 'port');
    if (port <= 0 || port > 65535) {
      throw FormatException('Pairing port $port out of valid TCP range.');
    }
    final fingerprint = normalizeFingerprintSha256(
      readRequired<String>(json, 'fingerprint_sha256'),
    );
    if (!isValidFingerprintSha256(fingerprint)) {
      throw const FormatException(
        'Pairing fingerprint_sha256 must be a 64-char SHA-256 hex string.',
      );
    }
    final pin = readRequired<String>(json, 'pin').trim();
    if (!isValidPairingPin(pin)) {
      throw const FormatException('Pairing PIN must be exactly 6 digits.');
    }
    return PairingQrPayload(
      protocolVersion: protocolVersion,
      host: host,
      port: port,
      fingerprintSha256: fingerprint,
      pin: pin,
    );
  }

  /// Protocol version advertised by the host agent.
  final int protocolVersion;

  /// IP address or hostname where the WSS endpoint listens.
  final String host;

  /// Listening TCP port.
  final int port;

  /// Expected SHA-256 hex fingerprint of the host TLS certificate.
  final String fingerprintSha256;

  /// Single-use 6-digit pairing PIN.
  final String pin;
}

/// Request payload sent to `POST /pair`.
class PairRequest {
  /// Creates a [PairRequest].
  const PairRequest({
    required this.pin,
    required this.deviceId,
    required this.deviceName,
  });

  /// One-time 6-digit pairing PIN.
  final String pin;

  /// Client-assigned unique device identifier.
  final String deviceId;

  /// Human-readable device label.
  final String deviceName;

  /// Encodes request fields matching Rust `PairRequest`.
  Map<String, Object?> toJson() => {
    'pin': pin.trim(),
    'device_id': deviceId.trim(),
    'device_name': deviceName.trim(),
  };
}

/// Credentials issued once by `POST /pair`.
class PairedCredentials {
  /// Creates [PairedCredentials].
  const PairedCredentials({
    required this.deviceId,
    required this.deviceToken,
    required this.fingerprintSha256,
  });

  /// Decodes JSON returned by `POST /pair`.
  factory PairedCredentials.fromJson(Map<String, Object?> json) {
    final deviceId = readRequired<String>(json, 'device_id');
    final deviceToken = readRequired<String>(json, 'device_token');
    final fingerprint = normalizeFingerprintSha256(
      readRequired<String>(json, 'fingerprint_sha256'),
    );
    if (deviceId.trim().isEmpty || deviceToken.trim().isEmpty) {
      throw const FormatException(
        'PairedCredentials cannot have empty device_id or device_token.',
      );
    }
    if (!isValidFingerprintSha256(fingerprint)) {
      throw const FormatException(
        'PairedCredentials contained invalid fingerprint_sha256.',
      );
    }
    return PairedCredentials(
      deviceId: deviceId,
      deviceToken: deviceToken,
      fingerprintSha256: fingerprint,
    );
  }

  /// Confirmed device identifier.
  final String deviceId;

  /// Issued bearer token for `/ws` authentication.
  final String deviceToken;

  /// SHA-256 hex fingerprint of the host TLS certificate.
  final String fingerprintSha256;
}
