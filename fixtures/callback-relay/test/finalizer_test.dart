import 'dart:isolate';

import 'package:test/test.dart';

import '../callback_relay.dart';
import 'support.dart';

/// Makes a Droppable that nothing refers to once this returns.
void _dropOne() {
  Droppable();
}

/// Allocates until [done], or for at most 30 seconds, so that the garbage
/// collector runs, and yields so that a `Finalizer` callback can run too.
Future<void> _collectUntil(bool Function() done) async {
  final deadline = DateTime.now().add(const Duration(seconds: 30));
  while (!done() && DateTime.now().isBefore(deadline)) {
    var garbage = List<List<int>>.generate(1000, (i) => List.filled(1000, i));
    garbage = const [];
    await Future<void>.delayed(const Duration(milliseconds: 10));
  }
}

/// Starts an async call that holds a Droppable for ten minutes, then signals.
Future<void> _holdThenSignal(SendPort started) async {
  final call = Droppable().holdFor(ms: 600000);
  started.send(null);
  await call;
}

void main() {
  test('an unreachable object is freed', () async {
    final before = droppedCount();
    _dropOne();
    await _collectUntil(() => droppedCount() > before);
    expect(droppedCount(), before + 1);
  });

  test(
    'the objects of an exited isolate are freed',
    () async {
      final before = droppedCount();
      // A Finalizer's callback would have to run in that isolate, which has
      // gone. A NativeFinalizer runs anyway, and the relay frees the object.
      await Isolate.run(_dropOne);
      await _collectUntil(() => droppedCount() > before);
      expect(droppedCount(), before + 1);
    },
    skip: companion == 'bundled' ? false : 'needs the relay library',
  );

  test(
    'an async call in flight when its isolate dies releases what it holds',
    () async {
      final before = droppedCount();
      final started = ReceivePort();
      final isolate = await Isolate.spawn(_holdThenSignal, started.sendPort);
      await started.first;
      isolate.kill(priority: Isolate.immediate);
      // Dart's own reference is freed with the object, but the call's Rust
      // future holds another until the future itself is freed.
      await _collectUntil(() => droppedCount() > before);
      expect(droppedCount(), before + 1);
    },
    skip: companion == 'bundled' ? false : 'needs the relay library',
  );
}
