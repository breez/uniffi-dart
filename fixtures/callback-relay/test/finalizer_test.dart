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
}
