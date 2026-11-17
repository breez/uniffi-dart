import 'package:test/test.dart';

import '../enum_field_type_names.dart';

void main() {
  test('a record with Error inside its name keeps it in a variant field', () {
    final out = roundtripOutcome(
      outcome: ProblemOutcome(InternalServerErrorProblem(detail: "boom")),
    );
    expect(out, isA<ProblemOutcome>());
    expect((out as ProblemOutcome).problem.detail, "boom");
  });

  test('the same record inside a sequence and an optional field', () {
    final many = roundtripOutcome(
      outcome: ProblemsOutcome([
        InternalServerErrorProblem(detail: "a"),
        InternalServerErrorProblem(detail: "b"),
      ]),
    );
    expect((many as ProblemsOutcome).problems.map((p) => p.detail).toList(), [
      "a",
      "b",
    ]);

    final some = roundtripOutcome(
      outcome: MaybeProblemOutcome(InternalServerErrorProblem(detail: "c")),
    );
    expect((some as MaybeProblemOutcome).problem?.detail, "c");

    final none = roundtripOutcome(outcome: MaybeProblemOutcome(null));
    expect((none as MaybeProblemOutcome).problem, isNull);
  });

  test('the same record in sequence and optional arguments and returns', () {
    final first = firstProblem(
      problems: [
        InternalServerErrorProblem(detail: "x"),
        InternalServerErrorProblem(detail: "y"),
      ],
    );
    expect(first?.detail, "x");
    expect(firstProblem(problems: []), isNull);
  });

  test('an error type with a trailing Error is still named ...Exception', () {
    final out = roundtripOutcome(
      outcome: LookupOutcome(NotFoundLookupException(7)),
    );
    final error = (out as LookupOutcome).error;
    expect(error, isA<LookupException>());
    expect((error as NotFoundLookupException).id, 7);
  });

  test('an error variant carries the record', () {
    expect(
      () => failWithProblem(detail: "down"),
      throwsA(
        isA<InternalServerApiException>().having(
          (e) => e.problem.detail,
          'problem.detail',
          "down",
        ),
      ),
    );
  });

  test('a fieldless variant is unaffected', () {
    expect(roundtripOutcome(outcome: OkOutcome()), isA<OkOutcome>());
  });
}
