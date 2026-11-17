//! Custom types mapped to Dart types with `[bindings.dart.custom_types]`, in
//! every position a custom type can take. `Count` has no configuration and
//! stays a typedef of its builtin.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

uniffi::custom_type!(u128, String, {
    remote,
    try_lift: |value| value.parse::<u128>().map_err(uniffi::deps::anyhow::Error::msg),
    lower: |value| value.to_string(),
});

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Url(pub String);

uniffi::custom_type!(Url, String, {
    lower: |url| url.0,
    try_lift: |value| Ok(Url(value)),
});

/// An amount in cents, carried as an `i64`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Money(pub i64);

uniffi::custom_type!(Money, i64, {
    lower: |money| money.0,
    try_lift: |cents| Ok(Money(cents)),
});

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Count(pub u32);

uniffi::custom_type!(Count, u32, {
    lower: |count| count.0,
    try_lift: |value| Ok(Count(value)),
});

#[derive(Debug, Clone, uniffi::Record)]
pub struct Wallet {
    pub balance: u128,
    pub homepage: Url,
    pub cash: Money,
    pub history: Vec<u128>,
    pub mirror: Option<Url>,
    pub by_name: HashMap<String, u128>,
    pub count: Count,
}

#[derive(Debug, Clone, uniffi::Enum)]
pub enum Payment {
    Sent { amount: u128 },
    Received { amount: u128, from: Url },
}

#[derive(Debug, uniffi::Error)]
pub enum WalletError {
    Insufficient { needed: u128 },
}

impl std::fmt::Display for WalletError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            WalletError::Insufficient { needed } => write!(f, "needs {needed} more"),
        }
    }
}

impl std::error::Error for WalletError {}

#[uniffi::export]
pub fn add(a: u128, b: u128) -> u128 {
    a + b
}

#[uniffi::export]
pub async fn add_async(a: u128, b: u128) -> u128 {
    a + b
}

#[uniffi::export]
pub fn echo_wallet(wallet: Wallet) -> Wallet {
    wallet
}

#[uniffi::export]
pub fn spend(wallet: Wallet, amount: u128) -> Result<Wallet, WalletError> {
    match wallet.balance.checked_sub(amount) {
        Some(balance) => Ok(Wallet { balance, ..wallet }),
        None => Err(WalletError::Insufficient { needed: amount - wallet.balance }),
    }
}

#[uniffi::export]
pub fn echo_payment(payment: Payment) -> Payment {
    payment
}

#[uniffi::export]
pub fn first_url(urls: Vec<Url>) -> Option<Url> {
    urls.into_iter().next()
}

#[uniffi::export]
pub fn total(by_name: HashMap<String, u128>) -> u128 {
    by_name.values().sum()
}

#[uniffi::export]
pub fn double_money(money: Money) -> Money {
    Money(money.0 * 2)
}

#[uniffi::export]
pub fn next_count(count: Count) -> Count {
    Count(count.0 + 1)
}

#[derive(uniffi::Object)]
pub struct Vault {
    total: Mutex<u128>,
}

#[uniffi::export]
impl Vault {
    #[uniffi::constructor]
    pub fn new(start: u128) -> Arc<Self> {
        Arc::new(Self { total: Mutex::new(start) })
    }

    pub fn deposit(&self, amount: u128) -> u128 {
        let mut total = self.total.lock().unwrap();
        *total += amount;
        *total
    }

    pub async fn total_async(&self) -> u128 {
        *self.total.lock().unwrap()
    }
}

/// A callback interface with custom-typed arguments and returns. A callback
/// returning a custom type over a scalar, like `Money`, needs #162's fix.
#[uniffi::export(callback_interface)]
pub trait Ledger: Send + Sync {
    fn record(&self, amount: u128, from: Url);
    fn balance(&self) -> u128;
    // #181 (in this fork) fixes callbacks returning custom scalars (#162).
    fn cash(&self) -> Money;
}

/// A foreign-implementable trait, sync and async.
#[uniffi::export(with_foreign)]
#[async_trait::async_trait]
pub trait Pricer: Send + Sync {
    fn price(&self, item: Url) -> u128;
    async fn price_async(&self, item: Url) -> u128;
    fn cents(&self, price: Money) -> u128;
    fn discount(&self, price: Money) -> Money;
}

#[uniffi::export]
pub fn record_and_read(ledger: Box<dyn Ledger>, amount: u128, from: Url) -> u128 {
    ledger.record(amount, from);
    ledger.balance()
}

#[uniffi::export]
pub fn price_of(pricer: Arc<dyn Pricer>, item: Url) -> u128 {
    pricer.price(item)
}

#[uniffi::export]
pub async fn price_of_async(pricer: Arc<dyn Pricer>, item: Url) -> u128 {
    pricer.price_async(item).await
}

#[uniffi::export]
pub fn cash_in(ledger: Box<dyn Ledger>) -> Money {
    ledger.cash()
}

#[uniffi::export]
pub fn discounted(pricer: Arc<dyn Pricer>, price: Money) -> Money {
    pricer.discount(price)
}

#[uniffi::export]
pub fn cents_of(pricer: Arc<dyn Pricer>, price: Money) -> u128 {
    pricer.cents(price)
}

// UniFFI 0.29 rejects default values on custom types, so the upstream
// fixture's default cases are left out here.

uniffi::include_scaffolding!("api");
