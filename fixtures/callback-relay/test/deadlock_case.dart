import '../callback_relay.dart';
import 'support.dart';

// Run by relay_test.dart with the relay library: this deadlocks, and the relay
// warns after two seconds. The test kills the process.
void main() {
  isRemoteJoined(signer: DartSigner());
  print('unexpected: the call returned');
}
