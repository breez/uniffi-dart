import '../callback_threads.dart';

// Registers listener callbacks, uses one from a worker thread, then returns
// from main. Listeners use keepIsolateAlive = false, so the program must exit.

class _Sink implements EventSink {
  @override
  Future<int> onEvent(String event) async => event.length;
}

Future<void> main() async {
  final result = await eventFromThread(sink: _Sink(), event: 'exit');
  if (result != 4) {
    throw StateError('unexpected result $result');
  }
  print('exit case done');
}
