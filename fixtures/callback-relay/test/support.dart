import 'dart:io';

import '../callback_relay.dart';

/// `bundled`, `absent` (generated but left out) or `none`, set by the harness.
final companion = Platform.environment['UNIFFI_DART_COMPANION'];

class DartSigner implements Signer {
  var calls = 0;

  @override
  bool isRemote() {
    calls++;
    return true;
  }

  @override
  String sign(String message) {
    if (message.isEmpty) {
      throw RejectedSignException('empty message');
    }
    return 'signed:$message';
  }
}

class DartCounter implements Counter {
  var value = 0;
  Signer? signer;
  final adopted = <Signer>[];

  @override
  void store(int value) {
    this.value = value;
  }

  @override
  int load() {
    final signer = this.signer;
    // Calls back into Rust, which calls the signer on this thread.
    if (signer != null && isRemoteHere(signer: signer)) {
      return value + 1000;
    }
    return value;
  }

  @override
  bool adopt(Signer signer) {
    adopted.add(signer);
    return signer.isRemote();
  }

  @override
  Future<int> addLater(int amount) async {
    await Future<void>.delayed(const Duration(milliseconds: 1));
    value += amount;
    return value;
  }
}

class DartKeeper implements Keeper {
  DartKeeper(this.current);

  final int current;
  final notes = <int>[];

  @override
  void note(int value) => notes.add(value);

  @override
  int value() => current;

  @override
  Future<int> valueLater() async => current;
}
