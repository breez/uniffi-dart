import 'dart:typed_data';

import 'package:test/test.dart';
import '../value_equality.dart';

// Each value is compared with a copy that went through Rust, which shares no
// lists, maps or byte arrays with it, and with a copy that differs in one place.
void expectValueEqual(Object a, Object b) {
  expect(a, equals(b));
  expect(b, equals(a));
  expect(a.hashCode, b.hashCode);
}

Payload payload({String name = 'p', List<int> data = const [1, 2, 3]}) =>
    Payload(
      name: name,
      data: Uint8List.fromList(data),
      tags: ['a', 'b'],
      attrs: {'x': 1, 'y': 2},
      note: null,
      ratio: 0.5,
    );

Nested nested({int y = 2}) => Nested(
      point: Point(x: 1, y: y),
      points: [Point(x: 1, y: 2), Point(x: 3, y: 4)],
      byName: {'a': Point(x: 5, y: 6)},
      grid: [
        Uint8List.fromList([1, 2]),
        Uint8List.fromList([3]),
      ],
      shape: RectShape(width: 2, height: 3),
      maybe: Point(x: 7, y: 8),
    );

void main() {
  group('records', () {
    test('compare by value', () {
      expectValueEqual(Point(x: 1, y: 2), Point(x: 1, y: 2));
      expectValueEqual(Point(x: 1, y: 2), roundtripPoint(value: Point(x: 1, y: 2)));
      expect(Point(x: 1, y: 2), isNot(equals(Point(x: 2, y: 1))));
    });

    test('compare bytes, lists and maps by their contents', () {
      expectValueEqual(payload(), roundtripPayload(value: payload()));
      expect(payload(), isNot(equals(payload(data: [1, 2, 4]))));
      expect(payload(), isNot(equals(payload(data: [1, 2]))));
      expect(payload(), isNot(equals(payload(name: 'q'))));

      final withNote = Payload(
        name: 'p',
        data: Uint8List.fromList([1, 2, 3]),
        tags: ['a', 'b'],
        attrs: {'x': 1, 'y': 3},
        note: 'n',
        ratio: 0.5,
      );
      expect(payload(), isNot(equals(withNote)));
      expectValueEqual(withNote, roundtripPayload(value: withNote));
    });

    test('map entry order does not matter', () {
      final a = Payload(
        name: 'p',
        data: Uint8List(0),
        tags: [],
        attrs: {'x': 1, 'y': 2},
        note: null,
        ratio: 0,
      );
      final b = Payload(
        name: 'p',
        data: Uint8List(0),
        tags: [],
        attrs: {'y': 2, 'x': 1},
        note: null,
        ratio: 0,
      );
      expectValueEqual(a, b);
    });

    test('maps keyed by byte arrays match keys by their contents', () {
      ByteIndex index(String second) => ByteIndex(byBytes: {
            Uint8List.fromList([1]): 'a',
            Uint8List.fromList([2, 3]): second,
          });
      expectValueEqual(index('b'), index('b'));
      expectValueEqual(index('b'), roundtripByteIndex(value: index('b')));
      expect(index('b'), isNot(equals(index('c'))));
      expect(
        index('b'),
        isNot(equals(ByteIndex(byBytes: {
          Uint8List.fromList([1]): 'a',
          Uint8List.fromList([2, 4]): 'b',
        }))),
      );
    });

    test('nested records, enums and collections compare deeply', () {
      expectValueEqual(nested(), nested());
      expectValueEqual(nested(), roundtripNested(value: nested()));
      expect(nested(), isNot(equals(nested(y: 3))));

      final otherGrid = nested();
      expect(
        otherGrid,
        isNot(equals(Nested(
          point: otherGrid.point,
          points: otherGrid.points,
          byName: otherGrid.byName,
          grid: [
            Uint8List.fromList([1, 2]),
            Uint8List.fromList([4]),
          ],
          shape: otherGrid.shape,
          maybe: otherGrid.maybe,
        ))),
      );
    });

    test('a field named other is compared with its counterpart', () {
      expectValueEqual(Pair(value: 1, other: 2), roundtripPair(value: Pair(value: 1, other: 2)));
      expect(Pair(value: 1, other: 2), isNot(equals(Pair(value: 1, other: 3))));
    });

    test('a record without fields equals any other instance', () {
      expectValueEqual(Empty(), roundtripEmpty(value: Empty()));
    });

    test('work as Set elements', () {
      final points = {Point(x: 1, y: 2), Point(x: 1, y: 2), Point(x: 3, y: 4)};
      expect(points, hasLength(2));
      expect(points.contains(roundtripPoint(value: Point(x: 3, y: 4))), isTrue);

      final payloads = {payload(), roundtripPayload(value: payload())};
      expect(payloads, hasLength(1));
    });

    test('work as Map keys', () {
      final byPoint = {Point(x: 1, y: 2): 'a', Point(x: 3, y: 4): 'b'};
      expect(byPoint[roundtripPoint(value: Point(x: 1, y: 2))], 'a');

      final byPayload = {payload(): 1};
      byPayload[roundtripPayload(value: payload())] = 2;
      expect(byPayload, hasLength(1));
      expect(byPayload[payload()], 2);
    });
  });

  group('enums with fields', () {
    test('variants compare by value', () {
      expectValueEqual(CircleShape(1.5), roundtripShape(value: CircleShape(1.5)));
      expectValueEqual(RectShape(width: 2, height: 3), RectShape(width: 2, height: 3));
      expect(CircleShape(1.5), isNot(equals(CircleShape(2.5))));
      expect(RectShape(width: 2, height: 3), isNot(equals(RectShape(width: 3, height: 2))));
    });

    test('variants with bytes, lists and maps compare their contents', () {
      final bytes = BytesEvent(Uint8List.fromList([9, 8]));
      expectValueEqual(bytes, roundtripEvent(value: bytes));
      expect(bytes, isNot(equals(BytesEvent(Uint8List.fromList([9, 7])))));

      final batch = BatchEvent([Point(x: 1, y: 2)]);
      expectValueEqual(batch, roundtripEvent(value: batch));
      expect(batch, isNot(equals(BatchEvent([Point(x: 1, y: 3)]))));

      final labels = LabelsEvent({1: 'one', 2: 'two'});
      expectValueEqual(labels, roundtripEvent(value: labels));
      expect(labels, isNot(equals(LabelsEvent({1: 'one', 2: 'deux'}))));

      final moved = MovedEvent(v0: Point(x: 1, y: 2), v1: DotShape());
      expectValueEqual(moved, roundtripEvent(value: moved));
      expect(moved, isNot(equals(MovedEvent(v0: Point(x: 1, y: 2), v1: CircleShape(1)))));
    });

    test('variants without fields are equal by type', () {
      expectValueEqual(DotShape(), DotShape());
      expectValueEqual(IdleEvent(), roundtripEvent(value: IdleEvent()));
      expect(IdleEvent(), isNot(equals(StoppedEvent())));
    });

    test('different variants are never equal', () {
      expect(BytesEvent(Uint8List(0)), isNot(equals(BatchEvent([]))));
      expect(DotShape(), isNot(equals(CircleShape(0))));
    });

    test('work as Set elements and Map keys', () {
      final shapes = <Shape>{CircleShape(1), CircleShape(1), DotShape(), DotShape()};
      expect(shapes, hasLength(2));

      final events = <Event, String>{
        BytesEvent(Uint8List.fromList([1])): 'bytes',
        IdleEvent(): 'idle',
      };
      expect(events[roundtripEvent(value: BytesEvent(Uint8List.fromList([1])))], 'bytes');
      expect(events[IdleEvent()], 'idle');
    });
  });

  group('error enums', () {
    test('thrown errors compare by value', () {
      expect(() => lookup(key: 'k'), throwsA(equals(NotFoundLookupException('k'))));
      expect(() => lookup(key: 'bad'), throwsA(equals(InvalidLookupException([1, 2, 3]))));
      expect(() => lookup(key: ''), throwsA(equals(UnknownLookupException())));
    });

    test('hash consistently', () {
      try {
        lookup(key: 'k');
        fail('lookup should throw');
      } on NotFoundLookupException catch (e) {
        expect(e.hashCode, NotFoundLookupException('k').hashCode);
        expect(e, isNot(equals(NotFoundLookupException('j'))));
      }
    });
  });

  group('objects', () {
    test('keep identity when they export no Eq', () {
      final counter = Counter(start: 1);
      expectValueEqual(
        Holder(counter: counter, label: 'a'),
        Holder(counter: counter, label: 'a'),
      );
      expect(
        Holder(counter: counter, label: 'a'),
        isNot(equals(Holder(counter: Counter(start: 1), label: 'a'))),
      );
      expectValueEqual(CountedOwner(counter), CountedOwner(counter));
      expect(CountedOwner(counter), isNot(equals(CountedOwner(Counter(start: 1)))));
    });

    test('compare with Rust Eq and Hash when they export them', () {
      final holder = KeyHolder(key: Key(id: 1), keys: [Key(id: 2), Key(id: 3)]);
      expectValueEqual(holder, KeyHolder(key: Key(id: 1), keys: [Key(id: 2), Key(id: 3)]));
      expectValueEqual(holder, roundtripKeyHolder(value: holder));
      expect(holder, isNot(equals(KeyHolder(key: Key(id: 1), keys: [Key(id: 2)]))));

      expectValueEqual(KeyedOwner(Key(id: 4)), roundtripOwner(value: KeyedOwner(Key(id: 4))));
      expect({KeyedOwner(Key(id: 4)), KeyedOwner(Key(id: 4))}, hasLength(1));
    });
  });
}
