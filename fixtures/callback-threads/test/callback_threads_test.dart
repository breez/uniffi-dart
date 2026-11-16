import 'dart:async';
import 'dart:io';

import 'package:test/test.dart';

// Rust calls these callbacks from worker threads. If a delivery path regresses
// to `Pointer.fromFunction`, the VM aborts, so every case runs in a child
// process and a crash fails this test instead of the whole runner.
void main() {
  test('callbacks invoked from worker threads', () async {
    // Count live handles without changing the generated API: instrument copies
    // of the generated files for the child run, then restore them.
    final originals = <File, String>{};
    try {
      for (final name in ['callback_threads.dart', 'uniffi_runtime.dart']) {
        final file = File(name);
        final source = file.readAsStringSync();
        originals[file] = source;
        String instrumented;
        if (name == 'uniffi_runtime.dart') {
          const anchor = 'class UniffiHandleMap<T> {';
          expect(source, contains(anchor));
          instrumented = source.replaceFirst(
            anchor,
            '$anchor int get testLength => _map.length;',
          );
        } else {
          instrumented = '$source\n'
              'int eventSinkHandlesForTest() => '
              'FfiConverterCallbackInterfaceEventSink._handleMap.testLength;\n'
              'int storeHandlesForTest() => '
              'FfiConverterCallbackInterfaceStore._handleMap.testLength;\n';
        }
        file.writeAsStringSync(instrumented);
      }
      final result = await Process.run(Platform.resolvedExecutable, [
        'test',
        'test/thread_cases.dart',
        '--reporter',
        'expanded',
      ]);
      print(result.stdout);
      print(result.stderr);
      expect(result.exitCode, 0);
    } finally {
      for (final entry in originals.entries) {
        entry.key.writeAsStringSync(entry.value);
      }
    }
  }, timeout: Timeout(Duration(minutes: 3)));

  test('listener callbacks do not keep a program alive', () async {
    final process = await Process.start(Platform.resolvedExecutable, [
      'run',
      'test/exit_case.dart',
    ]);
    final stdout = process.stdout.transform(SystemEncoding().decoder).join();
    final stderr = process.stderr.transform(SystemEncoding().decoder).join();
    final exitCode = await process.exitCode.timeout(
      Duration(minutes: 2),
      onTimeout: () {
        process.kill();
        return -1;
      },
    );
    print(await stdout);
    print(await stderr);
    expect(exitCode, 0, reason: 'the program should exit once main returns');
    expect(await stdout, contains('exit case done'));
  }, timeout: Timeout(Duration(minutes: 3)));
}
