import 'package:test/test.dart';

import '../callback_name_collisions.dart';

class DartLedger implements Ledger {
  @override
  int cashTotal() => 10;
}

class DartLedgerCash implements LedgerCash {
  @override
  int total() => 20;
}

class DartVault implements Vault {
  @override
  bool open() => true;
}

void main() {
  test('exports named like the old callback functions', () {
    expect(ledgerFreeCallback(), 1);
    expect(ledgerClonePointer(), 2);
    expect(vaultOpenPointer(), 3);
    expect(ledgerVTable(), 4);
    expect(initLedgerVTable(), 5);
  });

  test('callbacks whose old function names collided', () {
    expect(ledgerCashTotal(ledger: DartLedger()), 10);
    expect(totalOf(ledgerCash: DartLedgerCash()), 20);
    expect(vaultOpen(vault: DartVault()), isTrue);
  });
}
