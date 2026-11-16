// Regression fixture: `DartCodeOracle::class_name` renames only a trailing
// `Error` to `Exception`, so `LnurlErrorDetails` keeps its name in Dart. Fields
// of data-carrying enum variants used to replace `Error` anywhere in their type
// name, so a field typed `LnurlErrorDetails` referred to an undefined
// `LnurlExceptionDetails` class and converter, a Dart compile error.

#[derive(uniffi::Record, Clone, PartialEq, Debug)]
pub struct LnurlErrorDetails {
    pub reason: String,
}

#[derive(uniffi::Enum, Clone, PartialEq, Debug)]
pub enum AllowanceErrorReason {
    Expired,
    Exceeded,
}

// Ends in `Error`, so its Dart class is renamed to `SessionException`.
#[derive(uniffi::Error, Clone, PartialEq, Debug)]
pub enum SessionError {
    Expired { after_secs: u32 },
}

impl std::fmt::Display for SessionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SessionError::Expired { after_secs } => write!(f, "expired after {after_secs}s"),
        }
    }
}

impl std::error::Error for SessionError {}

#[derive(uniffi::Enum, Clone, PartialEq, Debug)]
pub enum CallbackStatus {
    Ok,
    ErrorStatus { error_details: LnurlErrorDetails },
    Denied { reason: AllowanceErrorReason },
    Session(SessionError),
}

#[uniffi::export]
pub fn roundtrip_status(s: CallbackStatus) -> CallbackStatus {
    s
}

#[uniffi::export]
pub fn check_session(after_secs: u32) -> Result<(), SessionError> {
    if after_secs > 0 {
        Err(SessionError::Expired { after_secs })
    } else {
        Ok(())
    }
}

uniffi::include_scaffolding!("api");
