use anyhow::Result;

#[test]
fn callback_threads() -> Result<()> {
    uniffi_dart::testing::run_test("callback_threads", "src/api.udl", Some("uniffi.toml"))
}
