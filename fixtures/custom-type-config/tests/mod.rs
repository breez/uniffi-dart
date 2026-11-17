use anyhow::Result;

#[test]
fn custom_type_config() -> Result<()> {
    uniffi_dart::testing::run_test("custom_type_config", "src/api.udl", Some("uniffi.toml"))
}
