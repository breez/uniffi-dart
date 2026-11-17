// Regression fixture: a data-carrying enum variant's field must reference its
// type by the Dart name that type is declared with. Only a trailing `Error` is
// renamed to `Exception` (`LookupError` -> `LookupException`); a type with
// `Error` elsewhere in its name keeps it (`InternalServerErrorProblem`). The
// generator used to rename every `Error` in a variant field's type, so it
// referenced an undeclared `InternalServerExceptionProblem` (and its converter);
// the `Optional` and `Sequence` helpers renamed it the same way wherever they
// were used.

#[derive(uniffi::Record, Clone, PartialEq, Debug)]
pub struct InternalServerErrorProblem {
    detail: String,
}

// An error type whose trailing `Error` is renamed, also held by a variant below.
#[derive(Debug, Clone, PartialEq, thiserror::Error, uniffi::Error)]
pub enum LookupError {
    #[error("not found: {id}")]
    NotFound { id: u32 },
}

#[derive(uniffi::Enum, Clone, PartialEq, Debug)]
pub enum Outcome {
    Problem { problem: InternalServerErrorProblem },
    Problems { problems: Vec<InternalServerErrorProblem> },
    MaybeProblem { problem: Option<InternalServerErrorProblem> },
    Lookup { error: LookupError },
    Ok,
}

// The same field inside an error enum, the shape this was found in.
#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum ApiError {
    #[error("internal server error: {problem:?}")]
    InternalServer { problem: InternalServerErrorProblem },
}

#[uniffi::export]
pub fn roundtrip_outcome(outcome: Outcome) -> Outcome {
    outcome
}

#[uniffi::export]
pub fn fail_with_problem(detail: String) -> Result<(), ApiError> {
    Err(ApiError::InternalServer { problem: InternalServerErrorProblem { detail } })
}

// The same type in sequence and optional helpers outside an enum.
#[uniffi::export]
pub fn first_problem(
    problems: Vec<InternalServerErrorProblem>,
) -> Option<InternalServerErrorProblem> {
    problems.into_iter().next()
}

uniffi::include_scaffolding!("api");
