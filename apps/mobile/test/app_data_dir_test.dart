import 'dart:io';

import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:nexdeck_mobile/src/models/panel_models.dart';
import 'package:nexdeck_mobile/src/storage/app_data_dir.dart';
import 'package:nexdeck_mobile/src/storage/deck_store.dart';
import 'package:nexdeck_mobile/src/storage/session_store.dart';

void main() {
  TestWidgetsFlutterBinding.ensureInitialized();

  const channel = MethodChannel('org.example.nexdeck_mobile/storage');

  tearDown(() {
    TestDefaultBinaryMessengerBinding.instance.defaultBinaryMessenger
        .setMockMethodCallHandler(channel, null);
  });

  test(
    'resolveMobileDataDir uses native MethodChannel path on Android',
    () async {
      final fakeNativeDir = await Directory.systemTemp.createTemp(
        'android_files_',
      );
      addTearDown(() async {
        if (await fakeNativeDir.exists()) {
          await fakeNativeDir.delete(recursive: true);
        }
      });

      TestDefaultBinaryMessengerBinding.instance.defaultBinaryMessenger
          .setMockMethodCallHandler(channel, (call) async {
            expect(call.method, 'getAppDataDir');
            return fakeNativeDir.path;
          });

      final resolved = await resolveMobileDataDir(
        channel: channel,
        environment: const {'HOME': '/'},
        isAndroidOverride: true,
        isIosOverride: false,
      );

      expect(
        resolved.path,
        '${fakeNativeDir.path}${Platform.pathSeparator}NexDeckMobile',
      );
    },
  );

  test('resolveMobileDataDir falls back to temp instead of root "/" when Android channel is unavailable', () async {
    final tempBase = await Directory.systemTemp.createTemp('android_fallback_');
    addTearDown(() async {
      if (await tempBase.exists()) {
        await tempBase.delete(recursive: true);
      }
    });

    final resolved = await resolveMobileDataDir(
      channel: const MethodChannel('org.example.nexdeck_mobile/unregistered'),
      environment: const {'HOME': '/'},
      isAndroidOverride: true,
      isIosOverride: false,
      systemTempOverride: tempBase,
    );

    expect(
      resolved.path,
      '${tempBase.path}${Platform.pathSeparator}NexDeckMobile',
    );
    expect(resolved.path, isNot('${Platform.pathSeparator}NexDeckMobile'));
  });

  test(
    'FileSessionStore and FileDeckStore defaultLocation persist via resolver',
    () async {
      final base = await Directory.systemTemp.createTemp('mobile_stores_');
      addTearDown(() async {
        if (await base.exists()) {
          await base.delete(recursive: true);
        }
      });

      final sessionStore = FileSessionStore.defaultLocation(
        dirResolver: () async => base,
      );
      final deckStore = FileDeckStore.defaultLocation(
        dirResolver: () async => base,
      );

      final session = HostSession(
        host: '127.0.0.1',
        port: 9443,
        fingerprintSha256: 'a' * 64,
        deviceId: 'phone-1',
        deviceName: 'ExamplePhone',
        deviceToken: 'example-token',
      );
      await sessionStore.saveSession(session);
      await deckStore.saveDeck(DeckConfig.defaultStarter());

      expect((await sessionStore.loadSession())?.deviceId, 'phone-1');
      expect((await deckStore.loadDeck())?.activePanelId, 'system-overview');
    },
  );
}
