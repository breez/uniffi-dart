import 'dart:isolate';

import 'package:test/test.dart';

import '../callback_relay.dart';
import 'support.dart';

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

    // Another isolate keeps a callback and exits. A new isolate takes the
    // vtable over, as after a hot restart, so calls through the old callback
    // reach the new isolate, where its handle misses.
    await Isolate.run(() => keep(keeper: DartKeeper(1)));
    final restored = await Isolate.run(() async {
      final mine = DartKeeper(5);
      noteWith(keeper: mine, value: 4);
      final dropped = [
        await keptNoteFromThread(value: 6),
        await keptNoteLaterFromTokio(value: 7),
      ];
      final failed = await keptValueFromThread();
      // Replacing the old callback frees it, which misses here too.
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
}
