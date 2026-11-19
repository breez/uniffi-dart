import 'dart:isolate';

import 'package:test/test.dart';

import '../callback_relay.dart';
import 'support.dart';

/// Registers a tally, then serves requests for its total until told to stop.
Future<void> _tallyIsolate(SendPort parent) async {
  final tally = DartTally('other');
  addTally(tally: tally);
  final requests = ReceivePort();
  parent.send(requests.sendPort);
  await for (final request in requests) {
    if (request is SendPort) {
      request.send(tally.total);
    } else {
      requests.close();
    }
  }
}

Future<int> _totalIn(SendPort isolate) async {
  final reply = ReceivePort();
  isolate.send(reply.sendPort);
  return await reply.first as int;
}

void main() {
  test(
    'isolates that implement the same callback type each get their own calls',
    () async {
      final ready = ReceivePort();
      final exited = ReceivePort();
      await Isolate.spawn(
        _tallyIsolate,
        ready.sendPort,
        onExit: exited.sendPort,
      );
      final other = await ready.first as SendPort;
      // Installed after the other isolate's, so before the relay routed by
      // handle, every call went to this isolate and the other's failed here.
      final mine = DartTally('main');
      addTally(tally: mine);

      expect(await tallyNamesFromThreads(), ['other', 'main']);
      expect(await addToTalliesFromTokio(amount: 2), isTrue);
      expect(await tallyTotalsFromTokio(), [2, 2]);
      expect(await _totalIn(other), 2);
      expect(mine.total, 2);

      // Once the other isolate exits, calls for its tally fail, or are
      // dropped if they return nothing, and this isolate's keep working.
      other.send('stop');
      await exited.first;
      expect(await tallyNamesFromThreads(), [null, 'main']);
      expect(await addToTalliesFromTokio(amount: 3), isTrue);
      expect(await tallyTotalsFromTokio(), [null, 5]);
      releaseTallies();
    },
    skip: companion == 'bundled' ? false : 'needs the relay library',
  );
}
