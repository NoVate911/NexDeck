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

  /// Decodes an execution acknowledgment from a `ControlCommand` envelope.
  ControlCommandResponse decodeControlResponse() {
    if (kind != MessageKind.controlCommand) {
      throw FormatException(
        'Expected ControlCommand envelope, received "${kind.wireName}".',
      );
    }
    return ControlCommandResponse.fromJson(decodePayloadJson());
  }

  /// Decodes a [NotificationEvent] from a `Notification` envelope.
  NotificationEvent decodeNotificationEvent() {
    if (kind != MessageKind.notification) {
      throw FormatException(
        'Expected Notification envelope, received "${kind.wireName}".',
      );
    }
    return NotificationEvent.fromJson(decodePayloadJson());
  }

  /// Decodes an [ApprovalRequest] from an `Approval` envelope.
  ApprovalRequest decodeApprovalRequest() {
    if (kind != MessageKind.approval) {
      throw FormatException(
        'Expected Approval envelope, received "${kind.wireName}".',
      );
    }
    return ApprovalRequest.fromJson(decodePayloadJson());
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

/// Allowlisted host media & volume control actions matching `ControlAction` in Rust.
enum ControlAction {
  /// Step host master audio volume up.
  volumeUp('VolumeUp', 'Громче'),

  /// Step host master audio volume down.
  volumeDown('VolumeDown', 'Тише'),

  /// Toggle host master audio mute state.
  volumeMuteToggle('VolumeMuteToggle', 'Без звука'),

  /// Toggle media playback (play / pause).
  mediaPlayPause('MediaPlayPause', 'Пауза / Пуск'),

  /// Skip to next media track.
  mediaNextTrack('MediaNextTrack', 'След. трек'),

  /// Return to previous media track.
  mediaPreviousTrack('MediaPreviousTrack', 'Пред. трек');

  const ControlAction(this.wireName, this.displayLabel);

  /// Exact enum variant name expected by `nexdeck-transport` serde JSON.
  final String wireName;

  /// Concise Russian UI label for buttons and notifications.
  final String displayLabel;

  /// Parses [wireName] into [ControlAction].
  static ControlAction fromWireName(String wireName) {
    for (final action in values) {
      if (action.wireName == wireName) {
        return action;
      }
    }
    throw FormatException('Unsupported ControlAction "$wireName".');
  }
}

/// Outbound command request matching `ControlCommandRequest` in `crates/transport`.
class ControlCommandRequest {
  /// Creates a [ControlCommandRequest].
  const ControlCommandRequest({required this.commandId, required this.action});

  /// Printable ASCII correlation ID (`1..=64` chars without spaces).
  final String commandId;

  /// Allowlisted action to trigger on the host.
  final ControlAction action;

  /// Encodes request matching Rust `serde` layout.
  Map<String, Object?> toJson() => {
    'command_id': commandId,
    'action': action.wireName,
  };
}

/// Host response matching `ControlCommandResponse` in `crates/transport`.
class ControlCommandResponse {
  /// Creates a [ControlCommandResponse].
  const ControlCommandResponse({
    required this.commandId,
    required this.action,
    required this.ok,
    required this.message,
  });

  /// Decodes JSON response returned over WSS.
  factory ControlCommandResponse.fromJson(Map<String, Object?> json) {
    return ControlCommandResponse(
      commandId: readRequired<String>(json, 'command_id'),
      action: ControlAction.fromWireName(readRequired<String>(json, 'action')),
      ok: readRequired<bool>(json, 'ok'),
      message: readRequired<String>(json, 'message'),
    );
  }

  /// Echoed request correlation identifier.
  final String commandId;

  /// Executed or rejected action.
  final ControlAction action;

  /// Whether the host executed the action cleanly.
  final bool ok;

  /// Diagnostic status message from the host agent.
  final String message;
}

/// Urgency level of a [NotificationEvent] matching `NotificationPriority`.
enum NotificationPriority {
  /// Low-priority background status message.
  low('Low', 'Низкий'),

  /// Standard notification priority.
  normal('Normal', 'Обычный'),

  /// Elevated notification (e.g. hardware threshold alert).
  high('High', 'Высокий'),

  /// Critical urgent event.
  critical('Critical', 'Критичный');

  const NotificationPriority(this.wireName, this.displayLabel);

  /// Exact enum name serialized by `nexdeck-notifications`.
  final String wireName;

  /// Localized Russian label for UI chips and badges.
  final String displayLabel;

  /// Returns `true` when this priority meets or exceeds [minimum].
  bool meetsMinimum(NotificationPriority minimum) => index >= minimum.index;

  /// Parses [wireName] into [NotificationPriority].
  static NotificationPriority fromWireName(String wireName) {
    for (final value in values) {
      if (value.wireName == wireName) {
        return value;
      }
    }
    throw FormatException('Unsupported NotificationPriority "$wireName".');
  }
}

/// Origin category of a [NotificationEvent].
enum NotificationSourceKind {
  /// Windows OS shell / system event.
  windowsSystem('WindowsSystem', 'Система Windows'),

  /// Host hardware threshold or power alert.
  hardwareAlert('HardwareAlert', 'Датчики ПК'),

  /// Registered external application using Universal API.
  application('Application', 'Приложение'),

  /// Sandboxed WebAssembly plugin.
  plugin('Plugin', 'Плагин');

  const NotificationSourceKind(this.wireName, this.displayLabel);

  /// Exact internally tagged enum variant name used by Rust serde.
  final String wireName;

  /// Localized category label.
  final String displayLabel;

  /// Parses [wireName] into [NotificationSourceKind].
  static NotificationSourceKind fromWireName(String wireName) {
    for (final value in values) {
      if (value.wireName == wireName) {
        return value;
      }
    }
    throw FormatException('Unsupported NotificationSource kind "$wireName".');
  }
}

/// Origin descriptor matching `NotificationSource` in `crates/notifications`.
class NotificationSource {
  /// Creates a [NotificationSource].
  const NotificationSource({required this.kind, this.name});

  /// Decodes internally tagged `NotificationSource` JSON.
  factory NotificationSource.fromJson(Map<String, Object?> json) {
    final kind = NotificationSourceKind.fromWireName(
      readRequired<String>(json, 'kind'),
    );
    final name = switch (kind) {
      NotificationSourceKind.application ||
      NotificationSourceKind.plugin => readRequired<String>(json, 'name'),
      NotificationSourceKind.windowsSystem ||
      NotificationSourceKind.hardwareAlert => null,
    };
    return NotificationSource(kind: kind, name: name);
  }

  /// Category of emitter.
  final NotificationSourceKind kind;

  /// Specific application or plugin name when applicable.
  final String? name;

  /// Human-readable origin label shown inside notification cards.
  String get originLabel =>
      name?.trim().isNotEmpty == true ? name!.trim() : kind.displayLabel;

  /// Matches the origin identifier checked by host `NotificationFilter`.
  String get filterOriginKey => name ?? kind.wireName;

  /// Serializes this source to wire JSON.
  Map<String, Object?> toJson() => {
    'kind': kind.wireName,
    if (name != null) 'name': name,
  };
}

/// Notification center item matching `NotificationEvent` in `crates/notifications`.
class NotificationEvent {
  /// Creates a [NotificationEvent].
  const NotificationEvent({
    required this.id,
    required this.source,
    required this.title,
    required this.body,
    required this.priority,
    required this.createdAtEpochMs,
  });

  /// Decodes a [NotificationEvent] from wire JSON.
  factory NotificationEvent.fromJson(Map<String, Object?> json) {
    return NotificationEvent(
      id: readRequired<int>(json, 'id'),
      source: NotificationSource.fromJson(readRequiredMap(json, 'source')),
      title: readRequired<String>(json, 'title'),
      body: readRequired<String>(json, 'body'),
      priority: NotificationPriority.fromWireName(
        readRequired<String>(json, 'priority'),
      ),
      createdAtEpochMs: readRequired<int>(json, 'created_at_epoch_ms'),
    );
  }

  /// Monotonic notification identifier assigned by `nexdeck-agent`.
  final int id;

  /// Notification origin metadata.
  final NotificationSource source;

  /// Concise headline.
  final String title;

  /// Notification body message.
  final String body;

  /// Assigned priority level.
  final NotificationPriority priority;

  /// Creation timestamp in Unix epoch milliseconds.
  final int createdAtEpochMs;

  /// Serializes this event back to wire JSON.
  Map<String, Object?> toJson() => {
    'id': id,
    'source': source.toJson(),
    'title': title,
    'body': body,
    'priority': priority.wireName,
    'created_at_epoch_ms': createdAtEpochMs,
  };
}

/// Mobile-side and host-synchronized notification filter rules.
class NotificationFilter {
  /// Creates a [NotificationFilter].
  const NotificationFilter({
    this.minPriority = NotificationPriority.low,
    this.mutedOrigins = const [],
  });

  /// Decodes [NotificationFilter] from wire JSON.
  factory NotificationFilter.fromJson(Map<String, Object?> json) {
    final rawMuted =
        readOptional<List<Object?>>(json, 'muted_origins') ?? const [];
    final muted = <String>[
      for (final item in rawMuted)
        if (item is String && item.trim().isNotEmpty) item.trim(),
    ];
    return NotificationFilter(
      minPriority: NotificationPriority.fromWireName(
        readOptional<String>(json, 'min_priority') ??
            NotificationPriority.low.wireName,
      ),
      mutedOrigins: List<String>.unmodifiable(muted),
    );
  }

  /// Minimum priority allowed through the notification feed.
  final NotificationPriority minPriority;

  /// Case-insensitive list of muted application/plugin/system origins.
  final List<String> mutedOrigins;

  /// Returns `true` if [event] passes priority and muted-origin checks.
  bool allows(NotificationEvent event) {
    if (!event.priority.meetsMinimum(minPriority)) {
      return false;
    }
    final originName = switch (event.source.kind) {
      NotificationSourceKind.application ||
      NotificationSourceKind.plugin => event.source.name?.trim().toLowerCase(),
      NotificationSourceKind.windowsSystem ||
      NotificationSourceKind.hardwareAlert => null,
    };
    if (originName == null || originName.isEmpty) {
      return true;
    }
    for (final raw in mutedOrigins) {
      final muted = raw.trim().toLowerCase();
      if (muted.isNotEmpty && muted == originName) {
        return false;
      }
    }
    return true;
  }

  /// Serializes this filter to wire JSON.
  Map<String, Object?> toJson() => {
    'min_priority': minPriority.wireName,
    'muted_origins': mutedOrigins,
  };
}

/// Outbound WebSocket synchronization command sent over `MessageKind.notification`.
class NotificationSyncCommand {
  const NotificationSyncCommand._({
    required this.op,
    this.eventId,
    this.filter,
  });

  /// Dismisses a single notification ID across connected devices and host buffer.
  const NotificationSyncCommand.dismiss(int eventId)
    : this._(op: 'dismiss', eventId: eventId);

  /// Clears all buffered notifications on host and mobile clients.
  const NotificationSyncCommand.clearAll() : this._(op: 'clear_all');

  /// Pushes updated notification filter rules to the host agent.
  const NotificationSyncCommand.updateFilter(NotificationFilter filter)
    : this._(op: 'update_filter', filter: filter);

  /// Tag matched by `#[serde(tag = "op", rename_all = "snake_case")]`.
  final String op;

  /// Target notification ID for `dismiss`.
  final int? eventId;

  /// Updated filter payload for `update_filter`.
  final NotificationFilter? filter;

  /// Serializes this sync command to wire JSON.
  Map<String, Object?> toJson() => {
    'op': op,
    if (eventId != null) 'event_id': eventId,
    if (filter != null) 'filter': filter!.toJson(),
  };
}

/// Security risk classification matching `RiskLevel` in `crates/approval`.
enum RiskLevel {
  /// Routine low-impact action.
  low('Low', 'Низкий риск'),

  /// Moderate-impact confirmation.
  medium('Medium', 'Средний риск'),

  /// Sensitive host state or file modification.
  high('High', 'Высокий риск'),

  /// Critical action requiring explicit scrutiny.
  critical('Critical', 'Критический риск');

  const RiskLevel(this.wireName, this.displayLabel);

  /// Exact enum name serialized by `nexdeck-approval`.
  final String wireName;

  /// Localized Russian risk badge text.
  final String displayLabel;

  /// Parses [wireName] into [RiskLevel].
  static RiskLevel fromWireName(String wireName) {
    for (final value in values) {
      if (value.wireName == wireName) {
        return value;
      }
    }
    throw FormatException('Unsupported RiskLevel "$wireName".');
  }
}

/// State tag inside [ApprovalStatus].
enum ApprovalStatusState {
  /// Request is active and awaiting user action before expiry.
  pending('Pending', 'Ожидает решения'),

  /// Single-use decision has already been recorded.
  resolved('Resolved', 'Решено'),

  /// Request TTL elapsed before user action.
  expired('Expired', 'Истёк срок'),

  /// Request was cancelled by the originating application/agent.
  revoked('Revoked', 'Отозвано');

  const ApprovalStatusState(this.wireName, this.displayLabel);

  /// Exact internally tagged serde variant name.
  final String wireName;

  /// Localized Russian label.
  final String displayLabel;

  /// Parses [wireName] into [ApprovalStatusState].
  static ApprovalStatusState fromWireName(String wireName) {
    for (final value in values) {
      if (value.wireName == wireName) {
        return value;
      }
    }
    throw FormatException('Unsupported ApprovalStatus state "$wireName".');
  }
}

/// Lifecycle state of an [ApprovalRequest].
class ApprovalStatus {
  /// Creates an [ApprovalStatus].
  const ApprovalStatus({required this.state, this.selectedAction});

  /// Decodes internally tagged `ApprovalStatus` JSON.
  factory ApprovalStatus.fromJson(Map<String, Object?> json) {
    final state = ApprovalStatusState.fromWireName(
      readRequired<String>(json, 'state'),
    );
    final selectedAction = state == ApprovalStatusState.resolved
        ? readRequired<String>(json, 'selected_action')
        : null;
    return ApprovalStatus(state: state, selectedAction: selectedAction);
  }

  /// Current lifecycle state.
  final ApprovalStatusState state;

  /// Action chosen when [state] is [ApprovalStatusState.resolved].
  final String? selectedAction;

  /// Returns `true` when the request is still awaiting a one-time decision.
  bool get isPending => state == ApprovalStatusState.pending;

  /// Serializes this status to wire JSON.
  Map<String, Object?> toJson() => {
    'state': state.wireName,
    if (selectedAction != null) 'selected_action': selectedAction,
  };
}

/// Remote approval prompt matching `ApprovalRequest` in `crates/approval`.
class ApprovalRequest {
  /// Creates an [ApprovalRequest].
  const ApprovalRequest({
    required this.requestId,
    required this.application,
    required this.title,
    required this.description,
    required this.expiresAtEpochMs,
    required this.riskLevel,
    required this.allowedActions,
    required this.status,
  });

  /// Decodes an [ApprovalRequest] from wire JSON.
  factory ApprovalRequest.fromJson(Map<String, Object?> json) {
    final rawActions = readRequired<List<Object?>>(json, 'allowed_actions');
    final actions = <String>[];
    for (final entry in rawActions) {
      if (entry is! String || entry.trim().isEmpty) {
        throw const FormatException(
          'Invalid action identifier in ApprovalRequest.allowed_actions.',
        );
      }
      actions.add(entry.trim());
    }
    return ApprovalRequest(
      requestId: readRequired<String>(json, 'request_id'),
      application: readRequired<String>(json, 'application'),
      title: readRequired<String>(json, 'title'),
      description: readRequired<String>(json, 'description'),
      expiresAtEpochMs: readRequired<int>(json, 'expires_at_epoch_ms'),
      riskLevel: RiskLevel.fromWireName(
        readRequired<String>(json, 'risk_level'),
      ),
      allowedActions: List<String>.unmodifiable(actions),
      status: ApprovalStatus.fromJson(readRequiredMap(json, 'status')),
    );
  }

  /// Unique single-use request identifier bound to this action.
  final String requestId;

  /// Human-readable originating application name.
  final String application;

  /// Prompt headline.
  final String title;

  /// Detailed description of the requested operation.
  final String description;

  /// Expiration deadline in Unix epoch milliseconds.
  final int expiresAtEpochMs;

  /// Risk classification.
  final RiskLevel riskLevel;

  /// Allowlisted decision action identifiers (for example `approve`, `deny`).
  final List<String> allowedActions;

  /// Current authoritative approval lifecycle state.
  final ApprovalStatus status;

  /// Returns `true` when this request is still awaiting user input.
  bool get isPending => status.isPending;

  /// Serializes this request back to wire JSON.
  Map<String, Object?> toJson() => {
    'request_id': requestId,
    'application': application,
    'title': title,
    'description': description,
    'expires_at_epoch_ms': expiresAtEpochMs,
    'risk_level': riskLevel.wireName,
    'allowed_actions': allowedActions,
    'status': status.toJson(),
  };
}

/// Outbound client decision matching `ApprovalDecisionMessage` in `crates/approval`.
class ApprovalDecisionMessage {
  /// Creates an [ApprovalDecisionMessage].
  const ApprovalDecisionMessage({
    required this.requestId,
    required this.action,
  });

  /// Target single-use request identifier.
  final String requestId;

  /// Selected action from [ApprovalRequest.allowedActions].
  final String action;

  /// Serializes this decision to wire JSON.
  Map<String, Object?> toJson() => {'request_id': requestId, 'action': action};
}
