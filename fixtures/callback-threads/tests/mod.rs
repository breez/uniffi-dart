use anyhow::Result;
use uniffi_dart::testing::{run_test, run_test_with_config, TestConfig};

#[test]
fn callback_threads() -> Result<()> {
    run_test("callback_threads", "src/api.udl", None)
}

/// Without the relay library: async methods and frees use listeners.
#[test]
fn callback_threads_without_companion() -> Result<()> {
    let config = TestConfig::from_env().without_companion();
    run_test_with_config("callback_threads", "src/api.udl", None, &config)
}
