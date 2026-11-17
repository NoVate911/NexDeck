import 'dart:io';

import '../models/panel_models.dart';

/// Persistence contract for storing and loading customizable [DeckConfig] layouts.
abstract interface class DeckStore {
  /// Loads the persisted [DeckConfig], or `null` if no panel file exists yet.
  Future<DeckConfig?> loadDeck();

  /// Validates and atomically persists [deck].
  Future<void> saveDeck(DeckConfig deck);

  /// Removes any saved deck file.
  Future<void> clearDeck();
}

/// Volatile [DeckStore] tailored for deterministic unit and widget tests.
class InMemoryDeckStore implements DeckStore {
  /// Creates an [InMemoryDeckStore] optionally seeded with [initialDeck].
  InMemoryDeckStore([this._deck]);

  DeckConfig? _deck;

  @override
  Future<DeckConfig?> loadDeck() async => _deck;

  @override
  Future<void> saveDeck(DeckConfig deck) async {
    deck.validate();
    _deck = deck;
  }

  @override
  Future<void> clearDeck() async {
    _deck = null;
  }
}

/// JSON file-backed [DeckStore] storing `panels.json` alongside `session.json`.
class FileDeckStore implements DeckStore {
  /// Creates a [FileDeckStore] targeting [file].
  FileDeckStore(this.file);

  /// Creates a [FileDeckStore] in the standard per-user NexDeckMobile directory.
  factory FileDeckStore.defaultLocation() {
    final separator = Platform.pathSeparator;
    final home =
        Platform.environment['LOCALAPPDATA'] ??
        Platform.environment['XDG_DATA_HOME'] ??
        Platform.environment['HOME'] ??
        Directory.systemTemp.path;
    final dir = '$home${separator}NexDeckMobile';
    return FileDeckStore(File('$dir${separator}panels.json'));
  }

  /// Target `panels.json` file.
  final File file;

  @override
  Future<DeckConfig?> loadDeck() async {
    if (!await file.exists()) {
      return null;
    }
    final content = await file.readAsString();
    if (content.trim().isEmpty) {
      return null;
    }
    return DeckConfig.importJson(content);
  }

  @override
  Future<void> saveDeck(DeckConfig deck) async {
    final encoded = deck.exportJson();
    final parent = file.parent;
    if (!await parent.exists()) {
      await parent.create(recursive: true);
    }
    final stamp = DateTime.now().microsecondsSinceEpoch;
    final tempFile = File('${file.path}.tmp.$pid.$stamp');
    await tempFile.writeAsString(encoded, flush: true);
    await tempFile.rename(file.path);
  }

  @override
  Future<void> clearDeck() async {
    if (await file.exists()) {
      await file.delete();
    }
  }
}
