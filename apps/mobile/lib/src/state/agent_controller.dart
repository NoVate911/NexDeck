import 'dart:async';
import 'dart:collection';
import 'dart:math';

import 'package:flutter/foundation.dart';

import '../models/monitoring_models.dart';
import '../models/protocol_models.dart';
import '../storage/session_store.dart';
import '../transport/pinned_http_client.dart';
import '../transport/transport_client.dart';

/// Coordinates host pairing, WSS connection lifecycle, and live telemetry updates.
class AgentController extends ChangeNotifier {
  /// Creates an [AgentController].
  AgentController({
    required this._sessionStore,
    required this._pairingClient,
    required this._transport,
    this._maxHistorySamples = 30,
  }) {
    _stateSub = _transport.states.listen(_onTransportStateChanged);
    _envelopeSub = _transport.envelopes.listen(
      _onEnvelopeReceived,
      onError: (Object error) {
        _errorMessage = error.toString();
        notifyListeners();
      },
    );
  }

  final SessionStore _sessionStore;
  final PairingClient _pairingClient;
  final NexDeckTransport _transport;
  final int _maxHistorySamples;

  late final StreamSubscription<TransportConnectionState> _stateSub;
  late final StreamSubscription<Envelope> _envelopeSub;

  final ListQueue<double> _cpuHistory = ListQueue<double>();
  final ListQueue<double> _ramHistory = ListQueue<double>();
  final ListQueue<double> _networkRxKibHistory = ListQueue<double>();
  final ListQueue<double> _networkTxKibHistory = ListQueue<double>();
  final List<NotificationEvent> _notifications = <NotificationEvent>[];
  final List<ApprovalRequest> _approvals = <ApprovalRequest>[];
  final Set<String> _submittingApprovalIds = <String>{};
  NotificationFilter _notificationFilter = const NotificationFilter();
  HostSession? _session;
  SystemSnapshot? _latestSnapshot;
  ControlCommandResponse? _lastControlResult;
  TransportConnectionState _connectionState =
      TransportConnectionState.disconnected;
  bool _isBusy = false;
  bool _isSendingControlCommand = false;
  bool _manualDisconnect = false;
  bool _disposed = false;
  int _reconnectAttempts = 0;
  int _nextMessageId = 1;
  Timer? _reconnectTimer;
  String? _errorMessage;
  String? _statusMessage;

  /// Active paired host session, or `null` before pairing.
  HostSession? get session => _session;

  /// Current transport state (including automatic reconnect backoff).
  TransportConnectionState get connectionState => _connectionState;

  /// Most recently received hardware telemetry snapshot.
  SystemSnapshot? get latestSnapshot => _latestSnapshot;

  /// Result of the most recent remote control command acknowledgment.
  ControlCommandResponse? get lastControlResult => _lastControlResult;

  /// Recent CPU load history samples (`0.0..=100.0`).
  List<double> get cpuHistory => List<double>.unmodifiable(_cpuHistory);

  /// Recent RAM utilization history samples (`0.0..=100.0`).
  List<double> get ramHistory => List<double>.unmodifiable(_ramHistory);

  /// Recent aggregate network download throughput history in KiB/s.
  List<double> get networkRxKibHistory =>
      List<double>.unmodifiable(_networkRxKibHistory);

  /// Recent aggregate network upload throughput history in KiB/s.
  List<double> get networkTxKibHistory =>
      List<double>.unmodifiable(_networkTxKibHistory);

  /// Whether a pairing or manual connection attempt is currently active.
  bool get isBusy => _isBusy;

  /// Whether a control command send is currently in flight.
  bool get isSendingControlCommand => _isSendingControlCommand;

  /// Latest user-facing error message, if any.
  String? get errorMessage => _errorMessage;

  /// Latest informational status note, if any.
  String? get statusMessage => _statusMessage;

  /// Buffered and live notification feed ordered newest-first.
  List<NotificationEvent> get notifications =>
      List<NotificationEvent>.unmodifiable(_notifications);

  /// Remote approval requests (pending first, then resolved/expired history).
  List<ApprovalRequest> get approvals =>
      List<ApprovalRequest>.unmodifiable(_approvals);

  /// Active pending approval requests awaiting a single-use decision.
  List<ApprovalRequest> get pendingApprovals =>
      _approvals.where((item) => item.isPending).toList(growable: false);

  /// Count of pending approval requests requiring attention.
  int get pendingApprovalCount =>
      _approvals.where((item) => item.isPending).length;

  /// Active notification priority and muted-origins filter.
  NotificationFilter get notificationFilter => _notificationFilter;

  /// Returns `true` while an outbound decision for [requestId] is in flight.
  bool isSubmittingApproval(String requestId) =>
      _submittingApprovalIds.contains(requestId);

  /// Loads any saved [HostSession] and initiates WSS connection if paired.
  Future<void> initialize() async {
    _isBusy = true;
    notifyListeners();
    try {
      final loaded = await _sessionStore.loadSession();
      _session = loaded;
      if (loaded != null) {
        await connectActiveSession();
      }
    } on Object catch (err) {
      _errorMessage = 'Не удалось загрузить сохранённую сессию: $err';
    } finally {
      _isBusy = false;
      if (!_disposed) {
        notifyListeners();
      }
    }
  }

  /// Verifies `/health`, executes `POST /pair`, saves credentials, and connects `/ws`.
  Future<bool> pairAndConnect({
    required String host,
    required int port,
    required String fingerprintSha256,
    required String pin,
    required String deviceName,
    String? deviceId,
  }) async {
    final trimmedHost = host.trim();
    final normalizedFingerprint = normalizeFingerprintSha256(fingerprintSha256);
    final trimmedPin = pin.trim();
    final trimmedName = deviceName.trim();

    if (trimmedHost.isEmpty) {
      _setError('Укажите адрес хоста (например, 127.0.0.1).');
      return false;
    }
    if (port <= 0 || port > 65535) {
      _setError('Порт должен быть в диапазоне 1..65535.');
      return false;
    }
    if (normalizedFingerprint.isNotEmpty &&
        !isValidFingerprintSha256(normalizedFingerprint)) {
      _setError(
        'SHA-256 отпечаток сертификата должен содержать 64 hex-символа (или оставьте поле пустым для автоопределения по ПИН-коду).',
      );
      return false;
    }
    if (!isValidPairingPin(trimmedPin)) {
      _setError('PIN-код должен состоять ровно из 6 цифр.');
      return false;
    }
    if (trimmedName.isEmpty) {
      _setError('Укажите имя устройства (например, ExamplePhone).');
      return false;
    }

    _isBusy = true;
    _errorMessage = null;
    _statusMessage = 'Проверка сертификата и состояния сервера...';
    notifyListeners();

    try {
      final baseUri = Uri(scheme: 'https', host: trimmedHost, port: port);
      final health = await _pairingClient.checkHealth(
        baseUri,
        expectedFingerprintSha256: normalizedFingerprint,
      );
      final resolvedFingerprint = normalizedFingerprint.isNotEmpty
          ? normalizedFingerprint
          : normalizeFingerprintSha256(health.fingerprintSha256);

      _statusMessage = 'Выполняется сопряжение по PIN...';
      notifyListeners();

      final assignedDeviceId = (deviceId != null && deviceId.trim().isNotEmpty)
          ? deviceId.trim()
          : _generateDeviceId();

      final credentials = await _pairingClient.pair(
        baseUri: baseUri,
        expectedFingerprintSha256: resolvedFingerprint,
        request: PairRequest(
          pin: trimmedPin,
          deviceId: assignedDeviceId,
          deviceName: trimmedName,
        ),
      );

      final pairedSession = HostSession(
        host: trimmedHost,
        port: port,
        fingerprintSha256: credentials.fingerprintSha256,
        deviceId: credentials.deviceId,
        deviceName: trimmedName,
        deviceToken: credentials.deviceToken,
      );

      await _sessionStore.saveSession(pairedSession);
      _session = pairedSession;
      _statusMessage = 'Устройство сопряжено. Подключение к WSS...';
      _isBusy = false;
      notifyListeners();

      await connectActiveSession();
      return true;
    } on Object catch (err) {
      _isBusy = false;
      _statusMessage = null;
      _errorMessage = err.toString();
      if (!_disposed) {
        notifyListeners();
      }
      return false;
    }
  }

  /// Connects (or reconnects) the WSS channel for the currently stored [session].
  Future<void> connectActiveSession() async {
    final active = _session;
    if (active == null) {
      return;
    }
    _reconnectTimer?.cancel();
    _reconnectTimer = null;
    _manualDisconnect = false;
    _errorMessage = null;
    _statusMessage = 'Подключение к ${active.host}:${active.port}...';
    notifyListeners();

    try {
      await _transport.connect(active);
      _reconnectAttempts = 0;
      _statusMessage = 'Подключено к ${active.host}:${active.port}';
    } on Object catch (err) {
      _errorMessage = 'Ошибка подключения WSS: $err';
      _statusMessage = null;
      _scheduleReconnect();
    }
    if (!_disposed) {
      notifyListeners();
    }
  }

  /// Gracefully disconnects the active WSS session without deleting saved pairing.
  Future<void> disconnect() async {
    _manualDisconnect = true;
    _reconnectTimer?.cancel();
    _reconnectTimer = null;
    _reconnectAttempts = 0;
    _statusMessage = 'Отключено пользователем';
    await _transport.disconnect();
    if (!_disposed) {
      notifyListeners();
    }
  }

  /// Sends a protocol `Heartbeat` keepalive frame to verify duplex WSS transport.
  Future<void> sendHeartbeat() async {
    final envelope = Envelope(
      messageId: _nextMessageId++,
      kind: MessageKind.heartbeat,
      payload: const [],
    );
    await _transport.sendEnvelope(envelope);
    _statusMessage = 'Отправлен Heartbeat #${envelope.messageId}';
    if (!_disposed) {
      notifyListeners();
    }
  }

  /// Sends an allowlisted media or volume control command to `nexdeck-agent`.
  Future<void> sendControlAction(ControlAction action) async {
    if (_connectionState != TransportConnectionState.connected) {
      _setError('Подключитесь к хосту перед отправкой команд управления.');
      return;
    }

    _isSendingControlCommand = true;
    _errorMessage = null;
    notifyListeners();

    try {
      final messageId = _nextMessageId++;
      final req = ControlCommandRequest(
        commandId: 'cmd-$messageId',
        action: action,
      );
      final envelope = Envelope.json(
        messageId: messageId,
        kind: MessageKind.controlCommand,
        body: req.toJson(),
      );
      await _transport.sendEnvelope(envelope);
      _statusMessage = 'Отправлена команда: ${action.displayLabel}';
    } on Object catch (err) {
      _errorMessage = 'Не удалось отправить команду: $err';
    } finally {
      _isSendingControlCommand = false;
      if (!_disposed) {
        notifyListeners();
      }
    }
  }

  /// Submits a one-time user decision for a pending remote [ApprovalRequest].
  Future<void> submitApprovalDecision(String requestId, String action) async {
    if (_connectionState != TransportConnectionState.connected) {
      _setError('Подключитесь к хосту перед подтверждением действия.');
      return;
    }

    ApprovalRequest? target;
    for (final entry in _approvals) {
      if (entry.requestId == requestId) {
        target = entry;
        break;
      }
    }
    if (target == null ||
        !target.isPending ||
        !target.allowedActions.contains(action)) {
      _setError('Запрос подтверждения уже закрыт или действие недоступно.');
      return;
    }
    if (!_submittingApprovalIds.add(requestId)) {
      return;
    }

    _errorMessage = null;
    notifyListeners();

    try {
      final messageId = _nextMessageId++;
      final decision = ApprovalDecisionMessage(
        requestId: requestId,
        action: action,
      );
      final envelope = Envelope.json(
        messageId: messageId,
        kind: MessageKind.approval,
        body: decision.toJson(),
      );
      await _transport.sendEnvelope(envelope);
      _statusMessage = 'Отправлен ответ на запрос $requestId: $action';
    } on Object catch (err) {
      _errorMessage = 'Не удалось отправить решение: $err';
    } finally {
      _submittingApprovalIds.remove(requestId);
      if (!_disposed) {
        notifyListeners();
      }
    }
  }

  /// Dismisses a single notification locally and synchronizes dismissal with `nexdeck-agent`.
  Future<void> dismissNotification(int eventId) async {
    _notifications.removeWhere((item) => item.id == eventId);
    if (!_disposed) {
      notifyListeners();
    }
    if (_connectionState == TransportConnectionState.connected) {
      await _sendNotificationSync(NotificationSyncCommand.dismiss(eventId));
    }
  }

  /// Clears all notifications locally and clears the host sleep/disconnect buffer.
  Future<void> clearNotifications() async {
    _notifications.clear();
    _statusMessage = 'Уведомления очищены';
    if (!_disposed) {
      notifyListeners();
    }
    if (_connectionState == TransportConnectionState.connected) {
      await _sendNotificationSync(const NotificationSyncCommand.clearAll());
    }
  }

  /// Updates active notification priority/origin filters and pushes rules to `nexdeck-agent`.
  Future<void> updateNotificationFilter(NotificationFilter filter) async {
    _notificationFilter = filter;
    _notifications.removeWhere((item) => !filter.allows(item));
    if (!_disposed) {
      notifyListeners();
    }
    if (_connectionState == TransportConnectionState.connected) {
      await _sendNotificationSync(NotificationSyncCommand.updateFilter(filter));
    }
  }

  Future<void> _sendNotificationSync(NotificationSyncCommand command) async {
    try {
      final envelope = Envelope.json(
        messageId: _nextMessageId++,
        kind: MessageKind.notification,
        body: command.toJson(),
      );
      await _transport.sendEnvelope(envelope);
    } on Object catch (err) {
      _errorMessage = 'Не удалось синхронизировать уведомления: $err';
      if (!_disposed) {
        notifyListeners();
      }
    }
  }

  /// Disconnects and clears persisted host pairing credentials.
  Future<void> unpairHost() async {
    _manualDisconnect = true;
    _reconnectTimer?.cancel();
    _reconnectTimer = null;
    _reconnectAttempts = 0;
    await _transport.disconnect();
    await _sessionStore.clearSession();
    _session = null;
    _latestSnapshot = null;
    _lastControlResult = null;
    _cpuHistory.clear();
    _ramHistory.clear();
    _networkRxKibHistory.clear();
    _networkTxKibHistory.clear();
    _notifications.clear();
    _approvals.clear();
    _submittingApprovalIds.clear();
    _notificationFilter = const NotificationFilter();
    _errorMessage = null;
    _statusMessage = 'Привязка хоста удалена';
    if (!_disposed) {
      notifyListeners();
    }
  }

  void _onTransportStateChanged(TransportConnectionState next) {
    if (_disposed) {
      return;
    }
    if (next == TransportConnectionState.disconnected &&
        !_manualDisconnect &&
        _session != null &&
        _connectionState == TransportConnectionState.connected) {
      _scheduleReconnect();
      return;
    }
    _connectionState = next;
    notifyListeners();
  }

  void _onEnvelopeReceived(Envelope envelope) {
    if (_disposed) {
      return;
    }
    if (envelope.kind == MessageKind.telemetry) {
      try {
        final snapshot = envelope.decodeTelemetrySnapshot();
        _latestSnapshot = snapshot;
        _appendHistory(
          _cpuHistory,
          snapshot.cpu.totalUsagePct.clamp(0.0, 100.0),
        );
        _appendHistory(_ramHistory, snapshot.memory.usagePct.clamp(0.0, 100.0));

        var totalRxBytesPerSec = 0;
        var totalTxBytesPerSec = 0;
        for (final net in snapshot.networks) {
          totalRxBytesPerSec += net.rxBytesPerSec;
          totalTxBytesPerSec += net.txBytesPerSec;
        }
        _appendHistory(_networkRxKibHistory, totalRxBytesPerSec / 1024.0);
        _appendHistory(_networkTxKibHistory, totalTxBytesPerSec / 1024.0);

        _errorMessage = null;
        notifyListeners();
      } on FormatException catch (err) {
        _errorMessage = 'Ошибка декодирования телеметрии: ${err.message}';
        notifyListeners();
      }
    } else if (envelope.kind == MessageKind.controlCommand) {
      try {
        final response = envelope.decodeControlResponse();
        _lastControlResult = response;
        if (response.ok) {
          _statusMessage = 'Выполнено: ${response.action.displayLabel}';
          _errorMessage = null;
        } else {
          _errorMessage =
              'Ошибка ${response.action.displayLabel}: ${response.message}';
        }
        notifyListeners();
      } on FormatException catch (err) {
        _errorMessage = 'Ошибка ответа команды: ${err.message}';
        notifyListeners();
      }
    } else if (envelope.kind == MessageKind.notification) {
      try {
        final event = envelope.decodeNotificationEvent();
        _upsertNotification(event);
        _errorMessage = null;
        notifyListeners();
      } on FormatException catch (err) {
        _errorMessage = 'Ошибка декодирования уведомления: ${err.message}';
        notifyListeners();
      }
    } else if (envelope.kind == MessageKind.approval) {
      try {
        final request = envelope.decodeApprovalRequest();
        _upsertApproval(request);
        _errorMessage = null;
        _statusMessage = request.isPending
            ? 'Запрос подтверждения от ${request.application}'
            : 'Запрос ${request.requestId}: ${request.status.state.displayLabel}';
        notifyListeners();
      } on FormatException catch (err) {
        _errorMessage = 'Ошибка декодирования подтверждения: ${err.message}';
        notifyListeners();
      }
    } else if (envelope.kind == MessageKind.heartbeat) {
      _statusMessage = 'Соединение активно (Heartbeat #${envelope.messageId})';
      notifyListeners();
    }
  }

  void _upsertNotification(NotificationEvent event) {
    if (!_notificationFilter.allows(event)) {
      return;
    }
    final existingIdx = _notifications.indexWhere(
      (item) => item.id == event.id,
    );
    if (existingIdx >= 0) {
      _notifications[existingIdx] = event;
    } else {
      _notifications.add(event);
    }
    _notifications.sort((a, b) => b.id.compareTo(a.id));
    if (_notifications.length > 100) {
      _notifications.removeRange(100, _notifications.length);
    }
  }

  void _upsertApproval(ApprovalRequest request) {
    _submittingApprovalIds.remove(request.requestId);
    final existingIdx = _approvals.indexWhere(
      (item) => item.requestId == request.requestId,
    );
    if (existingIdx >= 0) {
      _approvals[existingIdx] = request;
    } else {
      _approvals.insert(0, request);
    }
    final pending = _approvals.where((item) => item.isPending).toList();
    final settled = _approvals
        .where((item) => !item.isPending)
        .take(50)
        .toList();
    _approvals
      ..clear()
      ..addAll(pending)
      ..addAll(settled);
  }

  void _appendHistory(ListQueue<double> queue, double sample) {
    queue.addLast(sample);
    while (queue.length > _maxHistorySamples) {
      queue.removeFirst();
    }
  }

  void _scheduleReconnect() {
    if (_disposed || _manualDisconnect || _session == null) {
      return;
    }
    _reconnectTimer?.cancel();
    _connectionState = TransportConnectionState.reconnecting;
    final delaySecs = min(15, 1 << min(_reconnectAttempts, 4));
    _reconnectAttempts++;
    _statusMessage = 'Повторное подключение через $delaySecs с...';
    _reconnectTimer = Timer(Duration(seconds: delaySecs), () {
      if (!_disposed && !_manualDisconnect && _session != null) {
        unawaited(connectActiveSession());
      }
    });
  }

  void _setError(String message) {
    _errorMessage = message;
    _statusMessage = null;
    notifyListeners();
  }

  String _generateDeviceId() {
    final rng = Random.secure();
    final suffix = List<String>.generate(
      6,
      (_) => rng.nextInt(16).toRadixString(16),
    ).join();
    return 'mobile-$suffix';
  }

  @override
  void dispose() {
    _disposed = true;
    _reconnectTimer?.cancel();
    unawaited(_stateSub.cancel());
    unawaited(_envelopeSub.cancel());
    unawaited(_transport.dispose());
    super.dispose();
  }
}
