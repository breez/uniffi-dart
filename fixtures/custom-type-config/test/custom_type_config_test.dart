import 'package:test/test.dart';

import '../custom_type_config.dart';
import 'money.dart';

// Above the u64 range, so it only survives as a BigInt.
final big = BigInt.parse('340282366920938463463374607431768211455');

class DartLedger implements Ledger {
  BigInt recorded = BigInt.zero;
  Uri? lastFrom;

  @override
  void record(BigInt amount, Uri from) {
    recorded += amount;
    lastFrom = from;
  }

  @override
  BigInt balance() => recorded;

  @override
  Money cash() => const Money.fromCents(250);
}

class DartPricer implements Pricer {
  @override
  BigInt price(Uri item) => BigInt.from(item.pathSegments.length) * big ~/ BigInt.from(4);

  @override
  Future<BigInt> priceAsync(Uri item) async => price(item);

  @override
  BigInt cents(Money price) => BigInt.from(price.cents);

  @override
  Money discount(Money price) => Money.fromCents(price.cents ~/ 2);
}

final stampAt = DateTime.fromMillisecondsSinceEpoch(1700000000000, isUtc: true);
final secondLater = stampAt.add(const Duration(seconds: 1));

class DartClock implements Clock {
  @override
  DateTime now() => stampAt;

  @override
  Future<DateTime> nowAsync() async => secondLater;

  @override
  List<DateTime> history() => [stampAt, secondLater];
}

Wallet wallet({BigInt? balance}) => Wallet(
  balance: balance ?? big,
  homepage: Uri.parse('https://example.com/home'),
  cash: const Money.fromCents(1234),
  history: [BigInt.one, big],
  mirror: Uri.parse('https://mirror.example.com'),
  byName: {'a': BigInt.two, 'b': big - BigInt.two},
  count: 3,
);

void main() {
  test('configured types replace their builtins in signatures', () {
    // Compile-time checks: these only type-check with the configured types.
    final BigInt sum = add(a: BigInt.one, b: BigInt.two);
    final Uri? url = firstUrl(urls: [Uri.parse('https://a.example')]);
    final Money money = doubleMoney(money: const Money.fromCents(5));
    // Count has no configuration, so it stays its builtin int.
    final int count = nextCount(count: 41);
    expect(sum, BigInt.from(3));
    expect(url, Uri.parse('https://a.example'));
    expect(money, const Money.fromCents(10));
    expect(count, 42);
  });

  test('u128 values above the u64 range round-trip', () {
    expect(add(a: big - BigInt.one, b: BigInt.one), big);
  });

  test('async functions', () async {
    expect(await addAsync(a: big - BigInt.two, b: BigInt.two), big);
  });

  test('record fields, including Option, sequences and maps', () {
    final echoed = echoWallet(wallet: wallet());
    expect(echoed.balance, big);
    expect(echoed.homepage, Uri.parse('https://example.com/home'));
    expect(echoed.cash, const Money.fromCents(1234));
    expect(echoed.history, [BigInt.one, big]);
    expect(echoed.mirror, Uri.parse('https://mirror.example.com'));
    expect(echoed.byName, {'a': BigInt.two, 'b': big - BigInt.two});
    expect(total(byName: echoed.byName), big);
    expect(echoed.count, 3);
  });

  test('enum variant fields', () {
    final received = echoPayment(
      payment: ReceivedPayment(amount: big, from: Uri.parse('https://payer.example')),
    );
    expect(received, isA<ReceivedPayment>());
    expect((received as ReceivedPayment).amount, big);
    expect(received.from, Uri.parse('https://payer.example'));
    final sent = echoPayment(payment: SentPayment(BigInt.one));
    expect((sent as SentPayment).amount, BigInt.one);
  });

  test('error fields', () {
    expect(spend(wallet: wallet(), amount: BigInt.one).balance, big - BigInt.one);
    expect(
      () => spend(wallet: wallet(balance: big - BigInt.one), amount: big),
      throwsA(
        isA<InsufficientWalletException>().having((e) => e.needed, 'needed', BigInt.one),
      ),
    );
  });

  test('object methods, sync and async', () async {
    final vault = Vault(start: big - BigInt.from(10));
    addTearDown(vault.dispose);
    expect(vault.deposit(amount: BigInt.from(10)), big);
    expect(await vault.totalAsync(), big);
  });

  test('callback interface arguments and returns', () {
    final ledger = DartLedger();
    expect(
      recordAndRead(ledger: ledger, amount: big, from: Uri.parse('https://from.example')),
      big,
    );
    expect(ledger.lastFrom, Uri.parse('https://from.example'));
    expect(cashIn(ledger: DartLedger()), const Money.fromCents(250));
  });

  test('foreign trait arguments and returns, sync and async', () async {
    final pricer = DartPricer();
    final item = Uri.parse('https://shop.example/a/b/c/d');
    expect(priceOf(pricer: pricer, item: item), big);
    expect(await priceOfAsync(pricer: pricer, item: item), big);
    expect(centsOf(pricer: pricer, price: const Money.fromCents(100)), BigInt.from(100));
    expect(discounted(pricer: pricer, price: const Money.fromCents(100)), const Money.fromCents(50));
  });

  test('a configured u64 type is its own Dart type, not u64 input', () async {
    final DateTime echoed = echoStamp(stamp: stampAt);
    expect(echoed, stampAt);
    expect(await echoStampAsync(stamp: stampAt), stampAt);
    // The plain u64 next to it still takes an int or a BigInt.
    expect(addMillis(stamp: stampAt, millis: 1000), secondLater);
    expect(addMillis(stamp: stampAt, millis: BigInt.from(1000)), secondLater);
  });

  test('configured u64 record fields, plain and optional', () {
    final event = Event(at: stampAt, earlier: null, ticks: 5);
    expect(event.ticks, BigInt.from(5));
    final echoed = echoEvent(
      event: Event(at: secondLater, earlier: stampAt, ticks: BigInt.two),
    );
    final DateTime at = echoed.at;
    final DateTime? earlier = echoed.earlier;
    expect(at, secondLater);
    expect(earlier, stampAt);
    expect(echoed.ticks, BigInt.two);
  });

  test('configured u64 foreign trait results, sync and async', () async {
    // Compile-time check: the results are DateTime, not u64 input.
    final Clock clock = DartClock();
    final DateTime Function() now = clock.now;
    expect(now(), stampAt);
    expect(readClock(clock: clock), stampAt);
    expect(await readClockAsync(clock: clock), secondLater);
    expect(readHistory(clock: clock), [stampAt, secondLater]);
  });
}
