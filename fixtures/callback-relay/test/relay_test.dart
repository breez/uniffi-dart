import 'package:test/test.dart';

import '../callback_relay.dart';
import 'support.dart';

void main() {
  group('with the relay library', () {
    test('is bundled and active', () {
      expect(UniffiCallbackRelay.endpoint, isNotNull);
    });

    test('a sync method runs inline on the Dart thread', () {
      final signer = DartSigner();
      expect(isRemoteHere(signer: signer), isTrue);
      expect(signer.calls, 1);
    });

    test('a sync method from a std::thread', () async {
      final signer = DartSigner();
      expect(await isRemoteFromThread(signer: signer), isTrue);
      expect(signer.calls, 1);
    });

    test('a sync method from a tokio task', () async {
      expect(await isRemoteFromTokio(signer: DartSigner()), isTrue);
    });

    test('32 concurrent sync calls from threads', () async {
      expect(await signFromThreads(signer: DartSigner(), count: 32), [
        for (var i = 0; i < 32; i++) 'signed:$i',
      ]);
    });

    test(
      'an error from a sync method on another thread reaches Rust',
      () async {
        await expectLater(
          signFromThread(signer: DartSigner(), message: ''),
          throwsA(isA<RejectedSignException>()),
        );
      },
    );

    test('store then load on the Dart thread run in order', () {
      final counter = DartCounter();
      expect(storeThenLoadHere(counter: counter, value: 7), 7);
      expect(counter.value, 7);
    });

    test('store then load from one thread run in order', () async {
      // store is queued without blocking and load blocks, but both go
      // through the same queue, so load sees the stored value.
      expect(
        await storeThenLoadFromThread(counter: DartCounter(), value: 9),
        9,
      );
    });

    test('a relayed callback can call Rust, which calls back inline', () async {
      final counter = DartCounter()
        ..value = 3
        ..signer = DartSigner();
      expect(await loadFromThread(counter: counter), 1003);
    });

    test('a foreign trait passed to a callback on another thread', () async {
      final counter = DartCounter();
      expect(
        await adoptFromThread(counter: counter, signer: DartSigner()),
        isTrue,
      );
      expect(counter.adopted, hasLength(1));
    });

    test('an async method from a tokio task', () async {
      final counter = DartCounter()..value = 1;
      expect(await addLaterFromTokio(counter: counter, amount: 5), 6);
    });

    test('a callback dropped on a worker thread', () async {
      await dropFromThread(counter: DartCounter());
    });
  }, skip: companion == 'bundled' ? false : 'needs the relay library');
}
