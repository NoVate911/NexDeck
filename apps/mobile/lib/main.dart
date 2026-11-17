import 'dart:async';

import 'package:material_ui/material_ui.dart';

import 'src/state/agent_controller.dart';
import 'src/state/deck_controller.dart';
import 'src/storage/deck_store.dart';
import 'src/storage/session_store.dart';
import 'src/transport/pinned_http_client.dart';
import 'src/transport/transport_client.dart';
import 'src/ui/dashboard_screen.dart';
import 'src/ui/pairing_screen.dart';

void main() {
  runApp(const NexDeckApp());
}

/// Root Flutter application widget for the NexDeck mobile client.
class NexDeckApp extends StatefulWidget {
  /// Creates a [NexDeckApp], optionally injecting prebuilt controllers for tests.
  const NexDeckApp({
    this.controller,
    this.deckController,
    this.autoInitialize = true,
    super.key,
  });

  /// Optional injected [AgentController].
  final AgentController? controller;

  /// Optional injected [DeckController].
  final DeckController? deckController;

  /// Whether to trigger controller initialization on mount.
  final bool autoInitialize;

  @override
  State<NexDeckApp> createState() => _NexDeckAppState();
}

class _NexDeckAppState extends State<NexDeckApp> {
  late final AgentController _controller;
  late final bool _ownsController;
  late final DeckController _deckController;
  late final bool _ownsDeckController;

  @override
  void initState() {
    super.initState();
    final provided = widget.controller;
    if (provided != null) {
      _controller = provided;
      _ownsController = false;
    } else {
      _controller = AgentController(
        sessionStore: FileSessionStore.defaultLocation(),
        pairingClient: const PinnedHttpPairingClient(),
        transport: WssNexDeckTransport(),
      );
      _ownsController = true;
    }

    final providedDeck = widget.deckController;
    if (providedDeck != null) {
      _deckController = providedDeck;
      _ownsDeckController = false;
    } else {
      _deckController = DeckController(
        deckStore: FileDeckStore.defaultLocation(),
      );
      _ownsDeckController = true;
    }

    _controller.addListener(_forwardTelemetryToDeck);

    if (widget.autoInitialize) {
      unawaited(_controller.initialize());
      unawaited(_deckController.initialize());
    }
  }

  void _forwardTelemetryToDeck() {
    _deckController.syncTelemetry(_controller.latestSnapshot);
  }

  @override
  void dispose() {
    _controller.removeListener(_forwardTelemetryToDeck);
    if (_ownsController) {
      _controller.dispose();
    }
    if (_ownsDeckController) {
      _deckController.dispose();
    }
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    return MaterialApp(
      title: 'NexDeck',
      theme: ThemeData(
        colorScheme: ColorScheme.fromSeed(
          seedColor: const Color(0xFF0EA5E9),
          brightness: Brightness.dark,
        ),
        useMaterial3: true,
      ),
      home: AnimatedBuilder(
        animation: _controller,
        builder: (context, _) {
          if (_controller.session == null) {
            return PairingScreen(controller: _controller);
          }
          return DashboardScreen(
            controller: _controller,
            deckController: _deckController,
          );
        },
      ),
    );
  }
}
