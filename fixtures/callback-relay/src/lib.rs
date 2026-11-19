//! Rust calls synchronous Dart callbacks from threads other than Dart's, the
//! way the Breez SDK calls an external signer's `is_remote` from a tokio task.
//! With the generated callback relay library bundled, each call runs on the
//! Dart isolate's thread while the Rust thread waits for its result. Without
//! it, the call aborts the Dart VM ("Cannot invoke native callback outside an
//! isolate").

use std::sync::{Arc, Mutex, OnceLock};

fn runtime() -> &'static tokio::runtime::Runtime {
    static RUNTIME: OnceLock<tokio::runtime::Runtime> = OnceLock::new();
    RUNTIME.get_or_init(|| {
        tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_time()
            .build()
            .expect("tokio runtime")
    })
}

/// Runs `f` on a new thread and returns its result, or `None` if it panicked,
/// as UniFFI does when a callback reports an unexpected error.
async fn on_thread<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> Option<T> {
    let (tx, rx) = futures::channel::oneshot::channel();
    std::thread::spawn(move || {
        let _ = tx.send(std::panic::catch_unwind(std::panic::AssertUnwindSafe(f)).ok());
    });
    rx.await.ok().flatten()
}

#[derive(Debug, uniffi::Error)]
pub enum SignError {
    Rejected { reason: String },
}

impl std::fmt::Display for SignError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SignError::Rejected { reason } => write!(f, "rejected: {reason}"),
        }
    }
}

impl std::error::Error for SignError {}

/// A foreign-implementable trait, like the SDK's `ExternalSparkSigner`.
#[uniffi::export(with_foreign)]
pub trait Signer: Send + Sync {
    fn is_remote(&self) -> bool;
    fn sign(&self, message: String) -> Result<String, SignError>;
}

/// A callback interface with every kind of method.
#[uniffi::export(callback_interface)]
#[async_trait::async_trait]
pub trait Counter: Send + Sync {
    fn store(&self, value: u32);
    fn load(&self) -> u32;
    fn adopt(&self, signer: Arc<dyn Signer>) -> bool;
    async fn add_later(&self, amount: u32) -> u32;
}

/// Used only by the exited-isolate tests, so other test isolates never
/// install its vtable.
#[uniffi::export(callback_interface)]
#[async_trait::async_trait]
pub trait Keeper: Send + Sync {
    fn note(&self, value: u32);
    fn value(&self) -> u32;
    async fn value_later(&self) -> u32;
    async fn note_later(&self, value: u32);
}

#[uniffi::export]
pub fn is_remote_here(signer: Arc<dyn Signer>) -> bool {
    signer.is_remote()
}

#[uniffi::export]
pub async fn is_remote_from_thread(signer: Arc<dyn Signer>) -> Option<bool> {
    on_thread(move || signer.is_remote()).await
}

/// Deadlocks through the relay: the Dart thread blocks in this call while the
/// worker waits for the Dart thread to run `is_remote`.
#[uniffi::export]
pub fn is_remote_joined(signer: Arc<dyn Signer>) -> bool {
    std::thread::spawn(move || signer.is_remote()).join().expect("worker")
}

#[uniffi::export]
pub async fn is_remote_from_tokio(signer: Arc<dyn Signer>) -> bool {
    runtime().spawn(async move { signer.is_remote() }).await.expect("tokio task")
}

/// Signs `0` to `count - 1` from that many threads at once, in order.
#[uniffi::export]
pub async fn sign_from_threads(signer: Arc<dyn Signer>, count: u32) -> Vec<String> {
    let (tx, rx) = futures::channel::oneshot::channel();
    std::thread::spawn(move || {
        let workers: Vec<_> = (0..count)
            .map(|i| {
                let signer = signer.clone();
                std::thread::spawn(move || signer.sign(i.to_string()).expect("signed"))
            })
            .collect();
        let _ = tx.send(workers.into_iter().map(|w| w.join().expect("worker")).collect());
    });
    rx.await.expect("signatures")
}

#[uniffi::export]
pub async fn sign_from_thread(
    signer: Arc<dyn Signer>,
    message: String,
) -> Result<String, SignError> {
    on_thread(move || signer.sign(message)).await.expect("no panic")
}

#[uniffi::export]
pub fn store_then_load_here(counter: Box<dyn Counter>, value: u32) -> u32 {
    counter.store(value);
    counter.load()
}

#[uniffi::export]
pub async fn store_then_load_from_thread(counter: Box<dyn Counter>, value: u32) -> Option<u32> {
    on_thread(move || {
        counter.store(value);
        counter.load()
    })
    .await
}

/// Calls `load` from a worker thread. The test's `load` calls back into Rust,
/// which calls the signer on the Dart thread.
#[uniffi::export]
pub async fn load_from_thread(counter: Box<dyn Counter>) -> Option<u32> {
    on_thread(move || counter.load()).await
}

/// Passes a foreign trait object to a callback from a worker thread, which
/// makes Rust lower it there.
#[uniffi::export]
pub async fn adopt_from_thread(counter: Box<dyn Counter>, signer: Arc<dyn Signer>) -> Option<bool> {
    on_thread(move || counter.adopt(signer)).await
}

#[uniffi::export]
pub async fn add_later_from_tokio(counter: Box<dyn Counter>, amount: u32) -> u32 {
    runtime().spawn(async move { counter.add_later(amount).await }).await.expect("tokio task")
}

/// Drops the callback on a worker thread, which calls `uniffi_free` there.
#[uniffi::export]
pub async fn drop_from_thread(counter: Box<dyn Counter>) {
    on_thread(move || drop(counter)).await;
}

static KEPT: Mutex<Option<Arc<dyn Keeper>>> = Mutex::new(None);

fn kept() -> Arc<dyn Keeper> {
    KEPT.lock().unwrap().clone().expect("a kept Keeper")
}

#[uniffi::export]
pub fn keep(keeper: Box<dyn Keeper>) {
    *KEPT.lock().unwrap() = Some(Arc::from(keeper));
}

#[uniffi::export]
pub fn release_kept() {
    KEPT.lock().unwrap().take();
}

#[uniffi::export]
pub async fn kept_value_from_thread() -> Option<u32> {
    let keeper = kept();
    on_thread(move || keeper.value()).await
}

/// Returns false if the call panicked.
#[uniffi::export]
pub async fn kept_note_from_thread(value: u32) -> bool {
    let keeper = kept();
    on_thread(move || keeper.note(value)).await.is_some()
}

/// Returns false if the call panicked, as UniFFI does when an async callback
/// without an error type completes with an unexpected error.
#[uniffi::export]
pub async fn kept_note_later_from_tokio(value: u32) -> bool {
    let keeper = kept();
    runtime().spawn(async move { keeper.note_later(value).await }).await.is_ok()
}

/// Calls `note` on this thread, which installs the vtable in this isolate.
#[uniffi::export]
pub fn note_with(keeper: Box<dyn Keeper>, value: u32) {
    keeper.note(value);
}

/// Returns `None` if the call completed with an error.
#[uniffi::export]
pub async fn kept_value_later_from_tokio() -> Option<u32> {
    let keeper = kept();
    runtime().spawn(async move { keeper.value_later().await }).await.ok()
}

/// Used only by the two-isolate tests, so other test isolates never install
/// its vtable.
#[uniffi::export(callback_interface)]
#[async_trait::async_trait]
pub trait Tally: Send + Sync {
    fn name(&self) -> String;
    fn add(&self, amount: u32);
    async fn total_later(&self) -> u32;
}

static TALLIES: Mutex<Vec<Arc<dyn Tally>>> = Mutex::new(Vec::new());

fn tallies() -> Vec<Arc<dyn Tally>> {
    TALLIES.lock().unwrap().clone()
}

#[uniffi::export]
pub fn add_tally(tally: Box<dyn Tally>) {
    TALLIES.lock().unwrap().push(Arc::from(tally));
}

#[uniffi::export]
pub fn release_tallies() {
    TALLIES.lock().unwrap().clear();
}

/// Each tally's name, read from its own `std::thread`, or `None` if the call
/// panicked.
#[uniffi::export]
pub async fn tally_names_from_threads() -> Vec<Option<String>> {
    let mut names = Vec::new();
    for tally in tallies() {
        names.push(on_thread(move || tally.name()).await);
    }
    names
}

/// Adds `amount` to every tally from a tokio task. Returns false if a call
/// panicked.
#[uniffi::export]
pub async fn add_to_tallies_from_tokio(amount: u32) -> bool {
    runtime()
        .spawn(async move {
            for tally in tallies() {
                tally.add(amount);
            }
        })
        .await
        .is_ok()
}

/// Each tally's total from a tokio task, or `None` if the call failed.
#[uniffi::export]
pub async fn tally_totals_from_tokio() -> Vec<Option<u32>> {
    let mut totals = Vec::new();
    for tally in tallies() {
        totals.push(runtime().spawn(async move { tally.total_later().await }).await.ok());
    }
    totals
}

/// Finishes after `ms` milliseconds on a tokio worker, which wakes the
/// caller's continuation from that thread.
#[uniffi::export]
pub async fn finish_later(ms: u32) -> u32 {
    runtime()
        .spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(ms.into())).await;
            ms
        })
        .await
        .expect("tokio task")
}

uniffi::include_scaffolding!("api");
