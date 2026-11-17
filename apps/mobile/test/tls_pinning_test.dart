import 'dart:convert';
import 'dart:io';

import 'package:flutter_test/flutter_test.dart';
import 'package:nexdeck_mobile/src/transport/pinned_http_client.dart';

void main() {
  group('TLS fingerprint verification', () {
    test('computes deterministic SHA-256 hex digest from DER bytes', () {
      final digest = computeCertificateSha256(utf8.encode('example-der-cert'));
      expect(digest.length, 64);
      expect(digest, computeCertificateSha256(utf8.encode('example-der-cert')));
      expect(
        digest,
        isNot(computeCertificateSha256(utf8.encode('other-der-cert'))),
      );
    });

    test(
      'constantTimeHexEquals normalizes separators and rejects mismatches',
      () {
        final digest = computeCertificateSha256(
          utf8.encode('sample-host-cert'),
        );
        final colonFormatted = [
          for (var i = 0; i < digest.length; i += 2) digest.substring(i, i + 2),
        ].join(':').toUpperCase();

        expect(constantTimeHexEquals(digest, colonFormatted), isTrue);

        final altered =
            '${digest.substring(0, 63)}${digest.endsWith('0') ? '1' : '0'}';
        expect(constantTimeHexEquals(digest, altered), isFalse);
        expect(constantTimeHexEquals(digest, 'short'), isFalse);
        expect(constantTimeHexEquals(digest, 'z' * 64), isFalse);
      },
    );

    test('createPinnedHttpClient validates fingerprint format', () {
      expect(
        () => createPinnedHttpClient(expectedFingerprintSha256: 'bad'),
        throwsFormatException,
      );

      final client = createPinnedHttpClient(
        expectedFingerprintSha256: computeCertificateSha256(
          utf8.encode('trusted-self-signed-example'),
        ),
      );
      expect(client, isA<HttpClient>());
      client.close(force: true);
    });
  });
}
