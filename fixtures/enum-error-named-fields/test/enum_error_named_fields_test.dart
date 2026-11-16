import 'package:test/test.dart';
import '../enum_error_named_fields.dart';

void main() {
  test('a record with Error mid-name keeps its name inside a variant', () {
    final out = roundtripStatus(
      s: ErrorStatusCallbackStatus(LnurlErrorDetails(reason: 'denied')),
    );
    expect(out, isA<ErrorStatusCallbackStatus>());
    expect((out as ErrorStatusCallbackStatus).errorDetails.reason, 'denied');
  });

  test('a flat enum with Error mid-name keeps its name inside a variant', () {
    final out = roundtripStatus(
      s: DeniedCallbackStatus(AllowanceErrorReason.exceeded),
    );
    expect(out, isA<DeniedCallbackStatus>());
    expect((out as DeniedCallbackStatus).reason, AllowanceErrorReason.exceeded);
  });

  test('an error type inside a variant uses its Exception name', () {
    final out = roundtripStatus(
      s: SessionCallbackStatus(ExpiredSessionException(5)),
    );
    expect(out, isA<SessionCallbackStatus>());
    final inner = (out as SessionCallbackStatus).v0;
    expect(inner, isA<ExpiredSessionException>());
    expect((inner as ExpiredSessionException).afterSecs, 5);
  });

  test('a unit variant round-trips', () {
    expect(roundtripStatus(s: OkCallbackStatus()), isA<OkCallbackStatus>());
  });

  test('the error type is still thrown under its Exception name', () {
    expect(() => checkSession(afterSecs: 3), throwsA(isA<SessionException>()));
    checkSession(afterSecs: 0);
  });
}
