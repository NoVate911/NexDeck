import 'dart:io';

import '../models/panel_models.dart';
import 'app_data_dir.dart';

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
  FileDeckStore(File file) : _storage = AppJsonFile.fixed(file);

  /// Creates a [FileDeckStore] resolved via [resolveMobileDataDir].
  FileDeckStore.defaultLocation({Future<Directory> Function()? dirResolver})
    : _storage = AppJsonFile.inAppDataDir(
        'panels.json',
        dirResolver: dirResolver,
      );

  final AppJsonFile _storage;

  @override
  Future<DeckConfig?> loadDeck() async {
    final content = await _storage.readNonEmptyString();
    if (content == null) {
      return null;
    }
    return DeckConfig.importJson(content);
  }

  @override
  Future<void> saveDeck(DeckConfig deck) async {
    final encoded = deck.exportJson();
    await _storage.writeStringAtomic(encoded);
  }

  @override
  Future<void> clearDeck() => _storage.deleteIfExists();
}
