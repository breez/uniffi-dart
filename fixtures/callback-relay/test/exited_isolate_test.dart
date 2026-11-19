import 'dart:async';
import 'dart:isolate';

import 'package:test/test.dart';

import '../callback_relay.dart';
import 'support.dart';

/// Starts an async Rust call, signals, and waits for the call to finish.
Future<void> _finishLaterAfterSignal(SendPort started) async {
  final finishing = finishLater(ms: 200);
  started.send(null);
  await finishing;
}

void main() {
  test('callbacks of an exited isolate fail instead of aborting', () async {
    // The spawned isolate installs Keeper's vtable and exits while Rust
    // still holds its callback, as the old isolate does in a hot restart.
    await Isolate.run(() => keep(keeper: DartKeeper(1)));
    // Calls that return something fail. Calls that return nothing and can't
    // throw are dropped as if they had succeeded: an unexpected error would
    // make Rust panic.
    expect(await keptValueFromThread(), isNull);
    expect(await keptValueLaterFromTokio(), isNull);
    expect(await keptNoteFromThread(value: 2), isTrue);
    expect(await keptNoteLaterFromTokio(value: 3), isTrue);
    // The free goes to the exited isolate too.
    releaseKept();

    // Another isolate keeps a callback and exits. A new isolate registers its
    // own, as after a hot restart. Calls through the old callback still go to
    // the exited isolate, so they fail or are dropped, and never reach the new
    // one.
    await Isolate.run(() => keep(keeper: DartKeeper(1)));
    final restored = await Isolate.run(() async {
      final mine = DartKeeper(5);
      noteWith(keeper: mine, value: 4);
      final dropped = [
        await keptNoteFromThread(value: 6),
        await keptNoteLaterFromTokio(value: 7),
      ];
      final failed = await keptValueFromThread();
      // Replacing the old callback frees it, in the exited isolate.
      keep(keeper: mine);
      final value = await keptValueFromThread();
      releaseKept();
      return [dropped, failed, value, mine.notes];
    });
    expect(restored, [
      [true, true],
      null,
      5,
      [4],
    ]);
  }, skip: companion == 'bundled' ? false : 'needs the relay library');

  test('an async Rust call can finish after its isolate is killed', () async {
    final started = ReceivePort();
    final isolate = await Isolate.spawn(
      _finishLaterAfterSignal,
      started.sendPort,
    );
    await started.first;
    isolate.kill(priority: Isolate.immediate);
    // The Rust future finishes on a tokio worker after the isolate has gone,
    // as one does across a hot restart, and wakes its continuation.
    await Future<void>.delayed(const Duration(milliseconds: 400));
    expect(await finishLater(ms: 1), 1);
  }, skip: companion == 'bundled' ? false : 'needs the relay library');
}
