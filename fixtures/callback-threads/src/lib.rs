//! Rust calls Dart callbacks from threads other than Dart's: `std::thread`
//! workers and tokio tasks. Built with `callback_listeners = true`, so async
//! methods, fire-and-forget sync methods and `uniffi_free` are delivered through
//! `NativeCallable.listener`. With `Pointer.fromFunction` each of these calls
//! aborts the Dart VM ("Cannot invoke native callback outside an isolate").

use std::sync::{Arc, OnceLock};
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

/// A callback interface, like the SDK's `EventListener` and `Logger`.
#[uniffi::export(callback_interface)]
#[async_trait::async_trait]
pub trait EventSink: Send + Sync {
    async fn on_event(&self, event: String) -> u32;
    fn log(&self, message: String);
    fn log_count(&self) -> u32;
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

/// Returns once every call was made, before Dart necessarily ran them.
#[uniffi::export]
pub fn log_from_threads(sink: Box<dyn EventSink>, threads: u32) {
    let sink: Arc<dyn EventSink> = Arc::from(sink);
    let workers: Vec<_> = (0..threads)
        .map(|i| {
            let sink = sink.clone();
            std::thread::spawn(move || sink.log(format!("thread {i}")))
        })
        .collect();
    for worker in workers {
        worker.join().expect("worker thread");
    }
}

#[uniffi::export]
pub fn log_from_tokio(sink: Box<dyn EventSink>, tasks: u32) {
    let sink: Arc<dyn EventSink> = Arc::from(sink);
    runtime().block_on(async move {
        let tasks: Vec<_> = (0..tasks)
            .map(|i| {
                let sink = sink.clone();
                tokio::spawn(async move { sink.log(format!("task {i}")) })
            })
            .collect();
        for task in tasks {
            task.await.expect("tokio task");
        }
    });
}

/// Calls `log` on the calling thread, which is the Dart thread here.
#[uniffi::export]
pub fn log_here(sink: Box<dyn EventSink>, message: String) {
    sink.log(message);
}

/// Calls `log` and then `log_count` on the calling thread, and returns the count.
#[uniffi::export]
pub fn log_then_count(sink: Box<dyn EventSink>, message: String) -> u32 {
    sink.log(message);
    sink.log_count()
}

/// Logs `count` numbered messages from one worker thread, in order, and joins it.
#[uniffi::export]
pub fn log_in_order(sink: Box<dyn EventSink>, count: u32) {
    std::thread::spawn(move || {
        for i in 0..count {
            sink.log(format!("{i}"));
        }
    })
    .join()
    .expect("worker thread");
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

uniffi::include_scaffolding!("api");
