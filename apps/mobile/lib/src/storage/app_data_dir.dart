import 'dart:io';

import 'package:flutter/services.dart';

const MethodChannel _defaultStorageChannel = MethodChannel(
  'org.example.nexdeck_mobile/storage',
);

/// Resolves a writable application data directory across Android, iOS, and desktop OSes.
Future<Directory> resolveMobileDataDir({
  MethodChannel? channel,
  Map<String, String>? environment,
  bool? isAndroidOverride,
  bool? isIosOverride,
  Directory? systemTempOverride,
}) async {
  final sep = Platform.pathSeparator;
  final env = environment ?? Platform.environment;
  final isAndroid = isAndroidOverride ?? Platform.isAndroid;
  final isIos = isIosOverride ?? Platform.isIOS;
  final tempDir = systemTempOverride ?? Directory.systemTemp;

  if (isAndroid) {
    final activeChannel = channel ?? _defaultStorageChannel;
    try {
      final nativePath = await activeChannel.invokeMethod<String>(
        'getAppDataDir',
      );
      if (nativePath != null && nativePath.trim().isNotEmpty) {
        return Directory(
          '${_trimTrailingSeparator(nativePath, sep)}${sep}NexDeckMobile',
        );
      }
    } on MissingPluginException {
      // Headless Dart/widget test environment without Android MethodChannel.
    } on PlatformException {
      // Fall through to writable temporary fallback if platform query fails.
    }
    return Directory(
      '${_trimTrailingSeparator(tempDir.path, sep)}${sep}NexDeckMobile',
    );
  }

  if (isIos) {
    final home = _nonRootPath(env['HOME']);
    if (home != null) {
      return Directory(
        '${_trimTrailingSeparator(home, sep)}${sep}Documents${sep}NexDeckMobile',
      );
    }
    return Directory(
      '${_trimTrailingSeparator(tempDir.path, sep)}${sep}NexDeckMobile',
    );
  }

  final base =
      _nonRootPath(env['LOCALAPPDATA']) ??
      _nonRootPath(env['XDG_DATA_HOME']) ??
      _nonRootPath(env['HOME']) ??
      _trimTrailingSeparator(tempDir.path, sep);
  return Directory('${_trimTrailingSeparator(base, sep)}${sep}NexDeckMobile');
}

/// Shared atomic JSON file helper backed either by an explicit [File] or [resolveMobileDataDir].
class AppJsonFile {
  /// Targets an explicit fixed [file] (primarily for tests).
  AppJsonFile.fixed(File file)
    : _fixedFile = file,
      _fileName = null,
      _dirResolver = null;

  /// Resolves `<appDataDir>/<fileName>` lazily on first async I/O call.
  AppJsonFile.inAppDataDir(this._fileName, {this._dirResolver})
    : _fixedFile = null;

  final File? _fixedFile;
  final String? _fileName;
  final Future<Directory> Function()? _dirResolver;
  File? _resolved;

  /// Resolves and caches the target [File].
  Future<File> resolveFile() async {
    final cached = _resolved;
    if (cached != null) {
      return cached;
    }
    final fixed = _fixedFile;
    if (fixed != null) {
      _resolved = fixed;
      return fixed;
    }
    final dir = await (_dirResolver ?? resolveMobileDataDir)();
    final sep = Platform.pathSeparator;
    final target = File(
      '${_trimTrailingSeparator(dir.path, sep)}$sep$_fileName',
    );
    _resolved = target;
    return target;
  }

  /// Reads raw UTF-8 text if the target file exists and is non-empty.
  Future<String?> readNonEmptyString() async {
    final file = await resolveFile();
    if (!await file.exists()) {
      return null;
    }
    final content = await file.readAsString();
    if (content.trim().isEmpty) {
      return null;
    }
    return content;
  }

  /// Atomically persists [content] using a temporary sibling file and rename.
  Future<void> writeStringAtomic(String content) async {
    final file = await resolveFile();
    final parent = file.parent;
    if (!await parent.exists()) {
      await parent.create(recursive: true);
    }
    final stamp = DateTime.now().microsecondsSinceEpoch;
    final tempFile = File('${file.path}.tmp.$pid.$stamp');
    await tempFile.writeAsString(content, flush: true);
    await tempFile.rename(file.path);
  }

  /// Deletes the target file if it exists.
  Future<void> deleteIfExists() async {
    final file = await resolveFile();
    if (await file.exists()) {
      await file.delete();
    }
  }
}

String? _nonRootPath(String? candidate) {
  if (candidate == null) {
    return null;
  }
  final trimmed = candidate.trim();
  if (trimmed.isEmpty || trimmed == '/' || trimmed == r'\') {
    return null;
  }
  return trimmed;
}

String _trimTrailingSeparator(String path, String sep) {
  var result = path;
  while (result.length > 1 &&
      (result.endsWith(sep) || result.endsWith('/') || result.endsWith(r'\'))) {
    result = result.substring(0, result.length - 1);
  }
  return result;
}
