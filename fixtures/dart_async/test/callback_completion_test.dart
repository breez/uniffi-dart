import 'dart:async';
import 'dart:ffi';
import 'dart:typed_data';

import 'package:ffi/ffi.dart';
import 'package:test/test.dart';

import '../dart_async.dart';

class ByteSource implements AsyncByteSource {
  ByteSource(this.read);
  final Future<int> Function() read;
  @override
  Future<int> value() => read();
}

class ThrowingToString implements Exception {
  @override
  String toString() => throw StateError('toString failed');
}

class InputSource implements AsyncInputSource {
  @override
  Future<int> value(String input) async => throw StateError('must not run');
}

void main() {
  test('failed argument lift completes with an error', () async {
    final handle = FfiConverterCallbackInterfaceAsyncInputSource.lower(
      InputSource(),
    );
    final out = calloc<UniffiForeignFuture>();
    final invalidUtf8 = toRustBuffer(Uint8List.fromList([0xff]));
    final done = Completer<int>();
    var calls = 0;
    final completion =
        NativeCallable<UniffiForeignFutureCompleteU8>.isolateLocal((
          int data,
          UniffiForeignFutureStructU8 result,
        ) {
          calls++;
          result.callStatus.errorBuf.free();
          done.complete(result.callStatus.code);
        });
    try {
      asyncInputSourceValue(
        handle.address,
        invalidUtf8,
        completion.nativeFunction,
        0,
        out,
      );
      expect(
        await done.future.timeout(Duration(seconds: 5)),
        CALL_UNEXPECTED_ERROR,
      );
      expect(calls, 1);
      expect(out.ref.handle, 0);
      expect(out.ref.free, nullptr);
    } finally {
      completion.close();
      calloc.free(out);
      asyncInputSourceFreeCallback(handle.address);
    }
  });
  test('result lowering failure reaches the Rust future as an error', () async {
    expect(await readAsyncByte(source: ByteSource(() async => 42)), 42);
    await expectLater(
      readAsyncByte(source: ByteSource(() async => 256)),
      throwsA(isA<UnexpectedExceptionParserException>()),
    );
  }, timeout: Timeout(Duration(seconds: 10)));

  final cases = <String, (Future<int> Function(), int)>{
    'success': (() async => 42, CALL_SUCCESS),
    'invalid result': (() async => 256, CALL_UNEXPECTED_ERROR),
    'declared error': (() async => throw NotAnIntParserException(), CALL_ERROR),
    'declared error with invalid field': (
      () async => throw InvalidByteParserException(256),
      CALL_UNEXPECTED_ERROR,
    ),
    'unexpected error': (
      () async => throw StateError('test'),
      CALL_UNEXPECTED_ERROR,
    ),
    'synchronous throw': (
      () => throw StateError('test'),
      CALL_UNEXPECTED_ERROR,
    ),
    'error with throwing toString': (
      () async => throw ThrowingToString(),
      CALL_UNEXPECTED_ERROR,
    ),
  };
  for (final entry in cases.entries) {
    test('${entry.key} completes exactly once', () async {
      final handle = FfiConverterCallbackInterfaceAsyncByteSource.lower(
        ByteSource(entry.value.$1),
      );
      final out = calloc<UniffiForeignFuture>();
      final done = Completer<int>();
      var completions = 0;
      final completion =
          NativeCallable<UniffiForeignFutureCompleteU8>.isolateLocal((
            int data,
            UniffiForeignFutureStructU8 result,
          ) {
            completions++;
            final code = result.callStatus.code;
            if (code != CALL_SUCCESS) result.callStatus.errorBuf.free();
            if (!done.isCompleted) done.complete(code);
          });
      try {
        asyncByteSourceValue(handle.address, completion.nativeFunction, 0, out);
        expect(await done.future.timeout(Duration(seconds: 5)), entry.value.$2);
        await Future<void>.delayed(Duration.zero);
        expect(completions, 1);
        expect(out.ref.handle, 0);
        expect(out.ref.free, nullptr);
      } finally {
        completion.close();
        calloc.free(out);
        asyncByteSourceFreeCallback(handle.address);
      }
    });
  }

  test('stale handle completes with an error instead of escaping', () async {
    final handle = FfiConverterCallbackInterfaceAsyncByteSource.lower(
      ByteSource(() async => 42),
    );
    asyncByteSourceFreeCallback(handle.address);
    final out = calloc<UniffiForeignFuture>();
    final done = Completer<int>();
    final completion =
        NativeCallable<UniffiForeignFutureCompleteU8>.isolateLocal((
          int data,
          UniffiForeignFutureStructU8 result,
        ) {
          final code = result.callStatus.code;
          if (code != CALL_SUCCESS) result.callStatus.errorBuf.free();
          if (!done.isCompleted) done.complete(code);
        });
    try {
      asyncByteSourceValue(handle.address, completion.nativeFunction, 0, out);
      expect(
        await done.future.timeout(Duration(seconds: 5)),
        CALL_UNEXPECTED_ERROR,
      );
    } finally {
      completion.close();
      calloc.free(out);
    }
  });

  // Rust pre-fills the out-parameter, and an async callback method may run
  // after the Rust call has returned, so the bindings must never write it.
  // With no cancel hook, the Dart work runs to completion even if Rust stops
  // waiting, and Rust discards a late result.
  for (final fail in [false, true]) {
    test(
      'a late ${fail ? "error" : "success"} completes once and leaves Rust\'s out-parameter alone',
      () async {
        final pending = Completer<int>();
        final handle = FfiConverterCallbackInterfaceAsyncByteSource.lower(
          ByteSource(() => pending.future),
        );
        final out = calloc<UniffiForeignFuture>();
        const sentinelHandle = 0x5eed;
        final sentinelFree =
            Pointer<NativeFunction<UniffiForeignFutureFree>>.fromAddress(0x1000);
        out.ref.handle = sentinelHandle;
        out.ref.free = sentinelFree;
        var completions = 0;
        int? completedCode;
        final completion =
            NativeCallable<UniffiForeignFutureCompleteU8>.isolateLocal((
              int data,
              UniffiForeignFutureStructU8 result,
            ) {
              completions++;
              completedCode = result.callStatus.code;
              if (result.callStatus.code != CALL_SUCCESS) {
                result.callStatus.errorBuf.free();
              }
            });
        try {
          asyncByteSourceValue(
            handle.address,
            completion.nativeFunction,
            0,
            out,
          );
          expect(completions, 0);
          if (fail) {
            pending.completeError(StateError('late'));
          } else {
            pending.complete(42);
          }
          // Completing the Future schedules its continuation before this event.
          await Future<void>.delayed(Duration.zero);
          expect(completions, 1);
          expect(completedCode, fail ? CALL_UNEXPECTED_ERROR : CALL_SUCCESS);
          expect(out.ref.handle, sentinelHandle);
          expect(out.ref.free, sentinelFree);
        } finally {
          completion.close();
          calloc.free(out);
          asyncByteSourceFreeCallback(handle.address);
        }
      },
    );
  }
}
