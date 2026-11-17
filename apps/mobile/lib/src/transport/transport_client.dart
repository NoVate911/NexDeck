import 'dart:async';
import 'dart:convert';
import 'dart:io';

import '../models/json_helpers.dart';
import '../models/protocol_models.dart';
import '../storage/session_store.dart';
import 'pinned_http_client.dart';

/// Connection lifecycle state aligned with `TransportState` in `crates/transport`.
enum TransportConnectionState {
  /// No active connection to the PC host.
  disconnected,

  /// Initial TLS / WebSocket handshake in progress.
  connecting,

  /// Authenticated session ready for frames.
  connected,

  /// Temporary disconnect while waiting to reconnect.
  reconnecting,
}

/// Transport-agnostic client interface for exchanging [Envelope] frames with `Core Agent`.
abstract interface class NexDeckTransport {
  /// Most recent connection state.
  TransportConnectionState get currentState;

  /// Broadcast stream of connection state changes.
  Stream<TransportConnectionState> get states;

  /// Broadcast stream of validated inbound protocol [Envelope] frames.
  Stream<Envelope> get envelopes;

  /// Opens an authenticated channel to [session].
  Future<void> connect(HostSession session);

  /// Encodes, validates, and sends an outbound [Envelope].
  Future<void> sendEnvelope(Envelope envelope);

  /// Closes any active session cleanly.
  Future<void> disconnect();

  /// Releases all streams and socket resources.
  Future<void> dispose();
}

/// Wi-Fi WSS transport backed by `dart:io` [WebSocket] and pinned TLS verification.
class WssNexDeckTransport implements NexDeckTransport {
  /// Creates a [WssNexDeckTransport].
  WssNexDeckTransport({this.handshakeTimeout = const Duration(seconds: 6)});

  /// Timeout used for TLS + WebSocket upgrade.
  final Duration handshakeTimeout;

  final StreamController<TransportConnectionState> _stateController =
      StreamController<TransportConnectionState>.broadcast();
  final StreamController<Envelope> _envelopeController =
      StreamController<Envelope>.broadcast();

  TransportConnectionState _currentState =
      TransportConnectionState.disconnected;
  WebSocket? _socket;
  HttpClient? _httpClient;
  StreamSubscription<Object?>? _socketSubscription;
  bool _disposed = false;

  @override
  TransportConnectionState get currentState => _currentState;

  @override
  Stream<TransportConnectionState> get states => _stateController.stream;

  @override
  Stream<Envelope> get envelopes => _envelopeController.stream;

  @override
  Future<void> connect(HostSession session) async {
    if (_disposed) {
      throw StateError('WssNexDeckTransport has already been disposed.');
    }
    await disconnect();
    _emitState(TransportConnectionState.connecting);

    final httpClient = createPinnedHttpClient(
      expectedFingerprintSha256: session.fingerprintSha256,
      connectionTimeout: handshakeTimeout,
    );
    _httpClient = httpClient;

    try {
      // Closed cleanly in disconnect() / dispose().
      // ignore: close_sinks
      final socket = await WebSocket.connect(
        session.wssUri.toString(),
        headers: {
          HttpHeaders.authorizationHeader: 'Bearer ${session.deviceToken}',
        },
        customClient: httpClient,
      ).timeout(handshakeTimeout);

      socket.pingInterval = const Duration(seconds: 15);
      _socket = socket;
      _emitState(TransportConnectionState.connected);

      _socketSubscription = socket.listen(
        _handleSocketData,
        onError: (Object error, StackTrace stackTrace) {
          if (!_envelopeController.isClosed) {
            _envelopeController.addError(error, stackTrace);
          }
          _cleanupSocketOnly();
          _emitState(TransportConnectionState.disconnected);
        },
        onDone: () {
          _cleanupSocketOnly();
          _emitState(TransportConnectionState.disconnected);
        },
        cancelOnError: false,
      );
    } catch (_) {
      await _cleanupConnection();
      _emitState(TransportConnectionState.disconnected);
      rethrow;
    }
  }

  @override
  Future<void> sendEnvelope(Envelope envelope) async {
    final socket = _socket;
    if (socket == null || _currentState != TransportConnectionState.connected) {
      throw StateError('Cannot send envelope while disconnected.');
    }
    envelope.validate();
    socket.add(jsonEncode(envelope.toJson()));
  }

  @override
  Future<void> disconnect() async {
    await _cleanupConnection();
    _emitState(TransportConnectionState.disconnected);
  }

  @override
  Future<void> dispose() async {
    if (_disposed) {
      return;
    }
    _disposed = true;
    await _cleanupConnection();
    await _stateController.close();
    await _envelopeController.close();
  }

  void _handleSocketData(Object? data) {
    if (data is! String) {
      return;
    }
    try {
      final decoded = jsonDecode(data);
      final envelope = Envelope.fromJson(
        asJsonMap(decoded, context: 'ws.frame'),
      );
      if (!_envelopeController.isClosed) {
        _envelopeController.add(envelope);
      }
    } on FormatException catch (err, stackTrace) {
      if (!_envelopeController.isClosed) {
        _envelopeController.addError(err, stackTrace);
      }
    }
  }

  Future<void> _cleanupConnection() async {
    final sub = _socketSubscription;
    _socketSubscription = null;
    await sub?.cancel();

    final socket = _socket;
    _socket = null;
    if (socket != null) {
      try {
        await socket.close();
      } on Object {
        // Best-effort close on teardown.
      }
    }

    _httpClient?.close(force: true);
    _httpClient = null;
  }

  void _cleanupSocketOnly() {
    unawaited(_socketSubscription?.cancel());
    _socketSubscription = null;
    _socket = null;
    _httpClient?.close(force: true);
    _httpClient = null;
  }

  void _emitState(TransportConnectionState next) {
    _currentState = next;
    if (!_disposed && !_stateController.isClosed) {
      _stateController.add(next);
    }
  }
}
