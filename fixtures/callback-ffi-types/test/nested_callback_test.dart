import 'package:test/test.dart';

import '../callback_ffi_types.dart';

class Leaf implements LeafCallback {
  @override
  int value(int input) => input + 1;
}

class Nested implements NestedCallback {
  final Leaf leaf;
  Nested(this.leaf);
  @override
  LeafCallback get_() => leaf;
}

class ForeignLeaf implements ForeignLeafCallback {
  @override
  int value(int input) => input + 1;
}

class ForeignNested implements NestedForeignCallback {
  final ForeignLeafCallback leaf;
  ForeignNested(this.leaf);
  @override
  bool invoke(ForeignLeafCallback value) {
    // UniFFI 0.29 hands every `Arc<dyn Trait>` back as a Rust object, so a
    // Dart implementation that went through Rust comes back as a Rust-backed
    // wrapper, not the same instance. Calls still reach the Dart object.
    expect(identical(value, leaf), isFalse);
    return value.value(41) == 42;
  }

  @override
  ForeignLeafCallback get_() => leaf;
}

void main() {
  test(
    'foreign-implementable traits cross callback argument and result slots',
    () {
      final leaf = ForeignLeaf();
      expect(
        checkNestedForeignCallbacks(callback: ForeignNested(leaf), leaf: leaf),
        isTrue,
      );
    },
  );
  test('callback interfaces cross callback argument and result slots', () {
    final leaf = Leaf();
    expect(checkNestedCallbacks(callback: Nested(leaf)), isTrue);
  });
}
