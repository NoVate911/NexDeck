import 'dart:async';

import 'package:material_ui/material_ui.dart';

import 'src/state/agent_controller.dart';
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
  /// Creates a [NexDeckApp], optionally injecting a prebuilt [controller] for tests.
  const NexDeckApp({this.controller, this.autoInitialize = true, super.key});

  /// Optional injected [AgentController].
  final AgentController? controller;

  /// Whether to trigger [AgentController.initialize] on mount.
  final bool autoInitialize;

  @override
  State<NexDeckApp> createState() => _NexDeckAppState();
}

class _NexDeckAppState extends State<NexDeckApp> {
  late final AgentController _controller;
  late final bool _ownsController;

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

    if (widget.autoInitialize) {
      unawaited(_controller.initialize());
    }
  }

  @override
  void dispose() {
    if (_ownsController) {
      _controller.dispose();
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
          return DashboardScreen(controller: _controller);
        },
      ),
    );
  }
}
