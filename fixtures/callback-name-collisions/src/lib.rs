//! Exports and callback types whose names used to collide with the
//! generated callback functions, which were top-level `{trait}{Method}`,
//! `{trait}FreeCallback`, `{trait}ClonePointer`, `{trait}VTable` and
//! `init{Trait}VTable` names. Each export below shares its Dart name with one
//! of them, so the old bindings didn't compile.

use std::sync::Arc;

#[uniffi::export(callback_interface)]
pub trait Ledger: Send + Sync {
    fn cash_total(&self) -> u32;
}

/// `Ledger.cash_total` and `LedgerCash.total` both made `ledgerCashTotal`.
#[uniffi::export(callback_interface)]
pub trait LedgerCash: Send + Sync {
    fn total(&self) -> u32;
}

#[uniffi::export(with_foreign)]
pub trait Vault: Send + Sync {
    fn open(&self) -> bool;
}

/// Same Dart name as `Ledger.cash_total`'s callback function.
#[uniffi::export]
pub fn ledger_cash_total(ledger: Box<dyn Ledger>) -> u32 {
    ledger.cash_total()
}

#[uniffi::export]
pub fn total_of(ledger_cash: Box<dyn LedgerCash>) -> u32 {
    ledger_cash.total()
}

/// Same Dart name as `Vault.open`'s callback function.
#[uniffi::export]
pub fn vault_open(vault: Arc<dyn Vault>) -> bool {
    vault.open()
}

#[uniffi::export]
pub fn ledger_free_callback() -> u32 {
    1
}

#[uniffi::export]
pub fn ledger_clone_pointer() -> u32 {
    2
}

#[uniffi::export]
pub fn vault_open_pointer() -> u32 {
    3
}

#[uniffi::export]
pub fn ledger_v_table() -> u32 {
    4
}

#[uniffi::export]
pub fn init_ledger_v_table() -> u32 {
    5
}

uniffi::include_scaffolding!("api");
