/// The Dart type for the `Money` custom type, which `uniffi.toml` imports.
class Money {
  const Money.fromCents(this.cents);

  final int cents;

  @override
  bool operator ==(Object other) => other is Money && other.cents == cents;

  @override
  int get hashCode => cents.hashCode;

  @override
  String toString() => 'Money($cents)';
}
