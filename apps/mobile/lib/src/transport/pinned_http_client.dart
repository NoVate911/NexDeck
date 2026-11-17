import 'dart:convert';
import 'dart:io';

import 'package:crypto/crypto.dart';

import '../models/json_helpers.dart';
import '../models/protocol_models.dart';

/// Computes the lowercase SHA-256 hex digest of DER-encoded X.509 certificate bytes.
String computeCertificateSha256(List<int> derBytes) {
  return sha256.convert(derBytes).toString().toLowerCase();
}

/// Constant-time equality comparison for normalized 64-char SHA-256 hex digests.
bool constantTimeHexEquals(String left, String right) {
  final a = normalizeFingerprintSha256(left);
  final b = normalizeFingerprintSha256(right);
  if (a.length != 64 || b.length != 64) {
    return false;
  }
  final aBytes = utf8.encode(a);
  final bBytes = utf8.encode(b);
  if (aBytes.length != bBytes.length) {
    return false;
  }
  var diff = 0;
  for (var i = 0; i < aBytes.length; i++) {
    diff |= aBytes[i] ^ bBytes[i];
  }
  return diff == 0;
}

/// Verifies that [cert] matches [expectedFingerprintSha256] by SHA-256 of DER bytes.
bool verifyPinnedCertificate(
  X509Certificate cert,
  String expectedFingerprintSha256,
) {
  final actualSha256 = computeCertificateSha256(cert.der);
  return constantTimeHexEquals(actualSha256, expectedFingerprintSha256);
}

/// Creates a [HttpClient] configured strictly for pinned self-signed host certificates.
///
/// Using an empty [SecurityContext] with no trusted roots ensures every server
/// certificate passes through [HttpClient.badCertificateCallback] and is pinned
/// exclusively against [expectedFingerprintSha256].
HttpClient createPinnedHttpClient({
  required String expectedFingerprintSha256,
  Duration connectionTimeout = const Duration(seconds: 5),
}) {
  final normalized = normalizeFingerprintSha256(expectedFingerprintSha256);
  if (!isValidFingerprintSha256(normalized)) {
    throw const FormatException(
      'Expected SHA-256 fingerprint must be a 64-character hex string.',
    );
  }

  final context = SecurityContext(withTrustedRoots: false);
  final client = HttpClient(context: context)
    ..connectionTimeout = connectionTimeout
    ..badCertificateCallback = (X509Certificate cert, String host, int port) {
      return verifyPinnedCertificate(cert, normalized);
    };
  return client;
}

/// Error raised during `/health` or `/pair` communication with `nexdeck-agent`.
class PairingClientException implements Exception {
  /// Creates a [PairingClientException].
  const PairingClientException(this.message, {this.statusCode});

  /// Human-readable diagnostic error message.
  final String message;

  /// Optional HTTP status code returned by the agent.
  final int? statusCode;

  @override
  String toString() =>
      statusCode == null ? message : 'HTTP $statusCode: $message';
}

/// Abstraction over host health verification and PIN pairing requests.
abstract interface class PairingClient {
  /// Queries `GET /health` with certificate pinning against [expectedFingerprintSha256].
  Future<HealthResponse> checkHealth(
    Uri baseUri, {
    required String expectedFingerprintSha256,
  });

  /// Submits [request] to `POST /pair` with certificate pinning against [expectedFingerprintSha256].
  Future<PairedCredentials> pair({
    required Uri baseUri,
    required String expectedFingerprintSha256,
    required PairRequest request,
  });
}

/// Default [PairingClient] implementation backed by `dart:io` [HttpClient] with TLS pinning.
class PinnedHttpPairingClient implements PairingClient {
  /// Creates a [PinnedHttpPairingClient].
  const PinnedHttpPairingClient({
    this.requestTimeout = const Duration(seconds: 6),
  });

  /// Timeout applied to network requests.
  final Duration requestTimeout;

  @override
  Future<HealthResponse> checkHealth(
    Uri baseUri, {
    required String expectedFingerprintSha256,
  }) async {
    final normalizedExpected = normalizeFingerprintSha256(
      expectedFingerprintSha256,
    );
    String? observedCertSha256;
    final HttpClient client;
    if (normalizedExpected.isEmpty) {
      final context = SecurityContext(withTrustedRoots: false);
      client = HttpClient(context: context)
        ..connectionTimeout = requestTimeout
        ..badCertificateCallback =
            (X509Certificate cert, String host, int port) {
              observedCertSha256 = computeCertificateSha256(cert.der);
              return true;
            };
    } else {
      client = createPinnedHttpClient(
        expectedFingerprintSha256: normalizedExpected,
        connectionTimeout: requestTimeout,
      );
    }
    try {
      final uri = baseUri.replace(path: '/health');
      final req = await client.getUrl(uri).timeout(requestTimeout);
      req.headers.set(HttpHeaders.acceptHeader, 'application/json');
      final response = await req.close().timeout(requestTimeout);
      final body = await utf8.decodeStream(response).timeout(requestTimeout);

      if (response.statusCode != HttpStatus.ok) {
        throw PairingClientException(
          'Сервер вернул неожиданный статус на /health: ${response.statusCode}',
          statusCode: response.statusCode,
        );
      }

      final decoded = jsonDecode(body);
      final health = HealthResponse.fromJson(
        asJsonMap(decoded, context: 'health'),
      );
      if (health.status != 'ok') {
        throw PairingClientException(
          'Агент сообщил о неготовности: ${health.status}',
        );
      }
      if (health.protocolVersion != nexdeckProtocolVersion) {
        throw PairingClientException(
          'Несовместимая версия протокола агента: ${health.protocolVersion}',
        );
      }
      final checkTarget = normalizedExpected.isNotEmpty
          ? normalizedExpected
          : (observedCertSha256 ?? '');
      if (!constantTimeHexEquals(health.fingerprintSha256, checkTarget)) {
        throw const PairingClientException(
          'Отпечаток сертификата в ответе /health не совпадает с TLS-соединением.',
        );
      }
      return health;
    } on PairingClientException {
      rethrow;
    } on HandshakeException catch (err) {
      throw PairingClientException(
        'TLS-проверка сертификата не пройдена: ${err.message}',
      );
    } on SocketException catch (err) {
      throw PairingClientException('Не удалось подключиться к хосту: $err');
    } finally {
      client.close(force: true);
    }
  }

  @override
  Future<PairedCredentials> pair({
    required Uri baseUri,
    required String expectedFingerprintSha256,
    required PairRequest request,
  }) async {
    final client = createPinnedHttpClient(
      expectedFingerprintSha256: expectedFingerprintSha256,
      connectionTimeout: requestTimeout,
    );
    try {
      final uri = baseUri.replace(path: '/pair');
      final req = await client.postUrl(uri).timeout(requestTimeout);
      req.headers.contentType = ContentType.json;
      req.headers.set(HttpHeaders.acceptHeader, 'application/json');
      req.write(jsonEncode(request.toJson()));

      final response = await req.close().timeout(requestTimeout);
      final body = await utf8.decodeStream(response).timeout(requestTimeout);

      if (response.statusCode == HttpStatus.ok) {
        final decoded = jsonDecode(body);
        return PairedCredentials.fromJson(asJsonMap(decoded, context: 'pair'));
      }
      if (response.statusCode == HttpStatus.unauthorized) {
        throw const PairingClientException(
          'Неверный PIN-код сопряжения.',
          statusCode: HttpStatus.unauthorized,
        );
      }
      if (response.statusCode == HttpStatus.gone) {
        throw const PairingClientException(
          'Сессия сопряжения истекла или исчерпана. Запустите агента с --pair заново.',
          statusCode: HttpStatus.gone,
        );
      }
      if (response.statusCode == HttpStatus.badRequest) {
        throw PairingClientException(
          'Некорректные данные устройства: ${body.trim()}',
          statusCode: HttpStatus.badRequest,
        );
      }
      throw PairingClientException(
        'Ошибка сопряжения (${response.statusCode}): ${body.trim()}',
        statusCode: response.statusCode,
      );
    } on PairingClientException {
      rethrow;
    } on HandshakeException catch (err) {
      throw PairingClientException(
        'TLS-проверка сертификата не пройдена: ${err.message}',
      );
    } on SocketException catch (err) {
      throw PairingClientException('Ошибка сети при сопряжении: $err');
    } finally {
      client.close(force: true);
    }
  }
}
