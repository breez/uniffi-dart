use anyhow::Result;

#[test]
fn enum_error_named_fields() -> Result<()> {
    uniffi_dart::testing::run_test("enum_error_named_fields", "src/api.udl", None)
}
