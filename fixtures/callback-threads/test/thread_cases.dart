import 'dart:async';
import 'dart:ffi';
import 'dart:isolate';

import 'package:test/test.dart';

import '../callback_threads.dart';

// Run by callback_threads_test.dart in a child process, against bindings
// instrumented with eventSinkHandlesForTest and storeHandlesForTest.

class Sink implements EventSink {
  final events = <String>[];
  Completer<void>? gate;

  @override
  Future<int> onEvent(String event) async {
    events.add(event);
    final gate = this.gate;
    if (gate != null) {
      await gate.future;
    }
    return event.length;
  }
}

class MemoryStore implements Store {
  final values = <String, String>{};

  @override
  Future<void> put(String key, String value) async {
    if (key.isEmpty) {
      throw EmptyKeyStoreException();
    }
    values[key] = value;
  }

  @override
  Future<String?> fetch(String key) async => values[key];

  @override
  String name() => 'memory';
}

/// Waits for work that listeners deliver on a later event-loop turn.
Future<void> eventually(bool Function() condition, String what) async {
  final deadline = DateTime.now().add(const Duration(seconds: 20));
  while (!condition()) {
    if (DateTime.now().isAfter(deadline)) {
      fail('timed out waiting for $what');
    }
    await Future<void>.delayed(const Duration(milliseconds: 5));
  }
}

void main() {
  test('async method from a std::thread', () async {
    final sink = Sink();
    expect(await eventFromThread(sink: sink, event: 'hello'), 5);
    expect(sink.events, ['hello']);
  });

  test('async method from a tokio task', () async {
    final sink = Sink();
    expect(await eventFromTokio(sink: sink, event: 'tokio'), 5);
    expect(sink.events, ['tokio']);
  });

  test('concurrent async calls from many threads', () async {
    final sink = Sink();
    final total = await eventsFromManyThreads(sink: sink, count: 32);
    expect(sink.events, hasLength(32));
    expect(total, sink.events.fold<int>(0, (sum, e) => sum + e.length));
  });

  test('a free on the Dart thread runs after the call', () async {
    final before = eventSinkHandlesForTest();
    dropSinkHere(sink: Sink());
    // The free is delivered through the listener even on this thread.
    expect(eventSinkHandlesForTest(), before + 1);
    await uniffiCallbackBarrier();
    expect(eventSinkHandlesForTest(), before);
  });

  test('the barrier waits for a free queued from a worker thread', () async {
    final before = eventSinkHandlesForTest();
    dropSinkOnThread(sink: Sink());
    await uniffiCallbackBarrier();
    expect(eventSinkHandlesForTest(), before);
  });

  test('a callback interface dropped on a worker thread is freed', () async {
    final before = eventSinkHandlesForTest();
    dropSinkOnThread(sink: Sink());
    await eventually(
      () => eventSinkHandlesForTest() == before,
      'the sink handle to be freed',
    );
  });

  test('a foreign trait dropped on a tokio task is freed', () async {
    final before = storeHandlesForTest();
    dropStoreOnTokio(store: MemoryStore());
    await eventually(
      () => storeHandlesForTest() == before,
      'the store handle to be freed',
    );
  });

  test('a Rust future dropped before Dart completes', () async {
    final before = eventSinkHandlesForTest();
    final sink = Sink()..gate = Completer<void>();
    // Rust stops waiting after 50 ms while the Dart call is still gated.
    expect(await abandonEvent(sink: sink, event: 'late', waitMs: 50), isTrue);
    await eventually(() => sink.events.length == 1, 'the call to start');
    // Complete after Rust dropped its future: the late completion must be
    // dropped on the Rust side without crashing.
    sink.gate!.complete();
    await eventually(
      () => eventSinkHandlesForTest() == before,
      'the sink handle to be freed',
    );
    await Future<void>.delayed(const Duration(milliseconds: 50));
    expect(await eventFromTokio(sink: Sink(), event: 'after'), 5);
  });

  test('foreign trait async methods and errors on a tokio task', () async {
    final store = MemoryStore();
    expect(
      await storeRoundtripOnTokio(store: store, key: 'k', value: 'v'),
      'v',
    );
    expect(store.values, {'k': 'v'});
    await expectLater(
      storeRoundtripOnTokio(store: store, key: '', value: 'v'),
      throwsA(isA<EmptyKeyStoreException>()),
    );
  });

  test('handles from different isolates do not collide', () async {
    // After a hot restart Rust may still hold the old isolate's handles. They
    // must not match the new isolate's handles. Only the handle map is used in
    // the other isolate: lowering a callback there would move the vtable to it.
    final here = UniffiHandleMap<int>().insert(0);
    final there = await Isolate.run(() => UniffiHandleMap<int>().insert(0));
    expect(here & 1, 1);
    expect(there & 1, 1);
    if (sizeOf<IntPtr>() == 8) {
      expect(here, isNot(there));
    }
  });

  test('a sync method that returns a value runs on the Dart thread', () {
    expect(readStoreName(store: MemoryStore()), 'memory');
  });
}
