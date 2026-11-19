//! Rust calls Dart callbacks from threads other than Dart's: `std::thread`
//! workers and tokio tasks. Async methods and `uniffi_free` are delivered
//! through `NativeCallable.listener`, which any thread may call. With
//! `Pointer.fromFunction` each of these calls aborts the Dart VM ("Cannot
//! invoke native callback outside an isolate").

use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use futures::StreamExt;

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

/// A callback interface, like the SDK's `EventListener`.
#[uniffi::export(callback_interface)]
#[async_trait::async_trait]
pub trait EventSink: Send + Sync {
    async fn on_event(&self, event: String) -> u32;
}

#[derive(Debug, uniffi::Error)]
pub enum StoreError {
    EmptyKey,
}

impl std::fmt::Display for StoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "empty key")
    }
}

impl std::error::Error for StoreError {}

/// A foreign-implementable trait, like the SDK's storage and signer traits.
#[uniffi::export(with_foreign)]
#[async_trait::async_trait]
pub trait Store: Send + Sync {
    async fn put(&self, key: String, value: String) -> Result<(), StoreError>;
    async fn fetch(&self, key: String) -> Option<String>;
    /// Returns a value, so it stays a synchronous call on the Dart thread.
    fn name(&self) -> String;
}

#[uniffi::export]
pub async fn event_from_thread(sink: Box<dyn EventSink>, event: String) -> u32 {
    let (tx, rx) = futures::channel::oneshot::channel();
    std::thread::spawn(move || {
        let _ = tx.send(futures::executor::block_on(sink.on_event(event)));
    });
    rx.await.expect("worker thread result")
}

#[uniffi::export]
pub async fn event_from_tokio(sink: Box<dyn EventSink>, event: String) -> u32 {
    runtime().spawn(async move { sink.on_event(event).await }).await.expect("tokio task")
}

/// Sums the results of `count` concurrent calls, each from its own thread.
#[uniffi::export]
pub async fn events_from_many_threads(sink: Box<dyn EventSink>, count: u32) -> u32 {
    let sink: Arc<dyn EventSink> = Arc::from(sink);
    let (tx, rx) = futures::channel::mpsc::unbounded();
    for i in 0..count {
        let sink = sink.clone();
        let tx = tx.clone();
        std::thread::spawn(move || {
            let result = futures::executor::block_on(sink.on_event(format!("event {i}")));
            tx.unbounded_send(result).expect("result channel");
        });
    }
    drop(tx);
    rx.fold(0, |sum, result| async move { sum + result }).await
}

/// Drops the callback object, which calls `uniffi_free`, on the calling thread.
#[uniffi::export]
pub fn drop_sink_here(sink: Box<dyn EventSink>) {
    drop(sink);
}

/// Drops the callback object, which calls `uniffi_free`, on a worker thread.
#[uniffi::export]
pub fn drop_sink_on_thread(sink: Box<dyn EventSink>) {
    std::thread::spawn(move || drop(sink)).join().expect("worker thread");
}

#[uniffi::export]
pub fn drop_store_on_tokio(store: Arc<dyn Store>) {
    runtime().block_on(async move {
        tokio::spawn(async move { drop(store) }).await.expect("tokio task");
    });
}

/// Starts `on_event` on a tokio task and stops waiting after `wait_ms`. Returns
/// true when Rust dropped the pending call before Dart completed it.
#[uniffi::export]
pub async fn abandon_event(sink: Box<dyn EventSink>, event: String, wait_ms: u32) -> bool {
    runtime()
        .spawn(async move {
            tokio::select! {
                _ = sink.on_event(event) => false,
                _ = tokio::time::sleep(Duration::from_millis(wait_ms.into())) => true,
            }
        })
        .await
        .expect("tokio task")
}

/// Stores and reads back a value through the foreign store on a tokio task.
#[uniffi::export]
pub async fn store_roundtrip_on_tokio(
    store: Arc<dyn Store>,
    key: String,
    value: String,
) -> Result<Option<String>, StoreError> {
    runtime()
        .spawn(async move {
            store.put(key.clone(), value).await?;
            Ok(store.fetch(key).await)
        })
        .await
        .expect("tokio task")
}

#[uniffi::export]
pub fn read_store_name(store: Arc<dyn Store>) -> String {
    store.name()
}

/// Like the SDK's `Logger` and `EventListener`: methods that return nothing and
/// can't fail, plus one that returns a value. Only the stale-handle case uses
/// it, so no other case installs its vtable.
#[uniffi::export(callback_interface)]
#[async_trait::async_trait]
pub trait Notifier: Send + Sync {
    fn notify(&self, message: String);
    async fn notify_later(&self, message: String);
    fn count(&self) -> u32;
}

static HELD_NOTIFIER: Mutex<Option<Arc<dyn Notifier>>> = Mutex::new(None);

fn held_notifier() -> Arc<dyn Notifier> {
    HELD_NOTIFIER.lock().unwrap().clone().expect("a held notifier")
}

/// Keeps the notifier for the life of the process, as the SDK keeps a logger.
#[uniffi::export]
pub fn hold_notifier(notifier: Box<dyn Notifier>) {
    *HELD_NOTIFIER.lock().unwrap() = Some(Arc::from(notifier));
}

#[uniffi::export]
pub fn release_notifier() {
    HELD_NOTIFIER.lock().unwrap().take();
}

#[uniffi::export]
pub fn notify_with(notifier: Box<dyn Notifier>, message: String) {
    notifier.notify(message);
}

#[uniffi::export]
pub fn notify_held(message: String) {
    held_notifier().notify(message);
}

/// Returns false if the call panicked, as UniFFI does when a callback without
/// an error type reports an unexpected error.
#[uniffi::export]
pub async fn notify_held_later_on_tokio(message: String) -> bool {
    let notifier = held_notifier();
    runtime().spawn(async move { notifier.notify_later(message).await }).await.is_ok()
}

#[uniffi::export]
pub fn count_held() -> u32 {
    held_notifier().count()
}

uniffi::include_scaffolding!("api");
