use anyhow::Result;

#[test]
fn value_equality() -> Result<()> {
    uniffi_dart::testing::run_test("value_equality", "src/api.udl", None)
}
