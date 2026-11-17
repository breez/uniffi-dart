import 'dart:isolate';

import 'package:test/test.dart';

import '../callback_relay.dart';
import 'support.dart';

void main() {
  test('callbacks of an exited isolate fail instead of aborting', () async {
    // The spawned isolate installs Keeper's vtable and exits while Rust
    // still holds its callback, as the old isolate does in a hot restart.
    await Isolate.run(() => keep(keeper: DartKeeper(1)));
    expect(await keptValueFromThread(), isNull);
    await keptNoteFromThread(value: 2);
    expect(await keptValueLaterFromTokio(), isNull);
    // The free goes to the exited isolate too.
    releaseKept();

    // A new isolate takes the vtable over, as after a hot restart.
    final restored = await Isolate.run(() async {
      keep(keeper: DartKeeper(5));
      final value = await keptValueFromThread();
      releaseKept();
      return value;
    });
    expect(restored, 5);
  }, skip: companion == 'bundled' ? false : 'needs the relay library');
}
