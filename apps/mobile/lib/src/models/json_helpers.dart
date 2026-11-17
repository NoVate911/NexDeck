/// Strongly-typed JSON decoding helpers avoiding unchecked dynamic calls.
library;

/// Reads a required non-null value of type [T] from [json] under [key].
T readRequired<T extends Object>(Map<String, Object?> json, String key) {
  final value = json[key];
  if (value is T) {
    return value;
  }
  throw FormatException('Missing or invalid "$key" field (expected $T).');
}

/// Reads an optional value of type [T] from [json] under [key].
T? readOptional<T extends Object>(Map<String, Object?> json, String key) {
  final value = json[key];
  if (value == null) {
    return null;
  }
  if (value is T) {
    return value;
  }
  throw FormatException('Invalid optional "$key" field (expected $T?).');
}

/// Reads a required finite [double] from an integer or floating JSON number.
double readRequiredDouble(Map<String, Object?> json, String key) {
  final value = json[key];
  if (value is num && value.isFinite) {
    return value.toDouble();
  }
  throw FormatException('Missing or invalid numeric "$key" field.');
}

/// Reads an optional finite [double] from an integer or floating JSON number.
double? readOptionalDouble(Map<String, Object?> json, String key) {
  final value = json[key];
  if (value == null) {
    return null;
  }
  if (value is num && value.isFinite) {
    return value.toDouble();
  }
  throw FormatException('Invalid optional numeric "$key" field.');
}

/// Reads a JSON object under [key] into a typed string-keyed map.
Map<String, Object?> readRequiredMap(Map<String, Object?> json, String key) {
  final raw = json[key];
  return asJsonMap(raw, context: key);
}

/// Reads a list of JSON objects under [key].
List<Map<String, Object?>> readObjectList(
  Map<String, Object?> json,
  String key,
) {
  final raw = json[key];
  if (raw == null) {
    return const [];
  }
  if (raw is! List<Object?>) {
    throw FormatException('Invalid "$key" list field.');
  }
  return [
    for (var i = 0; i < raw.length; i++) asJsonMap(raw[i], context: '$key[$i]'),
  ];
}

/// Reads a list of finite numbers under [key] as doubles.
List<double> readDoubleList(Map<String, Object?> json, String key) {
  final raw = json[key];
  if (raw == null) {
    return const [];
  }
  if (raw is! List<Object?>) {
    throw FormatException('Invalid "$key" numeric list field.');
  }
  return [
    for (final entry in raw)
      if (entry is num && entry.isFinite)
        entry.toDouble()
      else
        throw FormatException('Invalid numeric element inside "$key".'),
  ];
}

/// Converts a decoded JSON object into a string-keyed map.
Map<String, Object?> asJsonMap(Object? raw, {String context = 'root'}) {
  if (raw is Map<Object?, Object?>) {
    final result = <String, Object?>{};
    for (final entry in raw.entries) {
      final key = entry.key;
      if (key is! String) {
        throw FormatException('Non-string JSON key inside "$context".');
      }
      result[key] = entry.value;
    }
    return result;
  }
  throw FormatException('Expected JSON object at "$context".');
}
