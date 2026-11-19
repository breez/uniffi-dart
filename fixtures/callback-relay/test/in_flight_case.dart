import '../callback_relay.dart';

// Run by in_flight_test.dart. main's await is all that's left to do, so the
// pending Rust call must keep the isolate alive until it finishes.
Future<void> main() async {
  print('finished ${await finishLater(ms: 200)}');
}
