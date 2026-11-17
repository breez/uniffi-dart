import 'dart:async';
import 'dart:convert';
import 'dart:io';

import 'package:test/test.dart';

import 'support.dart';

// In its own file because it starts a child process, which a device can't.
void main() {
  test(
    'a sync Rust call waiting on a relayed callback logs a warning',
    () async {
      final child = await Process.start(Platform.resolvedExecutable, [
        'run',
        'test/deadlock_case.dart',
      ]);
      final warned = Completer<void>();
      final stderr = StringBuffer();
      child.stderr.transform(utf8.decoder).listen((chunk) {
        stderr.write(chunk);
        if (stderr.toString().contains('has waited 2 s') &&
            !warned.isCompleted) {
          warned.complete();
        }
      });
      child.stdout.drain<void>();
      try {
        await warned.future.timeout(const Duration(seconds: 60));
      } finally {
        child.kill(ProcessSignal.sigkill);
        await child.exitCode;
      }
      expect(stderr.toString(), contains('callback_dispatch'));
    },
    timeout: const Timeout(Duration(minutes: 2)),
    skip: companion == 'bundled' ? false : 'needs the relay library',
  );
}
