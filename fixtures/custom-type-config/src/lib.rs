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

/// Milliseconds since the epoch, carried as a `u64` and configured as Dart's
/// `DateTime`. Plain u64 values take `int` or `BigInt` input, but a configured
/// type is its own Dart type and goes through its own converter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Stamp(pub u64);

uniffi::custom_type!(Stamp, u64, {
    lower: |stamp| stamp.0,
    try_lift: |millis| Ok(Stamp(millis)),
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

/// A callback interface with custom-typed arguments and returns, including a
/// configured custom type over a scalar, `Money` (#162).
#[uniffi::export(callback_interface)]
pub trait Ledger: Send + Sync {
    fn record(&self, amount: u128, from: Url);
    fn balance(&self) -> u128;
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
pub fn cents_of(pricer: Arc<dyn Pricer>, price: Money) -> u128 {
    pricer.cents(price)
}

#[uniffi::export]
pub fn cash_in(ledger: Box<dyn Ledger>) -> Money {
    ledger.cash()
}

#[uniffi::export]
pub fn discounted(pricer: Arc<dyn Pricer>, price: Money) -> Money {
    pricer.discount(price)
}

/// `Stamp` in a record, next to a plain u64.
#[derive(Debug, Clone, uniffi::Record)]
pub struct Event {
    pub at: Stamp,
    pub earlier: Option<Stamp>,
    pub ticks: u64,
}

#[uniffi::export]
pub fn echo_stamp(stamp: Stamp) -> Stamp {
    stamp
}

#[uniffi::export]
pub async fn echo_stamp_async(stamp: Stamp) -> Stamp {
    stamp
}

#[uniffi::export]
pub fn add_millis(stamp: Stamp, millis: u64) -> Stamp {
    Stamp(stamp.0 + millis)
}

#[uniffi::export]
pub fn echo_event(event: Event) -> Event {
    event
}

/// `Stamp` as a foreign trait's result, sync and async, and in a sequence.
#[uniffi::export(with_foreign)]
#[async_trait::async_trait]
pub trait Clock: Send + Sync {
    fn now(&self) -> Stamp;
    async fn now_async(&self) -> Stamp;
    fn history(&self) -> Vec<Stamp>;
}

#[uniffi::export]
pub fn read_clock(clock: Arc<dyn Clock>) -> Stamp {
    clock.now()
}

#[uniffi::export]
pub async fn read_clock_async(clock: Arc<dyn Clock>) -> Stamp {
    clock.now_async().await
}

#[uniffi::export]
pub fn read_history(clock: Arc<dyn Clock>) -> Vec<Stamp> {
    clock.history()
}

// UniFFI 0.29 rejects default values on custom types, so the upstream
// fixture's default cases are left out here.

uniffi::include_scaffolding!("api");
