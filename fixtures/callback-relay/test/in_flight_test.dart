import 'dart:io';

import 'package:test/test.dart';

// In its own file because it starts a child process, which a device can't.
void main() {
  test(
    'a pending async Rust call keeps a program alive',
    () async {
      final result = await Process.run(Platform.resolvedExecutable, [
        'run',
        'test/in_flight_case.dart',
      ]);
      expect(result.stdout, contains('finished 200'), reason: '${result.stderr}');
      expect(result.exitCode, 0);
    },
    timeout: const Timeout(Duration(minutes: 2)),
  );
}
