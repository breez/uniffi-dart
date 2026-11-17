use anyhow::Result;

#[test]
fn callback_name_collisions() -> Result<()> {
    uniffi_dart::testing::run_test("callback_name_collisions", "src/api.udl", None)
}
