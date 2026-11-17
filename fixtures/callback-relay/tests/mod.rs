use anyhow::Result;
use uniffi_dart::testing::{run_test, run_test_with_config, TestConfig};

#[test]
fn callback_relay() -> Result<()> {
    run_test("callback_relay", "src/api.udl", None)
}

/// The same bindings without the relay library bundled.
#[test]
fn callback_relay_without_companion() -> Result<()> {
    let config = TestConfig::from_env().without_companion();
    run_test_with_config("callback_relay", "src/api.udl", None, &config)
}
