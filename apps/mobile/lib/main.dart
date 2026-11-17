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
    final baseScheme =
        ColorScheme.fromSeed(
          seedColor: const Color(0xFF3B82F6),
          brightness: Brightness.dark,
        ).copyWith(
          primary: const Color(0xFF3B82F6),
          secondary: const Color(0xFF60A5FA),
          surface: const Color(0xFF161D29),
          surfaceContainerLowest: const Color(0xFF0F141E),
          surfaceContainerLow: const Color(0xFF161D29),
          surfaceContainer: const Color(0xFF1B2433),
          surfaceContainerHigh: const Color(0xFF222D40),
          outlineVariant: const Color(0xFF283548),
        );

    return MaterialApp(
      title: 'NexDeck',
      debugShowCheckedModeBanner: false,
      theme: ThemeData(
        colorScheme: baseScheme,
        scaffoldBackgroundColor: const Color(0xFF0D1117),
        useMaterial3: true,
        appBarTheme: const AppBarTheme(
          backgroundColor: Color(0xFF0D1117),
          surfaceTintColor: Colors.transparent,
          elevation: 0,
          centerTitle: false,
          titleTextStyle: TextStyle(
            fontSize: 19,
            fontWeight: FontWeight.w700,
            color: Color(0xFFF1F5F9),
            letterSpacing: -0.2,
          ),
        ),
        cardTheme: CardThemeData(
          color: const Color(0xFF161D29),
          surfaceTintColor: Colors.transparent,
          elevation: 0,
          margin: EdgeInsets.zero,
          shape: RoundedRectangleBorder(
            borderRadius: BorderRadius.circular(18),
            side: const BorderSide(color: Color(0xFF283548)),
          ),
        ),
        navigationBarTheme: NavigationBarThemeData(
          backgroundColor: const Color(0xFF121823),
          surfaceTintColor: Colors.transparent,
          indicatorColor: const Color(0x333B82F6),
          height: 66,
          labelBehavior: NavigationDestinationLabelBehavior.alwaysShow,
        ),
        inputDecorationTheme: InputDecorationTheme(
          filled: true,
          fillColor: const Color(0xFF111722),
          border: OutlineInputBorder(
            borderRadius: BorderRadius.circular(12),
            borderSide: const BorderSide(color: Color(0xFF283548)),
          ),
          enabledBorder: OutlineInputBorder(
            borderRadius: BorderRadius.circular(12),
            borderSide: const BorderSide(color: Color(0xFF283548)),
          ),
          focusedBorder: OutlineInputBorder(
            borderRadius: BorderRadius.circular(12),
            borderSide: const BorderSide(color: Color(0xFF3B82F6), width: 1.5),
          ),
        ),
      ),
      home: AnimatedBuilder(
        animation: _controller,
        builder: (context, _) {
          return AnimatedSwitcher(
            duration: const Duration(milliseconds: 240),
            switchInCurve: Curves.easeOutCubic,
            switchOutCurve: Curves.easeInCubic,
            child: _controller.session == null
                ? PairingScreen(
                    key: const ValueKey('pairing_screen'),
                    controller: _controller,
                  )
                : DashboardScreen(
                    key: const ValueKey('dashboard_screen'),
                    controller: _controller,
                    deckController: _deckController,
                  ),
          );
        },
      ),
    );
  }
}
