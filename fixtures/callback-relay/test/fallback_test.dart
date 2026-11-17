import 'dart:async';
import 'dart:io';

import 'package:test/test.dart';

import '../callback_relay.dart';
import 'support.dart';

void main() {
  group('without the relay library', () {
    test('the relay is not active', () {
      expect(UniffiCallbackRelay.endpoint, isNull);
    });

    test('sync callbacks still run inline, and one warning names them', () {
      final printed = <String>[];
      runZoned(
        () {
          expect(isRemoteHere(signer: DartSigner()), isTrue);
          expect(storeThenLoadHere(counter: DartCounter(), value: 4), 4);
        },
        zoneSpecification: ZoneSpecification(
          print: (self, parent, zone, line) => printed.add(line),
        ),
      );
      expect(printed, hasLength(1));
      expect(
        printed.single,
        allOf(
          contains('Signer.isRemote'),
          contains('Counter.load'),
          contains('uniffi:callback_relay_dispatch'),
        ),
      );
    });

    test('a sync call from another thread aborts, as it did before', () async {
      final result = await Process.run(Platform.resolvedExecutable, [
        'run',
        'test/fallback_abort.dart',
      ]);
      expect(result.exitCode, isNot(0));
      expect(
        result.stderr,
        contains('Cannot invoke native callback outside an isolate'),
      );
    }, timeout: const Timeout(Duration(minutes: 2)));
  }, skip: companion == 'absent' ? false : 'runs without the relay library');
}
