import '../callback_relay.dart';
import 'support.dart';

// Run by fallback_test.dart without the relay library: the call from a worker
// thread must abort the VM.
Future<void> main() async {
  await isRemoteFromThread(signer: DartSigner());
  print('unexpected: the call from a worker thread did not abort');
}
